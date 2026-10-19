package quota

import (
	"context"
	"net/http"
	"strings"
	"time"
)

// Local probes an OpenAI-compatible endpoint. It has no quota; it reports
// reachability, served model ids and round-trip latency.
type Local struct {
	BaseURL string
	APIKey  string
	Client  *http.Client
	Now     func() time.Time
}

func NewLocal(baseURL string) *Local { return &Local{BaseURL: baseURL} }

func (l *Local) Provider() string { return "local" }

func (l *Local) modelsURL() string {
	u := strings.TrimRight(l.BaseURL, "/")
	if strings.HasSuffix(u, "/v1") {
		return u + "/models"
	}
	return u + "/v1/models"
}

func (l *Local) Read(ctx context.Context) (Snapshot, error) {
	snap := Snapshot{Provider: "local", Source: "openai-compat"}
	if l.BaseURL == "" {
		return snap, ErrUnavailable
	}
	hdr := map[string]string{"Accept": "application/json"}
	if l.APIKey != "" {
		hdr["Authorization"] = "Bearer " + l.APIKey
	}
	var out struct {
		Data []struct {
			ID string `json:"id"`
		} `json:"data"`
	}
	start := time.Now()
	err := getJSON(ctx, l.Client, l.modelsURL(), hdr, &out)
	snap.Latency = time.Since(start)
	snap.FetchedAt = nowFn(l.Now)
	if err != nil {
		return snap, err
	}
	for _, m := range out.Data {
		snap.Models = append(snap.Models, m.ID)
	}
	return snap, nil
}
