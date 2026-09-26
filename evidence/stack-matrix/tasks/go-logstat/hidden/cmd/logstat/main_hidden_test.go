package main

import (
	"os/exec"
	"strings"
	"testing"
)

func TestHiddenCommandPrintsTheReport(t *testing.T) {
	cmd := exec.Command("go", "run", ".", "-top", "1")
	cmd.Stdin = strings.NewReader("2026-09-26T09:00:01Z GET /a?x=1 200 7\n2026-09-26T09:00:02Z GET /b 503 9\n")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("go run: %v\n%s", err, out)
	}
	want := `{"requests":2,"malformed":0,"status_classes":{"2xx":1,"5xx":1},"top_paths":[{"path":"/a","count":1}],"p50_ms":7,"p95_ms":9,"max_ms":9,"error_rate":0.5}` + "\n"
	if string(out) != want {
		t.Errorf("output = %q\nwant     %q", out, want)
	}
}
