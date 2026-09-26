#!/bin/sh
# Builds and starts the stack under a throwaway project name, runs the
# end-to-end tests against it, and always takes it down again.
set -eu
project="hits-e2e-$$"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
export HITS_PORT="$port"
down() { docker compose -p "$project" down -v --remove-orphans >/dev/null 2>&1 || true; }
trap down EXIT
docker compose -p "$project" up -d --build --wait --wait-timeout 180
HITS_URL="http://127.0.0.1:$port" HITS_PROJECT="$project" python3 tests/e2e.py
