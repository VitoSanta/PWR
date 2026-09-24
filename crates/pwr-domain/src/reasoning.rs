//! Reasoning Effort: how much of a generation a model may spend in an explicit
//! thinking phase before it has to move on to its answer or its next action.
//!
//! It is a budget for one phase of one generation. It is not how many searches,
//! tool calls or retries the agent makes -- the harness governs those
//! separately -- and it promises nothing about the quality of the answer.
//!
//! Models expose reasoning in different ways, so the setting is translated in
//! one place, [`plan_reasoning`], from three inputs: what the person chose,
//! what is known about the model ([`ReasoningProfile`]), and how much room the
//! context has left ([`GenerationEnvelope`]). Everything model-specific that
//! the translation depends on is read from the model's own chat template
//! ([`TemplateReasoning`]) or a declared profile, never from a family name.

use serde::{Deserialize, Serialize};

/// The person's choice. `Medium` is the default.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    #[default]
    Medium,
    High,
}

impl ReasoningEffort {
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Parses exactly the three wire names; anything else is refused rather
    /// than mapped to a default, so a typo never silently becomes `Medium`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|effort| effort.as_str() == name)
    }
}

/// How a deployment's thinking phase can be observed and controlled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningCapability {
    /// Calibration observed no separate reasoning phase.
    None,
    /// The template takes a token budget itself (Seed-OSS reads
    /// `thinking_budget`), and the engine can also close the phase.
    NativeBudget,
    /// Reasoning is written between delimiters PWR's engine tracks, so the
    /// engine counts it with the model's tokenizer and can close it.
    ExplicitThinkingStream,
    /// The template takes an effort level, not a token budget (harmony's
    /// `reasoning_effort`), and the engine cannot close the phase.
    TemplateControlled,
    /// Reasoning arrives on its own channel but the backend offers no way to
    /// bound it per request (llama.cpp's `reasoning_content`).
    ObservableOnly,
    /// Nothing established yet.
    Unknown,
}

/// Where a [`ReasoningProfile`]'s capability came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEvidence {
    /// Read from the model's chat template, without running the model.
    Template,
    /// Observed by Quick Calibration on this machine.
    Calibration,
    /// Declared by a model profile shipped with PWR.
    Profile,
}

/// What a chat template says about reasoning, found by reading it as text.
///
/// Nothing is rendered or executed here. A template that mentions none of
/// these markers can still produce reasoning, which is why the absence of a
/// marker yields [`ReasoningCapability::Unknown`], never `None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateReasoning {
    /// The delimiter pair the template uses for a thinking block, when it is
    /// one the engine can track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiters: Option<(String, String)>,
    /// The template reads `enable_thinking`, so thinking can be switched off.
    pub switchable: bool,
    /// The template reads `thinking_budget`: the model is told its budget.
    pub native_budget: bool,
    /// The template reads `reasoning_effort`: it takes a level, not a budget.
    pub effort_levels: bool,
}

impl TemplateReasoning {
    /// The delimiter pairs PWR's MLX engine tracks, most specific first.
    /// The sidecar carries the same list (`THINK_DELIMITERS`).
    pub const KNOWN_DELIMITERS: [(&'static str, &'static str); 2] =
        [("<seed:think>", "</seed:think>"), ("<think>", "</think>")];

    pub fn of(template: &str) -> Self {
        let delimiters = Self::KNOWN_DELIMITERS
            .into_iter()
            .find(|(open, close)| template.contains(open) && template.contains(close))
            .map(|(open, close)| (open.to_owned(), close.to_owned()));
        Self {
            delimiters,
            switchable: template.contains("enable_thinking"),
            native_budget: template.contains("thinking_budget"),
            effort_levels: template.contains("reasoning_effort"),
        }
    }

    /// The capability this template gives a backend. `engine_closes` is
    /// whether the backend can end a thinking block it is tracking (PWR's
    /// MLX engine can; llama.cpp's server cannot be told to per request).
    pub fn capability(&self, engine_closes: bool) -> ReasoningCapability {
        match (&self.delimiters, engine_closes) {
            (Some(_), true) if self.native_budget => ReasoningCapability::NativeBudget,
            (Some(_), true) => ReasoningCapability::ExplicitThinkingStream,
            (Some(_), false) => ReasoningCapability::ObservableOnly,
            (None, _) if self.effort_levels => ReasoningCapability::TemplateControlled,
            (None, _) => ReasoningCapability::Unknown,
        }
    }
}

/// Maximum thinking tokens for each effort level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReasoningBudgets {
    pub low: u32,
    pub medium: u32,
    pub high: u32,
}

impl ReasoningBudgets {
    /// For a model whose thinking PWR can bound but has not seen finish
    /// under a budget: Provisional models. `medium` is the ~4,000 tokens at
    /// which backlog C.15 proposed a reasoning phase with no action be
    /// stopped, after Qwen3.6 reasoned for 56,059 characters in one turn.
    pub const CONSERVATIVE: Self = Self {
        low: 1_024,
        medium: 4_096,
        high: 8_192,
    };

    /// For a model Quick Calibration saw reason and then answer after a
    /// forced close. `medium` keeps 6,144, the chat default PWR's engine
    /// shipped with from 2026-09-22 (backlog D.E2E-20).
    pub const CALIBRATED: Self = Self {
        low: 2_048,
        medium: 6_144,
        high: 12_288,
    };

    /// Strictly increasing and non-zero; anything else is refused where it is
    /// read, so a profile cannot make `High` smaller than `Low`.
    pub fn valid(self) -> bool {
        self.low > 0 && self.low < self.medium && self.medium < self.high
    }

    pub fn for_effort(self, effort: ReasoningEffort) -> u32 {
        match effort {
            ReasoningEffort::Low => self.low,
            ReasoningEffort::Medium => self.medium,
            ReasoningEffort::High => self.high,
        }
    }
}

/// Which mapping a plan's budget came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetSource {
    /// A profile written for this exact deployment.
    Profile,
    /// [`ReasoningBudgets::CALIBRATED`], after local calibration.
    Calibrated,
    /// [`ReasoningBudgets::CONSERVATIVE`].
    Conservative,
}

/// Everything known about one deployment's reasoning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReasoningProfile {
    pub capability: ReasoningCapability,
    pub evidence: ReasoningEvidence,
    /// Thinking can be switched off (the template reads `enable_thinking`).
    pub switchable: bool,
    /// A declared profile turns thinking off for this deployment.
    pub disabled_by_profile: bool,
    /// A mapping declared for this exact deployment, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budgets: Option<ReasoningBudgets>,
    /// Whether calibration saw an answer follow a forced close. `Some(false)`
    /// means closing the phase is not safe on this model, so no budget is
    /// enforced; `None` means it was not tested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalization_verified: Option<bool>,
}

impl Default for ReasoningProfile {
    fn default() -> Self {
        Self::unknown()
    }
}

impl ReasoningProfile {
    /// Nothing known: no budget is sent and the template decides.
    pub fn unknown() -> Self {
        Self {
            capability: ReasoningCapability::Unknown,
            evidence: ReasoningEvidence::Template,
            switchable: false,
            disabled_by_profile: false,
            budgets: None,
            finalization_verified: None,
        }
    }

    /// Whether PWR can bound the thinking phase in tokens.
    pub fn budget_enforceable(&self) -> bool {
        matches!(
            self.capability,
            ReasoningCapability::NativeBudget | ReasoningCapability::ExplicitThinkingStream
        ) && self.finalization_verified != Some(false)
    }

    /// Whether the Reasoning Effort setting changes anything for this model.
    pub fn effort_applies(&self) -> bool {
        !self.disabled_by_profile
            && (self.budget_enforceable()
                || self.capability == ReasoningCapability::TemplateControlled)
    }

    /// The mapping in force and where it came from.
    pub fn budgets(&self) -> (ReasoningBudgets, BudgetSource) {
        match self.budgets.filter(|budgets| budgets.valid()) {
            Some(declared) => (declared, BudgetSource::Profile),
            None if self.finalization_verified == Some(true) => {
                (ReasoningBudgets::CALIBRATED, BudgetSource::Calibrated)
            }
            None => (ReasoningBudgets::CONSERVATIVE, BudgetSource::Conservative),
        }
    }
}

/// The room one generation has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationEnvelope {
    /// The working window the engine serves.
    pub context_limit: u32,
    /// What the prompt takes, as a conservative estimate or the engine's count.
    pub input_tokens: u32,
    /// The cap on the answer/action phase alone: what a whole reply was
    /// allowed before reasoning had a budget of its own.
    pub answer_allowance: u32,
}

/// The least room an answer or tool call is left, whatever reasoning uses.
/// A tool call that writes a file of a few hundred lines needs about this.
pub const ANSWER_RESERVE_MIN: u32 = 4_096;

/// Below this a thinking budget is not worth opening a thinking phase for.
pub const MIN_USEFUL_REASONING: u32 = 256;

impl GenerationEnvelope {
    /// Kept free on top of everything counted: the input is partly an
    /// estimate, and the template adds tokens of its own.
    pub fn safety_margin(&self) -> u32 {
        (self.context_limit / 32).max(512)
    }

    /// What input and safety margin leave of the window.
    pub fn room(&self) -> u32 {
        self.context_limit
            .saturating_sub(self.input_tokens)
            .saturating_sub(self.safety_margin())
    }
}

/// What the engine is asked to do with the thinking phase.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReasoningDirective {
    /// Nothing is sent; the template's own default applies.
    TemplateDefault,
    /// Thinking is switched off.
    Off,
    /// The engine ends the thinking phase after this many tokens and lets the
    /// answer follow; a template with a native budget is told the same number.
    Budget { tokens: u32 },
    /// The template is given this level; no token budget is enforced.
    Level { effort: ReasoningEffort },
}

/// How one generation's reasoning is bounded, and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReasoningPlan {
    pub effort: ReasoningEffort,
    pub capability: ReasoningCapability,
    pub directive: ReasoningDirective,
    /// The configured budget for the effort, before clamping.
    pub requested_tokens: Option<u32>,
    /// The budget actually sent.
    pub effective_tokens: Option<u32>,
    /// The context could not hold the configured budget.
    pub clamped: bool,
    pub budget_source: Option<BudgetSource>,
    /// The cap on the whole generation: reasoning and answer together.
    pub max_tokens: u32,
    /// Tokens the answer or tool call keeps however much reasoning is used.
    pub answer_reserve: u32,
    /// Why the plan is what it is, in words for a person.
    pub note: String,
}

/// Translates the person's effort into what one generation may spend.
///
/// Invariants, pinned by tests: the effective budget never exceeds the
/// requested one; `max_tokens` never exceeds the room the context has; and
/// whatever reasoning uses, the answer keeps `answer_reserve`, which is at
/// least [`ANSWER_RESERVE_MIN`] whenever the room allows that much.
pub fn plan_reasoning(
    effort: ReasoningEffort,
    profile: &ReasoningProfile,
    envelope: GenerationEnvelope,
) -> ReasoningPlan {
    let room = envelope.room().max(1);
    let answer_only = envelope.answer_allowance.clamp(1, room);
    let plan = |directive, note: &str| ReasoningPlan {
        effort,
        capability: profile.capability,
        directive,
        requested_tokens: None,
        effective_tokens: None,
        clamped: false,
        budget_source: None,
        max_tokens: answer_only,
        answer_reserve: answer_only,
        note: note.to_owned(),
    };
    if profile.disabled_by_profile {
        return plan(
            ReasoningDirective::Off,
            "Thinking is turned off for this model by its profile.",
        );
    }
    match profile.capability {
        ReasoningCapability::TemplateControlled => {
            // The level is the only control; the cap on the whole reply is
            // widened by the conservative budget so a higher level has room,
            // but nothing stops the model spending it on reasoning.
            let headroom = ReasoningBudgets::CONSERVATIVE.for_effort(effort);
            let max_tokens = envelope
                .answer_allowance
                .saturating_add(headroom)
                .clamp(1, room);
            ReasoningPlan {
                max_tokens,
                answer_reserve: max_tokens,
                ..plan(
                    ReasoningDirective::Level { effort },
                    "This model's template takes the level directly; PWR does not enforce a \
                     token budget on it.",
                )
            }
        }
        ReasoningCapability::None => plan(
            ReasoningDirective::TemplateDefault,
            "No separate reasoning phase was observed for this model.",
        ),
        ReasoningCapability::ObservableOnly => plan(
            ReasoningDirective::TemplateDefault,
            "This backend shows the model's reasoning but cannot bound it per request.",
        ),
        ReasoningCapability::Unknown => plan(
            ReasoningDirective::TemplateDefault,
            "This model's reasoning behaviour is not known yet; its template's defaults apply.",
        ),
        ReasoningCapability::NativeBudget | ReasoningCapability::ExplicitThinkingStream
            if profile.finalization_verified == Some(false) =>
        {
            plan(
                ReasoningDirective::TemplateDefault,
                "Calibration found that ending this model's thinking early did not produce an \
                 answer, so no budget is enforced.",
            )
        }
        ReasoningCapability::NativeBudget | ReasoningCapability::ExplicitThinkingStream => {
            let (budgets, source) = profile.budgets();
            let requested = budgets.for_effort(effort);
            let floor = ANSWER_RESERVE_MIN.min(room);
            let mut effective = requested.min(room - floor);
            if effective < MIN_USEFUL_REASONING {
                effective = 0;
            }
            let clamped = effective < requested;
            let max_tokens = effective
                .saturating_add(envelope.answer_allowance.max(floor))
                .clamp(1, room);
            let directive = if effective == 0 && profile.switchable {
                ReasoningDirective::Off
            } else {
                ReasoningDirective::Budget { tokens: effective }
            };
            let note = match (effective, clamped) {
                (0, _) => {
                    "The context has no room for a thinking phase; the model answers directly."
                }
                (_, true) => "The thinking budget was lowered to fit the context left.",
                (_, false) => "The thinking budget for this level is in force.",
            };
            ReasoningPlan {
                effort,
                capability: profile.capability,
                directive,
                requested_tokens: Some(requested),
                effective_tokens: Some(effective),
                clamped,
                budget_source: Some(source),
                max_tokens,
                answer_reserve: max_tokens - effective,
                note: note.to_owned(),
            }
        }
    }
}

/// The plan for a retry after a generation reasoned to its budget and still
/// produced no answer: no further thinking where it can be prevented.
pub fn plan_finalization(
    effort: ReasoningEffort,
    profile: &ReasoningProfile,
    envelope: GenerationEnvelope,
) -> ReasoningPlan {
    let room = envelope.room().max(1);
    let answer = envelope
        .answer_allowance
        .max(ANSWER_RESERVE_MIN)
        .clamp(1, room);
    let directive = if profile.switchable {
        ReasoningDirective::Off
    } else if profile.budget_enforceable() {
        ReasoningDirective::Budget { tokens: 0 }
    } else {
        ReasoningDirective::TemplateDefault
    };
    ReasoningPlan {
        effort,
        capability: profile.capability,
        directive,
        requested_tokens: None,
        effective_tokens: (directive != ReasoningDirective::TemplateDefault).then_some(0),
        clamped: true,
        budget_source: None,
        max_tokens: answer,
        answer_reserve: answer,
        note: "The previous attempt reasoned to its budget without an answer; this one is asked \
               to answer directly."
            .into(),
    }
}

/// Where a generation's token counts come from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenAccounting {
    /// Counted by PWR's engine with the loaded model's own tokenizer:
    /// exact.
    EngineTokenizer,
    /// Reported by the backend's usage block: exact for the backend's
    /// tokenizer.
    ServerUsage,
    /// Characters divided by four: an estimate, and it understates code.
    Estimated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_names_parse_exactly() {
        assert_eq!(ReasoningEffort::parse("high"), Some(ReasoningEffort::High));
        assert_eq!(ReasoningEffort::parse("High"), None);
        assert_eq!(ReasoningEffort::parse("xhigh"), None);
        assert_eq!(ReasoningEffort::default(), ReasoningEffort::Medium);
    }

    #[test]
    fn templates_are_classified_by_their_markers() {
        let qwen = TemplateReasoning::of(
            "{%- if enable_thinking is defined and enable_thinking is false %}<think>\n\n</think>",
        );
        assert_eq!(qwen.delimiters, Some(("<think>".into(), "</think>".into())));
        assert!(qwen.switchable);
        assert_eq!(
            qwen.capability(true),
            ReasoningCapability::ExplicitThinkingStream
        );
        assert_eq!(qwen.capability(false), ReasoningCapability::ObservableOnly);

        let seed = TemplateReasoning::of("{{ thinking_budget }}<seed:think></seed:think>");
        assert_eq!(seed.capability(true), ReasoningCapability::NativeBudget);

        let harmony = TemplateReasoning::of("Reasoning: {{ reasoning_effort }}<|channel|>analysis");
        assert_eq!(
            harmony.capability(true),
            ReasoningCapability::TemplateControlled
        );

        let plain = TemplateReasoning::of("{% for m in messages %}{{ m.content }}{% endfor %}");
        assert_eq!(plain.capability(true), ReasoningCapability::Unknown);
    }

    #[test]
    fn builtin_mappings_are_ordered() {
        for budgets in [ReasoningBudgets::CONSERVATIVE, ReasoningBudgets::CALIBRATED] {
            assert!(budgets.valid());
        }
        assert!(
            !ReasoningBudgets {
                low: 10,
                medium: 10,
                high: 20
            }
            .valid()
        );
    }
}
