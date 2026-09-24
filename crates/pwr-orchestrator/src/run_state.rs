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
        if self
            .effects
            .insert((effect.workspace.clone(), effect.checks.clone()))
        {
            self.windows = 0;
            self.window.clear();
        }
        let novel = self.fingerprints.insert(fingerprint.clone());
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
