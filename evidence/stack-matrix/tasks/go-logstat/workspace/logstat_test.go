package logstat

import (
	"strings"
	"testing"
)

// Each line: <time RFC3339> <method> <path> <status> <duration_ms>
const sample = `2026-09-26T09:00:01Z GET /api/items 200 12
2026-09-26T09:00:02Z GET /api/items 200 18
2026-09-26T09:00:03Z POST /api/items 201 40
2026-09-26T09:00:04Z GET /health 200 1
2026-09-26T09:00:05Z GET /api/items/7 404 5
2026-09-26T09:00:06Z GET /api/items 500 250
2026-09-26T09:00:07Z GET /api/items?page=2 200 20
this line is not a request
2026-09-26T09:00:09Z DELETE /api/items/7 204 9
2026-09-26T09:00:10Z GET /api/items 200 30
`

func analyze(t *testing.T, input string, top int) Report {
	t.Helper()
	report, err := Analyze(strings.NewReader(input), top)
	if err != nil {
		t.Fatalf("Analyze: %v", err)
	}
	return report
}

func TestCountsRequestsAndSkipsMalformedLines(t *testing.T) {
	r := analyze(t, sample, 3)
	if r.Requests != 9 {
		t.Errorf("Requests = %d, want 9", r.Requests)
	}
	if r.Malformed != 1 {
		t.Errorf("Malformed = %d, want 1", r.Malformed)
	}
}

func TestCountsByStatusClass(t *testing.T) {
	r := analyze(t, sample, 3)
	want := map[string]int{"2xx": 7, "4xx": 1, "5xx": 1}
	for class, n := range want {
		if r.StatusClasses[class] != n {
			t.Errorf("StatusClasses[%s] = %d, want %d", class, r.StatusClasses[class], n)
		}
	}
	if len(r.StatusClasses) != len(want) {
		t.Errorf("StatusClasses = %v, want only %v", r.StatusClasses, want)
	}
}

func TestTopPathsIgnoreQueryAndBreakTiesByPath(t *testing.T) {
	r := analyze(t, sample, 3)
	want := []PathCount{{"/api/items", 6}, {"/api/items/7", 2}, {"/health", 1}}
	if len(r.TopPaths) != len(want) {
		t.Fatalf("TopPaths = %v, want %v", r.TopPaths, want)
	}
	for i := range want {
		if r.TopPaths[i] != want[i] {
			t.Errorf("TopPaths[%d] = %v, want %v", i, r.TopPaths[i], want[i])
		}
	}
}

func TestPercentilesUseNearestRank(t *testing.T) {
	r := analyze(t, sample, 3)
	// Sorted durations: 1 5 9 12 18 20 30 40 250 (nine values).
	if r.P50Ms != 18 {
		t.Errorf("P50Ms = %d, want 18", r.P50Ms)
	}
	if r.P95Ms != 250 {
		t.Errorf("P95Ms = %d, want 250", r.P95Ms)
	}
	if r.MaxMs != 250 {
		t.Errorf("MaxMs = %d, want 250", r.MaxMs)
	}
}

func TestErrorRateIsServerErrorsOverRequests(t *testing.T) {
	r := analyze(t, sample, 3)
	if r.ErrorRate < 0.1111 || r.ErrorRate > 0.1112 {
		t.Errorf("ErrorRate = %f, want 1/9", r.ErrorRate)
	}
}

func TestEmptyInput(t *testing.T) {
	r := analyze(t, "", 5)
	if r.Requests != 0 || r.Malformed != 0 || len(r.TopPaths) != 0 || r.P95Ms != 0 || r.ErrorRate != 0 {
		t.Errorf("empty input gave %+v", r)
	}
}

func TestInvalidFieldsAreMalformed(t *testing.T) {
	input := "2026-09-26T09:00:01Z GET /a abc 12\n" + // status not a number
		"yesterday GET /a 200 12\n" + // time not RFC 3339
		"2026-09-26T09:00:01Z GET /a 200 -3\n" + // negative duration
		"2026-09-26T09:00:01Z GET /a 200 7\n"
	r := analyze(t, input, 5)
	if r.Requests != 1 || r.Malformed != 3 {
		t.Errorf("Requests=%d Malformed=%d, want 1 and 3", r.Requests, r.Malformed)
	}
}

func TestJSONOutput(t *testing.T) {
	r := analyze(t, "2026-09-26T09:00:01Z GET /a 200 7\n", 5)
	got, err := r.JSON()
	if err != nil {
		t.Fatal(err)
	}
	want := `{"requests":1,"malformed":0,"status_classes":{"2xx":1},"top_paths":[{"path":"/a","count":1}],"p50_ms":7,"p95_ms":7,"max_ms":7,"error_rate":0}`
	if string(got) != want {
		t.Errorf("JSON = %s\nwant    %s", got, want)
	}
}
