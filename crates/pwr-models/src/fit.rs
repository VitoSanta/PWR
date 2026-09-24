//! Whether a model variant is realistically loadable on this machine.
//!
//! Deterministic, conservative, and not a performance claim. It answers one
//! question -- can this be loaded here with a useful context? -- by the same
//! arithmetic PWR uses to choose a working window after a model is chosen
//! ([`pwr_orchestrator::window::decide`]): the weights, the memory one token
//! of context costs (from the model's `config.json`), prefill's transient, and
//! the reserve the host keeps for itself. A download rated "should fit" is one
//! the window computation will then give a window of at least that size.
//!
//! Assumptions, each stated in every estimate it applies to:
//!
//! - weights occupy their file size once loaded (true of MLX's and llama.cpp's
//!   memory-mapped formats to within a few percent);
//! - the engine itself -- Python and MLX, or llama-server -- costs
//!   [`RUNTIME_OVERHEAD_BYTES`];
//! - the host keeps a quarter of its memory, never less than 8 GiB, for the
//!   system and everything else running ([`window::default_reserve_bytes`]);
//! - prefill's transient is what each engine reports to the window
//!   computation: a copy of the cache for PWR's MLX engine (fused
//!   attention), [`window::PREFILL_TRANSIENT_FACTOR`] times the cache for
//!   llama.cpp, which reports nothing;
//! - a discrete GPU's memory adds to what llama.cpp can use, but a model split
//!   between GPU and system memory is rated no better than "should fit",
//!   since the part on the CPU runs far slower.
//!
//! File size alone is never treated as the memory a model needs.

use crate::catalog::Format;
use pwr_orchestrator::window::{self, HostBudget, ModelShape};
use pwr_runtime::host::HostProfile;
use serde::{Deserialize, Serialize};

const GIB: u64 = 1024 * 1024 * 1024;

/// What the engine process costs beyond the weights and the cache.
pub const RUNTIME_OVERHEAD_BYTES: u64 = GIB;

/// The context an estimate's "expected memory" is quoted at.
pub const REFERENCE_CONTEXT: u32 = 32_768;

/// Windows below which agent work is not practical: a system prompt, tool
/// schemas and two file reads fill 8k.
pub const MIN_USEFUL_WINDOW: u32 = 8_192;
const SHOULD_FIT_WINDOW: u32 = 16_384;
const RECOMMENDED_WINDOW: u32 = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitLevel {
    Recommended,
    ShouldFit,
    TightFit,
    NotRecommended,
    /// No engine on this machine can run the format.
    Incompatible,
    /// The machine's memory or the model's size could not be read.
    Unknown,
}

impl FitLevel {
    pub fn label(self) -> &'static str {
        match self {
            FitLevel::Recommended => "Recommended",
            FitLevel::ShouldFit => "Should fit",
            FitLevel::TightFit => "Tight fit",
            FitLevel::NotRecommended => "Not recommended",
            FitLevel::Incompatible => "Incompatible",
            FitLevel::Unknown => "Unknown",
        }
    }

    /// Whether "compatible with my hardware" includes it.
    pub fn fits(self) -> bool {
        matches!(
            self,
            FitLevel::Recommended | FitLevel::ShouldFit | FitLevel::TightFit
        )
    }
}

/// The host as the estimate sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub total_memory_bytes: Option<u64>,
    pub unified_memory: bool,
    pub apple_silicon: bool,
    /// Dedicated GPU memory reported exactly, if any.
    pub vram_bytes: Option<u64>,
}

impl From<&HostProfile> for Capacity {
    fn from(host: &HostProfile) -> Self {
        Capacity {
            total_memory_bytes: host.memory.total_bytes,
            unified_memory: host.memory.unified,
            apple_silicon: host.is_apple_silicon(),
            vram_bytes: if host.memory.unified {
                None
            } else {
                host.largest_vram_bytes()
            },
        }
    }
}

/// What the model is, as far as the estimate needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub format: Format,
    pub weights_bytes: u64,
    /// Cache bytes per token of context, from the config; `None` if unknown.
    pub kv_bytes_per_token: Option<u64>,
    pub trained_max: Option<u32>,
}

impl Footprint {
    /// From a `config.json` where there is one.
    pub fn new(format: Format, weights_bytes: u64, config: Option<&serde_json::Value>) -> Self {
        let shape = config.map(ModelShape::from_config);
        Footprint {
            format,
            weights_bytes,
            kv_bytes_per_token: shape.and_then(|shape| shape.kv_bytes_per_token),
            trained_max: shape.and_then(|shape| shape.trained_max),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitEstimate {
    pub level: FitLevel,
    pub label: String,
    pub weights_bytes: u64,
    pub runtime_overhead_bytes: u64,
    pub kv_bytes_per_token: Option<u64>,
    /// Weights, overhead and the cache at [`REFERENCE_CONTEXT`] (or the
    /// trained length, if shorter); without the cache when it is unknown.
    pub expected_bytes: u64,
    pub reference_context: u32,
    /// What the host leaves for a model after its reserve.
    pub budget_bytes: Option<u64>,
    /// The working window PWR would compute on this host.
    pub window_tokens: Option<u32>,
    /// The model would sit entirely in GPU memory (discrete GPUs only).
    pub gpu_resident: Option<bool>,
    pub explanation: String,
    pub assumptions: Vec<String>,
}

pub fn estimate(capacity: &Capacity, model: &Footprint) -> FitEstimate {
    let overhead = RUNTIME_OVERHEAD_BYTES;
    let reference = model
        .trained_max
        .map_or(REFERENCE_CONTEXT, |max| max.min(REFERENCE_CONTEXT));
    let cache_at_reference = model
        .kv_bytes_per_token
        .map(|per_token| per_token * u64::from(reference));
    let expected = model.weights_bytes + overhead + cache_at_reference.unwrap_or(0);
    let mut assumptions = vec![
        format!(
            "the engine costs about {} beyond the weights",
            gib(RUNTIME_OVERHEAD_BYTES)
        ),
        "the host keeps a quarter of its memory (at least 8 GiB) for the system and other apps"
            .to_owned(),
    ];
    let mut result = FitEstimate {
        level: FitLevel::Unknown,
        label: String::new(),
        weights_bytes: model.weights_bytes,
        runtime_overhead_bytes: overhead,
        kv_bytes_per_token: model.kv_bytes_per_token,
        expected_bytes: expected,
        reference_context: reference,
        budget_bytes: None,
        window_tokens: None,
        gpu_resident: None,
        explanation: String::new(),
        assumptions: Vec::new(),
    };
    if model.format == Format::Mlx && !capacity.apple_silicon {
        return finish(
            result,
            FitLevel::Incompatible,
            "MLX models run only on Apple Silicon Macs. Choose a GGUF variant for llama.cpp."
                .into(),
            assumptions,
        );
    }
    let Some(total) = capacity.total_memory_bytes else {
        return finish(
            result,
            FitLevel::Unknown,
            "This machine's memory could not be read, so no fit can be estimated.".into(),
            assumptions,
        );
    };
    if model.weights_bytes == 0 {
        return finish(
            result,
            FitLevel::Unknown,
            "The size of this model's files is not known.".into(),
            assumptions,
        );
    }
    let reserve = window::default_reserve_bytes(total);
    let vram = if model.format == Format::Gguf {
        capacity.vram_bytes
    } else {
        None
    };
    let host = HostBudget {
        total_memory_bytes: total + vram.unwrap_or(0),
        reserve_bytes: reserve,
    };
    let budget = host.total_memory_bytes.saturating_sub(reserve);
    result.budget_bytes = Some(budget);
    let memory_word = if capacity.unified_memory {
        "unified memory"
    } else {
        "system memory"
    };
    let machine = match vram {
        Some(vram) => format!(
            "This machine has {} of {memory_word} and {} of GPU memory",
            gib(total),
            gib(vram)
        ),
        None => format!("This machine has {} of {memory_word}", gib(total)),
    };
    let expected_line = match cache_at_reference {
        Some(cache) => format!(
            "Expected memory ~{}: {} of weights, ~{} for the engine and {} of context cache at {}k tokens.",
            gib(expected),
            gib(model.weights_bytes),
            gib(overhead),
            gib(cache),
            reference / 1024
        ),
        None => format!(
            "Expected memory ~{} before the context cache: {} of weights and ~{} for the engine.",
            gib(expected),
            gib(model.weights_bytes),
            gib(overhead)
        ),
    };
    let shape = ModelShape {
        trained_max: model.trained_max,
        kv_bytes_per_token: model.kv_bytes_per_token,
        weights_bytes: Some(model.weights_bytes + overhead),
        // What each engine reports to the window computation: PWR's MLX
        // engine measures fused attention on the host and then holds a copy
        // of the cache while it grows (0 extra score bytes); llama.cpp
        // reports nothing, and the conservative transient rule applies.
        prefill_scores_bytes: (model.format == Format::Mlx).then_some(0),
    };
    assumptions.push(match model.format {
        Format::Mlx => "MLX: prefill holds a second copy of the context cache while it grows, as \
                        PWR's engine reports for fused attention"
            .to_owned(),
        Format::Gguf => format!(
            "llama.cpp: prefill's transient is taken as {}x the context cache, the conservative \
             rule PWR applies where the engine reports nothing",
            window::PREFILL_TRANSIENT_FACTOR
        ),
    });
    let decision = window::decide(&shape, Some(&host), None, None);
    let resident = vram.map(|vram| {
        let cache = model.kv_bytes_per_token.unwrap_or(0) * u64::from(SHOULD_FIT_WINDOW);
        model.weights_bytes + overhead + cache <= vram * 9 / 10
    });
    result.gpu_resident = resident;
    if resident == Some(false) {
        assumptions.push(
            "a model larger than GPU memory is split with system memory by llama.cpp, which is \
             much slower"
                .to_owned(),
        );
    }
    let (level, window_line) = match (&decision, model.kv_bytes_per_token) {
        (Err(_), _) => (
            FitLevel::NotRecommended,
            format!(
                "{machine}; after the reserve that leaves {} for a model, less than its weights and \
                 the engine need.",
                gib(budget)
            ),
        ),
        (Ok(decision), Some(_)) => {
            let memory_window = decision
                .ceilings
                .iter()
                .find_map(|(ceiling, limit)| match (ceiling, limit) {
                    (window::Ceiling::Memory, window::Limit::Tokens(tokens)) => Some(*tokens),
                    _ => None,
                })
                .unwrap_or(decision.tokens);
            let usable = decision.tokens;
            result.window_tokens = Some(usable);
            let headroom = model.weights_bytes + overhead <= budget * 6 / 10;
            let level = if memory_window >= RECOMMENDED_WINDOW && headroom {
                FitLevel::Recommended
            } else if memory_window >= SHOULD_FIT_WINDOW {
                FitLevel::ShouldFit
            } else if memory_window >= MIN_USEFUL_WINDOW {
                FitLevel::TightFit
            } else {
                FitLevel::NotRecommended
            };
            let line = if level == FitLevel::NotRecommended {
                format!(
                    "{machine}; after the reserve it leaves {}, room for only ~{}k tokens of \
                     context, which is too little for agent work.",
                    gib(budget),
                    memory_window / 1024
                )
            } else {
                format!(
                    "{machine}; after the reserve it leaves {}, enough for a ~{}k-token working \
                     window.{}",
                    gib(budget),
                    usable / 1024,
                    if level == FitLevel::TightFit {
                        " Long conversations will be compacted often, and other large apps may \
                         push it into swap."
                    } else {
                        ""
                    }
                )
            };
            (level, line)
        }
        (Ok(_), None) => {
            assumptions.push(
                "the memory one token of context costs is unknown (no config.json to read), so \
                 the context this machine can hold is not estimated"
                    .to_owned(),
            );
            let used = model.weights_bytes + overhead;
            let level = if used <= budget / 2 {
                FitLevel::ShouldFit
            } else if used <= budget * 8 / 10 {
                FitLevel::TightFit
            } else {
                FitLevel::NotRecommended
            };
            (
                level,
                format!(
                    "{machine}; after the reserve it leaves {} for the model and its context \
                     cache. Long contexts can increase memory use significantly.",
                    gib(budget)
                ),
            )
        }
    };
    let level = match (level, resident) {
        (FitLevel::Recommended, Some(false)) => FitLevel::ShouldFit,
        (level, _) => level,
    };
    let gpu_line = match resident {
        Some(true) => " It fits entirely in GPU memory.",
        Some(false) if level.fits() => " It would be split between GPU and system memory.",
        _ => "",
    };
    finish(
        result,
        level,
        format!("{expected_line} {window_line}{gpu_line}"),
        assumptions,
    )
}

fn finish(
    mut result: FitEstimate,
    level: FitLevel,
    explanation: String,
    assumptions: Vec<String>,
) -> FitEstimate {
    result.level = level;
    result.label = level.label().to_owned();
    result.explanation = explanation;
    result.assumptions = assumptions;
    result
}

/// Bytes as the interface reads them: "12.4 GB" (binary gigabytes, as the OS
/// reports memory).
pub fn gib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let value = bytes as f64 / GIB as f64;
    if value >= 10.0 {
        format!("{value:.0} GB")
    } else if value >= 1.0 {
        format!("{value:.1} GB")
    } else {
        #[allow(clippy::cast_precision_loss)]
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{mb:.0} MB")
    }
}
