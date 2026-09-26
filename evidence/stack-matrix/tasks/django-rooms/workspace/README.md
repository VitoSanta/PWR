# rooms

Meeting-room bookings: a Django 5 app `rooms` (the project is `config`,
SQLite). Write the app -- models, migrations, views, `rooms/urls.py` -- so
that `python manage.py test` passes. JSON in, JSON out; no Django REST
Framework, no forms library, no templates.

## Models

- `Room`: `name` (unique, up to 60 characters), `capacity` (positive integer).
- `Booking`: `room` (deleting a room deletes its bookings), `title` (1-120
  characters), `start` and `end` (aware datetimes), `attendees` (positive
  integer).

## Rules

A booking is valid when `start < end`, it lasts at most 8 hours, starts and
ends on a quarter hour (minutes 0, 15, 30, 45; seconds 0), `attendees` does
not exceed the room's capacity, and it does not overlap another booking of
the same room (touching is fine: one may end at 10:00 and the next start at
10:00).

## API (under `/api/`)

Datetimes are ISO 8601 with an offset (`2026-10-01T09:00:00+00:00`); a
naive one is invalid. Responses give them in UTC with `Z`
(`2026-10-01T09:00:00Z`).

| Request | Response |
|---|---|
| `POST /api/rooms` `{"name", "capacity"}` | `201` `{"id", "name", "capacity"}`; `400` invalid; `409` duplicate name |
| `GET /api/rooms` | `200`, rooms by name |
| `POST /api/rooms/<id>/bookings` `{"title", "start", "end", "attendees"}` | `201` `{"id", "room", "title", "start", "end", "attendees"}` (`room` is the id); `404` unknown room; `400` invalid; `409` overlapping, with `{"error": "overlap", "conflicts": [ids]}` |
| `GET /api/rooms/<id>/bookings?date=YYYY-MM-DD` | `200`, that room's bookings sorted by start; with `date`, only those overlapping that UTC day |
| `DELETE /api/bookings/<id>` | `204`, or `404` |
| `GET /api/rooms/available?start=...&end=...&attendees=n` | `200`, rooms big enough and free for the whole interval, by name; `400` if the parameters are invalid |

Errors other than the overlap are `{"error": "<message>"}`; a `400` for
invalid fields is `{"error": "invalid", "fields": {"<field>": "<message>", ...}}`
naming every invalid field. A body that is not a JSON object is `400`.
Requests have no CSRF token (tests post JSON directly), so the views must not
require one.

Set up with `python3 -m venv .venv && .venv/bin/pip install -r requirements.txt`,
test with `.venv/bin/python manage.py test`.
