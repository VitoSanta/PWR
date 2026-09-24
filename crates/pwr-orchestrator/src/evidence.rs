//! What compaction keeps, as a treatment rather than a constant.
//!
//! R3's H2 design (`docs/r3-h2-evidence-state.md`): at a 16,384-token window,
//! compaction took the history from about 8,500 estimated tokens to about 1,500
//! of an 8,192-token budget, keeping which files were read and none of what they
//! said, and the next turns read the same file again -- 41% of the actions in the
//! B1 runs of the R2 pilot that ran out of budget. Three policies are compared at
//! the same token target: what compaction keeps today, today's plus the most
//! recent exchanges, and today's plus the evidence section built here.
//!
//! The section is composed from the audit and from the files on disk, never
//! from the deployment's own account, so it can be tested without a model and
//! can never deliver bytes a file no longer has.

use pwr_store::EventRecord;
use std::collections::BTreeMap;
use std::path::Path;

/// Which compaction a run uses. `Current` is what every run did before this
/// existed and stays the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum ContextPolicy {
    #[default]
    Current,
    /// Today's compaction, then the most recent whole exchanges it dropped,
    /// newest first, until the history reaches `share_percent` of its budget.
    RecencyFill { share_percent: u8 },
    /// Today's compaction, then the evidence section, up to the same target.
    EvidenceState { share_percent: u8 },
}

impl ContextPolicy {
    /// The token count compaction fills to, or `None` for today's policy.
    pub fn target_tokens(self, history_budget: usize) -> Option<usize> {
        match self {
            Self::Current => None,
            Self::RecencyFill { share_percent } | Self::EvidenceState { share_percent } => {
                Some(history_budget * usize::from(share_percent.min(100)) / 100)
            }
        }
    }

    /// The name recorded with a run and compared between campaigns.
    pub fn label(self) -> String {
        match self {
            Self::Current => "current".into(),
            Self::RecencyFill { share_percent } => format!("recency-fill-{share_percent}"),
            Self::EvidenceState { share_percent } => format!("evidence-state-{share_percent}"),
        }
    }
}

/// Something that happens to a run between two of its actions, from outside it.
///
/// H2's contract injects two things on a declared subset of tasks: a revision of
/// the requirement, and an edit to a file the run has already read. The first is
/// what compaction must not lose; the second is what it must not deliver stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryEvent {
    /// Delivered to the deployment as a message from the person who set the task.
    Revision(String),
    /// Already applied to the workspace by the caller; recorded, not announced.
    ExternalEdit { path: String, detail: String },
}

/// Asked after every action of a B1 run, with the number of actions taken so
/// far: 1 after the first.
pub trait ActionBoundary: Send + Sync {
    fn after_action(&self, step: u8) -> Vec<BoundaryEvent>;
}

/// Tool results in `messages` that carry a file's content under a hash the file
/// on disk no longer has.
///
/// Measured the same way for every policy, because the arms differ exactly here:
/// a restored exchange can carry a read of a file edited since, and an evidence
/// window cannot.
pub fn stale_file_contents(messages: &[pwr_domain::ChatMessage], root: &Path) -> usize {
    fn visit(value: &serde_json::Value, root: &Path, count: &mut usize) {
        match value {
            serde_json::Value::Object(object) => {
                if let (Some(path), Some(hash), Some(_)) = (
                    object.get("path").and_then(serde_json::Value::as_str),
                    object
                        .get("artifact_hash")
                        .and_then(serde_json::Value::as_str),
                    object.get("content"),
                ) && let Ok(bytes) = std::fs::read(root.join(path))
                    && pwr_domain::hash_bytes(&bytes) != hash
                {
                    *count += 1;
                }
                for nested in object.values() {
                    visit(nested, root, count);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    visit(item, root, count);
                }
            }
            _ => {}
        }
    }
    let mut count = 0;
    for message in messages.iter().filter(|message| message.role == "tool") {
        if let Some(value) = crate::tool_result_json(&message.content) {
            visit(&value, root, &mut count);
        }
    }
    count
}

/// Lines of context kept around a line the run used.
const CONTEXT_LINES: usize = 12;
/// Windows closer than this are shown as one.
const MERGE_GAP: usize = 3;

/// The evidence section and what it could not fit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceSection {
    pub text: String,
    pub windows_included: usize,
    pub windows_omitted: usize,
    /// Files whose bytes on disk differ from the version the run last saw. Their
    /// windows are rendered from disk and labelled, so this counts disclosures,
    /// not stale bytes delivered -- by construction there are none.
    pub changed_since_read: usize,
}

/// Why a window is in the section, in the order windows are ranked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reason {
    Edited,
    Diagnostic,
    Read,
}

#[derive(Debug, Default)]
struct FileUse {
    /// 1-based inclusive line ranges.
    ranges: Vec<(usize, usize, Reason)>,
    /// Text the run wrote, located in the file as it is now.
    written: Vec<String>,
    /// Lines a search showed, kept only if the run then read or edited here.
    searched: Vec<usize>,
    /// The hash the run last saw, from a read or an edit.
    last_seen: Option<String>,
    /// Event index of the last time the run touched the file.
    last_touch: usize,
    touched_directly: bool,
    reason: Option<Reason>,
}

fn relative_to<'a>(root: &Path, path: &'a str) -> &'a str {
    let root = root.to_string_lossy();
    path.strip_prefix(root.as_ref())
        .map(|rest| rest.trim_start_matches('/'))
        .or_else(|| {
            // Diagnostics name files under a canonicalised root (`/private/var`
            // for `/var` on macOS); match the workspace by its final component.
            let name = Path::new(root.as_ref()).file_name()?.to_str()?;
            path.split_once(&format!("/{name}/")).map(|(_, rest)| rest)
        })
        .unwrap_or(path)
}

fn promote(file: &mut FileUse, reason: Reason) {
    file.reason = Some(file.reason.map_or(reason, |current| current.min(reason)));
}

/// Builds the section from a run's events and its workspace, within
/// `char_budget` characters.
pub fn evidence_section(
    events: &[EventRecord],
    root: &Path,
    char_budget: usize,
) -> EvidenceSection {
    let mut files: BTreeMap<String, FileUse> = BTreeMap::new();
    let mut revisions: Vec<String> = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let payload = &event.payload;
        match event.event_type.as_str() {
            "task.revision" => {
                if let Some(text) = payload["text"].as_str() {
                    revisions.push(text.to_owned());
                }
            }
            "tool.action" if payload["status"] == "allowed" => {
                let action = &payload["action"];
                let outcome = &payload["outcome"];
                // A search names files in its result, not in its arguments.
                if action["capability"] == "search" {
                    for hit in outcome["files"].as_array().into_iter().flatten() {
                        let Some(hit_path) = hit["path"].as_str() else {
                            continue;
                        };
                        let file = files.entry(hit_path.to_owned()).or_default();
                        for line in hit["lines"].as_array().into_iter().flatten() {
                            if let Some(number) = line["line"].as_u64() {
                                file.searched.push(number as usize);
                            }
                        }
                    }
                    continue;
                }
                let Some(path) = action["path"].as_str() else {
                    continue;
                };
                match action["capability"].as_str().unwrap_or_default() {
                    "read_file" => {
                        let file = files.entry(path.to_owned()).or_default();
                        let first = action["first_line"]
                            .as_u64()
                            .map_or(1, |n| n as usize)
                            .max(1);
                        // Only an explicit window is evidence of what was used. A
                        // read without `max_lines` used to become lines 1 to the
                        // end, which merged every window of the file into one no
                        // budget holds: on the development run of 2026-09-17 a
                        // sqlparse task read `sql.py` whole six times, the
                        // windows 375-475 and 448-482 it went on to read were
                        // swallowed, and all eight compactions under T added
                        // nothing -- the treatment silently became the control.
                        if let Some(count) = action["max_lines"].as_u64() {
                            let last = first + (count as usize).saturating_sub(1);
                            file.ranges.push((first, last, Reason::Read));
                        }
                        if let Some(hash) = outcome["artifact_hash"].as_str() {
                            file.last_seen = Some(hash.to_owned());
                        }
                        file.last_touch = index;
                        file.touched_directly = true;
                        promote(file, Reason::Read);
                    }
                    "replace_text" | "apply_patch" | "apply_replace" | "write_file" => {
                        let file = files.entry(path.to_owned()).or_default();
                        match action["capability"].as_str() {
                            Some("replace_text") => {
                                if let Some(text) = action["replace"].as_str() {
                                    file.written.push(text.to_owned());
                                }
                            }
                            Some("apply_patch") => {
                                for hunk in action["hunks"].as_array().into_iter().flatten() {
                                    if let Some(text) = hunk["replace"].as_str() {
                                        file.written.push(text.to_owned());
                                    }
                                }
                            }
                            _ => file.ranges.push((1, usize::MAX, Reason::Edited)),
                        }
                        if let Some(hash) = outcome["new_hash"].as_str() {
                            file.last_seen = Some(hash.to_owned());
                        }
                        file.last_touch = index;
                        file.touched_directly = true;
                        promote(file, Reason::Edited);
                    }
                    _ => {}
                }
            }
            "verification.diagnostics" if payload["passing"] == false => {
                // Only the latest failure matters; an earlier one was fixed or
                // superseded, and its lines may not mean anything now.
                for file in files.values_mut() {
                    file.ranges.retain(|range| range.2 != Reason::Diagnostic);
                }
                let failing = payload["diagnostics"]["failing_checks"].as_array();
                for check in failing.into_iter().flatten() {
                    for diagnostic in check["diagnostics"].as_array().into_iter().flatten() {
                        let (Some(path), Some(line)) =
                            (diagnostic["path"].as_str(), diagnostic["line"].as_u64())
                        else {
                            continue;
                        };
                        let relative = relative_to(root, path).to_owned();
                        let line = line as usize;
                        let file = files.entry(relative).or_default();
                        file.ranges.push((
                            line.saturating_sub(CONTEXT_LINES).max(1),
                            line + CONTEXT_LINES,
                            Reason::Diagnostic,
                        ));
                        file.last_touch = file.last_touch.max(index);
                        promote(file, Reason::Diagnostic);
                    }
                }
            }
            _ => {}
        }
    }

    struct Window {
        path: String,
        start: usize,
        end: usize,
        reason: Reason,
        last_touch: usize,
        changed: bool,
        lines: Vec<String>,
        hash: String,
    }
    let mut windows = Vec::new();
    let mut changed_since_read = 0;
    for (path, mut file) in files {
        let Some(reason) = file.reason else {
            continue;
        };
        // A search is evidence only where the run went on to look or change.
        if file.touched_directly {
            for line in &file.searched {
                file.ranges.push((
                    line.saturating_sub(CONTEXT_LINES).max(1),
                    line + CONTEXT_LINES,
                    Reason::Read,
                ));
            }
        }
        let Ok(bytes) = std::fs::read(root.join(&path)) else {
            continue;
        };
        let Ok(content) = String::from_utf8(bytes) else {
            continue;
        };
        let hash = pwr_domain::hash_bytes(content.as_bytes());
        let changed = file.last_seen.as_ref().is_some_and(|seen| *seen != hash);
        if changed {
            changed_since_read += 1;
        }
        let lines: Vec<String> = content.lines().map(str::to_owned).collect();
        for written in &file.written {
            let first = written.lines().find(|line| !line.trim().is_empty());
            let Some(first) = first else { continue };
            if let Some(at) = lines.iter().position(|line| line.contains(first.trim())) {
                let span = written.lines().count().max(1);
                file.ranges.push((
                    (at + 1).saturating_sub(CONTEXT_LINES).max(1),
                    at + span + CONTEXT_LINES,
                    Reason::Edited,
                ));
            }
        }
        let total = lines.len();
        let mut ranges: Vec<(usize, usize, Reason)> = file
            .ranges
            .iter()
            .map(|(start, end, why)| ((*start).min(total.max(1)), (*end).min(total), *why))
            .filter(|(start, end, _)| start <= end)
            .collect();
        ranges.sort();
        let mut merged: Vec<(usize, usize, Reason)> = Vec::new();
        for (start, end, why) in ranges {
            match merged.last_mut() {
                Some(last) if start <= last.1 + MERGE_GAP => {
                    last.1 = last.1.max(end);
                    last.2 = last.2.min(why);
                }
                _ => merged.push((start, end, why)),
            }
        }
        for (start, end, why) in merged {
            windows.push(Window {
                path: path.clone(),
                start,
                end,
                reason: why.min(reason),
                last_touch: file.last_touch,
                changed,
                lines: lines[start - 1..end].to_vec(),
                hash: hash.clone(),
            });
        }
    }
    // Edited files first, then what a failing check names, then the most
    // recently read; within a rank, the file touched last comes first.
    windows.sort_by(|a, b| {
        a.reason
            .cmp(&b.reason)
            .then(b.last_touch.cmp(&a.last_touch))
            .then(a.path.cmp(&b.path))
            .then(a.start.cmp(&b.start))
    });

    let header = "Evidence from files this run used, rendered from the workspace as it is now. \
                  Each window is the current text of those lines; a file marked changed has \
                  been modified since you last saw it.\n";
    let mut text = String::from(header);
    // A revision is the objective, and outranks every window: a run that keeps
    // the file and loses what it was asked to do with it has kept the wrong thing.
    if !revisions.is_empty() {
        text.push_str("\nRevisions to the task, newest last; they supersede the original where they differ:\n");
        for revision in &revisions {
            text.push_str(&format!("  - {revision}\n"));
        }
    }
    let mut included = 0;
    let mut omitted = Vec::new();
    for window in &windows {
        let title = format!(
            "\n--- {}:{}-{} (artifact_hash {}){}\n",
            window.path,
            window.start,
            window.end,
            window.hash,
            if window.changed {
                " -- changed since you read it"
            } else {
                ""
            }
        );
        let body = window.lines.join("\n");
        if text.len() + title.len() + body.len() + 1 > char_budget {
            omitted.push(format!("{}:{}-{}", window.path, window.start, window.end));
            continue;
        }
        text.push_str(&title);
        text.push_str(&body);
        text.push('\n');
        included += 1;
    }
    if !omitted.is_empty() {
        let note = format!("\nNot shown for lack of room: {}\n", omitted.join(", "));
        if text.len() + note.len() <= char_budget {
            text.push_str(&note);
        }
    }
    if included == 0 && revisions.is_empty() {
        text.clear();
    }
    EvidenceSection {
        text,
        windows_included: included,
        windows_omitted: omitted.len(),
        changed_since_read,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwr_store::Store;

    fn record(store: &Store, run: pwr_domain::Id, kind: &str, payload: serde_json::Value) {
        store.append(Some(run), kind, payload).unwrap();
    }

    fn numbered(count: usize) -> String {
        (1..=count).map(|n| format!("line {n}\n")).collect()
    }

    /// The loss this exists for: a large file read, then compacted away, then
    /// read again. What the run read comes back as the file's current text, and
    /// a file changed behind the run's back is said to have changed rather than
    /// shown as it was.
    #[test]
    fn windows_are_rendered_from_disk_and_a_changed_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.py"), numbered(400)).unwrap();
        let store = Store::open(":memory:").unwrap();
        let run = pwr_domain::new_id();
        record(
            &store,
            run,
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "read_file", "path": "big.py", "first_line": 200, "max_lines": 10},
                "outcome": {"artifact_hash": pwr_domain::hash_bytes(numbered(400))},
            }),
        );
        let events = store.events_for_run(run).unwrap();

        let section = evidence_section(&events, dir.path(), 10_000);
        assert_eq!(section.windows_included, 1);
        assert!(section.text.contains("big.py:200-209"), "{}", section.text);
        assert!(section.text.contains("line 205"));
        assert!(!section.text.contains("line 150"));
        assert_eq!(section.changed_since_read, 0);
        assert!(!section.text.contains("changed since"));

        // Edited outside the run: the window shows the new bytes and says so.
        let edited = numbered(400).replace("line 205\n", "line 205 EDITED\n");
        std::fs::write(dir.path().join("big.py"), &edited).unwrap();
        let section = evidence_section(&events, dir.path(), 10_000);
        assert!(section.text.contains("line 205 EDITED"), "{}", section.text);
        assert!(section.text.contains("changed since you read it"));
        assert_eq!(section.changed_since_read, 1);
    }

    /// Edited files outrank files only read, a window that does not fit is named
    /// rather than silently dropped, and nothing exceeds the budget.
    #[test]
    fn edits_come_first_and_what_does_not_fit_is_named() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("read.py"), numbered(300)).unwrap();
        let edited = numbered(300).replace("line 150\n", "fixed = True\n");
        std::fs::write(dir.path().join("edit.py"), &edited).unwrap();
        let store = Store::open(":memory:").unwrap();
        let run = pwr_domain::new_id();
        record(
            &store,
            run,
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "read_file", "path": "edit.py"},
                "outcome": {"artifact_hash": "old"},
            }),
        );
        record(
            &store,
            run,
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "replace_text", "path": "edit.py", "find": "line 150", "replace": "fixed = True"},
                "outcome": {"new_hash": pwr_domain::hash_bytes(&edited)},
            }),
        );
        record(
            &store,
            run,
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "read_file", "path": "read.py", "first_line": 1, "max_lines": 40},
                "outcome": {"artifact_hash": pwr_domain::hash_bytes(numbered(300))},
            }),
        );
        let events = store.events_for_run(run).unwrap();

        let roomy = evidence_section(&events, dir.path(), 100_000);
        let edit_at = roomy.text.find("--- edit.py").unwrap();
        let read_at = roomy.text.find("--- read.py").unwrap();
        assert!(edit_at < read_at, "{}", roomy.text);
        assert!(roomy.text.contains("fixed = True"));

        // Room for the edit window only.
        let edit_only = roomy.text[..read_at].len() + 60;
        let tight = evidence_section(&events, dir.path(), edit_only);
        assert!(
            tight.text.len() <= edit_only,
            "{} > {edit_only}",
            tight.text.len()
        );
        assert!(tight.text.contains("fixed = True"));
        assert!(tight.windows_omitted >= 1);
        assert!(
            tight
                .text
                .contains("Not shown for lack of room: read.py:1-40")
                || tight.text.len() + 40 > edit_only
        );
    }

    /// A whole-file read is not a window. Before this, it merged with every
    /// explicit window of the same file into one the size of the file, which
    /// never fit, so a large file the run kept returning to was never shown.
    #[test]
    fn a_whole_file_read_does_not_swallow_the_windows_the_run_used() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sql.py"), numbered(700)).unwrap();
        let hash = pwr_domain::hash_bytes(numbered(700));
        let store = Store::open(":memory:").unwrap();
        let run = pwr_domain::new_id();
        for action in [
            serde_json::json!({"capability": "read_file", "path": "sql.py"}),
            serde_json::json!({"capability": "read_file", "path": "sql.py", "first_line": 380, "max_lines": 20}),
            serde_json::json!({"capability": "read_file", "path": "sql.py"}),
        ] {
            record(
                &store,
                run,
                "tool.action",
                serde_json::json!({"status": "allowed", "action": action, "outcome": {"artifact_hash": hash}}),
            );
        }
        let events = store.events_for_run(run).unwrap();

        let section = evidence_section(&events, dir.path(), 2_000);
        assert_eq!(section.windows_included, 1, "{}", section.text);
        assert_eq!(section.windows_omitted, 0);
        assert!(section.text.contains("sql.py:380-399"), "{}", section.text);
        assert!(!section.text.contains("line 1\n"));
    }

    /// A search hit is evidence only where the run went on to read or edit.
    #[test]
    fn a_search_alone_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.py"), numbered(100)).unwrap();
        let store = Store::open(":memory:").unwrap();
        let run = pwr_domain::new_id();
        record(
            &store,
            run,
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "search", "path": null, "query": "line 50"},
                "outcome": {"files": [{"path": "a.py", "lines": [{"line": 50}]}]},
            }),
        );
        let events = store.events_for_run(run).unwrap();
        assert_eq!(
            evidence_section(&events, dir.path(), 10_000),
            EvidenceSection::default()
        );

        // Once the run reads the file, the lines the search showed are kept too.
        record(
            &store,
            run,
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "read_file", "path": "a.py", "first_line": 1, "max_lines": 5},
                "outcome": {"artifact_hash": pwr_domain::hash_bytes(numbered(100))},
            }),
        );
        let events = store.events_for_run(run).unwrap();
        let section = evidence_section(&events, dir.path(), 10_000);
        assert!(section.text.contains("a.py:38-62"), "{}", section.text);
        assert!(section.text.contains("a.py:1-5"), "{}", section.text);
    }

    #[test]
    fn today_has_no_target_and_the_treatments_share_one() {
        assert_eq!(ContextPolicy::Current.target_tokens(8192), None);
        assert_eq!(
            ContextPolicy::RecencyFill { share_percent: 60 }.target_tokens(8192),
            Some(4915)
        );
        assert_eq!(
            ContextPolicy::EvidenceState { share_percent: 60 }.target_tokens(8192),
            Some(4915)
        );
        assert_eq!(ContextPolicy::default(), ContextPolicy::Current);
    }
}
