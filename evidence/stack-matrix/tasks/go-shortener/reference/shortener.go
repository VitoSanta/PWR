package shortener

import (
	"crypto/rand"
	"encoding/json"
	"math/big"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"sync"
	"time"
)

type Options struct {
	BaseURL string
	Now     func() time.Time
	NewSlug func() string
}

type link struct {
	Slug      string     `json:"slug"`
	URL       string     `json:"url"`
	ShortURL  string     `json:"short_url"`
	ExpiresAt *time.Time `json:"-"`
	Visits    int        `json:"visits"`
}

func (l link) view() map[string]any {
	var expires any
	if l.ExpiresAt != nil {
		expires = l.ExpiresAt.UTC().Format(time.RFC3339)
	}
	return map[string]any{"slug": l.Slug, "url": l.URL, "short_url": l.ShortURL, "expires_at": expires, "visits": l.Visits}
}

var slugPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{3,32}$`)

const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"

func randomSlug() string {
	b := make([]byte, 7)
	for i := range b {
		n, _ := rand.Int(rand.Reader, big.NewInt(int64(len(alphabet))))
		b[i] = alphabet[n.Int64()]
	}
	return string(b)
}

type server struct {
	opts  Options
	mu    sync.Mutex
	links map[string]*link
}

func NewServer(opts Options) http.Handler {
	if opts.Now == nil {
		opts.Now = time.Now
	}
	if opts.NewSlug == nil {
		opts.NewSlug = randomSlug
	}
	s := &server{opts: opts, links: map[string]*link{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/links", s.create)
	mux.HandleFunc("GET /api/links", s.list)
	mux.HandleFunc("GET /api/links/{slug}", s.get)
	mux.HandleFunc("DELETE /api/links/{slug}", s.delete)
	mux.HandleFunc("GET /{slug}", s.follow)
	return mux
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}

func fail(w http.ResponseWriter, status int, message string) {
	writeJSON(w, status, map[string]string{"error": message})
}

func (s *server) create(w http.ResponseWriter, r *http.Request) {
	var body struct {
		URL        string  `json:"url"`
		Slug       *string `json:"slug"`
		TTLSeconds *int    `json:"ttl_seconds"`
	}
	decoder := json.NewDecoder(r.Body)
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&body); err != nil {
		fail(w, 400, "invalid JSON: "+err.Error())
		return
	}
	parsed, err := url.Parse(body.URL)
	if err != nil || (parsed.Scheme != "http" && parsed.Scheme != "https") || parsed.Host == "" {
		fail(w, 400, "url must be http or https with a host")
		return
	}
	if body.Slug != nil && !slugPattern.MatchString(*body.Slug) {
		fail(w, 400, "slug must match ^[A-Za-z0-9_-]{3,32}$")
		return
	}
	if body.TTLSeconds != nil && *body.TTLSeconds < 1 {
		fail(w, 400, "ttl_seconds must be at least 1")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	var slug string
	if body.Slug != nil {
		slug = *body.Slug
		if _, taken := s.links[slug]; taken {
			fail(w, 409, "slug taken")
			return
		}
	} else {
		for try := 0; try < 10; try++ {
			candidate := s.opts.NewSlug()
			if _, taken := s.links[candidate]; !taken {
				slug = candidate
				break
			}
		}
		if slug == "" {
			fail(w, 500, "could not find a free slug")
			return
		}
	}
	l := &link{Slug: slug, URL: body.URL, ShortURL: s.opts.BaseURL + "/" + slug}
	if body.TTLSeconds != nil {
		expires := s.opts.Now().Add(time.Duration(*body.TTLSeconds) * time.Second).UTC()
		l.ExpiresAt = &expires
	}
	s.links[slug] = l
	writeJSON(w, 201, l.view())
}

func (s *server) list(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	all := make([]link, 0, len(s.links))
	for _, l := range s.links {
		all = append(all, *l)
	}
	s.mu.Unlock()
	sort.Slice(all, func(i, j int) bool {
		if all[i].Visits != all[j].Visits {
			return all[i].Visits > all[j].Visits
		}
		return all[i].Slug < all[j].Slug
	})
	views := make([]map[string]any, len(all))
	for i, l := range all {
		views[i] = l.view()
	}
	writeJSON(w, 200, views)
}

func (s *server) get(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	l, ok := s.links[r.PathValue("slug")]
	if !ok {
		fail(w, 404, "not found")
		return
	}
	writeJSON(w, 200, l.view())
}

func (s *server) delete(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	slug := r.PathValue("slug")
	if _, ok := s.links[slug]; !ok {
		fail(w, 404, "not found")
		return
	}
	delete(s.links, slug)
	w.WriteHeader(204)
}

func (s *server) follow(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	l, ok := s.links[r.PathValue("slug")]
	if !ok {
		fail(w, 404, "not found")
		return
	}
	if l.ExpiresAt != nil && !s.opts.Now().Before(*l.ExpiresAt) {
		fail(w, 410, "expired")
		return
	}
	l.Visits++
	http.Redirect(w, r, l.URL, http.StatusFound)
}
