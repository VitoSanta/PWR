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
            // pytest's `FAILED path::test - reason`; unittest's closing
            // `FAILED (failures=1)` is a count, not a test.
            test.split(" - ").next().filter(|id| !id.starts_with('('))
        } else if let Some(test) = line.strip_prefix("Failed ") {
            test.split(" [").next()
        } else if let Some(test) = line.strip_suffix(" --- FAILED") {
            Some(test)
        } else if let Some(test) = line.strip_prefix("● ").or_else(|| line.strip_prefix("× ")) {
            Some(test)
        } else if let Some(test) = line.strip_prefix("✖ ") {
            // node:test's spec reporter: `✖ name (1.2ms)`, and once more under
            // the `✖ failing tests:` heading, which names nothing itself.
            test.rsplit_once(" (")
                .filter(|(_, tail)| tail.ends_with("ms)"))
                .map(|(name, _)| name)
        } else if let Some(test) = line.strip_prefix("not ok ") {
            // TAP, node:test's reporter when its output is not a terminal on
            // older Node: `not ok 3 - name`.
            test.split_once(" - ").map(|(_, name)| name)
        } else if let Some(test) = line
            .strip_suffix(" ... FAIL")
            .or_else(|| line.strip_suffix(" ... ERROR"))
        {
            // unittest -v: `test_done (pkg.Case.test_done) ... FAIL`.
            Some(unittest_id(test))
        } else if let Some(test) = line
            .strip_prefix("FAIL: ")
            .or_else(|| line.strip_prefix("ERROR: "))
        {
            // unittest's own summary of each failure, printed without -v too.
            Some(unittest_id(test))
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
/// `pkg.Case.test_done` out of `test_done (pkg.Case.test_done)`; Python
/// before 3.11 prints `test_done (pkg.Case)`, completed here with the name.
fn unittest_id(line: &str) -> &str {
    line.split_once(" (")
        .and_then(|(_, tail)| tail.strip_suffix(')'))
        .filter(|qualified| {
            line.split(' ')
                .next()
                .is_some_and(|name| qualified.ends_with(name))
        })
        .unwrap_or(line)
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
            (
                "✖ an invoice issued on a weekend starts on Monday (0.52ms)",
                "an invoice issued on a weekend starts on Monday",
            ),
            ("not ok 3 - rows end with CRLF", "rows end with CRLF"),
            (
                "test_done (test_todo.TodoContract.test_done) ... FAIL",
                "test_todo.TodoContract.test_done",
            ),
            (
                "test_unicode (test_hidden.Hidden.test_unicode) ... ERROR",
                "test_hidden.Hidden.test_unicode",
            ),
            (
                "FAIL: test_done (test_todo.TodoContract.test_done)",
                "test_todo.TodoContract.test_done",
            ),
        ] {
            assert!(fingerprint(line, "").identifiers.contains(id), "{line}");
        }
    }
    #[test]
    fn node_and_unittest_failures_are_named_once_and_headings_are_not_tests() {
        // The outputs of the ledger and todo diagnostics (pwr-evidence,
        // 2026-10-05/06): until then both fell back to hashing the last forty
        // lines, so two different failing sets of one suite were told apart
        // only by their noise and never by which tests failed.
        let node = "✔ business days after a weekday (1.0ms)\n\
                    ✖ an invoice issued on a weekend starts on Monday (0.5ms)\n\
                    ℹ tests 2\nℹ fail 1\n\n✖ failing tests:\n\n\
                    test at test/dates.test.ts:11:1\n\
                    ✖ an invoice issued on a weekend starts on Monday (0.9ms)\n";
        assert_eq!(
            fingerprint(node, "").identifiers,
            BTreeSet::from(["an invoice issued on a weekend starts on Monday".to_owned()])
        );
        let unittest = "test_blank (test_todo.TodoContract.test_blank) ... ok\n\
                        test_done (test_todo.TodoContract.test_done) ... FAIL\n\
                        ======\nFAIL: test_done (test_todo.TodoContract.test_done)\n\
                        ------\nRan 2 tests in 0.3s\n\nFAILED (failures=1)\n";
        assert_eq!(
            fingerprint("", unittest).identifiers,
            BTreeSet::from(["test_todo.TodoContract.test_done".to_owned()])
        );
        assert_eq!(
            fingerprint("FAIL: test_old (test_todo.TodoContract)", "").identifiers,
            BTreeSet::from(["test_old (test_todo.TodoContract)".to_owned()])
        );
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
