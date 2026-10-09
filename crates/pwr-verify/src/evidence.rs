//! Deterministic check evidence shared by every presentation layer.
use super::VerificationBaseline;
use pwr_domain::{BaselineOutcome, ChecksOutcome};

pub fn checks(after: &VerificationBaseline) -> ChecksOutcome {
    if after.checks.is_empty() {
        return ChecksOutcome::Unavailable {
            why: "no checks were run".into(),
        };
    }
    let failures: Vec<_> = after
        .checks
        .iter()
        .filter(|check| check.result.exit_code != Some(0))
        .collect();
    if !failures.is_empty() {
        if failures
            .iter()
            .any(|check| check.result.exit_code.is_none())
        {
            return ChecksOutcome::CouldNotRun {
                why: failures
                    .iter()
                    .map(|check| check.command.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            };
        }
        return ChecksOutcome::Failed {
            fingerprints: failures
                .iter()
                .map(|check| {
                    format!(
                        "{}:{}",
                        check.command,
                        super::failure::fingerprint(&check.result.stdout, &check.result.stderr)
                            .digest
                    )
                })
                .collect(),
        };
    }
    if after.checks.iter().any(|check| {
        let output = format!("{}\n{}", check.result.stdout, check.result.stderr);
        ran_zero_tests(&check.command, &output) || passed_without_tests(&check.command, &output)
    }) {
        return ChecksOutcome::RanZeroTests;
    }
    ChecksOutcome::Passed
}
pub fn baseline(
    before: Option<&VerificationBaseline>,
    after: &VerificationBaseline,
) -> BaselineOutcome {
    match before {
        None => BaselineOutcome::NoBaseline,
        Some(before) => {
            let regressions = super::compare(before, after).new_failures;
            if regressions.is_empty() {
                BaselineOutcome::Preserved
            } else {
                BaselineOutcome::Regressed {
                    checks: regressions,
                }
            }
        }
    }
}
/// Only known zero-test signatures count. Missing telemetry is never guessed
/// to be zero; build/lint commands do not claim that tests executed at all.
pub fn ran_zero_tests(command: &str, output: &str) -> bool {
    if command.starts_with("cargo test") {
        let counts: Vec<usize> = output
            .lines()
            .filter_map(|line| line.trim().strip_prefix("running "))
            .filter_map(|line| line.split_whitespace().next()?.parse().ok())
            .collect();
        return !counts.is_empty() && counts.iter().sum::<usize>() == 0;
    }
    if command.starts_with("go test")
        && output.lines().any(|line| {
            line.trim_start().starts_with("ok ") || line.trim_start().starts_with("ok\t")
        })
    {
        return false;
    }
    let output = output.to_lowercase();
    let is_test = command.contains("test")
        || command.contains("pytest")
        || command.contains("vitest")
        || command.contains("jest");
    is_test
        && [
            "no tests found",
            "no test files found",
            "no tests ran",
            "collected 0 items",
            "total tests: 0",
            "total tests:     0",
            "[no test files]",
            "no tests were found",
            "no test is available",
            "no tests to run",
            "0 examples",
            "no tests executed",
            // unittest, and Django's runner over it: `Ran 0 tests in 0.000s`,
            // followed by `OK` and exit 0 before Python 3.12.
            "ran 0 tests in",
        ]
        .iter()
        .any(|signature| output.contains(signature))
}

/// How many tests a run executed, where its output says that tests ran and
/// that every one of them passed. `None` for anything else: a failure, no
/// test executed, or a runner whose words are not known here -- never a
/// guess.
///
/// - unittest, and Django's runner over it: `Ran 12 tests in 0.3s` then `OK`;
/// - pytest: `12 passed in 0.31s`, with nothing failed or in error beside it;
/// - cargo: every `test result:` line `ok`, with at least one test among them.
pub fn passed_tests(output: &str) -> Option<usize> {
    let lines: Vec<&str> = output.lines().map(str::trim).collect();
    let count_before = |line: &str, word: &str| -> Option<usize> {
        let before = line.split(word).next()?.trim_end();
        before.rsplit([' ', '=']).next()?.parse().ok()
    };
    // unittest: the last run stands for the output.
    if let Some(at) = lines.iter().rposition(|line| {
        line.starts_with("Ran ") && (line.contains(" tests in ") || line.contains(" test in "))
    }) {
        let ran: usize = lines[at].split_whitespace().nth(1)?.parse().ok()?;
        let verdict = lines[at + 1..].iter().find(|line| !line.is_empty())?;
        let ok = *verdict == "OK" || verdict.starts_with("OK (");
        return (ran > 0 && ok).then_some(ran);
    }
    // pytest: its closing line, with or without the rule of `=` around it.
    if let Some(summary) = lines.iter().rev().find(|line| {
        line.contains(" passed")
            && line.contains(" in ")
            && line.trim_end_matches('=').trim_end().ends_with('s')
    }) {
        let bad = [" failed", " error", " errors"];
        if bad.iter().any(|word| summary.contains(word)) {
            return None;
        }
        return count_before(summary, " passed").filter(|passed| *passed > 0);
    }
    // cargo: one line per test binary.
    let results: Vec<&&str> = lines
        .iter()
        .filter(|line| line.starts_with("test result: "))
        .collect();
    if !results.is_empty() {
        if results
            .iter()
            .any(|line| !line.starts_with("test result: ok."))
        {
            return None;
        }
        let passed: usize = results
            .iter()
            .filter_map(|line| count_before(line, " passed"))
            .sum();
        return (passed > 0).then_some(passed);
    }
    None
}

/// Test runners that say how many tests ran whenever any did, so that a pass
/// with no count in it is a pass of nothing. Not `npm test`, `make test` or
/// a build tool run quietly (`mvn -q`, Gradle): what those print is the
/// project's choice, and silence there is not evidence.
const COUNTING_RUNNERS: &[&str] = &[
    "dotnet test",
    "go test",
    "pytest",
    "swift test",
    "mix test",
    "flutter test",
    "ctest",
    "rspec",
    "vitest",
    "jest",
    "phpunit",
];

/// A check that succeeded without running a test: for a runner that counts,
/// a pass whose output counts none -- no number at all, or zero.
///
/// The signatures of [`ran_zero_tests`] are sentences, and a sentence can be
/// missing: seen 2026-10-07, `dotnet test spese.sln` on a solution file
/// that listed no project printed a restore warning, exited 0 and was a
/// passed check, with ten tests sitting in a project it never opened. The
/// opposite question has an answer in every language a runner speaks: did
/// it say that tests ran. Asked only of a command that succeeded -- one
/// that failed to compile ran no tests either, and that is a failure.
pub fn passed_without_tests(command: &str, output: &str) -> bool {
    if command.starts_with("cargo test") {
        // Counted by `ran_zero_tests`; a crate with no tests is not an error.
        return false;
    }
    if !COUNTING_RUNNERS
        .iter()
        .any(|runner| command.contains(runner))
    {
        return false;
    }
    if command.contains("go test") {
        return !output.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("ok ") || line.starts_with("ok\t") || line.starts_with("PASS")
        });
    }
    tests_counted(output).unwrap_or(0) == 0
}

/// The largest number of tests the output says ran or passed, read from the
/// forms runners use: `12 passed`, `Passed: 12`, `Total tests: 12`, `Ran 12
/// tests`, `12 tests, 0 failures`, `12 examples`, `out of 12`, `# tests 12`.
/// `None` when it names no count.
pub fn tests_counted(output: &str) -> Option<usize> {
    let text = output.to_lowercase();
    let mut found: Option<usize> = None;
    let mut keep = |count: Option<usize>| {
        if let Some(count) = count {
            found = Some(found.map_or(count, |seen| seen.max(count)));
        }
    };
    // A label, then the number.
    for label in [
        "total tests:",
        "total:",
        "tests run:",
        "passed:",
        "# tests",
        "ℹ tests",
        "# pass",
        "ℹ pass",
        "executed",
        "ran",
        "out of",
        "test run with",
    ] {
        for (at, _) in text.match_indices(label) {
            let rest = text[at + label.len()..].trim_start();
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            let after = rest[digits.len()..].trim_start();
            // "ran 12 tests", "executed 12 tests", "test run with 12 tests":
            // a bare verb counts only when tests are what it counted.
            let needs_noun = matches!(label, "executed" | "ran" | "test run with");
            if !digits.is_empty() && (!needs_noun || after.starts_with("test")) {
                keep(digits.parse().ok());
            }
        }
    }
    // The number, then the word.
    for word in [
        " passed",
        " tests passed",
        " tests,",
        " test,",
        " examples",
        " example,",
    ] {
        for (at, _) in text.match_indices(word) {
            let digits: String = text[..at]
                .chars()
                .rev()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            keep(digits.parse().ok());
        }
    }
    // Flutter's last line carries its count as `+12`; it says tests ran.
    if text.contains("all tests passed") {
        keep(Some(1));
    }
    found
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_tests_are_recognized_across_toolchains_without_confusing_empty_cargo_crates() {
        for (command, output) in [
            ("cargo test", "running 0 tests"),
            ("pytest", "collected 0 items"),
            ("npm test", "No tests found, exiting with code 0"),
            ("vitest run", "No test files found"),
            ("dotnet test", "Total tests: 0"),
            ("go test ./...", "pkg [no test files]"),
            (
                "./tests/runtests.py forms_tests.no_such",
                "Ran 0 tests in 0.000s\n\nOK",
            ),
            ("python -m unittest discover", "Ran 0 tests in 0.000s\n\nOK"),
        ] {
            assert!(ran_zero_tests(command, output), "{command}");
        }
        assert!(!ran_zero_tests(
            "cargo test",
            "running 0 tests\nrunning 2 tests"
        ));
        assert!(!ran_zero_tests("cargo build", "no tests found"));
        assert!(!ran_zero_tests("pytest", "10 passed"));
        assert!(!ran_zero_tests(
            "./tests/runtests.py forms_tests",
            "Ran 10 tests in 0.200s\n\nOK"
        ));
    }

    #[test]
    fn tests_that_ran_and_all_passed_are_counted_and_nothing_else_is() {
        // Django's runner, as the model reads it.
        let django = "System check identified no issues (0 silenced).\n....\n\
                      ----------------------------------------------------------------------\n\
                      Ran 45 tests in 0.010s\n\nOK\n";
        assert_eq!(passed_tests(django), Some(45));
        assert_eq!(
            passed_tests("Ran 646 tests in 1.2s\n\nOK (skipped=3)\n"),
            Some(646)
        );
        assert_eq!(passed_tests("Ran 1 test in 0.001s\n\nOK\n"), Some(1));
        // Not one test executed: nothing is confirmed.
        assert_eq!(passed_tests("Ran 0 tests in 0.000s\n\nOK\n"), None);
        assert_eq!(
            passed_tests("Ran 2 tests in 0.000s\n\nFAILED (errors=2)\n"),
            None
        );
        // A run that passed followed by one that did not: the last one stands.
        assert_eq!(
            passed_tests("Ran 3 tests in 0.1s\n\nOK\nRan 3 tests in 0.1s\n\nFAILED (failures=1)\n"),
            None
        );
        // pytest, with its rule and without.
        assert_eq!(
            passed_tests("======== 7 passed, 1 warning in 1.97s ========\n"),
            Some(7)
        );
        assert_eq!(passed_tests("7 passed in 0.31s\n"), Some(7));
        assert_eq!(
            passed_tests("==== 1 failed, 7 passed in 1.96s ====\n"),
            None
        );
        assert_eq!(passed_tests("==== 7 passed, 1 error in 2.0s ====\n"), None);
        assert_eq!(passed_tests("==== no tests ran in 0.01s ====\n"), None);
        assert_eq!(passed_tests("==== 41 skipped in 0.5s ====\n"), None);
        // cargo: every binary ok, and at least one test among them.
        let cargo = "test result: ok. 5 passed; 0 failed; 0 ignored\n\
                     test result: ok. 0 passed; 0 failed; 0 ignored\n";
        assert_eq!(passed_tests(cargo), Some(5));
        assert_eq!(
            passed_tests(
                "test result: ok. 5 passed; 0 failed\ntest result: FAILED. 1 passed; 2 failed\n"
            ),
            None
        );
        assert_eq!(passed_tests("test result: ok. 0 passed; 0 failed\n"), None);
        // Words nobody here knows, or no tests at all: not a guess.
        assert_eq!(passed_tests("All good!\n"), None);
        assert_eq!(passed_tests("Build succeeded.\n    0 Warning(s)\n"), None);
        assert_eq!(passed_tests(""), None);
    }

    #[test]
    fn a_counting_runner_that_passed_and_counted_nothing_ran_nothing() {
        // The solution that listed no project: a warning, exit 0, no count.
        assert!(passed_without_tests(
            "dotnet test spese.sln --nologo",
            "warning : Unable to find a project to restore!"
        ));
        for (command, nothing) in [
            ("pytest -q", "no tests ran in 0.01s"),
            ("go test ./...", "?   example.com/app [no test files]"),
            ("swift test --package-path app", "Build complete!"),
            ("mix test", "Finished in 0.0 seconds\n0 tests, 0 failures"),
            ("flutter test", ""),
            ("ctest --test-dir build", "No tests were found!!!"),
            ("bundle exec rspec", "0 examples, 0 failures"),
            ("npx vitest run", "No test files found, exiting with code 0"),
        ] {
            assert!(passed_without_tests(command, nothing), "{command}");
        }
        for (command, ran) in [
            (
                "dotnet test spese.sln --nologo",
                "Passed!  - Failed: 0, Passed: 10, Skipped: 0, Total: 10",
            ),
            ("dotnet test", "Total tests: 3\n     Passed: 3"),
            ("pytest -q", "12 passed in 0.31s"),
            ("go test ./...", "ok  \texample.com/app\t0.012s"),
            ("swift test", "Executed 7 tests, with 0 failures"),
            ("mix test", "5 tests, 0 failures"),
            ("flutter test", "00:02 +9: All tests passed!"),
            (
                "ctest --test-dir build",
                "100% tests passed, 0 tests failed out of 4",
            ),
            ("bundle exec rspec", "8 examples, 0 failures"),
            ("npx vitest run", "Tests  6 passed (6)"),
            ("npx jest", "Tests:       4 passed, 4 total"),
        ] {
            assert!(!passed_without_tests(command, ran), "{command}");
        }
        // What a project's own script or a quiet build tool prints is not
        // read for a count, and neither is a command that is not a test.
        for command in [
            "npm test --silent",
            "make test",
            "mvn -q test",
            "gradle test",
            "dotnet build",
        ] {
            assert!(!passed_without_tests(command, ""), "{command}");
        }
        assert_eq!(tests_counted("# tests 12\n# pass 12"), Some(12));
        assert_eq!(tests_counted("Ran 5 tests in 0.002s\n\nOK"), Some(5));
        assert_eq!(tests_counted("ran the linter"), None);
    }
}
