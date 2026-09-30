//! Window arithmetic lives in runtime; task execution profiles stay here.
use pwr_domain::{EvidenceLabel, ExecutionProfile};
pub use pwr_runtime::window::*;

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
    #[test]
    fn the_profile_takes_the_smaller_of_decided_and_served_and_says_so() {
        let shape =
            ModelShape::from_config(&serde_json::json!({"max_position_embeddings": 262144}));
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
        let shape =
            ModelShape::from_config(&serde_json::json!({"max_position_embeddings": 262144}));
        let strategy = pwr_domain::new_id();
        let decision = decide(&shape, None, None, Some(65_536)).unwrap();
        let one = computed_profile(strategy, &decision, 262_144, "host", "model").unwrap();
        let two = computed_profile(strategy, &decision, 262_144, "host", "model").unwrap();
        let elsewhere = computed_profile(strategy, &decision, 262_144, "other", "model").unwrap();
        assert_eq!(one.id, two.id);
        assert_ne!(one.id, elsewhere.id);
    }
}
