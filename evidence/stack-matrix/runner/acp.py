"""A headless ACP client for `pwr serve --stdio` -- the requests the desktop app sends.

One process per task, kept for the whole task, as the app keeps it: the model
engine and its prompt cache live in that process, and a client that restarts
it between prompts measures cold prefill, not PWR.

Every message both ways is written to the transcript with a timestamp, so a
run can be read back as the conversation it was.
"""

import json
import os
import queue
import subprocess
import threading
import time


class Core:
    def __init__(self, binary, transcript_path, env=None):
        self.transcript = open(transcript_path, "a", buffering=1)
        self.proc = subprocess.Popen(
            [binary, "serve", "--stdio"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=dict(os.environ, **(env or {})),
            text=True,
            bufsize=1,
        )
        self.next_id = 0
        self.incoming = queue.Queue()
        self.lock = threading.Lock()
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._stderr, daemon=True).start()

    def record(self, direction, message):
        self.transcript.write(json.dumps({"t": round(time.time(), 3), direction: message}) + "\n")

    def _read(self):
        for line in self.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                self.record("unparsed", line)
                continue
            self.record("in", message)
            self.incoming.put(message)
        self.incoming.put(None)

    def _stderr(self):
        for line in self.proc.stderr:
            self.record("stderr", line.rstrip())

    def send(self, message):
        with self.lock:
            self.record("out", message)
            self.proc.stdin.write(json.dumps(message) + "\n")
            self.proc.stdin.flush()

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def request(self, method, params, on_message=None, timeout=None):
        self.next_id += 1
        ident = self.next_id
        self.send({"jsonrpc": "2.0", "id": ident, "method": method, "params": params})
        deadline = None if timeout is None else time.time() + timeout
        while True:
            left = None if deadline is None else max(0.1, deadline - time.time())
            try:
                message = self.incoming.get(timeout=left)
            except queue.Empty:
                raise TimeoutError(f"{method} did not answer in {timeout}s")
            if message is None:
                raise RuntimeError("the core exited")
            if message.get("id") == ident and "method" not in message:
                if "error" in message:
                    raise RuntimeError(f"{method}: {message['error']}")
                return message.get("result")
            if on_message:
                on_message(message)

    def answer(self, request_id, result):
        self.send({"jsonrpc": "2.0", "id": request_id, "result": result})

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=30)
        except Exception:
            self.proc.kill()
        self.transcript.close()


def initialize(core):
    return core.request(
        "initialize",
        {
            "protocolVersion": 1,
            "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False},
        },
        timeout=120,
    )
