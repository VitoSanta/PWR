//! Session listing and inspection, through the binary a user actually runs.
//!
//! A session resumed onto a different branch from the one it was opened on is
//! the case worth seeing before resuming rather than after, so the two states
//! are reported side by side rather than merged into one "current" answer.

use std::process::Command;

fn pwr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pwr"))
}

thread_local! {
    /// One id per test thread, so a test can ask for the report of the run its
    /// own workspace recorded.
    static RUN_ID: pwr_domain::Id = pwr_domain::new_id();
}

fn git(root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed");
}

/// A workspace holding one session whose only run edited a file on `main`.
fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "t@e"]);
    git(path, &["config", "user.name", "T"]);
    std::fs::write(path.join("code.py"), "x = 1\n").unwrap();
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "base"]);

    std::fs::create_dir_all(path.join(".poorai")).unwrap();
    let store = pwr_store::Store::open(path.join(".poorai/state.sqlite")).unwrap();
    let run = RUN_ID.with(|id| *id);
    store
        .append(
            Some(run),
            "session.opened",
            serde_json::json!({
                "name": "refactor",
                "root": path.display().to_string(),
                "continues_runs": 0,
                "version_control": {"branch": "main", "head": "abc123", "uncommitted_files": 0},
            }),
        )
        .unwrap();
    store
        .append(
            Some(run),
            "run.started",
            serde_json::json!({"task": "rename the helper"}),
        )
        .unwrap();
    store
        .append(
            Some(run),
            "tool.action",
            serde_json::json!({
                "status": "allowed",
                "action": {"capability": "replace_text", "path": "code.py"},
                "outcome": {"new_hash": pwr_domain::hash_bytes("x = 1\n")},
            }),
        )
        .unwrap();
    root
}

fn run_json(root: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = pwr()
        .args(args)
        .current_dir(root)
        .output()
        .expect("pwr");
    serde_json::from_slice(&output.stdout).expect("json on stdout")
}

#[test]
fn the_listing_names_the_session_and_what_it_was_last_asked() {
    let root = workspace();
    let value = run_json(root.path(), &["--json", "session", "list"]);
    let sessions = value["result"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["name"], "refactor");
    assert_eq!(sessions[0]["runs"], 1);
    // Enough to choose between sessions without opening each one.
    assert_eq!(sessions[0]["last_task"], "rename the helper");
}

#[test]
fn showing_a_session_reports_where_it_opened_and_where_the_workspace_is_now() {
    let root = workspace();
    // The user moves to another branch before resuming.
    git(root.path(), &["checkout", "-q", "-b", "experiment"]);
    let value = run_json(root.path(), &["--json", "session", "show", "refactor"]);
    let result = &value["result"];
    assert_eq!(result["opened_on"]["branch"], "main");
    assert_eq!(
        result["workspace_now"]["branch"], "experiment",
        "the branch the workspace is on now was not reported"
    );
    assert!(result["ledger"].as_str().unwrap().contains("code.py"));
}

#[test]
fn an_unknown_session_is_refused_rather_than_answered_empty() {
    let root = workspace();
    let value = run_json(root.path(), &["--json", "session", "show", "no-such"]);
    assert_eq!(value["ok"], false);
    assert!(
        value["error"]["context"]
            .as_str()
            .unwrap()
            .contains("no session named")
    );
}

/// A workspace that is not a git checkout has no branch, and saying so is
/// honest where inventing `main` is not.
#[test]
fn a_workspace_outside_version_control_reports_no_branch() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".poorai")).unwrap();
    let store = pwr_store::Store::open(root.path().join(".poorai/state.sqlite")).unwrap();
    let run = pwr_domain::new_id();
    store
        .append(
            Some(run),
            "session.opened",
            serde_json::json!({"name": "plain", "root": "/w", "version_control": {}}),
        )
        .unwrap();
    store
        .append(Some(run), "run.started", serde_json::json!({"task": "t"}))
        .unwrap();
    let value = run_json(root.path(), &["--json", "session", "show", "plain"]);
    assert!(value["result"]["workspace_now"].get("branch").is_none());
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

    /// A declared context is a fact about the tag, and must never be what the
    /// request carries -- that substitution is how a profile calibrated at
    /// 32768 authorised a quarter-million-token request.
    #[test]
    fn a_declared_context_is_not_what_the_request_uses() {
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).unwrap();
        assert!(
            !source.contains("context_for("),
            "the request builder is reading a declared context again"
        );
        assert!(
            source.contains("context_tokens: execution.context_tokens"),
            "the request no longer carries the resolved execution context"
        );
    }
}

/// The sequence in a run report is the part a person actually reads, and it
/// was rendering nothing. Every action line said the literal word "capability"
/// -- the first key of the action object rather than its value -- and every
/// transition said "Null → Null", because `from`/`to` were never fields of a
/// checkpoint. Twenty-seven lines, ten of them empty of content.
#[test]
fn a_run_report_names_the_action_and_the_state_it_moved_to() {
    let root = workspace();
    let run = RUN_ID.with(|id| id.to_string());
    let store = pwr_store::Store::open(root.path().join(".poorai/state.sqlite")).unwrap();
    store
        .append(
            Some(run.parse().unwrap()),
            "task.transition",
            serde_json::json!({
                "id": pwr_domain::new_id(),
                "state": "Act",
                "detail": "action loop entered",
                "at": pwr_domain::now(),
            }),
        )
        .unwrap();
    drop(store);

    let answer = run_json(root.path(), &["--json", "report", &run, "--format", "md"]);
    let markdown = answer["result"]["markdown"]
        .as_str()
        .unwrap_or_else(|| panic!("no markdown in {answer}"));
    let sequence = markdown
        .split("## Sequence")
        .nth(1)
        .expect("the report has no sequence");
    assert!(
        sequence.contains("replace_text — allowed"),
        "the action line does not name the capability: {sequence}"
    );
    assert!(
        sequence.contains("→ Act: action loop entered"),
        "the transition line does not name the state it moved to: {sequence}"
    );
    assert!(
        !sequence.contains("Null → Null"),
        "transitions are still rendered from fields a checkpoint does not have"
    );
}
