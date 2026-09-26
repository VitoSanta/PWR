# bookmarks

A small bookmarks service: FastAPI, Python 3.11+, storage in SQLite through
the standard library's `sqlite3`. `app/main.py` exposes `create_app(db_path:
str = ":memory:") -> FastAPI`; every call returns an independent application
with its own empty database.

## Resource

```json
{"id": 1, "url": "https://example.com/a", "title": "example.com", "tags": ["python", "web"], "created": 1}
```

- `id`: assigned by the server, 1, 2, 3... never reused.
- `url`: required, must start with `http://` or `https://` and have a host;
  otherwise `422`.
- `title`: optional; when missing or blank it is the URL's host.
- `tags`: optional list; stored trimmed, lower-cased, without duplicates or
  empty strings, sorted.
- `created`: a sequence number, the same as the order of creation (1, 2...).

## Endpoints

| Request | Response |
|---|---|
| `POST /bookmarks` | `201` and the bookmark; `409` with `{"detail": "duplicate url", "id": <existing id>}` if the URL is already saved |
| `GET /bookmarks` | `200`, newest first. `?tag=` keeps bookmarks with that tag (case-insensitive); `?q=` keeps those whose title or URL contains the text (case-insensitive); both combine. `?limit=` (default 20, 1-100) and `?offset=` (default 0) page the result; the header `X-Total-Count` is the number before paging. Out-of-range `limit`/`offset` is `422`. |
| `GET /bookmarks/{id}` | `200`, or `404` |
| `PATCH /bookmarks/{id}` | changes only the fields given (`url`, `title`, `tags`), with the same rules; `200` with the bookmark, `404`, `409` if the new URL belongs to another bookmark, `422` if invalid |
| `DELETE /bookmarks/{id}` | `204`, or `404` |
| `GET /tags` | `200`, `[{"tag": "python", "count": 2}, ...]` for every tag in use, most used first, ties by tag |

Errors that are not validation errors have a JSON body with `detail`.

Set up with `python3 -m venv .venv && .venv/bin/pip install -r requirements.txt`,
test with `.venv/bin/python -m pytest -q`.
