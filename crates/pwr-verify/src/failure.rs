//! Stable failure identity shared by goal progress and flaky-check detection.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureFingerprint {
    pub identifiers: BTreeSet<String>,
    pub digest: String,
}
pub fn fingerprint(stdout: &str, stderr: &str) -> FailureFingerprint {
    let output = format!("{stdout}\n{stderr}");
    let mut identifiers = BTreeSet::new();
    for line in output.lines().map(str::trim) {
        let identifier = if let Some(test) = line
            .strip_prefix("test ")
            .filter(|line| line.ends_with("FAILED"))
        {
            test.split(" ...").next()
        } else if let Some(test) = line.strip_prefix("--- FAIL: ") {
            test.split_whitespace().next()
        } else if let Some(test) = line.strip_prefix("FAILED ") {
            test.split(" - ").next()
        } else if let Some(test) = line.strip_prefix("Failed ") {
            test.split(" [").next()
        } else if let Some(test) = line.strip_suffix(" --- FAILED") {
            Some(test)
        } else if let Some(test) = line.strip_prefix("● ").or_else(|| line.strip_prefix("× ")) {
            Some(test)
        } else {
            line.strip_prefix("FAIL ").map(str::trim)
        };
        if let Some(identifier) = identifier.filter(|identifier| !identifier.is_empty()) {
            identifiers.insert(normalize(identifier));
        }
    }
    let evidence = if identifiers.is_empty() {
        output
            .lines()
            .rev()
            .take(40)
            .map(normalize)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        identifiers.iter().cloned().collect::<Vec<_>>().join("\n")
    };
    FailureFingerprint {
        identifiers,
        digest: pwr_domain::hash_bytes(evidence),
    }
}
fn normalize(line: &str) -> String {
    line.split_whitespace()
        .map(|word| {
            let bare = word.trim_matches(['(', ')', '[', ']', ',']);
            if bare.starts_with("0x") && bare[2..].chars().all(|ch| ch.is_ascii_hexdigit()) {
                return "<address>".to_owned();
            }
            if ["ms", "s"].iter().any(|suffix| {
                bare.strip_suffix(suffix)
                    .is_some_and(|number| number.parse::<f64>().is_ok())
            }) {
                return "<duration>".to_owned();
            }
            if word.starts_with('/') {
                return format!("<path>/{}", word.rsplit('/').next().unwrap_or_default());
            }
            word.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failing_test_ids_are_stable_across_toolchains() {
        for (line, id) in [
            ("test module::a ... FAILED", "module::a"),
            ("module::a --- FAILED", "module::a"),
            ("● accepts valid input", "accepts valid input"),
            ("× rejects invalid input", "rejects invalid input"),
            (
                "FAILED tests/a.py::test_a - AssertionError",
                "tests/a.py::test_a",
            ),
            ("--- FAIL: TestA (0.01s)", "TestA"),
            ("Failed Tests.A [23 ms]", "Tests.A"),
            ("FAIL src/a.test.ts > works", "src/a.test.ts > works"),
        ] {
            assert!(fingerprint(line, "").identifiers.contains(id), "{line}");
        }
    }
    #[test]
    fn same_exit_code_with_different_tests_is_a_different_failure() {
        assert_ne!(
            fingerprint("test a ... FAILED", ""),
            fingerprint("test b ... FAILED", "")
        );
        assert_eq!(
            fingerprint("error at /tmp/one/a.rs took 1.2s pointer 0x123", ""),
            fingerprint("error at /tmp/two/a.rs took 2.4s pointer 0x456", "")
        );
    }
}
