package shortener

import (
	"net/http"
	"regexp"
	"strings"
	"sync"
	"testing"
)

func TestHiddenConcurrentVisits(t *testing.T) {
	h, _ := newTest("x")
	do(h, "POST", "/api/links", `{"url":"https://a.b","slug":"busy"}`)
	var wg sync.WaitGroup
	for i := 0; i < 200; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); do(h, "GET", "/busy", "") }()
	}
	wg.Wait()
	if v := decode(t, do(h, "GET", "/api/links/busy", ""))["visits"]; v != float64(200) {
		t.Errorf("visits = %v", v)
	}
}

func TestHiddenDefaultSlugs(t *testing.T) {
	h := NewServer(Options{BaseURL: "http://s"})
	seen := map[string]bool{}
	for i := 0; i < 50; i++ {
		slug := decode(t, do(h, "POST", "/api/links", `{"url":"https://a.b/`+strings.Repeat("x", i)+`"}`))["slug"].(string)
		if !regexp.MustCompile(`^[A-Za-z0-9]{7}$`).MatchString(slug) || seen[slug] {
			t.Fatalf("slug %q", slug)
		}
		seen[slug] = true
	}
}

func TestHiddenTooManyCollisions(t *testing.T) {
	h, _ := newTest("same000")
	do(h, "POST", "/api/links", `{"url":"https://a.b"}`)
	if rec := do(h, "POST", "/api/links", `{"url":"https://c.d"}`); rec.Code != 500 {
		t.Errorf("status %d", rec.Code)
	}
}

func TestHiddenErrorsAreJSON(t *testing.T) {
	h, _ := newTest("x")
	rec := do(h, "GET", "/api/links/none", "")
	if rec.Code != http.StatusNotFound || !strings.HasPrefix(rec.Header().Get("Content-Type"), "application/json") {
		t.Errorf("%d %q", rec.Code, rec.Header().Get("Content-Type"))
	}
	if decode(t, rec)["error"] == nil {
		t.Error("no error field")
	}
}

func TestHiddenExpiredDoesNotCount(t *testing.T) {
	h, c := newTest("x")
	do(h, "POST", "/api/links", `{"url":"https://a.b","slug":"old","ttl_seconds":1}`)
	c.now = c.now.Add(5e9)
	do(h, "GET", "/old", "")
	if v := decode(t, do(h, "GET", "/api/links/old", ""))["visits"]; v != float64(0) {
		t.Errorf("visits = %v", v)
	}
}
