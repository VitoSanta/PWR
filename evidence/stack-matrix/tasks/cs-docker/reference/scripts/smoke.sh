#!/usr/bin/env bash
set -euo pipefail
image=inventory-api-smoke
port=${SMOKE_PORT:-18080}
docker build -t "$image" .
id=$(docker run -d -p "$port:8080" "$image")
trap 'docker rm -f "$id" >/dev/null' EXIT
for _ in $(seq 1 60); do
  curl -fsS "http://localhost:$port/health" >/dev/null 2>&1 && break
  sleep 1
done
curl -fsS "http://localhost:$port/health" | grep -q '"ok"'
curl -fsS -X POST "http://localhost:$port/items" -H 'Content-Type: application/json' \
  -d '{"name":"Smoke","sku":"SMOKE-1","quantity":3,"price":2.5}' >/dev/null
curl -fsS "http://localhost:$port/stats" | grep -q '"totalItems":1'
echo "smoke: ok"
