package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"text/tabwriter"
	"time"

	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// out renders proto messages either as stable one-line proto-JSON or as
// aligned text. Every command prints through it.
type out struct {
	w    io.Writer
	json bool
}

// print writes m as compact proto-JSON in json mode, otherwise calls human
// with a tabwriter (tab-separated columns are aligned).
func (o out) print(m proto.Message, human func(w io.Writer)) error {
	if o.json {
		b, err := protoJSON(m)
		if err != nil {
			return err
		}
		_, err = fmt.Fprintf(o.w, "%s\n", b)
		return err
	}
	tw := tabwriter.NewWriter(o.w, 0, 0, 2, ' ', 0)
	human(tw)
	return tw.Flush()
}

// protoJSON is protojson made deterministic: protojson deliberately
// randomizes whitespace, so the output is compacted.
func protoJSON(m proto.Message) ([]byte, error) {
	raw, err := protojson.MarshalOptions{EmitUnpopulated: false}.Marshal(m)
	if err != nil {
		return nil, err
	}
	var buf bytes.Buffer
	if err := json.Compact(&buf, raw); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func fmtDur(d time.Duration) string {
	d = d.Round(time.Minute)
	switch {
	case d <= 0:
		return "0m"
	case d < time.Hour:
		return fmt.Sprintf("%dm", int(d.Minutes()))
	case d < 48*time.Hour:
		return fmt.Sprintf("%dh%02dm", int(d.Hours()), int(d.Minutes())%60)
	}
	return fmt.Sprintf("%dd%02dh", int(d.Hours())/24, int(d.Hours())%24)
}

func freshnessLabel(q *pb.ProviderQuota) string {
	switch q.Freshness {
	case pb.Freshness_FRESHNESS_FRESH:
		return "fresh"
	case pb.Freshness_FRESHNESS_STALE:
		return "STALE (" + fmtDur(q.Age.AsDuration()) + " old)"
	}
	return "UNKNOWN"
}

// writeQuota renders one row per window. Providers without usable data show
// "?" rather than any capacity figure.
func writeQuota(w io.Writer, r *pb.GetQuotaResponse, now time.Time) {
	fmt.Fprintln(w, "PROVIDER\tPLAN\tWINDOW\tUSED\tRESETS IN\tPROJ. UNUSED (est.)\tFRESHNESS")
	var notes []string
	for _, q := range r.Quotas {
		plan := orDash(q.Plan)
		fresh := freshnessLabel(q)
		if len(q.Windows) == 0 {
			fmt.Fprintf(w, "%s\t%s\t-\t?\t-\t-\t%s\n", q.Provider, plan, fresh)
		}
		for _, win := range q.Windows {
			resets, proj := "-", "-"
			if win.ResetsAt != nil {
				resets = fmtDur(win.ResetsAt.AsTime().Sub(now))
			}
			if win.EstimatedUnusedPercentAtReset != nil {
				proj = fmt.Sprintf("~%.0f%%", *win.EstimatedUnusedPercentAtReset)
			}
			fmt.Fprintf(w, "%s\t%s\t%s\t%.1f%%\t%s\t%s\t%s\n", q.Provider, plan, win.Name, win.UsedPercent, resets, proj, fresh)
		}
		for _, rc := range q.Resets {
			exp := ""
			if rc.ExpiresAt != nil {
				exp = ", expires in " + fmtDur(rc.ExpiresAt.AsTime().Sub(now))
			}
			notes = append(notes, fmt.Sprintf("%s: %d reset credit(s) [%s]%s", q.Provider, rc.Count, rc.Kind, exp))
		}
		if len(q.Models) > 0 {
			notes = append(notes, fmt.Sprintf("%s: models %s", q.Provider, strings.Join(q.Models, ", ")))
		}
		if q.Error != "" {
			notes = append(notes, fmt.Sprintf("%s: last read failed: %s", q.Provider, q.Error))
		}
	}
	if len(notes) > 0 {
		fmt.Fprintln(w)
		for _, n := range notes {
			fmt.Fprintln(w, n)
		}
	}
}

func orDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}
