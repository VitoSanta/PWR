//! The Gemma 4 line: the models PWR runs, one per memory tier, and what is
//! fixed about all of them.
//!
//! PWR runs the Gemma 4 family only (owner's decision, 2026-10-14). The packs
//! are known in advance, so nothing about them is discovered, calibrated or
//! inferred from a model card at run time: their sampling, their context and
//! their reasoning are written here, once. A model that is not one of these
//! packs is not run.

use crate::{ReasoningBudgets, ReasoningCapability, ReasoningEvidence, ReasoningProfile};

/// Every tier is served its trained context, whole (owner's requirement:
/// large repositories are the target).
pub const CONTEXT_TOKENS: u32 = 262_144;

/// Google's sampling for Gemma 4, for all use cases.
pub const TEMPERATURE: f64 = 1.0;
pub const TOP_P: f64 = 0.95;
pub const TOP_K: u64 = 64;

/// The unified memory a pack is meant for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// 16 GB.
    Low,
    /// 24 to 32 GB.
    Middle,
    /// 64 GB and more.
    High,
}

impl Tier {
    /// The least unified memory the tier is for, in GiB.
    pub fn memory_gib(self) -> u64 {
        match self {
            Self::Low => 16,
            Self::Middle => 24,
            Self::High => 64,
        }
    }
}

/// The shape of a pack's attention, as far as its memory depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Attention {
    /// Layers that attend over the whole context; their cache grows with it.
    pub global_layers: u32,
    pub global_kv_heads: u32,
    pub global_head_dim: u32,
    /// Layers that attend over the last `window` tokens only.
    pub sliding_layers: u32,
    pub sliding_kv_heads: u32,
    pub sliding_head_dim: u32,
    pub window: u32,
    pub attention_heads: u32,
}

impl Attention {
    /// What one token of context costs the global layers' keys and values at
    /// 16 bits.
    pub fn cache_bytes_per_token(self) -> u64 {
        u64::from(self.global_layers)
            * u64::from(self.global_kv_heads)
            * u64::from(self.global_head_dim)
            * 2 // keys and values
            * 2 // 16 bits
    }

    /// What the sliding layers hold whatever the context: their window, and
    /// as much again while a prefill step is appended to it.
    pub fn sliding_bytes(self) -> u64 {
        u64::from(self.sliding_layers)
            * u64::from(self.sliding_kv_heads)
            * u64::from(self.sliding_head_dim)
            * 2
            * 2
            * u64::from(self.window)
            * 2
    }
}

/// One pack of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Pack {
    /// Where the pack lives under the models folder, and its Hub repository.
    pub model_ref: &'static str,
    pub label: &'static str,
    pub tier: Tier,
    /// The weights' bytes on disk (`strategies/artifacts.json`).
    pub weights_bytes: u64,
    pub attention: Attention,
    /// The small "assistant" model Google publishes beside it, for
    /// speculative decoding. Not a model to choose.
    pub assistant: &'static str,
}

const GEMMA_12B: Attention = Attention {
    global_layers: 8,
    global_kv_heads: 1,
    global_head_dim: 512,
    sliding_layers: 40,
    sliding_kv_heads: 8,
    sliding_head_dim: 256,
    window: 1024,
    attention_heads: 16,
};

const GEMMA_26B: Attention = Attention {
    global_layers: 5,
    global_kv_heads: 2,
    global_head_dim: 512,
    sliding_layers: 25,
    sliding_kv_heads: 8,
    sliding_head_dim: 256,
    window: 1024,
    attention_heads: 16,
};

const GEMMA_31B: Attention = Attention {
    global_layers: 10,
    global_kv_heads: 4,
    global_head_dim: 512,
    sliding_layers: 50,
    sliding_kv_heads: 16,
    sliding_head_dim: 256,
    window: 1024,
    attention_heads: 32,
};

/// The line, lightest first.
pub const PACKS: [Pack; 5] = [
    Pack {
        model_ref: "mlx-community/gemma-4-12B-it-4bit",
        label: "Gemma 4 12B",
        tier: Tier::Low,
        weights_bytes: 6_741_039_511,
        attention: GEMMA_12B,
        assistant: "mlx-community/gemma-4-12B-it-assistant-bf16",
    },
    Pack {
        model_ref: "mlx-community/gemma-4-26b-a4b-it-4bit",
        label: "Gemma 4 26B",
        tier: Tier::Middle,
        weights_bytes: 15_341_205_776,
        attention: GEMMA_26B,
        assistant: "mlx-community/gemma-4-26B-A4B-it-assistant-bf16",
    },
    Pack {
        model_ref: "mlx-community/gemma-4-26B-A4B-it-qat-4bit",
        label: "Gemma 4 26B QAT",
        tier: Tier::Middle,
        weights_bytes: 15_608_838_574,
        attention: GEMMA_26B,
        assistant: "mlx-community/gemma-4-26B-A4B-it-assistant-bf16",
    },
    Pack {
        model_ref: "mlx-community/gemma-4-26b-a4b-it-6bit",
        label: "Gemma 4 26B 6-bit",
        tier: Tier::Middle,
        weights_bytes: 21_647_423_449,
        attention: GEMMA_26B,
        assistant: "mlx-community/gemma-4-26B-A4B-it-assistant-bf16",
    },
    Pack {
        model_ref: "mlx-community/gemma-4-31b-it-8bit",
        label: "Gemma 4 31B",
        tier: Tier::High,
        weights_bytes: 33_763_120_415,
        attention: GEMMA_31B,
        assistant: "mlx-community/gemma-4-31B-it-assistant-bf16",
    },
];

/// Memory kept free for the system and the person's other applications,
/// on every tier (owner's decision, 2026-10-13: 6 GiB, the 24 GB tier
/// included).
pub const RESERVE_BYTES: u64 = 6 * GIB;

/// What a prefill step may hold in attention scores. Gemma's global layers
/// have 512-wide heads, which MLX's fused attention does not take, so a step
/// materialises its scores; the step shrinks as the context grows to stay
/// under this (`PWR_MLX_SCORES_MB` in the engine).
pub const SCORES_BYTES: u64 = GIB;

/// The engine's own memory beside weights, cache and scores: activations,
/// the vision encoder's buffers, the allocator's slack. An estimate until
/// measured per pack at the full window.
pub const ENGINE_BYTES: u64 = 3 * GIB / 2;

const GIB: u64 = 1024 * 1024 * 1024;

/// The memory a pack needs to hold `tokens` of context with its global
/// layers' cache at `kv_bits` (16, or 8 where a tier needs it).
pub fn memory_for(pack: &Pack, tokens: u32, kv_bits: u32) -> u64 {
    let per_token = pack.attention.cache_bytes_per_token() * u64::from(kv_bits.clamp(8, 16)) / 16;
    pack.weights_bytes
        + per_token * u64::from(tokens)
        + pack.attention.sliding_bytes()
        + SCORES_BYTES
        + ENGINE_BYTES
        + RESERVE_BYTES
}

/// The context a pack can hold in `memory` bytes of unified memory, at most
/// [`CONTEXT_TOKENS`], in whole thousands of tokens.
pub fn window(pack: &Pack, memory: u64, kv_bits: u32) -> u32 {
    let fixed = memory_for(pack, 0, kv_bits);
    let per_token = pack.attention.cache_bytes_per_token() * u64::from(kv_bits.clamp(8, 16)) / 16;
    let tokens = memory.saturating_sub(fixed) / per_token.max(1);
    (u32::try_from(tokens)
        .unwrap_or(u32::MAX)
        .min(CONTEXT_TOKENS)
        / 1024)
        * 1024
}

/// The pack a model reference names, if it is one of the line.
pub fn pack(model_ref: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|pack| pack.model_ref == model_ref)
}

/// Whether a model reference is one of Google's assistant drafts: kept on
/// disk for speculative decoding, never offered as a model to run.
pub fn is_assistant(model_ref: &str) -> bool {
    PACKS.iter().any(|pack| pack.assistant == model_ref)
        || model_ref.to_ascii_lowercase().contains("-assistant")
}

/// What the person is told when a model that is not one of the line is
/// asked for.
pub fn not_in_line(model_ref: &str) -> String {
    format!(
        "{model_ref} is not one of the Gemma 4 packs PWR runs ({})",
        PACKS
            .iter()
            .map(|pack| pack.label)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Gemma's reasoning, the same for every pack: a thought channel the engine
/// counts and can close, switched by the template's `enable_thinking`. Gemma
/// has thinking on or off and nothing between, so the person has one switch
/// ([`effort`]); with thinking on, the engine closes a thought at
/// [`THOUGHT_CAP`] tokens -- a safety cap against the thought that never ends
/// (google-deepmind/gemma#727), not a quality setting.
pub fn reasoning() -> ReasoningProfile {
    ReasoningProfile {
        capability: ReasoningCapability::ExplicitThinkingStream,
        evidence: ReasoningEvidence::Profile,
        switchable: true,
        disabled_by_profile: false,
        budgets: Some(ReasoningBudgets {
            low: 256,
            medium: THOUGHT_CAP,
            high: THOUGHT_CAP * 2,
        }),
        finalization_verified: None,
        off_at_low: true,
    }
}

/// The most a thought may run before the engine closes it.
pub const THOUGHT_CAP: u32 = 4_096;

/// The effort the reasoning planner is given for the person's switch.
pub fn effort(thinking: bool) -> crate::ReasoningEffort {
    if thinking {
        crate::ReasoningEffort::Medium
    } else {
        crate::ReasoningEffort::Low
    }
}

/// The sampling every request carries.
pub fn sampling() -> std::collections::BTreeMap<String, serde_json::Value> {
    [
        ("temperature", serde_json::json!(TEMPERATURE)),
        ("top_p", serde_json::json!(TOP_P)),
        ("top_k", serde_json::json!(TOP_K)),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_a_token_costs_is_the_global_layers() {
        assert_eq!(GEMMA_12B.cache_bytes_per_token(), 16_384);
        assert_eq!(GEMMA_26B.cache_bytes_per_token(), 20_480);
        assert_eq!(GEMMA_31B.cache_bytes_per_token(), 81_920);
    }

    #[test]
    fn the_full_context_is_counted_whole() {
        let gib = |bytes: u64| bytes as f64 / GIB as f64;
        let p31 = pack("mlx-community/gemma-4-31b-it-8bit").unwrap();
        // 31B at 8 bits: weights 31.4 GiB, global cache 20 GiB at 262k,
        // sliding layers 1.6 GiB (50 layers of 16 heads), then scores,
        // engine and reserve.
        assert!((gib(memory_for(p31, CONTEXT_TOKENS, 16)) - 61.5).abs() < 0.3);
        assert_eq!(window(p31, 64 * GIB, 16), CONTEXT_TOKENS);
        let p12 = pack("mlx-community/gemma-4-12B-it-4bit").unwrap();
        // On 16 GB the 12B does not hold 262k at 16 bits, whatever is hoped.
        assert!(window(p12, 16 * GIB, 16) < CONTEXT_TOKENS);
        assert_eq!(window(p12, 24 * GIB, 16), CONTEXT_TOKENS);
    }

    #[test]
    fn packs_and_assistants_are_told_apart() {
        assert_eq!(
            pack("mlx-community/gemma-4-26b-a4b-it-4bit").unwrap().tier,
            Tier::Middle
        );
        assert!(pack("mlx-community/gemma-4-26B-A4B-it-assistant-bf16").is_none());
        assert!(is_assistant("mlx-community/gemma-4-31B-it-assistant-bf16"));
        assert!(!is_assistant("mlx-community/gemma-4-31b-it-8bit"));
        assert!(pack("mlx-community/Qwen3.6-27B-4bit").is_none());
    }

    #[test]
    fn the_registry_and_the_line_agree() {
        let registry: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../strategies/artifacts.json")).unwrap();
        for pack in PACKS {
            let entry = registry["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["source"]["repository"] == pack.model_ref)
                .unwrap_or_else(|| panic!("{} is not in the registry", pack.model_ref));
            let weights: u64 = entry["source"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|file| file["path"].as_str().unwrap().ends_with(".safetensors"))
                .map(|file| file["bytes"].as_u64().unwrap())
                .sum();
            assert_eq!(weights, pack.weights_bytes, "{}", pack.model_ref);
        }
    }
}
