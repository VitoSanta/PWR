//! Telling a broken environment from broken code.
//!
//! The distinction decides what a deployment is told next. Called `Compilation`,
//! a failure becomes "your edit broke the build, edit and retry"; called
//! `Environment`, it becomes "this could not be run here". Getting it backwards
//! spends a run's budget asking a deployment to fix code that was already
//! correct, and the run is then recorded as the deployment failing the task.
//!
//! The first fixture below carries bytes a real run produced, not bytes chosen
//! to make a point.

use pwr_tools::ToolResult;
use pwr_verify::{FailureClass, classify};

fn failed(stderr: &str, stdout: &str, failing_files: Option<&str>) -> ToolResult {
    ToolResult {
        exit_code: Some(2),
        stdout: stdout.to_owned(),
        stderr: stderr.to_owned(),
        duration_ms: 1,
        redacted: false,
        artifact_hash: "hash".into(),
        stdout_truncated: false,
        stderr_truncated: false,
        sandboxed: true,
        failing_files: failing_files.map(str::to_owned),
    }
}

/// Observed on 2026-09-12: more-itertools at `247e15b3`, `qwen/qwen3.6-35b-a3b`
/// on LM Studio, task `external-numeric-range-reversed`. The deployment made the
/// correct one-line fix on its tenth action and declared completion on its
/// fourteenth. Check discovery had chosen `make requirements check`, whose first
/// target pip-installs and so needs a network the sandbox denies, and this is
/// what it printed.
///
/// It was classified `Compilation`, because `make: error:` contains `error:` and
/// that branch came first, so the environment branch -- which looks for exactly
/// these words -- was unreachable. The run then spent nineteen further actions
/// being told to fix code that was already right, and ended `action budget of 26
/// exhausted before verified completion`.
#[test]
fn a_sandbox_denial_and_a_missing_interpreter_are_not_a_compiler_error() {
    let observed = "make: error: couldn't create cache file \
                    '/var/folders/ts/ksw5hn3s28g6my_f229fy2wc0000gn/T/xcrun_db-pXU4cesb' \
                    (errno=Operation not permitted)\n\
                    make: python: No such file or directory\n\
                    make: *** [requirements] Error 1\n";
    let result = failed(
        observed,
        "python -m pip install --upgrade -r requirements.txt\n",
        None,
    );
    assert_eq!(classify(&result), FailureClass::Environment);
}

/// Observed on 2026-09-14 in the R2 pilot, `which-implementation-runs` on both
/// deployments: `cargo test --quiet --test contract` at the red baseline the
/// task starts from. xcrun could not write its cache under the sandbox and said
/// so inside two warnings; cargo then ran the test and it failed on its
/// assertion, exactly as the task intends.
///
/// Classified `Environment`, the baseline check was exempted as unrunnable, the
/// deployment's correct change completed as "every discovered check failed to
/// run in this environment", and the evaluator recorded no completion. The same
/// happened on every red-baseline Rust task in the pilot.
#[test]
fn a_denial_inside_a_warning_is_not_what_the_check_failed_on() {
    let stderr = "warning: output of `xcrun` while finding MacOSX.sdk\n  |\n  = note: xcrun: error: couldn't create cache file '/var/folders/ts/ksw5hn3s28g6my_f229fy2wc0000gn/T/xcrun_db-gXxSLu6k' (errno=Operation not permitted)\n          xcrun: error: couldn't create cache file '/var/folders/ts/ksw5hn3s28g6my_f229fy2wc0000gn/T/xcrun_db-Q6dB0BIn' (errno=Operation not permitted)\n\nwarning: linker stderr: cc: error: couldn't create cache file '/var/folders/ts/ksw5hn3s28g6my_f229fy2wc0000gn/T/xcrun_db-hKSzxHRJ' (errno=Operation not permitted)\n         cc: error: couldn't create cache file '/var/folders/ts/ksw5hn3s28g6my_f229fy2wc0000gn/T/xcrun_db-ukhVUgUe' (errno=Operation not permitted)\n  |\n  = note: `#[warn(linker_messages)]` on by default\n\nerror: test failed, to rerun pass `--test contract`\n";
    let stdout = "\nrunning 1 test\na_trailing_empty_field_is_a_field --- FAILED\n\nfailures:\n\n---- a_trailing_empty_field_is_a_field stdout ----\n\nthread 'a_trailing_empty_field_is_a_field' (6159873) panicked at tests/contract.rs:3:5:\nassertion `left == right` failed\n  left: [[\"a\", \"b\"]]\n right: [[\"a\", \"b\", \"\"]]\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\nfailures:\n    a_trailing_empty_field_is_a_field\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n";
    let mut result = failed(stderr, stdout, Some("tests/contract.rs (1)"));
    result.exit_code = Some(101);
    let class = classify(&result);
    // What matters is that the check stays required: either class that
    // authorises an edit keeps it so. Which of the two a cargo test failure is
    // called is a separate question this fixture does not settle.
    assert!(
        matches!(class, FailureClass::Assertion | FailureClass::Compilation),
        "a test that ran and failed was called {class:?}"
    );

    // The same denial outside a warning is still the environment.
    let unwarned = stderr.replace("warning: ", "");
    assert_eq!(
        classify(&failed(&unwarned, stdout, None)),
        FailureClass::Environment
    );
}

/// The generic half on its own. Nothing the repository contains can make the
/// sandbox permit a write, so nothing about this is evidence against an edit.
#[test]
fn a_denial_is_environmental_however_it_is_worded() {
    for denial in [
        "cp: /etc/hosts: Operation not permitted",
        "bash: /usr/local/bin/tool: Permission denied",
        "curl: (7) Network is unreachable",
    ] {
        assert_eq!(
            classify(&failed(denial, "", None)),
            FailureClass::Environment,
            "{denial}"
        );
    }
}

#[test]
fn dotnet_sandbox_tmp_denial_is_not_a_compiler_error() {
    let stderr = "Unhandled exception. System.TypeInitializationException: The type initializer for 'NuGet.Common.Migrations' threw an exception.\n ---> System.IO.IOException: The system cannot open the device or file specified. : '/tmp/.dotnet'\n   at NuGet.Common.Migrations..cctor()\nmkdtemp(\"/tmp/.dotnetXXXXXX\") == nullptr; errno == EPERM\n";
    assert_eq!(
        classify(&failed(stderr, "", None)),
        FailureClass::Environment
    );
}

/// A real compiler error keeps its class. The fix moved an ordering; it did not
/// make compilation unreachable, and a change that cured one misclassification
/// by causing another would not be a fix.
#[test]
fn a_located_compiler_error_is_still_a_compiler_error() {
    let rustc = "error[E0308]: mismatched types\n  --> src/parser.rs:42:17\n";
    assert_eq!(
        classify(&failed(rustc, "", Some("src/parser.rs:42"))),
        FailureClass::Compilation
    );
    // Without the bracketed code, and without a located file, `error:` alone
    // still reads as compilation: it is the weaker signal, not a disqualified
    // one.
    assert_eq!(
        classify(&failed("error: expected `;`\n", "", None)),
        FailureClass::Compilation
    );
}

/// A missing include and a missing interpreter print the same words, and are
/// told apart by whether anything in the output points at a source location.
/// The compiler names the file and line that asked; `make` naming a program it
/// could not run names nothing.
#[test]
fn a_missing_file_is_read_by_whether_the_output_points_at_source() {
    let include = "src/main.c:3:10: fatal error: missing.h: No such file or directory\n";
    assert_eq!(
        classify(&failed(include, "", Some("src/main.c:3"))),
        FailureClass::Compilation
    );
    assert_eq!(
        classify(&failed(
            "make: python: No such file or directory\n",
            "",
            None
        )),
        FailureClass::Environment
    );
}

/// A test that ran and disagreed is neither of those, and still is not.
#[test]
fn a_failing_assertion_is_untouched() {
    assert_eq!(
        classify(&failed("assert 1 == 2\n", "", None)),
        FailureClass::Assertion
    );
    assert_eq!(
        classify(&failed(
            "",
            "FAILED tests/test_more.py::test_reversed\n",
            None
        )),
        FailureClass::Assertion
    );
}
