"""Hidden end-to-end checks: Redis going away and coming back."""

import json
import os
import subprocess
import time
import unittest
import urllib.error
import urllib.request

BASE = os.environ["HITS_URL"]
PROJECT = os.environ["HITS_PROJECT"]


def call(method, path, client=None):
    request = urllib.request.Request(BASE + path, method=method, data=b"" if method == "POST" else None)
    if client:
        request.add_header("X-Client-Id", client)
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return response.status, json.loads(response.read() or b"null")
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read() or b"null")


def compose(*args):
    subprocess.run(["docker", "compose", "-p", PROJECT, *args], check=True, capture_output=True)


def wait_for(status, path="/health", seconds=60):
    deadline = time.time() + seconds
    while time.time() < deadline:
        try:
            if call("GET", path)[0] == status:
                return True
        except OSError:
            pass
        time.sleep(1)
    return False


class Hidden(unittest.TestCase):
    def test_redis_down_then_back_keeps_the_counts(self):
        for _ in range(4):
            call("POST", "/hit/persist", client="h1")
        compose("stop", "redis")
        self.assertTrue(wait_for(503), "health did not report Redis down")
        self.assertEqual(call("GET", "/health")[1].get("redis"), "down")
        compose("start", "redis")
        self.assertTrue(wait_for(200), "health did not recover")
        self.assertEqual(call("GET", "/count/persist"), (200, {"page": "persist", "count": 4}))

    def test_top_limits_and_ties(self):
        for page in ["tie-b", "tie-a"]:
            for _ in range(2):
                call("POST", f"/hit/{page}", client=f"t-{page}")
        status, top = call("GET", "/top?n=50")
        self.assertEqual(status, 200)
        names = [entry["page"] for entry in top]
        self.assertLess(names.index("tie-a"), names.index("tie-b"))
        self.assertEqual(call("GET", "/top?n=x")[0], 400)
