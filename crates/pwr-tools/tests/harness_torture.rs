//! What the harness does to the model, rather than what the model does.
//!
//! A benchmark that only scores the final code cannot tell a deployment that
//! could not reason from one that was handed the wrong 64 kilobytes. These
//! measure the second, and they need no model to run: they are properties of
//! the harness, and a property can be checked.

use pwr_tools::{SandboxPolicy, ToolPolicy, run_command};
use std::time::Duration;

fn policy(root: &std::path::Path, output_limit: usize) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["sh".into()],
        output_limit,
        timeout: Duration::from_secs(30),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    }
}

/// A test run that prints twenty thousand lines and fails on the last one.
///
/// Every test runner puts its verdict at the end: `cargo test` ends with
/// `test result: FAILED`, `pytest` with its short summary, `go test` with
/// `FAIL`. A bound that keeps the first N bytes therefore keeps twenty
/// thousand lines of passing noise and discards the only line that says what
/// went wrong -- and the deployment is then asked to diagnose a failure it was
/// never shown.
#[test]
fn a_huge_test_output_keeps_the_verdict_at_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy(dir.path(), 8 * 1024);
    let script = "for i in $(seq 1 20000); do echo \"test case_$i ... ok\"; done; \
                  echo 'test case_20001 ... FAILED'; \
                  echo 'failures:'; echo '    case_20001'; \
                  echo 'test result: FAILED. 20000 passed; 1 failed'";

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_command(&policy, "sh", &["-c".into(), script.into()]))
        .expect("the command should run");

    assert!(result.stdout_truncated, "the fixture is not large enough");
    assert!(
        result.stdout.contains("test result: FAILED"),
        "the verdict was dropped; the deployment is asked to diagnose a \
         failure it was never shown.\n--- kept {} bytes, ending: ---\n{}",
        result.stdout.len(),
        &result.stdout[result.stdout.len().saturating_sub(300)..]
    );
    assert!(
        result.stdout.contains("case_20001"),
        "the failing case's name was dropped"
    );
}

/// And the head is kept too: the first error a compiler prints is the one that
/// caused the rest, so a bound that kept only the tail would be the same defect
/// mirrored.
#[test]
fn a_huge_output_keeps_its_beginning_as_well() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy(dir.path(), 8 * 1024);
    let script = "echo 'error[E0308]: mismatched types at src/lib.rs:1'; \
                  for i in $(seq 1 20000); do echo \"note: consequence $i\"; done; \
                  echo 'error: could not compile'";

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_command(&policy, "sh", &["-c".into(), script.into()]))
        .expect("the command should run");

    assert!(result.stdout_truncated);
    assert!(
        result.stdout.contains("E0308") && result.stdout.contains("src/lib.rs:1"),
        "the first error was dropped"
    );
    assert!(
        result.stdout.contains("could not compile"),
        "the verdict was dropped"
    );
}

/// Whatever is dropped is said to have been dropped, and by how much. A gap
/// that reads as continuous output is worse than no output: the deployment
/// draws conclusions from lines that were never adjacent.
#[test]
fn what_was_dropped_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy(dir.path(), 4 * 1024);
    let script = "for i in $(seq 1 20000); do echo \"line $i\"; done";

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_command(&policy, "sh", &["-c".into(), script.into()]))
        .expect("the command should run");

    assert!(result.stdout_truncated);
    assert!(
        result.stdout.contains("omitted"),
        "the elision is invisible in the text the deployment reads:\n{}",
        &result.stdout[..result.stdout.len().min(200)]
    );
    // Still bounded, and by the limit itself rather than by the limit plus
    // whatever the fix costs. The elision marker is paid for out of the
    // budget: a bound that can be exceeded is not a bound.
    assert!(
        result.stdout.len() <= 4 * 1024,
        "the bound stopped bounding: {} bytes against a 4096 limit",
        result.stdout.len()
    );
}

/// A limit too small to hold the marker keeps the head and says nothing in
/// band. `truncated` still reports the loss, so the caller is not misled --
/// only the in-band note is absent, because printing it would itself break the
/// bound. A degenerate limit gets degenerate behaviour rather than a special
/// case that quietly grows it.
#[test]
fn a_bound_too_small_for_the_marker_stays_a_bound() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy(dir.path(), 16);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_command(
            &policy,
            "sh",
            &["-c".into(), "echo aaaaaaaaaaaaaaaaaaaaaaaa".into()],
        ))
        .expect("the command should run");

    assert!(result.stdout.len() <= 16, "{} bytes", result.stdout.len());
    assert!(result.stdout_truncated, "the loss went unreported");
    assert!(!result.stdout.contains("omitted"));
}

/// H02 — a word that appears five thousand times.
///
/// The bound is not the question: `search` stops at `max_matches` and at the
/// output limit, so nothing is dumped. The question is whether the deployment
/// is told it is looking at a slice. A list of two hundred matches that does not
/// say five thousand exist reads as the whole answer, and a run that concludes
/// "the symbol is used in these places" from it has been misled by the harness
/// rather than by the model.
#[test]
fn a_search_that_hit_its_cap_says_so() {
    let dir = tempfile::tempdir().unwrap();
    for file in 0..50 {
        let body: String = (0..100)
            .map(|line| format!("let x = tenant_id + {file}_{line};\n"))
            .collect();
        std::fs::write(dir.path().join(format!("file_{file:02}.rs")), body).unwrap();
    }
    let policy = policy(dir.path(), 64 * 1024);

    let found = pwr_tools::search(&policy, "tenant_id", 200).expect("search should run");
    assert_eq!(found.matches_returned, 200, "the cap did not apply");
    assert!(
        found.truncated,
        "five thousand matches came back as two hundred and the result does not \
         say it is a slice; the deployment reads a partial list as complete"
    );
}

/// H02b — and the same two hundred lines are spent across the repository.
///
/// The other half of the question above, and the one the flat list answered
/// badly. Two hundred matches drawn in file order all came from the first two
/// files of fifty: a caller reading that learned where the term is dense and
/// nothing at all about the other forty-eight files containing it. With a
/// per-file cap the same two hundred lines cover twenty-five files, and each
/// one says how many more it holds.
#[test]
fn a_search_spends_its_budget_across_files_rather_than_inside_one() {
    let dir = tempfile::tempdir().unwrap();
    for file in 0..50 {
        let body: String = (0..100)
            .map(|line| format!("let x = tenant_id + {file}_{line};\n"))
            .collect();
        std::fs::write(dir.path().join(format!("file_{file:02}.rs")), body).unwrap();
    }
    let policy = policy(dir.path(), 64 * 1024);

    let found = pwr_tools::search(&policy, "tenant_id", 200).expect("search should run");
    assert_eq!(found.matches_returned, 200);
    assert_eq!(
        found.files.len(),
        25,
        "two hundred lines came from {} file(s); the per-file cap did not spread them",
        found.files.len()
    );
    for file in &found.files {
        assert_eq!(file.lines.len(), 8, "{} was not capped", file.path);
        assert_eq!(
            file.more, 92,
            "{} does not say how much of it was left out",
            file.path
        );
    }
}

/// And a search that saw everything says that too, or `truncated` is a flag
/// nobody can act on.
#[test]
fn a_complete_search_is_not_reported_as_a_slice() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("one.rs"), "let tenant_id = 1;\n").unwrap();
    let policy = policy(dir.path(), 64 * 1024);

    let found = pwr_tools::search(&policy, "tenant_id", 200).expect("search should run");
    assert_eq!(found.matches_returned, 1);
    assert_eq!(found.files.len(), 1);
    assert_eq!(found.files[0].more, 0);
    assert!(!found.truncated);
}

/// H03 — an eight-thousand-line file.
///
/// A read with no window must come back bounded, and must say how long the file
/// actually is, or the deployment cannot know to ask for the rest.
#[test]
fn a_huge_file_comes_back_bounded_and_says_how_long_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let body: String = (1..=8000).map(|n| format!("// line {n}\n")).collect();
    std::fs::write(dir.path().join("big.rs"), &body).unwrap();
    let policy = policy(dir.path(), 4 * 1024);

    let read =
        pwr_tools::read_file(&policy, std::path::Path::new("big.rs")).expect("read should work");
    assert!(read.truncated, "an 8000-line file came back whole");
    assert_eq!(
        read.total_lines, 8000,
        "the deployment cannot know to ask for more"
    );
    assert!(read.content.len() <= 4 * 1024);

    // And the window it asks for is the window it gets.
    let window = pwr_tools::read_file_window(
        &policy,
        std::path::Path::new("big.rs"),
        Some(7900),
        Some(20),
    )
    .expect("windowed read");
    assert_eq!(window.first_line, 7900);
    assert!(window.content.contains("// line 7900"));
    assert!(window.content.contains("// line 7919"));
    assert!(!window.content.contains("// line 7920"));
}
