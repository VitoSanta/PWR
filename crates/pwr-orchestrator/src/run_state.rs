//! Bounded model-visible results and progress since the last new effect.
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn json_bytes(value: &Value) -> usize {
    serde_json::to_vec(value)
        .expect("JSON value serializes")
        .len()
}

/// Timing and artifact bookkeeping must not turn the same failure into progress.
pub(crate) fn check_signature(failures: &[Value]) -> String {
    let stable: Vec<_> = failures.iter().map(|failure| json!({
        "command":failure["command"], "exit_code":failure["exit_code"],
        "stdout":failure["stdout"], "stderr":failure["stderr"],
        "stdout_truncated":failure["stdout_truncated"], "stderr_truncated":failure["stderr_truncated"],
        "diagnostics":failure["diagnostics"]
    })).collect();
    pwr_domain::hash_bytes(serde_json::to_vec(&stable).expect("JSON serializes"))
}

/// Bound serialized bytes, including escaping and truncation metadata. This is
/// a byte contract, not a claim to know the provider's tokenization.
pub(crate) fn bounded_result(value: Value, limit: usize) -> Value {
    if json_bytes(&value) <= limit {
        return value;
    }
    assert!(limit >= 256, "room for truncation metadata");
    let original_bytes = json_bytes(&value);
    let (mut bounded, text, key) =
        if let Some(content) = value.get("content").and_then(Value::as_str) {
            let mut metadata = value.clone();
            metadata["content"] = json!("");
            (metadata, content.to_string(), "content")
        } else {
            (json!({}), value.to_string(), "preview")
        };
    bounded["truncated"] = json!(true);
    bounded["original_bytes"] = json!(original_bytes);
    bounded["note"] = json!(
        "Result shortened for context. Read a smaller window with first_line/max_lines, or narrow the query. The audit retains the full result."
    );
    bounded[key] = json!("");
    if json_bytes(&bounded) > limit {
        // Oversized metadata is no exception to the bound.
        bounded = json!({"truncated":true,"original_bytes":original_bytes,
            "note":"Result shortened; request a smaller window. Full result remains in the audit.","preview":""});
        return fit_preview(bounded, &value.to_string(), "preview", limit);
    }
    fit_preview(bounded, &text, key, limit)
}

fn fit_preview(mut value: Value, text: &str, key: &str, limit: usize) -> Value {
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let (mut low, mut high) = (0, boundaries.len() - 1);
    while low < high {
        let mid = (low + high).div_ceil(2);
        value[key] = json!(&text[..boundaries[mid]]);
        if json_bytes(&value) <= limit {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    value[key] = json!(&text[..boundaries[low]]);
    value
}

/// A previously visited effect (including edit/revert) does not earn a fresh
/// budget. New content/check outcomes do; successful unchanged reads do not.
#[derive(Debug, Default)]
pub struct ProgressTracker {
    effects: BTreeSet<(String, String)>,
    fingerprints: BTreeSet<String>,
    window: Vec<(String, super::EffectSignature, bool)>,
    pub windows: usize,
    pub historical_windows: usize,
}

/// Scope changes ask a genuinely different question, even when both searches
/// are empty. Keep the typed fields: a query or glob may itself contain ':'.
pub(crate) fn search_scope(action: &crate::ActionProposal) -> Option<String> {
    match action {
        crate::ActionProposal::Search {
            regex,
            path_glob,
            in_dependencies,
            ..
        } => Some(json!([regex, path_glob, in_dependencies]).to_string()),
        _ => None,
    }
}

/// New spelling alone is not new evidence after a completed, explicitly empty
/// search in the same scope. Unknown, truncated or informative results retain
/// their original identity; the audit always retains the proposed action.
pub(crate) fn investigation_key(fingerprint: &str, scope: Option<&str>, result: &Value) -> String {
    let empty = result["matches_returned"].as_u64() == Some(0)
        && result["truncated"].as_bool() == Some(false)
        && result["files"].as_array().is_some_and(Vec::is_empty)
        && result
            .get("files_named")
            .is_none_or(|named| named.is_null() || named.as_array().is_some_and(Vec::is_empty))
        && result.get("note").is_none_or(Value::is_null)
        && result.get("denied").is_none()
        && result.get("error").is_none();
    match scope.filter(|_| empty) {
        Some(scope) => format!("empty_search:{scope}"),
        None => fingerprint.to_owned(),
    }
}

impl ProgressTracker {
    /// The operator spoke: the windows already counted were a verdict on the
    /// request they answered, not on the next one. What has been tried and
    /// what the workspace has been stay remembered, so a repeat is still a
    /// repeat.
    pub fn acknowledge(&mut self) {
        self.windows = 0;
        self.window.clear();
    }

    pub fn observe(
        &mut self,
        fingerprint: String,
        effect: super::EffectSignature,
    ) -> Option<Vec<String>> {
        self.observe_keyed(fingerprint.clone(), fingerprint, effect)
    }

    pub(crate) fn observe_keyed(
        &mut self,
        fingerprint: String,
        information_key: String,
        effect: super::EffectSignature,
    ) -> Option<Vec<String>> {
        if self
            .effects
            .insert((effect.workspace.clone(), effect.checks.clone()))
        {
            self.windows = 0;
            self.window.clear();
        }
        let novel = self.fingerprints.insert(information_key);
        self.window.push((fingerprint, effect, novel));
        if self.window.len() > super::NO_PROGRESS_WINDOW {
            self.window.remove(0);
        }
        if !super::window_made_no_progress(&self.window) {
            return None;
        }
        self.windows += 1;
        self.historical_windows += 1;
        let actions = self.window.drain(..).map(|(name, _, _)| name).collect();
        Some(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(case: &Value, outcome_aware: bool) -> (Option<usize>, usize) {
        let mut progress = ProgressTracker::default();
        let mut changed = std::collections::BTreeMap::new();
        let mut stopped = None;
        for (index, row) in case["actions"].as_array().unwrap().iter().enumerate() {
            // The conversation observes Ok(value), not proposals rejected
            // before execution or errors returned without a tool result.
            if row["observed"] == false {
                continue;
            }
            let action: crate::ActionProposal =
                serde_json::from_value(row["action"].clone()).unwrap();
            let fingerprint = crate::repetition::action_fingerprint(&action);
            crate::stall::record_effect(&mut changed, &row["outcome"]);
            let effect = crate::EffectSignature::of(&changed, "checks unchanged");
            if outcome_aware {
                let scope = search_scope(&action);
                let key = investigation_key(&fingerprint, scope.as_deref(), &row["outcome"]);
                progress.observe_keyed(fingerprint, key, effect);
            } else {
                progress.observe(fingerprint, effect);
            }
            if progress.windows >= crate::stall::NO_PROGRESS_LIMIT {
                stopped = Some(index + 1);
                break;
            }
        }
        (stopped, progress.historical_windows)
    }

    #[test]
    fn recorded_gemma_search_loop_stops_without_stopping_the_five_successes() {
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/gemma-progress-replay.json"))
                .unwrap();
        for case in cases.as_array().unwrap() {
            let (old_stop, old_windows) = replay(case, false);
            assert!(
                old_stop.is_none(),
                "old replay differs from recorded completion"
            );
            assert_eq!(
                old_windows,
                case["recorded_windows"].as_u64().unwrap() as usize,
                "replay must reproduce the recorded guard: {}",
                case["name"]
            );
            let (stop, windows) = replay(case, true);
            eprintln!("{}: stop={stop:?}, windows={windows}", case["name"]);
            if case["successful"] == true {
                assert!(stop.is_none(), "successful run stopped: {}", case["name"]);
            } else {
                assert_eq!(old_windows, 1, "replay must reproduce the recorded guard");
                assert!(stop.is_some_and(|step| step <= 48), "{stop:?}");
            }
        }
    }

    #[test]
    fn only_explicitly_empty_uninformative_searches_share_an_information_key() {
        let empty = json!({"matches_returned":0,"truncated":false,"files":[]});
        assert_eq!(
            investigation_key("search:a", Some("scope"), &empty),
            investigation_key("search:b", Some("scope"), &empty)
        );
        assert_ne!(
            investigation_key("search:a", Some("scope-a"), &empty),
            investigation_key("search:b", Some("scope-b"), &empty)
        );
        assert_eq!(
            investigation_key("read_file:a", None, &empty),
            "read_file:a"
        );
        for informative in [
            json!({}),
            json!({"matches_returned":0,"files":[]}),
            json!({"matches_returned":0,"truncated":true,"files":[]}),
            json!({"matches_returned":0,"truncated":false,"files":[],"files_named":["source.rs"]}),
            json!({"matches_returned":0,"truncated":false,"files":[],"note":"set regex: true"}),
            json!({"matches_returned":0,"truncated":false,"files":[],"denied":"not allowed"}),
            json!({"matches_returned":0,"truncated":false,"files":[{"path":"source.rs"}]}),
            json!({"matches_returned":1,"truncated":false,"files":[]}),
        ] {
            assert_eq!(
                investigation_key("original", Some("scope"), &informative),
                "original",
                "{informative}"
            );
        }
    }
    #[test]
    fn repeated_failures_with_different_timings_are_not_progress() {
        let first = json!({"command":"build", "exit_code":1, "stderr":"bad template", "duration_ms":1, "artifact_hash":"old"});
        let mut second = first.clone();
        second["duration_ms"] = json!(999);
        second["artifact_hash"] = json!("new");
        assert_eq!(
            check_signature(std::slice::from_ref(&first)),
            check_signature(&[second.clone()])
        );
        second["stderr"] = json!("another error");
        assert_ne!(check_signature(&[first]), check_signature(&[second]));
    }
    fn effect(name: &str) -> crate::EffectSignature {
        crate::EffectSignature {
            workspace: name.into(),
            checks: "same".into(),
        }
    }
    #[test]
    fn serialization_bound_preserves_read_hash_and_unicode() {
        let input =
            json!({"content":"\"\\\n💡".repeat(10_000), "expected_hash":"hash", "first_line":1});
        let result = bounded_result(input, 1024);
        assert!(json_bytes(&result) <= 1024);
        assert_eq!(result["expected_hash"], "hash");
        assert_eq!(result["truncated"], true);
        assert!(result["content"].as_str().unwrap().len() > 10);
    }
    #[test]
    fn large_metadata_and_arrays_cannot_bypass_the_bound() {
        for value in [
            json!({"content":"x", "path":"p".repeat(4000)}),
            json!(["x".repeat(10000)]),
        ] {
            let result = bounded_result(value, 256);
            assert!(json_bytes(&result) <= 256);
            assert_eq!(result["truncated"], true);
        }
    }
    #[test]
    fn progress_resets_strikes_but_rereads_and_reverts_do_not() {
        let mut tracker = ProgressTracker::default();
        for _ in 0..7 {
            tracker.observe("read:a".into(), effect("a"));
        }
        assert_eq!(tracker.windows, 1);
        tracker.observe("edit:a".into(), effect("b"));
        assert_eq!(tracker.windows, 0);
        for _ in 0..6 {
            tracker.observe("read:a".into(), effect("b"));
        }
        assert_eq!(tracker.windows, 1);
        tracker.observe("edit:a".into(), effect("a"));
        tracker.observe("edit:a".into(), effect("b"));
        assert_eq!(
            tracker.windows, 1,
            "revisiting content must not forgive strikes"
        );
        for _ in 0..6 {
            tracker.observe("read:a".into(), effect("b"));
        }
        assert_eq!(tracker.windows, 2);
        assert_eq!(tracker.historical_windows, 3);
    }
    /// S03: stopping unproductive cycles must not stop the cycle a hard fix
    /// needs -- change, run the test, still red, change again -- as long as each
    /// change is a new state of the workspace.
    #[test]
    fn fix_red_test_fix_is_progress_but_the_same_two_states_in_a_circle_is_not() {
        let mut tracker = ProgressTracker::default();
        for round in 0..12 {
            // Read the failure, change the file to something new, run the test
            // (red again, same failing check): eight actions' worth per round.
            for action in ["read:test-output", "read:source", "run:test"] {
                tracker.observe(
                    action.into(),
                    crate::EffectSignature {
                        workspace: format!("w{round}"),
                        checks: "red".into(),
                    },
                );
            }
            tracker.observe(
                format!("edit:{round}"),
                crate::EffectSignature {
                    workspace: format!("w{}", round + 1),
                    checks: "red".into(),
                },
            );
            assert_eq!(tracker.windows, 0, "round {round} was progress");
        }
        assert_eq!(tracker.historical_windows, 0);

        // The same cycle, but the edit flips between two contents the run has
        // already been in: nothing new happens, and it is named and ends.
        let mut circle = ProgressTracker::default();
        let mut stopped_at = None;
        for round in 0..30 {
            let state = if round % 2 == 0 { "a" } else { "b" };
            for action in ["read:test-output", "run:test", "edit:flip"] {
                circle.observe(
                    action.into(),
                    crate::EffectSignature {
                        workspace: state.into(),
                        checks: "red".into(),
                    },
                );
            }
            if circle.windows >= crate::stall::NO_PROGRESS_LIMIT {
                stopped_at = Some(round);
                break;
            }
        }
        assert!(stopped_at.is_some_and(|round| round < 12), "{stopped_at:?}");
    }
    #[test]
    fn a_batch_of_new_reads_is_investigation_but_repeating_it_stalls() {
        let mut tracker = ProgressTracker::default();
        for i in 0..6 {
            assert!(tracker.observe(format!("read:{i}"), effect("a")).is_none());
        }
        let mut detected = None;
        for i in 0..6 {
            detected = tracker.observe(format!("read:{i}"), effect("a"));
        }
        assert_eq!(detected.unwrap().len(), 6);
        assert_eq!(tracker.windows, 1);
    }
}
