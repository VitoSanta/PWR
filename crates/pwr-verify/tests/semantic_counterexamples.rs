//! Independent dev counterexamples, never copied from reserved campaign tasks.
//! A superficial green check is deliberately insufficient; frozen boundary tests
//! reject the bad candidate and accept the correct one using the same runner.
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::time::Duration;

#[tokio::test]
async fn declared_green_does_not_establish_unicode_precision_bool_or_atomicity() {
    let cases = [
        (
            "ascii",
            "def valid(s): return s.isdecimal()",
            "def valid(s): return bool(s) and all('0' <= c <= '9' for c in s)",
            "assert valid('123')",
            "assert not valid('１２３'); assert not valid('١٢٣'); assert not valid('')",
        ),
        (
            "precision",
            "def increment(n): return int(float(n) + 1)",
            "def increment(n): return n + 1",
            "assert increment(3) == 4",
            "assert increment(9007199254740992) == 9007199254740993; assert increment(-9007199254740994) == -9007199254740993",
        ),
        (
            "boolean",
            "def numeric(x): return isinstance(x, (int, float))",
            "def numeric(x): return isinstance(x, (int, float)) and not isinstance(x, bool)",
            "assert numeric(4)",
            "assert not numeric(True); assert not numeric(False); assert numeric(0)",
        ),
        (
            "atomic",
            "def batch(state, values):\n for x in values:\n  if x < 0: raise ValueError()\n  state.append(x)",
            "def batch(state, values):\n if any(x < 0 for x in values): raise ValueError()\n state.extend(values)",
            "s=[]; batch(s,[1,2]); assert s == [1,2]",
            "s=[7]\ntry: batch(s,[1,-1])\nexcept ValueError: pass\nelse: raise AssertionError('invalid batch accepted')\nassert s == [7]",
        ),
    ];
    for (name, bad, good, shallow, boundary) in cases {
        let root = tempfile::tempdir().unwrap();
        for (file, body) in [("shallow.py", shallow), ("independent.py", boundary)] {
            std::fs::write(
                root.path().join(file),
                format!("from candidate import *\n{body}\n"),
            )
            .unwrap();
        }
        let checker_hash =
            pwr_domain::hash_bytes(std::fs::read(root.path().join("independent.py")).unwrap());
        let policy = ToolPolicy {
            root: root.path().into(),
            extra_readable: vec![],
            protected: vec![],
            allow_commands: vec!["python3".into()],
            output_limit: 4096,
            timeout: Duration::from_secs(3),
            sandbox: SandboxPolicy::Disabled,
            approvals: vec![],
        };
        let checks = vec![
            ("python3".into(), vec!["-B".into(), "shallow.py".into()]),
            ("python3".into(), vec!["-B".into(), "independent.py".into()]),
        ];
        for (candidate, expected) in [(bad, false), (good, true)] {
            std::fs::write(root.path().join("candidate.py"), format!("{candidate}\n")).unwrap();
            let result = pwr_verify::baseline(&policy, &checks).await.unwrap();
            assert!(
                result.checks[0].succeeded(),
                "superficial control must pass: {name}"
            );
            assert_eq!(
                result.checks[1].succeeded(),
                expected,
                "independent boundary: {name}"
            );
            assert_eq!(
                matches!(
                    pwr_verify::evidence::checks(&result),
                    pwr_domain::ChecksOutcome::Passed
                ),
                expected,
                "{name}"
            );
            assert!(result.integrity_error().is_none());
            assert_eq!(
                checker_hash,
                pwr_domain::hash_bytes(std::fs::read(root.path().join("independent.py")).unwrap())
            );
        }
    }
}
