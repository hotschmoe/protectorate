package e2e

import (
	"encoding/json"
	"io"
	"net/http"
	"testing"
)

func (c *cluster) diagnosticsScenario(t *testing.T) {
	scoped := c.call("TokenService", "CreateToken", map[string]any{"name": "diagnostics", "scopes": []string{"*"}})["secret"].(string)
	for _, path := range []string{"/debug/metrics", "/debug/pprof/heap", "/debug/pprof/goroutine", "/debug/pprof/profile?seconds=1", "/debug/nodes/peer/metrics"} {
		for _, token := range []string{"", "invalid", scoped, c.token} {
			req, err := http.NewRequestWithContext(c.ctx, "GET", c.url+path, nil)
			if err != nil {
				t.Fatal(err)
			}
			req.Header.Set("Authorization", "Bearer "+token)
			res, err := c.client.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			body, err := io.ReadAll(res.Body)
			res.Body.Close()
			if err != nil {
				t.Fatal(err)
			}
			want := 403
			if token == c.token {
				want = 200
				if !c.rust && path == "/debug/nodes/peer/metrics" {
					want = 401
				}
			}
			if res.StatusCode != want {
				t.Fatalf("%s: status=%d want=%d body=%s", path, res.StatusCode, want, body)
			}
			if want == 200 && (len(body) == 0 || res.Header.Get("Cache-Control") != "no-store") {
				t.Fatalf("diagnostics %s: empty or cacheable", path)
			}
			if want == 200 && path == "/debug/metrics" {
				var values map[string]uint64
				key := "/sched/goroutines:goroutines"
				if c.rust {
					key = "/process/threads:count"
				}
				if err := json.Unmarshal(body, &values); err != nil || values[key] == 0 {
					t.Fatalf("metrics: %s %v", body, err)
				}
			}
		}
	}
}
