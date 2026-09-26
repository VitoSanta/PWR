package logstat

import (
	"bufio"
	"encoding/json"
	"io"
	"sort"
	"strconv"
	"strings"
	"time"
)

type PathCount struct {
	Path  string `json:"path"`
	Count int    `json:"count"`
}

type Report struct {
	Requests      int            `json:"requests"`
	Malformed     int            `json:"malformed"`
	StatusClasses map[string]int `json:"status_classes"`
	TopPaths      []PathCount    `json:"top_paths"`
	P50Ms         int            `json:"p50_ms"`
	P95Ms         int            `json:"p95_ms"`
	MaxMs         int            `json:"max_ms"`
	ErrorRate     float64        `json:"error_rate"`
}

func Analyze(r io.Reader, top int) (Report, error) {
	rep := Report{StatusClasses: map[string]int{}, TopPaths: []PathCount{}}
	counts := map[string]int{}
	var durations []int
	server := 0
	sc := bufio.NewScanner(r)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}
		f := strings.Fields(line)
		if len(f) != 5 {
			rep.Malformed++
			continue
		}
		if _, err := time.Parse(time.RFC3339, f[0]); err != nil {
			rep.Malformed++
			continue
		}
		status, err1 := strconv.Atoi(f[3])
		dur, err2 := strconv.Atoi(f[4])
		if err1 != nil || err2 != nil || dur < 0 || status < 100 || status > 599 {
			rep.Malformed++
			continue
		}
		rep.Requests++
		rep.StatusClasses[strconv.Itoa(status/100)+"xx"]++
		if status >= 500 {
			server++
		}
		path := f[2]
		if i := strings.IndexByte(path, '?'); i >= 0 {
			path = path[:i]
		}
		counts[path]++
		durations = append(durations, dur)
	}
	if err := sc.Err(); err != nil {
		return rep, err
	}
	for p, c := range counts {
		rep.TopPaths = append(rep.TopPaths, PathCount{p, c})
	}
	sort.Slice(rep.TopPaths, func(i, j int) bool {
		a, b := rep.TopPaths[i], rep.TopPaths[j]
		if a.Count != b.Count {
			return a.Count > b.Count
		}
		return a.Path < b.Path
	})
	if len(rep.TopPaths) > top {
		rep.TopPaths = rep.TopPaths[:top]
	}
	if n := len(durations); n > 0 {
		sort.Ints(durations)
		rank := func(p float64) int {
			k := int(float64(n)*p + 0.999999999)
			if k < 1 {
				k = 1
			}
			return durations[k-1]
		}
		rep.P50Ms, rep.P95Ms, rep.MaxMs = rank(0.50), rank(0.95), durations[n-1]
		rep.ErrorRate = float64(server) / float64(rep.Requests)
	}
	return rep, nil
}

func (r Report) JSON() ([]byte, error) { return json.Marshal(r) }
