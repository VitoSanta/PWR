# logstat

Reads an HTTP access log and reports what happened. A Go library (package
`logstat`, module `logstat`) plus a command, `cmd/logstat`, that reads the log
on stdin and prints the report as JSON. Standard library only.

## Log format

One request per line, five fields separated by spaces:

    <time RFC 3339> <method> <path> <status> <duration_ms>

A line is malformed (counted, then skipped) unless it has exactly five fields,
a valid RFC 3339 time, an integer status between 100 and 599 and a
non-negative integer duration. Blank lines are ignored.

## Library

```go
func Analyze(r io.Reader, top int) (Report, error)
func (r Report) JSON() ([]byte, error)

type PathCount struct {
    Path  string `json:"path"`
    Count int    `json:"count"`
}

type Report struct {
    Requests      int            `json:"requests"`
    Malformed     int            `json:"malformed"`
    StatusClasses map[string]int `json:"status_classes"` // "2xx", "4xx", ...
    TopPaths      []PathCount    `json:"top_paths"`
    P50Ms         int            `json:"p50_ms"`
    P95Ms         int            `json:"p95_ms"`
    MaxMs         int            `json:"max_ms"`
    ErrorRate     float64        `json:"error_rate"`
}
```

- Paths are counted without their query string (`/a?page=2` is `/a`).
- `TopPaths`: the `top` most requested paths, most requests first, ties by path
  in ascending order.
- Percentiles use the nearest-rank method on the durations of valid requests.
- `ErrorRate`: 5xx responses divided by requests; 0 when there are none.
- `JSON()` writes the fields in the order above.

## Command

    go run ./cmd/logstat -top 5 < access.log

Prints `Report.JSON()` followed by a newline. The acceptance tests are in
`logstat_test.go`; run them with `go test ./...`.
