#!/bin/sh
# Independent acceptance: the compose file's shape, the owner's end-to-end
# tests, then the hidden ones, against a stack started here.
set -u
fail() { echo "FAIL: $*"; exit 1; }
[ -f compose.yaml ] || [ -f docker-compose.yml ] || fail "no compose file"
[ -f app/Dockerfile ] || fail "no app/Dockerfile"
config=$(HITS_PORT=18123 docker compose config --format json) || fail "compose config does not validate"
echo "$config" | python3 -c '
import json, sys
c = json.load(sys.stdin)
s = c["services"]
assert "app" in s and "redis" in s, "services app and redis"
r, a = s["redis"], s["app"]
assert r.get("image", "").startswith("redis:7"), "redis image " + r.get("image", "")
command = " ".join(r.get("command") or [])
assert "appendonly yes" in command.replace("--appendonly", "appendonly"), "append-only not on: " + command
vols = [v for v in r.get("volumes", []) if v.get("type") == "volume"]
assert vols, "redis data is not on a named volume"
dep = a.get("depends_on", {}).get("redis", {})
assert dep.get("condition") == "service_healthy", "app does not wait for a healthy redis"
assert a.get("healthcheck", {}).get("test"), "app has no healthcheck"
ports = a.get("ports", [])
assert any(str(p.get("target")) == "8000" and str(p.get("published")) == "18123" for p in ports), "port mapping " + json.dumps(ports)
' || fail "compose file"
sh scripts/e2e.sh || fail "the owner's end-to-end tests"
project="hits-hidden-$$"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
trap 'docker compose -p "$project" down -v --remove-orphans >/dev/null 2>&1' EXIT
HITS_PORT=$port docker compose -p "$project" up -d --build --wait --wait-timeout 180 >/dev/null 2>&1 || fail "stack does not come up"
HITS_URL="http://127.0.0.1:$port" HITS_PROJECT="$project" python3 tests/hidden_e2e.py || fail "hidden end-to-end tests"
echo "ACCEPTED"
