"""Resource ownership of the stand-in ACP client; no model inference."""
import json
import pathlib
import queue
import tempfile
import unittest
from unittest import mock

from acp import Core


class Cleanup(unittest.TestCase):
    def test_streaming_notifications_cannot_extend_the_request_deadline(self):
        core = Core.__new__(Core)
        core.next_id = 0
        core.send = mock.Mock()
        core.incoming = queue.Queue()
        for _ in range(3):
            core.incoming.put({"method": "progress"})
        with mock.patch("acp.time.monotonic", side_effect=[10, 10.1, 10.9, 11.1]):
            with self.assertRaisesRegex(TimeoutError, "did not answer"):
                core.request("prompt", {}, timeout=1)
        self.assertEqual(core.incoming.qsize(), 1)

    def test_close_drains_output_and_closes_every_owned_stream(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            binary = root / "fake-core"
            binary.write_text('#!/usr/bin/env python3\nimport sys\nfor line in sys.stdin: pass\nprint("last stdout", flush=True)\nprint("last stderr", file=sys.stderr, flush=True)\n')
            binary.chmod(0o755)
            transcript = root / "trace.jsonl"
            core = Core(str(binary), transcript)
            core.close()
            self.assertIsNotNone(core.proc.returncode)
            self.assertTrue(all(stream.closed for stream in (core.proc.stdin, core.proc.stdout, core.proc.stderr, core.transcript)))
            self.assertTrue(all(not reader.is_alive() for reader in core.readers))
            events = [json.loads(line) for line in transcript.read_text().splitlines()]
            self.assertIn({"unparsed": "last stdout"}, [{k: v for k, v in event.items() if k != "t"} for event in events])
            self.assertIn("last stderr", [event.get("stderr") for event in events])

    def test_spawn_failure_closes_the_transcript(self):
        with tempfile.TemporaryDirectory() as folder:
            transcript = open(pathlib.Path(folder) / "trace.jsonl", "a")
            with mock.patch("acp.open", return_value=transcript), mock.patch("acp.subprocess.Popen", side_effect=OSError("spawn failed")):
                with self.assertRaisesRegex(OSError, "spawn failed"):
                    Core("missing", "ignored")
            self.assertTrue(transcript.closed)
