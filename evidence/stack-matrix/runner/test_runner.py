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
            with open(pathlib.Path(cwd).parent / "prompts.jsonl", "a") as log:
                log.write(json.dumps(params) + "\\n")
            meta = {"terminal": "completed"}
            if params.get("harness") == "minimal":
                meta["harness"] = "minimal"
            result = {"stopReason": "end_turn", "_meta": {"pwr": meta}}
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
    def runner(self, root, *arguments):
        core = root / "pwr-fake"
        core.write_text(FAKE_CORE)
        core.chmod(0o755)
        env = dict(os.environ,
                   PWR_BIN=str(core),
                   PWR_EVIDENCE_TASKS=str(root / "tasks"),
                   PWR_EVIDENCE_RESULTS=str(root / "runs"),
                   PWR_EVIDENCE_LEASE=str(root / "engine.lock"),
                   PWR_EVIDENCE_SPLITS=str(root / "splits.json"),
                   PWR_MLX_MODELS=str(root / "models"),
                   PWR_EVIDENCE_MODEL="owner/model")
        # Only the stand-in task: the repository's own tasks are left out.
        env["PWR_EVIDENCE_ONLY_EXTRA"] = "1"
        return subprocess.run([sys.executable, str(HERE / "run.py"), *arguments],
                              env=env, capture_output=True, text=True, timeout=300)

    def run_campaign(self, root, *extra):
        return self.runner(root, "run", "smoke-done", "--run", "t1", *extra)

    def test_a_run_records_its_verdict_and_provenance(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            self.assertEqual(self.runner(root, "freeze").returncode, 0)
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
        self.assertEqual(prov["splits"]["unfrozen"], [])
        self.assertEqual(len(prov["splits"]["manifest_digest"]), 12)

    def test_each_arm_sends_its_own_parameters_and_is_recorded(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            self.assertEqual(self.runner(root, "freeze").returncode, 0)
            done = self.run_campaign(root, "--arm", "minimal")
            self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
            prompt = json.loads((root / "runs/t1/smoke-done/prompts.jsonl").read_text().splitlines()[0])
            result = json.loads((root / "runs/t1/smoke-done/result.json").read_text())
        self.assertEqual(prompt["harness"], "minimal")
        self.assertFalse(prompt["goalMode"])
        self.assertEqual(result["arm"], "minimal")
        self.assertTrue(result["passed"])

    def test_a_task_changed_after_the_freeze_is_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            self.assertEqual(self.runner(root, "freeze").returncode, 0)
            (root / "tasks/smoke-done/brief.md").write_text("Create done.txt, and more.")
            refused = self.run_campaign(root)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("changed since the manifest was frozen", refused.stderr)
            self.assertFalse((root / "runs/t1/smoke-done/result.json").exists())
            allowed = self.run_campaign(root, "--allow-unfrozen")
            self.assertEqual(allowed.returncode, 0, allowed.stdout + allowed.stderr)
            result = json.loads((root / "runs/t1/smoke-done/result.json").read_text())
            self.assertEqual(result["provenance"]["splits"]["unfrozen"],
                             ["smoke-done: changed since the manifest was frozen"])

    def test_a_new_freeze_needs_a_reason_and_keeps_history(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            self.assertEqual(self.runner(root, "freeze").returncode, 0)
            (root / "tasks/smoke-done/brief.md").write_text("Revised brief.")
            self.assertNotEqual(self.runner(root, "freeze").returncode, 0)
            self.assertEqual(self.runner(root, "freeze", "--reason", "brief revised").returncode, 0)
            manifest = json.loads((root / "splits.json").read_text())
        self.assertEqual(manifest["history"][0]["reason_replaced"], "brief revised")
        self.assertEqual(manifest["history"][0]["changed"], ["smoke-done"])

    def test_a_campaign_is_refused_while_another_holds_the_engine(self):
        sys.path.insert(0, str(HERE))
        import provenance
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            make_task(root)
            self.assertEqual(self.runner(root, "freeze").returncode, 0)
            with provenance.Lease(root / "engine.lock", "the other campaign"):
                done = self.run_campaign(root)
            self.assertNotEqual(done.returncode, 0)
            self.assertIn("the other campaign", done.stdout + done.stderr)
            self.assertFalse((root / "runs/t1/smoke-done/result.json").exists())


if __name__ == "__main__":
    unittest.main()
