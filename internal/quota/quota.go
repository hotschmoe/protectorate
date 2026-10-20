// Package quota reads subscription usage windows and reset credits from
// provider accounts using read-only calls and locally stored credentials.
package quota

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"
)

var (
	ErrExpired      = errors.New("credentials expired; re-login with the provider CLI")
	ErrUnauthorized = errors.New("credentials rejected (HTTP 401)")
	ErrScope        = errors.New("credentials lack the scope required for usage queries")
	ErrUnavailable  = errors.New("source unavailable")
)

type Window struct {
	Name        string    `json:"name"`
	UsedPercent float64   `json:"used_percent"`
	ResetsAt    time.Time `json:"resets_at,omitzero"`
}

type Reset struct {
	Kind      string    `json:"kind"`
	Count     int       `json:"count"`
	ExpiresAt time.Time `json:"expires_at,omitzero"`
}

type Snapshot struct {
	Provider  string        `json:"provider"`
	Plan      string        `json:"plan,omitempty"`
	Source    string        `json:"source,omitempty"`
	Windows   []Window      `json:"windows,omitempty"`
	Resets    []Reset       `json:"resets,omitempty"`
	Models    []string      `json:"models,omitempty"`
	Latency   time.Duration `json:"latency,omitempty"`
	FetchedAt time.Time     `json:"fetched_at"`
	Err       string        `json:"err,omitempty"`
}

type Reader interface {
	Provider() string
	Read(ctx context.Context) (Snapshot, error)
}

// ReadAll runs every reader concurrently, each bounded by timeout, and
// returns one snapshot per reader in input order. Failures are folded into
// Snapshot.Err so the result is always complete.
func ReadAll(ctx context.Context, readers []Reader, timeout time.Duration) []Snapshot {
	out := make([]Snapshot, len(readers))
	var wg sync.WaitGroup
	for i, r := range readers {
		wg.Go(func() {
			rctx, cancel := context.WithTimeout(ctx, timeout)
			defer cancel()
			snap, err := safeRead(rctx, r)
			snap.Provider = r.Provider()
			if snap.FetchedAt.IsZero() {
				snap.FetchedAt = time.Now()
			}
			if err != nil {
				snap.Err = err.Error()
			}
			out[i] = snap
		})
	}
	wg.Wait()
	return out
}

func safeRead(ctx context.Context, r Reader) (snap Snapshot, err error) {
	defer func() {
		if p := recover(); p != nil {
			err = fmt.Errorf("reader panic: %v", p)
		}
	}()
	return r.Read(ctx)
}

type HTTPError struct{ Status int }

func (e *HTTPError) Error() string { return fmt.Sprintf("unexpected HTTP status %d", e.Status) }

const maxBody = 1 << 20

// doRaw performs a request and returns the body on 2xx. 401 maps to
// ErrUnauthorized and a 403 mentioning scope or permission maps to ErrScope.
func doRaw(ctx context.Context, c *http.Client, method, url string, hdr map[string]string, body []byte) ([]byte, error) {
	var rd io.Reader
	if body != nil {
		rd = bytes.NewReader(body)
	}
	req, err := http.NewRequestWithContext(ctx, method, url, rd)
	if err != nil {
		return nil, errors.New("invalid request URL")
	}
	for k, v := range hdr {
		req.Header.Set(k, v)
	}
	if c == nil {
		c = http.DefaultClient
	}
	resp, err := c.Do(req)
	if err != nil {
		return nil, fmt.Errorf("request failed: %w", scrubURLError(err))
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(io.LimitReader(resp.Body, maxBody))
	if err != nil {
		return nil, fmt.Errorf("read body: %w", err)
	}
	switch {
	case resp.StatusCode == http.StatusUnauthorized:
		return nil, ErrUnauthorized
	case resp.StatusCode == http.StatusForbidden:
		l := strings.ToLower(string(data))
		if strings.Contains(l, "scope") || strings.Contains(l, "permission") {
			return nil, ErrScope
		}
		return nil, &HTTPError{Status: resp.StatusCode}
	case resp.StatusCode < 200 || resp.StatusCode > 299:
		return nil, &HTTPError{Status: resp.StatusCode}
	}
	return data, nil
}

func scrubURLError(err error) error {
	if ue, ok := err.(interface{ Unwrap() error }); ok && ue.Unwrap() != nil {
		return ue.Unwrap()
	}
	return err
}

func getJSON(ctx context.Context, c *http.Client, url string, hdr map[string]string, out any) error {
	data, err := doRaw(ctx, c, http.MethodGet, url, hdr, nil)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(data, out); err != nil {
		return fmt.Errorf("decode response: %w", err)
	}
	return nil
}

func homeDir(h string) (string, error) {
	if h != "" {
		return h, nil
	}
	return os.UserHomeDir()
}

func readCredFile(path string) ([]byte, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, fmt.Errorf("%w: credentials file not found", ErrUnavailable)
		}
		var pe *os.PathError
		if errors.As(err, &pe) {
			err = pe.Err
		}
		return nil, fmt.Errorf("read credentials: %w", err)
	}
	return data, nil
}

func parseTime(s string) time.Time {
	if s == "" {
		return time.Time{}
	}
	t, err := time.Parse(time.RFC3339Nano, s)
	if err != nil {
		return time.Time{}
	}
	return t
}

func clampPercent(p float64) float64 {
	if p < 0 {
		return 0
	}
	return p
}

func nowFn(f func() time.Time) time.Time {
	if f != nil {
		return f()
	}
	return time.Now()
}
