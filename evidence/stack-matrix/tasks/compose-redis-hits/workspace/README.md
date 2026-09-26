# hits

A page-hit counter service with Redis, run with Docker Compose. Build it:

- `app/`: the HTTP service, in Python 3.12+ (use whatever framework you like;
  declare dependencies in `app/requirements.txt`). It reads Redis from
  `REDIS_URL` (default `redis://redis:6379/0`) and listens on port 8000.
- `app/Dockerfile`: the service's image.
- `compose.yaml`: services `app` (built from `app/`) and `redis` (the official
  `redis:7` image, with its data on a named volume and append-only
  persistence on). `app` publishes container port 8000 on host port
  `${HITS_PORT:-8000}`, starts after `redis` is healthy, and has a
  healthcheck on `/health`.

## API

All JSON.

| Request | Response |
|---|---|
| `GET /health` | `200 {"status": "ok", "redis": "ok"}`; `503` with `"redis": "down"` when Redis does not answer |
| `POST /hit/{page}` | counts one hit for `page` (1-64 of `[a-z0-9-]`, else `400 {"error": ...}`) and answers `200 {"page": page, "count": n}` with the page's new total |
| `GET /count/{page}` | `200 {"page": page, "count": n}` (0 for a page never hit) |
| `GET /top?n=3` | `200 [{"page": ..., "count": ...}, ...]`: the `n` (1-50, default 3, else `400`) most hit pages, most first, ties by page name |

**Rate limit**: requests to `POST /hit/...` carry a header `X-Client-Id`
(missing: `400`). Each client may make 5 hits per fixed 60-second window
(the window starts with the client's first hit); the sixth answers `429
{"error": "rate limited", "retry_after": s}` with `s` the whole seconds
left in the window (at least 1), and does not count.

Counts live in Redis, so they survive a restart of `app`.

The owner's end-to-end test is `sh scripts/e2e.sh`: it builds and starts the
stack on a free port, runs `tests/e2e.py` against it (restarting `app` in the
middle), and takes the stack down.
