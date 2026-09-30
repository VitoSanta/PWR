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
        ran_zero_tests(
            &check.command,
            &format!("{}\n{}", check.result.stdout, check.result.stderr),
        )
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
        ]
        .iter()
        .any(|signature| output.contains(signature))
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
}
