package quota

import (
	"bytes"
	"cmp"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"
)

// Antigravity reads quota only from the local `agy` CLI; no remote Google
// endpoint is ever contacted.
type Antigravity struct {
	Bin string
	Now func() time.Time
}

func NewAntigravity() *Antigravity { return &Antigravity{Bin: "agy"} }

func (a *Antigravity) Provider() string { return "antigravity" }

type agyBucket struct {
	BucketID          string   `json:"bucketId"`
	BucketIDSnake     string   `json:"bucket_id"`
	DisplayName       string   `json:"displayName"`
	DisplayNameSnake  string   `json:"display_name"`
	Name              string   `json:"name"`
	RemainingFraction *float64 `json:"remainingFraction"`
	RemainingSnake    *float64 `json:"remaining_fraction"`
	Remaining         *struct {
		F  *float64 `json:"remainingFraction"`
		FS *float64 `json:"remaining_fraction"`
	} `json:"remaining"`
	ResetTime      json.RawMessage `json:"resetTime"`
	ResetTimeSnake json.RawMessage `json:"reset_time"`
	Disabled       bool            `json:"disabled"`
}

type agyGroup struct {
	DisplayName      string      `json:"displayName"`
	DisplayNameSnake string      `json:"display_name"`
	Name             string      `json:"name"`
	Buckets          []agyBucket `json:"buckets"`
}

type agyReport struct {
	Status  string `json:"status"`
	Command struct {
		Name string `json:"name"`
		Data struct {
			Groups []agyGroup `json:"groups"`
		} `json:"data"`
	} `json:"command"`
}

func (a *Antigravity) Read(ctx context.Context) (Snapshot, error) {
	snap := Snapshot{Provider: "antigravity", Source: "agy-cli"}
	bin := a.Bin
	if bin == "" {
		bin = "agy"
	}
	path, err := exec.LookPath(bin)
	if err != nil {
		return snap, fmt.Errorf("%w: agy CLI not installed", ErrUnavailable)
	}
	cmd := exec.CommandContext(ctx, path, "-p", "/usage", "--output-format", "json", "--print-timeout", "90s")
	configureProc(cmd)
	cmd.Dir = os.TempDir()
	cmd.Env = append(os.Environ(), "BROWSER=false")
	var stdout bytes.Buffer
	cmd.Stdout = &limitedWriter{w: &stdout, n: maxBody}
	if err := cmd.Run(); err != nil {
		return snap, fmt.Errorf("agy usage report failed: %w", ctxOr(ctx, err))
	}
	snap.FetchedAt = nowFn(a.Now)
	return parseAgyReport(stdout.Bytes(), snap)
}

func ctxOr(ctx context.Context, err error) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if ee, ok := err.(*exec.ExitError); ok {
		return fmt.Errorf("exit status %d", ee.ExitCode())
	}
	return err
}

type limitedWriter struct {
	w io.Writer
	n int
}

func (l *limitedWriter) Write(p []byte) (int, error) {
	if len(p) > l.n {
		p = p[:l.n]
	}
	l.n -= len(p)
	l.w.Write(p)
	return len(p), nil
}

func parseAgyReport(data []byte, snap Snapshot) (Snapshot, error) {
	var r agyReport
	if err := json.Unmarshal(data, &r); err != nil {
		return snap, fmt.Errorf("decode agy report: %w", err)
	}
	if r.Status != "SUCCESS" || r.Command.Name != "usage" {
		return snap, fmt.Errorf("%w: agy usage report unsuccessful", ErrUnavailable)
	}
	for _, g := range r.Command.Data.Groups {
		gname := cmp.Or(g.DisplayName, g.DisplayNameSnake, g.Name)
		for _, b := range g.Buckets {
			if b.Disabled {
				continue
			}
			frac := cmp.Or(b.RemainingFraction, b.RemainingSnake)
			if frac == nil && b.Remaining != nil {
				frac = cmp.Or(b.Remaining.F, b.Remaining.FS)
			}
			if frac == nil {
				continue
			}
			name := cmp.Or(b.DisplayName, b.DisplayNameSnake, b.Name, b.BucketID, b.BucketIDSnake)
			if gname != "" {
				name = gname + " " + name
			}
			snap.Windows = append(snap.Windows, Window{
				Name:        strings.TrimSpace(name),
				UsedPercent: clampPercent((1 - *frac) * 100),
				ResetsAt:    agyTime(cmp.Or(string(b.ResetTime), string(b.ResetTimeSnake))),
			})
		}
	}
	if len(snap.Windows) == 0 {
		return snap, fmt.Errorf("%w: agy usage report has no known quota", ErrUnavailable)
	}
	return snap, nil
}

func agyTime(raw string) time.Time {
	raw = strings.Trim(raw, `"`)
	if raw == "" || raw == "null" {
		return time.Time{}
	}
	if t := parseTime(raw); !t.IsZero() {
		return t
	}
	if f, err := strconv.ParseFloat(raw, 64); err == nil {
		return time.Unix(int64(f), 0).UTC()
	}
	return time.Time{}
}
