"""The runner end to end against a stand-in core: no model, no Docker.

The stand-in speaks the few ACP requests the runner sends and does the task's
work when prompted, so what is under test is the runner -- lease, engine check,
provenance, verdict, result -- not PWR.
"""

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import textwrap
import unittest

HERE = pathlib.Path(__file__).resolve().parent

FAKE_CORE = textwrap.dedent('''\
    #!/usr/bin/env python3
    import json, pathlib, sys
    cwd = None
    for line in sys.stdin:
        message = json.loads(line)
        method, ident, params = message.get("method"), message.get("id"), message.get("params") or {}
        if ident is None:
            continue
        if method == "_pwr/models":
            cwd = params["cwd"]
            result = {"contextTokens": 32768, "reasoningEffort": "medium",
                      "contextDecision": {"grantedTokens": 32768, "requestedTokens": 32768, "computed": True}}
        elif method == "_pwr/model_sampling":
            result = {"fields": [{"name": "temperature", "value": 0.6, "source": {"kind": "declared_profile"}}]}
        elif method == "session/new":
            result = {"sessionId": "s1"}
        elif method == "session/prompt":
            pathlib.Path(cwd, "done.txt").write_text("done")
            result = {"stopReason": "end_turn", "_meta": {"pwr": {"terminal": "completed"}}}
        else:
            result = {}
        print(json.dumps({"jsonrpc": "2.0", "id": ident, "result": result}), flush=True)
''')


def make_task(root):
    task = root / "tasks" / "smoke-done"
    (task / "workspace").mkdir(parents=True)
    (task / "workspace" / "README.md").write_text("Create done.txt.\n")
    (task / "brief.md").write_text("Create done.txt.")
    (task / "task.json").write_text(json.dumps({
        "id": "smoke-done", "title": "smoke", "stacks": ["sh"], "category": "smoke",
        "split": "dev", "turns": 1, "minutes": 5,
        "verify": {"host": "test -f done.txt"},
    }))


class Runner(unittest.TestCase):
    def run_campaign(self, root, *extra, lease=None):
        core = root / "pwr-fake"
        core.write_text(FAKE_CORE)
        core.chmod(0o755)
        env = dict(os.environ,
                   PWR_BIN=str(core),
                   PWR_EVIDENCE_TASKS=str(root / "tasks"),
                   PWR_EVIDENCE_RESULTS=str(root / "runs"),
                   PWR_EVIDENCE_LEASE=str(lease or root / "engine.lock"),
                   PWR_MLX_MODELS=str(root / "models"),
                   PWR_EVIDENCE_MODEL="owner/model")
        return subprocess.run([sys.executable, str(HERE / "run.py"), "run", "smoke-done", "--run", "t1", *extra],
                              env=env, capture_output=True, text=True, timeout=300)

    def test_a_run_records_its_verdict_and_provenance(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            done = self.run_campaign(root)
            self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
            result = json.loads((root / "runs/t1/smoke-done/result.json").read_text())
        self.assertTrue(result["passed"], result)
        prov = result["provenance"]
        self.assertEqual(prov["window"]["granted_tokens"], 32768)
        self.assertEqual(prov["sampling"]["temperature"]["value"], 0.6)
        self.assertEqual(prov["model"]["ref"], "owner/model")
        self.assertIn("memory_bytes", prov["machine"])
        self.assertIn("load_avg", prov["load_start"])
        self.assertIn("load_avg", prov["load_end"])
        self.assertEqual(prov["engines_at_start"], [])
        self.assertEqual(set(prov["runner_digest"]), {"run.py", "acp.py", "provenance.py"})
        self.assertIsNone(prov["seed"])

    def test_a_campaign_is_refused_while_another_holds_the_engine(self):
        sys.path.insert(0, str(HERE))
        import provenance
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            with provenance.Lease(root / "engine.lock", "the other campaign"):
                done = self.run_campaign(root)
            self.assertNotEqual(done.returncode, 0)
            self.assertIn("the other campaign", done.stdout + done.stderr)
            self.assertFalse((root / "runs/t1/smoke-done/result.json").exists())


if __name__ == "__main__":
    unittest.main()
