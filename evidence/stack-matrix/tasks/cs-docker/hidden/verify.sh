#!/usr/bin/env bash
# Independent acceptance for the containerised API, run on the host.
set -uo pipefail
fail() { echo "FAIL: $*"; exit 1; }
tag="sm-cs-docker-verify-$$"
cleanup() { docker rm -f "$tag" >/dev/null 2>&1; docker rmi -f "$tag" >/dev/null 2>&1; }
trap cleanup EXIT
[ -f Dockerfile ] || fail "no Dockerfile"
[ -f .dockerignore ] || fail "no .dockerignore"
[ -f compose.yaml ] || [ -f docker-compose.yml ] || [ -f compose.yml ] || fail "no compose file"
[ -f scripts/smoke.sh ] || fail "no scripts/smoke.sh"
grep -Eiq '^FROM .*dotnet/sdk:10' Dockerfile || fail "build stage is not the .NET 10 SDK image"
grep -Eiq '^FROM .*dotnet/aspnet:10' Dockerfile || fail "runtime stage is not the ASP.NET Core 10 image"
for kept in bin obj tests; do grep -Eq "(^|/|\*\*/)$kept/?\s*$" .dockerignore || fail ".dockerignore does not exclude $kept"; done
docker compose config -q || fail "compose file does not validate"
docker compose config | grep -q '/health' || fail "compose healthcheck does not probe /health"
docker build -q -t "$tag" . >/dev/null || fail "image does not build"
user=$(docker run --rm --entrypoint sh "$tag" -c 'id -u' 2>/dev/null || docker inspect -f '{{.Config.User}}' "$tag")
[ -n "$user" ] && [ "$user" != "0" ] && [ "$user" != "root" ] || fail "container runs as root ($user)"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("",0)); print(s.getsockname()[1])')
docker run -d --name "$tag" -p "$port:8080" "$tag" >/dev/null || fail "container does not start"
for _ in $(seq 1 60); do curl -fsS "http://localhost:$port/health" >/dev/null 2>&1 && break; sleep 1; done
curl -fsS "http://localhost:$port/health" | grep -q '"ok"' || fail "/health does not answer on 8080"
curl -fsS -X POST "http://localhost:$port/items" -H 'Content-Type: application/json' -d '{"name":"V","sku":"V-1","quantity":2,"price":1.5}' >/dev/null || fail "POST /items"
curl -fsS "http://localhost:$port/stats" | grep -q '"totalUnits":2' || fail "/stats does not report the item"
cleanup
SMOKE_PORT=$port bash scripts/smoke.sh || fail "the smoke script fails"
echo "ACCEPTED"
