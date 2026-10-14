//! Session listing and inspection, through the binary a user actually runs.
//!
//! A session resumed onto a different branch from the one it was opened on is
//! the case worth seeing before resuming rather than after, so the two states
//! are reported side by side rather than merged into one "current" answer.

thread_local! {
    /// One id per test thread, so a test can ask for the report of the run its
    /// own workspace recorded.
    static RUN_ID: pwr_domain::Id = pwr_domain::new_id();
}

/// A field that no production path reads is a defect, and it is the defect
/// this whole audit was about: `ReasoningControl::Think` serialised into a
/// profile and never reached Ollama, `RuntimeSnapshot.loaded_models` was built
/// empty, `concurrency` was a number nobody enforced.
///
/// The instances are fixed. This is the guard against the class: every value a
/// declared profile carries has to arrive somewhere a run can act on it, and
/// the fixture fails when one stops arriving.
mod declared_values_reach_the_request {
    use pwr_domain::{ModelProfile, ReasoningControl};

    fn declared() -> Vec<ModelProfile> {
        #[derive(serde::Deserialize)]
        struct File {
            profiles: Vec<ModelProfile>,
        }
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../strategies/models.json"
        ))
        .expect("strategies/models.json");
        serde_json::from_slice::<File>(&bytes).unwrap().profiles
    }

    #[test]
    fn every_declared_sampling_option_is_sent() {
        for profile in declared() {
            let sent = profile.sampling_options();
            for name in profile.sampling.keys() {
                assert!(
                    sent.contains_key(name),
                    "{}: declares {name} and does not send it",
                    profile.model_selector
                );
            }
        }
    }

    /// The instance that made this necessary. `Think { enabled: true }` was
    /// declared for one deployment, serialised, validated, and dropped: neither
    /// the request nor the adapter carried a `think` field, so the profile
    /// described a mode the backend was never told about.
    #[test]
    fn a_declared_reasoning_mode_reaches_a_channel() {
        for profile in declared() {
            let Some(reasoning) = &profile.reasoning else {
                continue;
            };
            // Three channels, and a declared mode has to arrive on one of
            // them: the backend's own toggle, a backend option, or a line the
            // system prompt carries. Asserted against the resolved policy
            // rather than against the source text, because grepping for an
            // identifier passes again the moment the identifier moves, and
            // an assertion that holds for every possible value asserts
            // nothing -- the first draft of this test said
            // `*enabled || !*enabled`, which is the defect it was written to
            // guard against.
            let resolved = pwr_orchestrator::TaskProfile::resolve(None, Some(&profile));
            match reasoning {
                ReasoningControl::Think { enabled } => assert_eq!(
                    resolved.sampling.get("think"),
                    Some(&serde_json::json!(enabled)),
                    "{}: declares Think and nothing builds the request's think field",
                    profile.model_selector
                ),
                ReasoningControl::BackendOption { name, value } => assert_eq!(
                    resolved.sampling.get(name.as_str()),
                    Some(&serde_json::json!(value)),
                    "{}: declares a backend option that nothing sends",
                    profile.model_selector
                ),
                ReasoningControl::PromptDirective { text } => assert!(
                    !text.is_empty() && resolved.prompt_suffix.contains(text.as_str()),
                    "{}: declares a prompt directive that reaches no prompt",
                    profile.model_selector
                ),
            }
        }
    }
}
