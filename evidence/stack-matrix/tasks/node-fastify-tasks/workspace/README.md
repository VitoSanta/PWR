# tasks-api

A task-tracking HTTP API on Fastify 5 (Node 22+, ES modules, no database: an
in-memory store per app). `src/app.js` exports `buildApp()`, which returns a
new Fastify instance with its own empty store; tests use `app.inject`.

## Resource

`{"id": 1, "title": "Write docs", "status": "todo", "priority": 2, "due": "2026-10-01", "labels": ["docs"], "version": 1}`

- `id`: assigned, 1, 2, 3...; never reused.
- `title`: required string, 1-200 characters after trimming (stored trimmed).
- `status`: `todo` (default), `doing` or `done`.
- `priority`: integer 1 (highest) to 4, default 3.
- `due`: optional `YYYY-MM-DD` that is a real date, or `null`.
- `labels`: array of strings, default `[]`, stored lower-cased, unique, in
  the order first given.
- `version`: starts at 1, goes up by one on every change.

Unknown properties in a body are a `400`. Validation errors answer `400` with
`{"error": "validation", "details": [...]}` -- `details` lists a message per
problem (at least one).

## Endpoints

| Request | Response |
|---|---|
| `POST /tasks` | `201` with the task and `Location: /tasks/{id}` |
| `GET /tasks` | `200`, sorted by priority, then `due` (no due date last), then id. Filters: `?status=`, `?label=`, `?overdue=true` (due before `?today=YYYY-MM-DD`, or before the real today when not given, and not done) |
| `GET /tasks/{id}` | `200` or `404` `{"error": "not found"}` |
| `PATCH /tasks/{id}` | changes the given fields; requires the header `If-Match: <version>`: missing is `428`, a stale version is `409` with the current task as `{"error": "conflict", "current": {...}}` |
| `POST /tasks/{id}/transition` | body `{"to": "doing"}`: allowed moves are todo→doing, doing→done, doing→todo, done→todo; any other is `422` `{"error": "invalid transition", "from": ..., "to": ...}` |
| `DELETE /tasks/{id}` | `204` or `404` |
| `GET /stats` | `200 {"total": n, "byStatus": {"todo": n, "doing": n, "done": n}, "overdue": n}` (overdue as above, with the same `?today=`) |

Run the tests with `npm test` after `npm install`.
