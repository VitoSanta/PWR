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
        ]
        .iter()
        .any(|signature| output.contains(signature))
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
        ] {
            assert!(ran_zero_tests(command, output), "{command}");
        }
        assert!(!ran_zero_tests(
            "cargo test",
            "running 0 tests\nrunning 2 tests"
        ));
        assert!(!ran_zero_tests("cargo build", "no tests found"));
        assert!(!ran_zero_tests("pytest", "10 passed"));
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
