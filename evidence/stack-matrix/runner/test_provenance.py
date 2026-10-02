"""Provenance and the engine lease, without a model, Docker or the network.

    python3 -m unittest discover -s evidence/stack-matrix/runner
"""

import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import provenance  # noqa: E402


class Engines(unittest.TestCase):
    PS = "\n".join([
        "  101 /Applications/Ollama.app/Contents/Resources/ollama serve",
        "  102 /Applications/Ollama.app/Contents/Resources/ollama runner --model /x/blobs/sha256-1 --port 5",
        "  103 /venv/bin/python /repo/crates/pwr-mlx/sidecar/pwr_mlx.py",
        "  104 /venv/bin/python -m mlx_lm.server --model m --port 8080",
        "  105 /opt/llama/bin/llama-server -m model.gguf",
        "  106 vim notes-about-llama-server.md",
        "  107 ng serve --port 4300",
    ])

    def test_engines_are_recognised_and_bystanders_are_not(self):
        found = provenance.engines_in(self.PS)
        self.assertEqual([(e["pid"], e["engine"]) for e in found], [
            (102, "ollama-runner"), (103, "pwr-mlx"), (104, "mlx-lm-server"), (105, "llama-server"),
        ])

    def test_the_runners_own_processes_are_not_competitors(self):
        found = provenance.engines_in(self.PS, own_pids={103})
        self.assertNotIn(103, [e["pid"] for e in found])


class LeaseTests(unittest.TestCase):
    def test_a_second_campaign_is_refused_and_told_who_holds_the_engine(self):
        with tempfile.TemporaryDirectory() as folder:
            path = pathlib.Path(folder) / "engine.lock"
            with provenance.Lease(path, "run-a"):
                with self.assertRaises(provenance.LeaseHeld) as refused:
                    with provenance.Lease(path, "run-b"):
                        self.fail("two campaigns held the engine")
                self.assertIn("run-a", str(refused.exception))
            # Released on exit: the next campaign takes it.
            with provenance.Lease(path, "run-c"):
                self.assertIn("run-c", path.read_text())
            self.assertEqual(path.read_text(), "")


class ModelFacts(unittest.TestCase):
    def test_an_artifact_is_named_by_revision_quantization_and_template(self):
        with tempfile.TemporaryDirectory() as root:
            folder = pathlib.Path(root) / "owner/model"
            folder.mkdir(parents=True)
            (folder / ".pwr-revision").write_text("abc123\n")
            (folder / "config.json").write_text(json.dumps({
                "text_config": {"model_type": "qwen3"},
                "quantization": {"bits": 4, "group_size": 64, "layers.0.gate": {"bits": 8}},
            }))
            (folder / "chat_template.jinja").write_text("{{ messages }}")
            (folder / "model.safetensors").write_bytes(b"\0" * 10)
            facts = provenance.model_facts("owner/model", pathlib.Path(root))
        self.assertEqual(facts["revision"], "abc123")
        self.assertEqual(facts["quantization"], {"bits": 4, "group_size": 64})
        self.assertEqual(facts["quantization_overrides"], 1)
        self.assertEqual(facts["model_type"], "qwen3")
        self.assertEqual(facts["weights_bytes"], 10)
        self.assertEqual(len(facts["chat_template_digest"]), 12)

    def test_a_missing_artifact_says_unknown_rather_than_failing(self):
        with tempfile.TemporaryDirectory() as root:
            facts = provenance.model_facts("owner/absent", pathlib.Path(root))
        self.assertIsNone(facts["revision"])
        self.assertIsNone(facts["quantization"])
        self.assertIsNone(facts["weights_bytes"])


class Replies(unittest.TestCase):
    def test_sampling_keeps_each_value_with_its_source(self):
        reply = {"fields": [
            {"name": "temperature", "value": 0.6, "source": {"kind": "declared_profile"}},
            {"name": "min_p", "value": None, "source": "unset"},
        ]}
        self.assertEqual(provenance.sampling_from(reply), {
            "temperature": {"value": 0.6, "source": {"kind": "declared_profile"}},
            "min_p": {"value": None, "source": "unset"},
        })

    def test_window_is_the_granted_one(self):
        reply = {"contextTokens": 65536, "reasoningEffort": "medium",
                 "contextDecision": {"grantedTokens": 65536, "requestedTokens": 131072,
                                     "computed": True, "rationale": "capped by memory"}}
        window = provenance.window_from(reply)
        self.assertEqual(window["granted_tokens"], 65536)
        self.assertEqual(window["requested_tokens"], 131072)
        self.assertEqual(window["reasoning_effort"], "medium")


if __name__ == "__main__":
    unittest.main()
