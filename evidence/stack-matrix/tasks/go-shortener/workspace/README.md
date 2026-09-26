# shortener

A URL shortener as an `http.Handler`, standard library only (Go 1.23+),
package `shortener` at the module root.

```go
type Options struct {
    BaseURL string           // e.g. "https://sho.rt"; short URLs are BaseURL + "/" + slug
    Now     func() time.Time // nil: time.Now
    NewSlug func() string    // nil: 7 random characters from [A-Za-z0-9]
}

func NewServer(opts Options) http.Handler
```

Each `NewServer` has its own in-memory store, safe for concurrent use.

## A link

```json
{"slug": "docs", "url": "https://go.dev/doc/", "short_url": "https://sho.rt/docs", "expires_at": null, "visits": 0}
```

`expires_at` is RFC 3339 in UTC (`2026-09-26T10:00:00Z`) or `null`.

## Endpoints

| Request | Response |
|---|---|
| `POST /api/links` with `{"url": ..., "slug"?: ..., "ttl_seconds"?: n}` | `201` and the link. `400` for a body that is not JSON or has unknown fields, a URL that is not `http`/`https` with a host, a slug not matching `^[A-Za-z0-9_-]{3,32}$`, or a `ttl_seconds` below 1. `409` when the slug is taken. Without a slug, `NewSlug` is called until it gives one that is free (at most 10 tries, then `500`). |
| `GET /{slug}` | `302` to the URL, counting a visit. `404` when there is no such link, `410` from the moment it expires (`now >= expires_at`); neither counts. |
| `GET /api/links/{slug}` | `200` and the link, or `404` |
| `DELETE /api/links/{slug}` | `204`, or `404` |
| `GET /api/links` | `200`, every link, most visited first, then by slug |

Errors are JSON `{"error": "..."}` with `Content-Type: application/json`, as
is every link. A method the path does not support is `405` with an `Allow`
header.

Run the tests with `go test ./...`.
