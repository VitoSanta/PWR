//! Reasoning Effort: effort to budget, clamped by the context, with the
//! answer's room reserved whatever reasoning uses.
use proptest::prelude::*;
use pwr_domain::{
    ANSWER_RESERVE_MIN, BudgetSource, GenerationEnvelope, ReasoningBudgets, ReasoningCapability,
    ReasoningDirective, ReasoningEffort, ReasoningEvidence, ReasoningProfile, plan_finalization,
    plan_reasoning,
};

fn profile(capability: ReasoningCapability) -> ReasoningProfile {
    ReasoningProfile {
        capability,
        evidence: ReasoningEvidence::Template,
        switchable: true,
        disabled_by_profile: false,
        budgets: None,
        finalization_verified: None,
    }
}

fn envelope(context_limit: u32, input_tokens: u32) -> GenerationEnvelope {
    GenerationEnvelope {
        context_limit,
        input_tokens,
        answer_allowance: 16_384,
    }
}

fn budget_of(effort: ReasoningEffort, profile: &ReasoningProfile) -> u32 {
    match plan_reasoning(effort, profile, envelope(262_144, 8_000)).directive {
        ReasoningDirective::Budget { tokens } => tokens,
        other => panic!("expected a budget, got {other:?}"),
    }
}

#[test]
fn low_medium_high_are_genuinely_different_budgets() {
    for finalization in [None, Some(true)] {
        let explicit = ReasoningProfile {
            finalization_verified: finalization,
            ..profile(ReasoningCapability::ExplicitThinkingStream)
        };
        let (low, medium, high) = (
            budget_of(ReasoningEffort::Low, &explicit),
            budget_of(ReasoningEffort::Medium, &explicit),
            budget_of(ReasoningEffort::High, &explicit),
        );
        assert!(low < medium && medium < high, "{low} {medium} {high}");
    }
}

#[test]
fn medium_is_the_default() {
    assert_eq!(ReasoningEffort::default(), ReasoningEffort::Medium);
}

#[test]
fn a_provisional_model_gets_the_conservative_mapping_and_a_calibrated_one_the_wider() {
    let provisional = profile(ReasoningCapability::ExplicitThinkingStream);
    let plan = plan_reasoning(
        ReasoningEffort::Medium,
        &provisional,
        envelope(262_144, 8_000),
    );
    assert_eq!(plan.budget_source, Some(BudgetSource::Conservative));
    assert_eq!(
        plan.effective_tokens,
        Some(ReasoningBudgets::CONSERVATIVE.medium)
    );

    let calibrated = ReasoningProfile {
        finalization_verified: Some(true),
        evidence: ReasoningEvidence::Calibration,
        ..provisional
    };
    let plan = plan_reasoning(
        ReasoningEffort::Medium,
        &calibrated,
        envelope(262_144, 8_000),
    );
    assert_eq!(plan.budget_source, Some(BudgetSource::Calibrated));
    assert_eq!(
        plan.effective_tokens,
        Some(ReasoningBudgets::CALIBRATED.medium)
    );
}

#[test]
fn a_profile_mapping_wins_and_a_disordered_one_is_ignored() {
    let declared = ReasoningProfile {
        budgets: Some(ReasoningBudgets {
            low: 300,
            medium: 900,
            high: 2_700,
        }),
        ..profile(ReasoningCapability::ExplicitThinkingStream)
    };
    let plan = plan_reasoning(ReasoningEffort::High, &declared, envelope(65_536, 1_000));
    assert_eq!(plan.budget_source, Some(BudgetSource::Profile));
    assert_eq!(plan.effective_tokens, Some(2_700));

    let disordered = ReasoningProfile {
        budgets: Some(ReasoningBudgets {
            low: 900,
            medium: 300,
            high: 2_700,
        }),
        ..declared
    };
    let plan = plan_reasoning(ReasoningEffort::High, &disordered, envelope(65_536, 1_000));
    assert_eq!(plan.budget_source, Some(BudgetSource::Conservative));
}

#[test]
fn high_is_clamped_to_what_the_context_can_hold() {
    // 32K window, 20K of prompt: 32768 - 20000 - 1024 (safety) = 11744 of
    // room, of which 4096 is the answer's.
    let explicit = profile(ReasoningCapability::ExplicitThinkingStream);
    let plan = plan_reasoning(ReasoningEffort::High, &explicit, envelope(32_768, 20_000));
    assert_eq!(plan.requested_tokens, Some(8_192));
    assert_eq!(plan.effective_tokens, Some(11_744 - ANSWER_RESERVE_MIN));
    assert!(plan.clamped);
    assert_eq!(plan.max_tokens, 11_744);
    assert_eq!(plan.answer_reserve, ANSWER_RESERVE_MIN);
}

#[test]
fn no_room_for_thinking_switches_it_off_rather_than_eating_the_answer() {
    // 16384 - 11700 - 512 = 4172 of room: the answer's 4096 leaves 76, below
    // the least worth opening a thinking phase for.
    let explicit = profile(ReasoningCapability::ExplicitThinkingStream);
    let plan = plan_reasoning(ReasoningEffort::Low, &explicit, envelope(16_384, 11_700));
    assert_eq!(plan.effective_tokens, Some(0));
    assert_eq!(plan.directive, ReasoningDirective::Off);
    // A template that cannot switch thinking off is given a zero budget,
    // which the engine closes at once.
    let unswitchable = ReasoningProfile {
        switchable: false,
        ..explicit
    };
    let plan = plan_reasoning(
        ReasoningEffort::Low,
        &unswitchable,
        envelope(16_384, 11_700),
    );
    assert_eq!(plan.directive, ReasoningDirective::Budget { tokens: 0 });
}

#[test]
fn models_without_a_controllable_phase_are_not_given_a_budget() {
    for capability in [
        ReasoningCapability::None,
        ReasoningCapability::Unknown,
        ReasoningCapability::ObservableOnly,
    ] {
        for effort in ReasoningEffort::ALL {
            let plan = plan_reasoning(effort, &profile(capability), envelope(65_536, 1_000));
            assert_eq!(plan.directive, ReasoningDirective::TemplateDefault);
            assert_eq!(plan.effective_tokens, None);
            assert!(!profile(capability).effort_applies());
        }
    }
}

#[test]
fn a_template_that_takes_levels_gets_the_level_and_no_invented_budget() {
    let harmony = profile(ReasoningCapability::TemplateControlled);
    assert!(harmony.effort_applies());
    for effort in ReasoningEffort::ALL {
        let plan = plan_reasoning(effort, &harmony, envelope(65_536, 1_000));
        assert_eq!(plan.directive, ReasoningDirective::Level { effort });
        assert_eq!(plan.effective_tokens, None);
    }
}

#[test]
fn a_profile_that_turns_thinking_off_is_respected_at_every_level() {
    let off = ReasoningProfile {
        disabled_by_profile: true,
        ..profile(ReasoningCapability::ExplicitThinkingStream)
    };
    assert!(!off.effort_applies());
    for effort in ReasoningEffort::ALL {
        assert_eq!(
            plan_reasoning(effort, &off, envelope(65_536, 1_000)).directive,
            ReasoningDirective::Off
        );
    }
}

#[test]
fn a_model_whose_forced_close_failed_calibration_gets_no_budget() {
    let unsafe_close = ReasoningProfile {
        finalization_verified: Some(false),
        ..profile(ReasoningCapability::ExplicitThinkingStream)
    };
    assert!(!unsafe_close.effort_applies());
    let plan = plan_reasoning(
        ReasoningEffort::High,
        &unsafe_close,
        envelope(65_536, 1_000),
    );
    assert_eq!(plan.directive, ReasoningDirective::TemplateDefault);
}

#[test]
fn the_finalization_retry_asks_for_no_further_thinking() {
    let explicit = profile(ReasoningCapability::ExplicitThinkingStream);
    let plan = plan_finalization(ReasoningEffort::High, &explicit, envelope(65_536, 1_000));
    assert_eq!(plan.directive, ReasoningDirective::Off);
    let unswitchable = ReasoningProfile {
        switchable: false,
        ..explicit
    };
    let plan = plan_finalization(
        ReasoningEffort::High,
        &unswitchable,
        envelope(65_536, 1_000),
    );
    assert_eq!(plan.directive, ReasoningDirective::Budget { tokens: 0 });
    assert!(plan.answer_reserve >= ANSWER_RESERVE_MIN);
}

proptest! {
    /// Whatever the window, prompt and effort: the budget never exceeds what
    /// was asked for, the generation never exceeds the room, and the answer
    /// keeps its reserve.
    #[test]
    fn the_plan_never_overflows_and_always_reserves_the_answer(
        context_limit in 2_048u32..300_000,
        input_share in 0u32..=100,
        allowance in 1_024u32..40_000,
        effort in prop::sample::select(ReasoningEffort::ALL.to_vec()),
        capability in prop::sample::select(vec![
            ReasoningCapability::ExplicitThinkingStream,
            ReasoningCapability::NativeBudget,
            ReasoningCapability::TemplateControlled,
            ReasoningCapability::Unknown,
        ]),
        calibrated in any::<bool>(),
    ) {
        let input_tokens = context_limit / 100 * input_share;
        let envelope = GenerationEnvelope { context_limit, input_tokens, answer_allowance: allowance };
        let profile = ReasoningProfile {
            finalization_verified: calibrated.then_some(true),
            ..profile(capability)
        };
        let plan = plan_reasoning(effort, &profile, envelope);
        let room = envelope.room().max(1);
        prop_assert!(plan.max_tokens <= room);
        prop_assert!(plan.max_tokens >= 1);
        if let (Some(requested), Some(effective)) = (plan.requested_tokens, plan.effective_tokens) {
            prop_assert!(effective <= requested);
            prop_assert_eq!(plan.clamped, effective < requested);
            prop_assert_eq!(plan.answer_reserve, plan.max_tokens - effective);
            prop_assert!(plan.answer_reserve >= ANSWER_RESERVE_MIN.min(room));
            prop_assert!(u64::from(input_tokens) + u64::from(plan.max_tokens)
                <= u64::from(context_limit));
        }
    }
}
