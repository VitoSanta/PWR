//! H2's mechanism check, read from events, and injections that do not move the
//! revision of any corpus written before they existed.

use pwr_eval::{Injection, InjectionKind, MechanismMetrics, Suite};
use std::path::Path;

#[test]
fn metrics_fold_rereads_compactions_and_injections() {
    let events = [
        (
            "tool.action",
            serde_json::json!({"outcome": {"artifact_hash": "a"}}),
        ),
        (
            "tool.action",
            serde_json::json!({"outcome": {"already_read": {"at_step": 0}}}),
        ),
        ("task.revision", serde_json::json!({"text": "twenty"})),
        (
            "context.compacted",
            serde_json::json!({"added": {"evidence_windows": 3, "files_changed_since_read": 1}, "stale_file_contents_kept": 0}),
        ),
        (
            "tool.action",
            serde_json::json!({"outcome": {"already_read": {"at_step": 0}}}),
        ),
        (
            "injection.external_edit",
            serde_json::json!({"path": "a.py"}),
        ),
        (
            "context.compacted",
            serde_json::json!({"added": {}, "stale_file_contents_kept": 2}),
        ),
    ];
    let metrics =
        MechanismMetrics::from_events(events.iter().map(|(kind, payload)| (*kind, payload)));
    assert_eq!(
        metrics,
        MechanismMetrics {
            rereads_unchanged: 2,
            rereads_after_compaction: 1,
            compactions: 2,
            evidence_windows: 3,
            files_changed_since_read_disclosed: 1,
            stale_file_contents_kept: 2,
            revisions_delivered: 1,
            external_edits_applied: 1,
        }
    );
}

/// A corpus revision is a hash of its tasks' serialization. Every task written
/// before injections existed has none, and must hash as it did, or no campaign
/// already run would pair with one run after. The long-horizon corpus has both
/// kinds, so it checks both.
#[test]
fn a_task_without_injections_keeps_its_corpus_revision() {
    let path = Path::new("../../corpus/longhorizon-v1.json");
    let suite = Suite::load(path).unwrap();
    let (plain, injected): (Vec<_>, Vec<_>) = suite
        .tasks
        .iter()
        .partition(|task| task.injections.is_empty());
    assert!(!plain.is_empty() && !injected.is_empty());
    for task in &plain {
        let written = serde_json::to_value(task).unwrap();
        assert!(
            written.get("injections").is_none(),
            "{}: an empty list was serialized",
            task.id
        );
    }
    for task in &injected {
        let written = serde_json::to_value(task).unwrap();
        let read: pwr_eval::Task = serde_json::from_value(written).unwrap();
        assert_eq!(read.injections, task.injections, "{}", task.id);
        assert!(
            task.id.ends_with("-revised") || task.id.ends_with("-edited"),
            "{}",
            task.id
        );
    }

    let mut task = plain[0].clone();
    task.injections.push(Injection {
        after_action: 4,
        kind: InjectionKind::ExternalEdit {
            path: "pycparser/c_parser.py".into(),
            find: "a".into(),
            replace: "b".into(),
        },
    });
    let written = serde_json::to_value(&task).unwrap();
    assert_eq!(written["injections"][0]["kind"], "external_edit");
    assert_eq!(written["injections"][0]["after_action"], 4);
}
