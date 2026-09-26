#!/bin/sh
# Applies migrations/*.sql in order to a fresh database, then runs every
# tests/*.sql against it; each test file cleans up after itself.
# Uses the PG* environment when PGHOST is set; otherwise starts a throwaway
# PostgreSQL 17 in Docker on 127.0.0.1:55432.
set -eu
if [ -z "${PGHOST:-}" ]; then
  name="pg-ledger-test-$$"
  docker run -d --rm --name "$name" -e POSTGRES_PASSWORD=test -p 127.0.0.1:55432:5432 postgres:17 >/dev/null
  trap 'docker stop "$name" >/dev/null 2>&1 || true' EXIT
  export PGHOST=127.0.0.1 PGPORT=55432 PGUSER=postgres PGPASSWORD=test
  for _ in $(seq 1 60); do pg_isready -q && break; sleep 1; done
  sleep 2
fi
db="ledger_test_$$"
psql -q -v ON_ERROR_STOP=1 -d postgres -c "CREATE DATABASE $db" >/dev/null
cleanup_db() { psql -q -d postgres -c "DROP DATABASE IF EXISTS $db" >/dev/null 2>&1 || true; }
for migration in migrations/*.sql; do
  psql -q -X -v ON_ERROR_STOP=1 -d "$db" -f "$migration" >/dev/null
done
status=0
for test in tests/*.sql; do
  if psql -q -X -v ON_ERROR_STOP=1 -d "$db" -f "$test" >/dev/null; then
    echo "ok   $test"
  else
    echo "FAIL $test"
    status=1
  fi
done
cleanup_db
[ "$status" -eq 0 ] && echo "ALL TESTS PASSED"
exit "$status"
