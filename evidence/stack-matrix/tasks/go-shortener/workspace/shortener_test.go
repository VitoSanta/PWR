package shortener

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

type clock struct{ now time.Time }

func (c *clock) Now() time.Time { return c.now }

func newTest(slugs ...string) (http.Handler, *clock) {
	c := &clock{now: time.Date(2026, 9, 26, 10, 0, 0, 0, time.UTC)}
	next := 0
	h := NewServer(Options{BaseURL: "https://sho.rt", Now: c.Now, NewSlug: func() string {
		s := slugs[next%len(slugs)]
		next++
		return s
	}})
	return h, c
}

func do(h http.Handler, method, path, body string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(method, path, strings.NewReader(body))
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func decode(t *testing.T, rec *httptest.ResponseRecorder) map[string]any {
	t.Helper()
	var out map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("not JSON: %q", rec.Body.String())
	}
	return out
}

func TestCreateWithSlugAndGenerated(t *testing.T) {
	h, _ := newTest("abc1234")
	rec := do(h, "POST", "/api/links", `{"url":"https://go.dev/doc/","slug":"docs"}`)
	if rec.Code != 201 {
		t.Fatalf("status %d: %s", rec.Code, rec.Body)
	}
	if ct := rec.Header().Get("Content-Type"); !strings.HasPrefix(ct, "application/json") {
		t.Errorf("content type %q", ct)
	}
	got := decode(t, rec)
	want := map[string]any{"slug": "docs", "url": "https://go.dev/doc/", "short_url": "https://sho.rt/docs", "expires_at": nil, "visits": float64(0)}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("%s = %v, want %v", k, got[k], v)
		}
	}
	gen := decode(t, do(h, "POST", "/api/links", `{"url":"http://example.com"}`))
	if gen["slug"] != "abc1234" {
		t.Errorf("generated slug %v", gen["slug"])
	}
}

func TestInvalidBodies(t *testing.T) {
	h, _ := newTest("zzzzzzz")
	for _, body := range []string{`nope`, `{"url":"ftp://x.org"}`, `{"url":"https://"}`, `{"url":"https://a.b","slug":"a b"}`, `{"url":"https://a.b","slug":"ab"}`, `{"url":"https://a.b","ttl_seconds":0}`, `{"url":"https://a.b","extra":1}`} {
		rec := do(h, "POST", "/api/links", body)
		if rec.Code != 400 {
			t.Errorf("%s: status %d", body, rec.Code)
			continue
		}
		if decode(t, rec)["error"] == "" {
			t.Errorf("%s: no error message", body)
		}
	}
}

func TestSlugTaken(t *testing.T) {
	h, _ := newTest("taken01", "taken01", "free001")
	do(h, "POST", "/api/links", `{"url":"https://a.b","slug":"taken01"}`)
	if rec := do(h, "POST", "/api/links", `{"url":"https://c.d","slug":"taken01"}`); rec.Code != 409 {
		t.Errorf("status %d", rec.Code)
	}
	if got := decode(t, do(h, "POST", "/api/links", `{"url":"https://c.d"}`))["slug"]; got != "free001" {
		t.Errorf("generated slug after collisions = %v", got)
	}
}

func TestRedirectCountsVisits(t *testing.T) {
	h, _ := newTest("x")
	do(h, "POST", "/api/links", `{"url":"https://go.dev/","slug":"go-home"}`)
	for i := 0; i < 3; i++ {
		rec := do(h, "GET", "/go-home", "")
		if rec.Code != 302 || rec.Header().Get("Location") != "https://go.dev/" {
			t.Fatalf("redirect: %d %q", rec.Code, rec.Header().Get("Location"))
		}
	}
	if v := decode(t, do(h, "GET", "/api/links/go-home", ""))["visits"]; v != float64(3) {
		t.Errorf("visits = %v", v)
	}
	if rec := do(h, "GET", "/nothing", ""); rec.Code != 404 {
		t.Errorf("unknown slug: %d", rec.Code)
	}
}

func TestExpiry(t *testing.T) {
	h, c := newTest("x")
	link := decode(t, do(h, "POST", "/api/links", `{"url":"https://a.b","slug":"soon","ttl_seconds":60}`))
	if link["expires_at"] != "2026-09-26T10:01:00Z" {
		t.Errorf("expires_at = %v", link["expires_at"])
	}
	c.now = c.now.Add(59 * time.Second)
	if rec := do(h, "GET", "/soon", ""); rec.Code != 302 {
		t.Errorf("before expiry: %d", rec.Code)
	}
	c.now = c.now.Add(time.Second)
	if rec := do(h, "GET", "/soon", ""); rec.Code != 410 {
		t.Errorf("at expiry: %d", rec.Code)
	}
	if v := decode(t, do(h, "GET", "/api/links/soon", ""))["visits"]; v != float64(1) {
		t.Errorf("visits = %v", v)
	}
}

func TestListDeleteAndMethods(t *testing.T) {
	h, _ := newTest("x")
	for _, slug := range []string{"bbb", "aaa", "ccc"} {
		do(h, "POST", "/api/links", `{"url":"https://a.b/`+slug+`","slug":"`+slug+`"}`)
	}
	do(h, "GET", "/ccc", "")
	var list []map[string]any
	rec := do(h, "GET", "/api/links", "")
	if err := json.Unmarshal(rec.Body.Bytes(), &list); err != nil {
		t.Fatal(err)
	}
	var slugs []string
	for _, l := range list {
		slugs = append(slugs, l["slug"].(string))
	}
	if strings.Join(slugs, ",") != "ccc,aaa,bbb" {
		t.Errorf("order %v", slugs)
	}
	if rec := do(h, "DELETE", "/api/links/aaa", ""); rec.Code != 204 {
		t.Errorf("delete: %d", rec.Code)
	}
	if rec := do(h, "DELETE", "/api/links/aaa", ""); rec.Code != 404 {
		t.Errorf("delete again: %d", rec.Code)
	}
	rec = do(h, "PUT", "/api/links", "")
	if rec.Code != 405 || rec.Header().Get("Allow") == "" {
		t.Errorf("PUT: %d allow=%q", rec.Code, rec.Header().Get("Allow"))
	}
}
