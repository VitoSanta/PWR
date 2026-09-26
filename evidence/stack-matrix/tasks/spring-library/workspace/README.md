# library

A small lending library's API: Spring Boot 3.5, Java 21, Spring Data JPA on
an in-memory H2 database, Bean Validation. Package `com.example.library`.
Everything is JSON.

Today's date comes from a `java.time.Clock` bean (the tests replace it with
a fixed one), in UTC. Define one for the application: `Clock.systemUTC()`.

## Books

| Request | Response |
|---|---|
| `POST /api/books` `{"isbn", "title", "author", "copies"}` | `201` with the book, `{"isbn", "title", "author", "copies", "available"}` (`available` starts at `copies`) |
| `GET /api/books` | `200`, all books by title; `?author=` keeps those whose author contains the text, ignoring case |
| `GET /api/books/{isbn}` | `200` or `404` |

`isbn` is 13 digits, `title` and `author` are required and not blank,
`copies` is 1 to 100. Invalid input is `400` with
`{"errors": {"<field>": "<message>", ...}}`, one entry per invalid field. A
duplicate `isbn` is `409` `{"error": "duplicate isbn"}`.

## Loans

| Request | Response |
|---|---|
| `POST /api/loans` `{"isbn", "member"}` | `201` `{"id", "isbn", "member", "loanedOn", "dueOn", "returnedOn": null}` -- due 14 days after today; dates `YYYY-MM-DD` |
| `POST /api/loans/{id}/return` | `200` with the loan, `returnedOn` today |
| `GET /api/members/{member}/loans` | `200`, the member's loans not yet returned, by `dueOn` then `id`, each with `"overdue": true/false` (due before today) |

A loan needs a free copy (`available` > 0, else `409` `{"error": "no copies
available"}`), a member may hold 3 unreturned loans (a fourth is `409`
`{"error": "loan limit reached"}`), and a member with an overdue loan may not
borrow (`409` `{"error": "overdue loans"}`). An unknown book or loan is
`404` `{"error": "..."}`; returning a returned loan is `409` `{"error":
"already returned"}`. `member` is required and not blank (`400` as above).
Lending takes a copy, returning gives it back.

Run the tests with `mvn -q test`.
