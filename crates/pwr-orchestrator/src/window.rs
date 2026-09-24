//! The working window, computed from the host and the model instead of probed.
//!
//! The old regime chose a window by filling the context at a ladder of sizes and
//! keeping the largest that passed. That took minutes, it was the gate on every
//! run, and on this project it produced 16,384 tokens from a speed measurement
//! later found to be wrong by a factor of six. The redesign (Part D) replaces it
//! with arithmetic: what the model's own config says a token of cache costs, what
//! the host has left once the weights are resident, and what the model was
//! trained to hold.
//!
//! Every ceiling is kept, not only the one that won, and the decision names
//! which one bound it. A window a person cannot trace back to a reason is the
//! same problem as a window nobody measured.
//!
//! Nothing here does I/O. The caller reads the config and the host; this decides.

use pwr_domain::{EvidenceLabel, ExecutionProfile};
use serde::{Deserialize, Serialize};

/// Bytes per element of a cached key or value when the config does not say.
///
/// Half precision is what MLX and llama.cpp both default the cache to. A
/// quantised cache is smaller, so assuming two bytes can only make the memory
/// ceiling cautious, never optimistic.
pub const DEFAULT_CACHE_ELEMENT_BYTES: u64 = 2;

/// Prefill's transient memory, as a multiple of the cache it is filling.
///
/// Measured once, by the engine spike of 2026-09-17, on Qwen3.6-35B-A3B through
/// MLX: 24 GB of transient on top of a cache of about 5.4 GB at 256k, with the
/// whole run peaking at 49 GB. One model, one engine, one chunk size, so this is
/// a single observation and labelled as one; smaller prefill chunks reduce it.
/// Leaving it out is what would make a computed window fit on paper and swap in
/// practice.
///
/// It is also the most consequential number in this module, and modelling the
/// transient as proportional to the cache is a proxy: prefill's scratch memory
/// follows the chunk and the attention shape, not the cache's size. On a dense
/// model with a large cache the proxy likely overstates it -- for Seed-OSS-36B
/// on a 64 GB host it gives about 23k tokens against about 119k with no
/// transient at all -- and an overstated transient recreates the starved window
/// this module exists to end. Measuring it per architecture is the first thing
/// the catalogue owes this constant.
pub const PREFILL_TRANSIENT_FACTOR: u64 = 4;

/// What the host keeps for itself and for everything the user is also running.
///
/// A quarter of memory, never less than 8 GiB. On a 64 GB machine that leaves
/// 48 GB to the model, which is the order of what Metal lets a process wire by
/// default on Apple silicon. It is a reserve, not a measurement: the app will
/// replace it with observed pressure once turns are recorded.
pub fn default_reserve_bytes(total_memory_bytes: u64) -> u64 {
    (total_memory_bytes / 4).max(8 * GIB)
}

/// Windows are rounded down to this, so a computed window reads as a choice
/// and two hosts a few megabytes apart do not get two different windows.
pub const WINDOW_GRANULARITY: u32 = 1024;

const GIB: u64 = 1024 * 1024 * 1024;

/// What the model's config says about its cache and its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelShape {
    /// The length the model was trained to, from `max_position_embeddings`.
    pub trained_max: Option<u32>,
    /// Cache bytes one token of context costs, across every layer that keeps one.
    pub kv_bytes_per_token: Option<u64>,
    /// Bytes the weights occupy once loaded.
    pub weights_bytes: Option<u64>,
    /// Attention scores one prefill step materialises, 0 when the engine's
    /// attention is fused; `None` when the engine cannot say. See `decide`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefill_scores_bytes: Option<u64>,
}

impl ModelShape {
    /// Reads a HuggingFace-style `config.json`.
    ///
    /// Multimodal models nest the language model's settings under
    /// `text_config`; those are the ones that describe the cache, so they win
    /// where both are present. The weights are not in the config and are left
    /// for the caller, who knows the artifact's size on disk.
    pub fn from_config(config: &serde_json::Value) -> Self {
        let text = config.get("text_config").unwrap_or(config);
        let field = |name: &str| text.get(name).or_else(|| config.get(name));
        let trained_max = field("max_position_embeddings")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok());
        ModelShape {
            trained_max,
            kv_bytes_per_token: kv_bytes_per_token(text, config),
            weights_bytes: None,
            prefill_scores_bytes: None,
        }
    }

    /// Reads GGUF metadata as llama.cpp and Ollama publish it, with keys
    /// prefixed by the architecture (`qwen35moe.block_count`).
    ///
    /// `head_count_kv` is a number for most models and a per-layer array for
    /// hybrid ones, where a layer with no attention has zero; summing the array
    /// counts exactly the heads that keep a cache, which is the per-layer rule
    /// `from_config` follows with `layer_types`. Key and value lengths are read
    /// separately where published, since they need not be equal.
    pub fn from_gguf(metadata: &serde_json::Value) -> Self {
        let Some(arch) = metadata
            .get("general.architecture")
            .and_then(serde_json::Value::as_str)
        else {
            return ModelShape::unknown();
        };
        let get = |key: &str| metadata.get(format!("{arch}.{key}"));
        let number = |key: &str| get(key).and_then(serde_json::Value::as_u64);
        let trained_max = number("context_length").and_then(|n| u32::try_from(n).ok());
        let kv_bytes_per_token = (|| {
            let layers = number("block_count")?;
            let heads = number("attention.head_count");
            let head_dim = number("embedding_length").zip(heads).map(|(e, h)| e / h);
            let key_len = number("attention.key_length").or(head_dim)?;
            let value_len = number("attention.value_length").unwrap_or(key_len);
            let kv_heads_summed = match get("attention.head_count_kv") {
                Some(serde_json::Value::Array(per_layer)) => {
                    per_layer.iter().filter_map(serde_json::Value::as_u64).sum()
                }
                Some(value) => {
                    let kv = value.as_u64()?;
                    let growing = match number("full_attention_interval") {
                        Some(interval) if interval > 0 => layers / interval,
                        _ => layers,
                    };
                    kv * growing
                }
                None => heads? * layers,
            };
            Some(kv_heads_summed * (key_len + value_len) * DEFAULT_CACHE_ELEMENT_BYTES)
        })();
        ModelShape {
            trained_max,
            kv_bytes_per_token,
            weights_bytes: None,
            prefill_scores_bytes: None,
        }
    }

    /// Reads whatever the backend could supply: a config, else GGUF metadata,
    /// with the backend's own trained length and size filling what those lack.
    pub fn from_facts(facts: &pwr_provider::ModelFacts) -> Self {
        let parsed = if let Some(config) = &facts.hf_config {
            ModelShape::from_config(config)
        } else if let Some(metadata) = &facts.gguf_metadata {
            ModelShape::from_gguf(metadata)
        } else {
            ModelShape::unknown()
        };
        ModelShape {
            trained_max: parsed.trained_max.or(facts.trained_max),
            kv_bytes_per_token: parsed.kv_bytes_per_token,
            weights_bytes: facts.weights_bytes,
            prefill_scores_bytes: facts.prefill_scores_bytes,
        }
    }

    pub fn unknown() -> Self {
        ModelShape {
            trained_max: None,
            kv_bytes_per_token: None,
            weights_bytes: None,
            prefill_scores_bytes: None,
        }
    }
}

/// Cache bytes per token of context, or `None` when the config does not say
/// enough to know.
///
/// Only layers that keep a growing key-value cache count. A linear-attention
/// layer keeps a fixed-size state, so Qwen 3.5/3.6's thirty such layers cost
/// nothing per token and only its ten full-attention layers do; counting all
/// forty would overstate the cost fourfold. A sliding-window layer's cache stops
/// growing at its window, so it is left out of the per-token cost too -- its
/// bounded size is small next to the weights, and a per-token figure cannot
/// express it honestly.
fn kv_bytes_per_token(text: &serde_json::Value, config: &serde_json::Value) -> Option<u64> {
    let get = |name: &str| {
        text.get(name)
            .or_else(|| config.get(name))
            .and_then(serde_json::Value::as_u64)
    };
    let layers = get("num_hidden_layers")?;
    let element = cache_element_bytes(text).unwrap_or(DEFAULT_CACHE_ELEMENT_BYTES);
    // Multi-head latent attention (DeepSeek, GLM-4.7-Flash): mlx-lm caches
    // the compressed latent and the rotary key, one of each per layer, not a
    // key and a value per head. Read as ordinary heads, GLM-4.7-Flash came out
    // at 383,520 bytes a token against the 54,144 its cache holds.
    if let (Some(rank), Some(rope)) = (get("kv_lora_rank"), get("qk_rope_head_dim")) {
        return Some(layers * (rank + rope) * element);
    }
    let heads = get("num_attention_heads");
    let kv_heads = get("num_key_value_heads").or(heads)?;
    let head_dim = get("head_dim").or_else(|| Some(get("hidden_size")? / heads?))?;
    // `layer_types` (Qwen 3.5/3.6) or `layers_block_type` (Nemotron-H, whose
    // Mamba and MoE layers keep no growing cache): either names each layer.
    let types = text
        .get("layer_types")
        .or_else(|| text.get("layers_block_type"))
        .or_else(|| config.get("layers_block_type"));
    let growing_layers = match types.and_then(|t| t.as_array()) {
        Some(types) => types
            .iter()
            .filter(|t| matches!(t.as_str(), Some("full_attention" | "attention")))
            .count() as u64,
        None => match get("full_attention_interval") {
            Some(interval) if interval > 0 => layers / interval,
            _ => layers,
        },
    };
    Some(growing_layers * kv_heads * head_dim * 2 * element)
}

fn cache_element_bytes(text: &serde_json::Value) -> Option<u64> {
    match text.get("torch_dtype")?.as_str()? {
        "float32" => Some(4),
        "float16" | "bfloat16" => Some(2),
        _ => None,
    }
}

/// What the machine offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostBudget {
    pub total_memory_bytes: u64,
    pub reserve_bytes: u64,
}

impl HostBudget {
    pub fn with_default_reserve(total_memory_bytes: u64) -> Self {
        HostBudget {
            total_memory_bytes,
            reserve_bytes: default_reserve_bytes(total_memory_bytes),
        }
    }
}

/// One limit on the window, and what it came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ceiling {
    /// The length the model was trained to hold.
    Trained,
    /// What fits in memory once the weights, the reserve and prefill's
    /// transient are paid for.
    Memory,
    /// The longest context the model is published to actually use well, which
    /// is often well short of its trained length.
    EffectiveContext,
    /// What the person chose.
    Setting,
    /// The conservative window that applies when memory cannot be computed
    /// and no window was chosen in settings.
    ///
    /// Without it an unknown memory ceiling would leave the trained length to
    /// bind, and a 16 GB machine would be handed a model's full 262,144 tokens
    /// because nothing said it could not hold them.
    Fallback,
}

/// A ceiling's value, or why there is none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "tokens")]
pub enum Limit {
    Tokens(u32),
    /// The inputs were not available, so this ceiling says nothing. Recorded
    /// rather than dropped, because "not known" and "no limit" read the same
    /// when a field is simply missing.
    Unknown,
    /// Not applicable: the person chose no window, or no effective length is
    /// published.
    NotSet,
}

/// The window, the ceiling that set it, and every ceiling considered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowDecision {
    pub tokens: u32,
    /// The ceiling that bound it. Always one of them; `Option` only so a
    /// persisted decision from before the fallback was a ceiling still reads.
    pub bound_by: Option<Ceiling>,
    pub ceilings: Vec<(Ceiling, Limit)>,
    /// Said in words, for the artifact and for the person.
    pub rationale: String,
}

/// Why no window could be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowError {
    /// The weights alone, with the reserve, do not fit this host.
    WeightsDoNotFit {
        needed_bytes: u64,
        available_bytes: u64,
    },
}

impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowError::WeightsDoNotFit {
                needed_bytes,
                available_bytes,
            } => write!(
                f,
                "the weights need {:.1} GiB and this host leaves {:.1} GiB after its reserve",
                *needed_bytes as f64 / GIB as f64,
                *available_bytes as f64 / GIB as f64
            ),
        }
    }
}

/// The window used when memory cannot be computed and nothing was chosen.
///
/// The value the retired bootstrap profile used, for the same reason: small
/// enough that a local deployment can be expected to serve it, and not a claim
/// about this machine. A decision that falls back to it says so.
pub const FALLBACK_WINDOW: u32 = 32_768;

/// Decides the working window.
///
/// The smallest known ceiling wins. With no ceiling known, the fallback
/// applies and the decision says that nothing bound it.
pub fn decide(
    shape: &ModelShape,
    host: Option<&HostBudget>,
    effective_context: Option<u32>,
    setting: Option<u32>,
) -> Result<WindowDecision, WindowError> {
    let memory = match (host, shape.kv_bytes_per_token, shape.weights_bytes) {
        (Some(host), Some(per_token), Some(weights)) if per_token > 0 => {
            let available = host.total_memory_bytes.saturating_sub(host.reserve_bytes);
            if weights >= available {
                return Err(WindowError::WeightsDoNotFit {
                    needed_bytes: weights,
                    available_bytes: available,
                });
            }
            let room = available - weights;
            let tokens = match shape.prefill_scores_bytes {
                // The engine's own account (PWR's MLX engine). The peak is
                // weights + cache + the larger of two transients that do not
                // overlap: a copy of the cache while it grows, and the scores a
                // prefill step materialises when attention is not fused.
                // Measured 2026-09-19 on an M2 Max: Qwen3.6-35B-A3B at 240,916
                // tokens peaked at 36.1 GB against 35.7 predicted (scores
                // bind); Seed-OSS-36B at 60,112 tokens at 52.0 against 51.9
                // (the cache copy binds, attention fused).
                Some(scores) => {
                    let with_copy = room / (2 * per_token);
                    let with_scores = room.saturating_sub(scores) / per_token;
                    with_copy.min(with_scores)
                }
                None => room / (per_token * (1 + PREFILL_TRANSIENT_FACTOR)),
            };
            Limit::Tokens(u32::try_from(tokens).unwrap_or(u32::MAX))
        }
        _ => Limit::Unknown,
    };
    let ceilings = vec![
        (
            Ceiling::Trained,
            shape.trained_max.map_or(Limit::Unknown, Limit::Tokens),
        ),
        (Ceiling::Memory, memory),
        (
            Ceiling::EffectiveContext,
            effective_context.map_or(Limit::NotSet, Limit::Tokens),
        ),
        (
            Ceiling::Setting,
            setting.map_or(Limit::NotSet, Limit::Tokens),
        ),
        // Only when memory is unknown and the person chose nothing. The
        // fallback stands in for what PWR does not know; a window someone
        // set on purpose is theirs to answer for, and overriding it with a
        // guess would be the harness claiming knowledge it just said it lacks.
        (
            Ceiling::Fallback,
            if memory == Limit::Unknown && setting.is_none() {
                Limit::Tokens(FALLBACK_WINDOW)
            } else {
                Limit::NotSet
            },
        ),
    ];
    let binding = ceilings
        .iter()
        .filter_map(|(ceiling, limit)| match limit {
            Limit::Tokens(tokens) => Some((*ceiling, *tokens)),
            _ => None,
        })
        // First on ties, so an equal setting and trained length reads as the
        // model's limit rather than the person's.
        .min_by_key(|(_, tokens)| *tokens);
    let (tokens, bound_by) = match binding {
        Some((ceiling, tokens)) => (tokens, Some(ceiling)),
        None => (FALLBACK_WINDOW, Some(Ceiling::Fallback)),
    };
    let tokens = (tokens / WINDOW_GRANULARITY * WINDOW_GRANULARITY).max(WINDOW_GRANULARITY);
    let rationale = match bound_by {
        Some(Ceiling::Trained) => "the model's trained length".to_owned(),
        Some(Ceiling::Memory) => match shape.prefill_scores_bytes {
            Some(_) => "what fits in memory after the weights, the host's reserve and prefill's \
                        transient as the engine reports it (the larger of a cache copy and the \
                        attention scores it materialises)"
                .to_owned(),
            None => format!(
                "what fits in memory after the weights, the host's reserve and prefill's \
                 transient ({PREFILL_TRANSIENT_FACTOR}x the cache, observed once)"
            ),
        },
        Some(Ceiling::EffectiveContext) => {
            "the model's published effective context, shorter than its trained length".to_owned()
        }
        Some(Ceiling::Setting) => "the window chosen in settings".to_owned(),
        Some(Ceiling::Fallback) => format!(
            "the conservative fallback of {FALLBACK_WINDOW} tokens, because the memory this \
             model needs per token could not be computed from what the backend reports"
        ),
        None => unreachable!("the fallback ceiling is always known when memory is not"),
    };
    Ok(WindowDecision {
        tokens,
        bound_by,
        ceilings,
        rationale: format!("window of {tokens} tokens, set by {rationale}"),
    })
}

/// The execution profile for a computed window.
///
/// `window_in_force` is what the backend reports it actually loaded, which can
/// be less than was asked for -- LM Studio has been seen to ignore a requested
/// window for some model families. The profile takes the smaller and says so,
/// because a run told it has 262,144 tokens on a model loaded at 4,096 fails in
/// a way that looks like the model's fault.
///
/// The id is derived from what decided the window, not drawn at random, so two
/// campaigns run under identical conditions carry the same profile id and the
/// evaluator can pair them. The strategy id is left out of it: callers draw a
/// fresh one per run, and including it would make every id unique again.
pub fn computed_profile(
    strategy_id: pwr_domain::Id,
    decision: &WindowDecision,
    window_in_force: u32,
    compatibility_key: &str,
    model_identity: &str,
) -> Result<ExecutionProfile, String> {
    let context_tokens = decision.tokens.min(window_in_force);
    if context_tokens == 0 {
        return Err("the backend reports a context window of zero".into());
    }
    let rationale = if window_in_force < decision.tokens {
        format!(
            "{}; the backend loaded only {window_in_force}, so that is the window in force",
            decision.rationale
        )
    } else {
        decision.rationale.clone()
    };
    let identity = serde_json::json!({
        "compatibility_key": compatibility_key,
        "model": model_identity,
        "context_tokens": context_tokens,
        "ceilings": decision.ceilings,
        "transient_factor": PREFILL_TRANSIENT_FACTOR,
    });
    Ok(ExecutionProfile {
        schema_version: 1,
        id: pwr_domain::id_from_content(identity.to_string()),
        strategy_id,
        calibration_id: None,
        context_tokens,
        reserve_tokens: (context_tokens / 8).max(1),
        concurrency: 1,
        budgets: serde_json::json!({
            "max_actions": crate::DEFAULT_MAX_ACTIONS,
            "edit_verify_cycles": 3,
            "context_retries": 1,
        }),
        rationale,
        evidence: EvidenceLabel::Computed,
        compatibility_key: compatibility_key.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    /// The config of `lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit`, reduced to
    /// the fields that matter. Thirty linear-attention layers and ten full.
    fn qwen36() -> serde_json::Value {
        let mut types = Vec::new();
        for layer in 0..40 {
            types.push(if layer % 4 == 3 {
                "full_attention"
            } else {
                "linear_attention"
            });
        }
        serde_json::json!({
            "model_type": "qwen3_5_moe",
            "text_config": {
                "num_hidden_layers": 40,
                "num_attention_heads": 16,
                "num_key_value_heads": 2,
                "head_dim": 256,
                "hidden_size": 2048,
                "max_position_embeddings": 262144,
                "full_attention_interval": 4,
                "layer_types": types,
            }
        })
    }

    #[test]
    fn qwen36_cache_cost_matches_what_the_spike_measured() {
        // 10 full-attention layers x 2 KV heads x 256 x key and value x 2
        // bytes. The spike computed 20.5 KB and measured 20.7.
        let shape = ModelShape::from_config(&qwen36());
        assert_eq!(shape.kv_bytes_per_token, Some(20_480));
        assert_eq!(shape.trained_max, Some(262_144));
    }

    #[test]
    fn the_interval_stands_in_for_layer_types_when_they_are_absent() {
        let mut config = qwen36();
        config["text_config"]
            .as_object_mut()
            .unwrap()
            .remove("layer_types");
        assert_eq!(
            ModelShape::from_config(&config).kv_bytes_per_token,
            Some(20_480)
        );
    }

    #[test]
    fn a_dense_model_with_no_head_dim_derives_it() {
        // Llama-style: every layer attends, head_dim from hidden / heads.
        let config = serde_json::json!({
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "hidden_size": 4096,
            "max_position_embeddings": 131072,
            "torch_dtype": "bfloat16",
        });
        let shape = ModelShape::from_config(&config);
        // 32 layers x 8 KV heads x 128 x 2 x 2 bytes.
        assert_eq!(shape.kv_bytes_per_token, Some(131_072));
    }

    #[test]
    fn a_config_without_attention_shape_gives_no_cost_rather_than_a_guess() {
        let config = serde_json::json!({"max_position_embeddings": 8192});
        let shape = ModelShape::from_config(&config);
        assert_eq!(shape.kv_bytes_per_token, None);
        assert_eq!(shape.trained_max, Some(8192));
    }

    #[test]
    fn on_this_mac_qwen36_is_bound_by_its_trained_length() {
        // What the spike found: the whole trained window fits in 64 GB.
        let shape = ModelShape {
            weights_bytes: Some(20_430 * 1_000_000),
            prefill_scores_bytes: None,
            ..ModelShape::from_config(&qwen36())
        };
        let host = HostBudget::with_default_reserve(64 * GIB);
        let decision = decide(&shape, Some(&host), None, None).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Trained));
        assert_eq!(decision.tokens, 262_144);
        let Limit::Tokens(memory) = decision.ceilings[1].1 else {
            panic!("memory ceiling should be computed");
        };
        assert!(memory > 262_144, "memory ceiling {memory}");
    }

    #[test]
    fn on_a_32_gb_host_the_same_model_is_bound_by_memory() {
        let shape = ModelShape {
            weights_bytes: Some(20_430 * 1_000_000),
            prefill_scores_bytes: None,
            ..ModelShape::from_config(&qwen36())
        };
        let host = HostBudget::with_default_reserve(32 * GIB);
        let decision = decide(&shape, Some(&host), None, None).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Memory));
        assert!(
            decision.tokens < 262_144 && decision.tokens >= 16_384,
            "{}",
            decision.tokens
        );
        assert_eq!(decision.tokens % WINDOW_GRANULARITY, 0);
    }

    #[test]
    fn weights_that_do_not_fit_are_refused_not_squeezed() {
        let shape = ModelShape {
            weights_bytes: Some(40 * GB),
            prefill_scores_bytes: None,
            ..ModelShape::from_config(&qwen36())
        };
        let host = HostBudget::with_default_reserve(32 * GIB);
        assert!(matches!(
            decide(&shape, Some(&host), None, None),
            Err(WindowError::WeightsDoNotFit { .. })
        ));
    }

    #[test]
    fn a_setting_below_every_other_ceiling_binds_and_says_so() {
        let shape = ModelShape::from_config(&qwen36());
        let decision = decide(&shape, None, None, Some(65_536)).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Setting));
        assert_eq!(decision.tokens, 65_536);
        assert!(decision.rationale.contains("settings"));
    }

    #[test]
    fn a_published_effective_context_undercuts_the_trained_length() {
        let shape = ModelShape {
            weights_bytes: Some(20_430 * 1_000_000),
            prefill_scores_bytes: None,
            ..ModelShape::from_config(&qwen36())
        };
        let host = HostBudget::with_default_reserve(64 * GIB);
        let decision = decide(&shape, Some(&host), Some(131_072), None).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::EffectiveContext));
        assert_eq!(decision.tokens, 131_072);
    }

    #[test]
    fn an_unknown_memory_ceiling_brings_in_the_fallback_rather_than_the_trained_length() {
        // No host and no weights: memory cannot be computed. Letting the
        // trained 262,144 bind would hand a small machine the whole window.
        let shape = ModelShape::from_config(&qwen36());
        let decision = decide(&shape, None, None, None).unwrap();
        assert_eq!(decision.ceilings[1], (Ceiling::Memory, Limit::Unknown));
        assert_eq!(decision.bound_by, Some(Ceiling::Fallback));
        assert_eq!(decision.tokens, FALLBACK_WINDOW);
    }

    #[test]
    fn a_chosen_window_is_honoured_when_memory_is_unknown() {
        let decision = decide(&ModelShape::unknown(), None, None, Some(65_536)).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Setting));
        assert_eq!(decision.tokens, 65_536);
    }

    #[test]
    fn a_chosen_window_never_exceeds_a_known_memory_ceiling() {
        let shape = ModelShape {
            weights_bytes: Some(20_430 * 1_000_000),
            prefill_scores_bytes: None,
            ..ModelShape::from_config(&qwen36())
        };
        let host = HostBudget::with_default_reserve(32 * GIB);
        let decision = decide(&shape, Some(&host), None, Some(262_144)).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Memory));
    }

    #[test]
    fn with_nothing_known_the_fallback_binds_and_says_why() {
        let decision = decide(&ModelShape::unknown(), None, None, None).unwrap();
        assert_eq!(decision.tokens, FALLBACK_WINDOW);
        assert_eq!(decision.bound_by, Some(Ceiling::Fallback));
        assert!(decision.rationale.contains("fallback"));
    }

    #[test]
    fn a_computed_memory_ceiling_leaves_the_fallback_out() {
        let shape = ModelShape {
            weights_bytes: Some(20_430 * 1_000_000),
            prefill_scores_bytes: None,
            ..ModelShape::from_config(&qwen36())
        };
        let host = HostBudget::with_default_reserve(64 * GIB);
        let decision = decide(&shape, Some(&host), None, None).unwrap();
        assert_eq!(decision.ceilings[4], (Ceiling::Fallback, Limit::NotSet));
    }

    #[test]
    fn a_gguf_with_a_scalar_kv_count_and_an_interval_matches_the_config() {
        // The same Qwen3.6 shape as published in GGUF metadata.
        let metadata = serde_json::json!({
            "general.architecture": "qwen35moe",
            "qwen35moe.block_count": 40,
            "qwen35moe.context_length": 262144,
            "qwen35moe.attention.head_count": 16,
            "qwen35moe.attention.head_count_kv": 2,
            "qwen35moe.attention.key_length": 256,
            "qwen35moe.attention.value_length": 256,
            "qwen35moe.full_attention_interval": 4,
        });
        let shape = ModelShape::from_gguf(&metadata);
        assert_eq!(shape.kv_bytes_per_token, Some(20_480));
        assert_eq!(shape.trained_max, Some(262_144));
    }

    #[test]
    fn a_gguf_with_a_per_layer_kv_array_counts_only_attending_layers() {
        let mut per_layer = vec![0u64; 40];
        for layer in (3..40).step_by(4) {
            per_layer[layer] = 2;
        }
        let metadata = serde_json::json!({
            "general.architecture": "qwen35moe",
            "qwen35moe.block_count": 40,
            "qwen35moe.attention.head_count": 16,
            "qwen35moe.attention.head_count_kv": per_layer,
            "qwen35moe.attention.key_length": 256,
        });
        assert_eq!(
            ModelShape::from_gguf(&metadata).kv_bytes_per_token,
            Some(20_480)
        );
    }

    #[test]
    fn a_gguf_without_an_architecture_says_nothing() {
        assert_eq!(
            ModelShape::from_gguf(&serde_json::json!({"x": 1})),
            ModelShape::unknown()
        );
    }

    #[test]
    fn a_hybrid_counts_only_its_attention_layers() {
        // Nemotron 3.5 Lightning: 23 Mamba, 23 MoE and 6 attention layers.
        let mut types = ["mamba", "moe"].repeat(23);
        types.extend(["attention"; 6]);
        let config = serde_json::json!({
            "model_type": "nemotron_h", "num_hidden_layers": 52,
            "num_attention_heads": 32, "num_key_value_heads": 2, "head_dim": 128,
            "layers_block_type": types, "max_position_embeddings": 262144,
        });
        assert_eq!(
            ModelShape::from_config(&config).kv_bytes_per_token,
            Some(6 * 2 * 128 * 2 * 2)
        );
    }

    #[test]
    fn latent_attention_caches_its_latent() {
        // GLM-4.7-Flash, as mlx-lm's glm4_moe_lite caches it.
        let config = serde_json::json!({
            "model_type": "glm4_moe_lite", "num_hidden_layers": 47,
            "num_attention_heads": 20, "num_key_value_heads": 20,
            "kv_lora_rank": 512, "qk_rope_head_dim": 64, "qk_nope_head_dim": 192,
            "v_head_dim": 256, "max_position_embeddings": 202752,
        });
        assert_eq!(
            ModelShape::from_config(&config).kv_bytes_per_token,
            Some(47 * 576 * 2)
        );
    }

    /// The two points the engine's memory model was fitted to, 2026-09-19 on
    /// a 64 GiB M2 Max: each measured peak must fit in what the window
    /// allows, and the window must not be far below what fits.
    #[test]
    fn the_engine_model_reproduces_its_two_measured_peaks() {
        let host = HostBudget::with_default_reserve(64 * GIB);
        let available = host.total_memory_bytes - host.reserve_bytes;
        // Seed-OSS-36B: fused attention, 256 KiB of cache per token.
        let seed = ModelShape {
            trained_max: Some(524_288),
            kv_bytes_per_token: Some(262_144),
            weights_bytes: Some(20_337_267_064),
            prefill_scores_bytes: Some(0),
        };
        let decision = decide(&seed, Some(&host), None, None).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Memory));
        let tokens = u64::from(decision.tokens);
        assert!((55_000..=62_000).contains(&tokens), "{tokens}");
        let peak = 20_337_267_064 + 2 * 262_144 * tokens;
        assert!(peak <= available, "{peak} > {available}");
        // Measured: 60,112 tokens peaked at 51,966,181,904 bytes.
        let predicted = 20_337_267_064u64 + 2 * 262_144 * 60_112;
        assert!(
            predicted.abs_diff(51_966_181_904) < 1_000_000_000,
            "{predicted}"
        );

        // Qwen3.6-35B-A3B: scores materialised, bounded to a quarter of a
        // 41.7 GB Metal buffer; the trained length binds.
        let qwen = ModelShape {
            trained_max: Some(262_144),
            kv_bytes_per_token: Some(20_480),
            weights_bytes: Some(20_402_204_271),
            prefill_scores_bytes: Some(41_747_087_360 / 4),
        };
        let decision = decide(&qwen, Some(&host), None, None).unwrap();
        assert_eq!(decision.bound_by, Some(Ceiling::Trained));
        // Measured: 240,916 tokens peaked at 36,062,635,422 bytes.
        let predicted = 20_402_204_271u64 + 20_480 * 240_916 + 41_747_087_360 / 4;
        assert!(
            predicted.abs_diff(36_062_635_422) < 1_000_000_000,
            "{predicted}"
        );
    }

    /// Without the engine's account the coarse rule stands: an HTTP backend's
    /// prefill is not ours to know.
    #[test]
    fn without_the_engines_account_the_coarse_rule_stands() {
        let host = HostBudget::with_default_reserve(64 * GIB);
        let seed = ModelShape {
            trained_max: Some(524_288),
            kv_bytes_per_token: Some(262_144),
            weights_bytes: Some(20_337_267_064),
            prefill_scores_bytes: None,
        };
        let decision = decide(&seed, Some(&host), None, None).unwrap();
        assert_eq!(decision.tokens, 23_552);
    }

    #[test]
    fn facts_prefer_the_config_and_take_the_size_from_the_backend() {
        let facts = pwr_provider::ModelFacts {
            hf_config: Some(qwen36()),
            trained_max: Some(4096),
            weights_bytes: Some(7),
            ..Default::default()
        };
        let shape = ModelShape::from_facts(&facts);
        assert_eq!(shape.trained_max, Some(262_144));
        assert_eq!(shape.kv_bytes_per_token, Some(20_480));
        assert_eq!(shape.weights_bytes, Some(7));
    }

    #[test]
    fn the_profile_takes_the_smaller_of_decided_and_served_and_says_so() {
        let shape = ModelShape::from_config(&qwen36());
        let decision = decide(&shape, None, None, Some(65_536)).unwrap();
        let profile =
            computed_profile(pwr_domain::new_id(), &decision, 4_096, "host", "model").unwrap();
        assert_eq!(profile.context_tokens, 4_096);
        assert_eq!(profile.evidence, EvidenceLabel::Computed);
        assert!(profile.calibration_id.is_none());
        assert!(profile.rationale.contains("loaded only 4096"));
        profile.validate_against(None).unwrap();
    }

    #[test]
    fn identical_conditions_give_the_same_profile_id_so_campaigns_can_pair() {
        let shape = ModelShape::from_config(&qwen36());
        let strategy = pwr_domain::new_id();
        let decision = decide(&shape, None, None, Some(65_536)).unwrap();
        let one = computed_profile(strategy, &decision, 262_144, "host", "model").unwrap();
        let two = computed_profile(strategy, &decision, 262_144, "host", "model").unwrap();
        let elsewhere = computed_profile(strategy, &decision, 262_144, "other", "model").unwrap();
        assert_eq!(one.id, two.id);
        assert_ne!(one.id, elsewhere.id);
    }

    #[test]
    fn the_reserve_is_a_quarter_but_never_below_8_gib() {
        assert_eq!(default_reserve_bytes(64 * GIB), 16 * GIB);
        assert_eq!(default_reserve_bytes(16 * GIB), 8 * GIB);
    }
}
