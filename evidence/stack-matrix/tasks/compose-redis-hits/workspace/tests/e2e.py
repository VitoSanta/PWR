"""End-to-end tests against the running stack (see scripts/e2e.sh)."""

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


class EndToEnd(unittest.TestCase):
    def test_1_health(self):
        self.assertEqual(call("GET", "/health"), (200, {"status": "ok", "redis": "ok"}))

    def test_2_counting_and_top(self):
        for page, hits in [("home", 3), ("about", 1), ("blog", 3), ("zeta", 2)]:
            for i in range(hits):
                status, body = call("POST", f"/hit/{page}", client=f"c-{page}")
                self.assertEqual(status, 200, body)
            self.assertEqual(body, {"page": page, "count": hits})
        self.assertEqual(call("GET", "/count/home"), (200, {"page": "home", "count": 3}))
        self.assertEqual(call("GET", "/count/never"), (200, {"page": "never", "count": 0}))
        status, top = call("GET", "/top?n=3")
        self.assertEqual(status, 200)
        self.assertEqual(top, [{"page": "blog", "count": 3}, {"page": "home", "count": 3}, {"page": "zeta", "count": 2}])
        self.assertEqual(call("GET", "/top")[1][0], {"page": "blog", "count": 3})

    def test_3_validation(self):
        self.assertEqual(call("POST", "/hit/Bad_Page", client="v")[0], 400)
        self.assertEqual(call("POST", "/hit/ok")[0], 400)
        self.assertEqual(call("GET", "/top?n=0")[0], 400)
        self.assertEqual(call("GET", "/top?n=51")[0], 400)

    def test_4_rate_limit(self):
        for _ in range(5):
            self.assertEqual(call("POST", "/hit/limited", client="greedy")[0], 200)
        status, body = call("POST", "/hit/limited", client="greedy")
        self.assertEqual(status, 429)
        self.assertEqual(body["error"], "rate limited")
        self.assertTrue(1 <= body["retry_after"] <= 60, body)
        self.assertEqual(call("GET", "/count/limited")[1]["count"], 5)
        self.assertEqual(call("POST", "/hit/limited", client="patient")[0], 200)

    def test_5_counts_survive_an_app_restart(self):
        before = call("GET", "/count/home")[1]["count"]
        subprocess.run(["docker", "compose", "-p", PROJECT, "restart", "app"], check=True, capture_output=True)
        deadline = time.time() + 60
        while time.time() < deadline:
            try:
                if call("GET", "/health")[0] == 200:
                    break
            except OSError:
                pass
            time.sleep(1)
        self.assertEqual(call("GET", "/count/home")[1]["count"], before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
