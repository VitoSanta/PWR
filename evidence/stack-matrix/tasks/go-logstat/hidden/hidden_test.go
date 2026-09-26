package logstat

import "testing"

func TestHiddenBlankLinesAreIgnoredNotMalformed(t *testing.T) {
	r := analyze(t, "\n\n2026-09-26T09:00:01Z GET /a 200 7\n   \n", 5)
	if r.Requests != 1 || r.Malformed != 0 {
		t.Errorf("Requests=%d Malformed=%d, want 1 and 0", r.Requests, r.Malformed)
	}
}

func TestHiddenStatusOutOfRangeIsMalformed(t *testing.T) {
	r := analyze(t, "2026-09-26T09:00:01Z GET /a 600 7\n2026-09-26T09:00:01Z GET /a 99 7\n", 5)
	if r.Requests != 0 || r.Malformed != 2 {
		t.Errorf("Requests=%d Malformed=%d, want 0 and 2", r.Requests, r.Malformed)
	}
}

func TestHiddenTopLimitsAndOrdersTies(t *testing.T) {
	input := "2026-09-26T09:00:01Z GET /b 200 1\n" +
		"2026-09-26T09:00:01Z GET /a 200 1\n" +
		"2026-09-26T09:00:01Z GET /c 200 1\n" +
		"2026-09-26T09:00:01Z GET /c 200 1\n"
	r := analyze(t, input, 2)
	want := []PathCount{{"/c", 2}, {"/a", 1}}
	if len(r.TopPaths) != 2 || r.TopPaths[0] != want[0] || r.TopPaths[1] != want[1] {
		t.Errorf("TopPaths = %v, want %v", r.TopPaths, want)
	}
}
