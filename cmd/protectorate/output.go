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
	"github.com/hotschmoe/protectorate/internal/node"
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

var taskSizes = map[string]pb.TaskSize{
	"small": pb.TaskSize_TASK_SIZE_SMALL, "medium": pb.TaskSize_TASK_SIZE_MEDIUM, "large": pb.TaskSize_TASK_SIZE_LARGE,
}

func pctOrDash(v *float64) string {
	if v == nil {
		return "-"
	}
	return fmt.Sprintf("%.0f%%", *v)
}

// writeSuggest renders the ranking, best first. Rows that are not
// recommended carry their reason like the others.
func writeSuggest(w io.Writer, r *pb.SuggestResponse) {
	fmt.Fprintln(w, "RANK\tHARNESS\tPICK\tSCORE\tUSABLE\tEXPIRING\tRESETS IN\tREASON")
	for _, s := range r.Suggestions {
		pick, resets := "no", "-"
		if s.Recommended {
			pick = "yes"
		}
		if s.ResetsIn != nil {
			resets = fmtDur(s.ResetsIn.AsDuration())
		}
		fmt.Fprintf(w, "%d\t%s\t%s\t%.1f\t%s\t%s\t%s\t%s\n", s.Rank, harnessName(s.Harness), pick, s.Score,
			pctOrDash(s.UsablePercent), pctOrDash(s.ExpiringPercent), resets, s.Reason)
	}
}

func orDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}

func fmtBytes[T ~uint64 | ~int64](n T) string {
	const unit = 1024
	v := float64(n)
	if v < unit {
		return fmt.Sprintf("%dB", int64(v))
	}
	for _, s := range []string{"KiB", "MiB", "GiB", "TiB", "PiB"} {
		v /= unit
		if v < unit || s == "PiB" {
			return fmt.Sprintf("%.1f%s", v, s)
		}
	}
	return ""
}

func nodeFreshnessLabel(s *pb.NodeStatus) string {
	age := ""
	if s.Age != nil {
		age = " (" + fmtDur(s.Age.AsDuration()) + " old)"
	}
	switch s.Freshness {
	case pb.NodeFreshness_NODE_FRESHNESS_FRESH:
		return "fresh"
	case pb.NodeFreshness_NODE_FRESHNESS_STALE:
		return "STALE" + age
	case pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE:
		if s.Metrics == nil {
			return "UNREACHABLE (never seen)"
		}
		return "UNREACHABLE" + age
	}
	return "pending"
}

func alertLabel(alerts []*pb.NodeAlert) string {
	var parts []string
	for _, a := range alerts {
		switch a.Kind {
		case pb.AlertKind_ALERT_KIND_DISK_PERCENT:
			parts = append(parts, "disk:"+a.Subject)
		case pb.AlertKind_ALERT_KIND_MEM_PRESSURE:
			parts = append(parts, "mem")
		case pb.AlertKind_ALERT_KIND_UNREACHABLE:
			parts = append(parts, "down")
		}
	}
	return orDash(strings.Join(parts, ","))
}

func gpuSummary(gpus []*pb.GpuMetrics) string {
	if len(gpus) == 0 {
		return "-"
	}
	var used, total uint64
	var utilSum float64
	var utilN int
	for _, g := range gpus {
		used += g.MemoryUsedBytes
		total += g.MemoryTotalBytes
		if g.UtilizationPercent >= 0 {
			utilSum += g.UtilizationPercent
			utilN++
		}
	}
	util := "?"
	if utilN > 0 {
		util = fmt.Sprintf("%.0f%%", utilSum/float64(utilN))
	}
	return fmt.Sprintf("%dx %s %s/%s", len(gpus), util, fmtBytes(used), fmtBytes(total))
}

func writeNodeList(w io.Writer, r *pb.ListNodesResponse) {
	fmt.Fprintln(w, "NAME\tOS/ARCH\tCPU\tGPU\tMEM\tDISK\tCONT\tFRESHNESS\tALERTS")
	for _, s := range r.Nodes {
		n, m := s.Node, s.Metrics
		plat := "?"
		if n.Os != "" {
			plat = n.Os + "/" + n.Arch
		}
		cpuS, memS, diskS, contS, gpuS := "-", "-", "-", "-", "-"
		if m != nil {
			if m.Cpu != nil {
				cpuS = fmt.Sprintf("%.0f%%", m.Cpu.UsagePercent)
			}
			if m.Memory != nil {
				memS = fmt.Sprintf("%.0f%%", m.Memory.UsedPercent)
			}
			if wf := node.WorstFilesystem(m); wf != nil {
				diskS = fmt.Sprintf("%.0f%% %s", wf.UsedPercent, wf.Mount)
			}
			if m.Engine != nil {
				contS = fmt.Sprint(len(m.Engine.Containers))
			}
			gpuS = gpuSummary(m.Gpus)
		}
		name := n.Name
		if n.Local {
			name += "*"
		}
		fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n",
			name, plat, cpuS, gpuS, memS, diskS, contS, nodeFreshnessLabel(s), alertLabel(s.Alerts))
	}
	fmt.Fprintln(w, "\n* the hub's own node")
}

func writeNodeStatus(w io.Writer, s *pb.NodeStatus) {
	n := s.Node
	fmt.Fprintf(w, "name\t%s\n", n.Name)
	fmt.Fprintf(w, "address\t%s\n", n.Address)
	fmt.Fprintf(w, "tier\t%s\n", orDash(n.Tier))
	fmt.Fprintf(w, "ssh alias\t%s\n", orDash(n.SshAlias))
	fmt.Fprintf(w, "tags\t%s\n", orDash(strings.Join(n.Tags, ", ")))
	fmt.Fprintf(w, "notes\t%s\n", orDash(n.Notes))
	fmt.Fprintf(w, "freshness\t%s\n", nodeFreshnessLabel(s))
	if s.Error != "" {
		fmt.Fprintf(w, "last error\t%s\n", s.Error)
	}
	for _, a := range s.Alerts {
		fmt.Fprintf(w, "ALERT\t%s\n", a.Message)
	}
	if s.Metrics == nil {
		return
	}
	fmt.Fprintln(w)
	writeMetrics(w, s.Metrics)
}

func writeMetrics(w io.Writer, m *pb.NodeMetrics) {
	fmt.Fprintf(w, "hostname\t%s\n", m.Hostname)
	fmt.Fprintf(w, "platform\t%s/%s %s\n", m.Os, m.Arch, m.Platform)
	fmt.Fprintf(w, "agent\t%s\n", orDash(m.AgentVersion))
	fmt.Fprintf(w, "uptime\t%s\n", fmtDur(time.Duration(m.UptimeSeconds)*time.Second))
	if c := m.Cpu; c != nil {
		line := fmt.Sprintf("%.1f%% of %d logical cores", c.UsagePercent, c.LogicalCores)
		if c.Load1 != nil {
			line += fmt.Sprintf(", load %.2f %.2f %.2f", *c.Load1, *c.Load5, *c.Load15)
		}
		fmt.Fprintf(w, "cpu\t%s\n", line)
	}
	if mm := m.Memory; mm != nil {
		fmt.Fprintf(w, "memory\t%.1f%% (%s of %s used, %s available)\n", mm.UsedPercent, fmtBytes(mm.UsedBytes), fmtBytes(mm.TotalBytes), fmtBytes(mm.AvailableBytes))
		if mm.SwapTotalBytes > 0 {
			fmt.Fprintf(w, "swap\t%.1f%% (%s of %s)\n", mm.SwapUsedPercent, fmtBytes(mm.SwapUsedBytes), fmtBytes(mm.SwapTotalBytes))
		}
	}

	fmt.Fprintln(w, "\nFILESYSTEM\tTYPE\tTOTAL\tUSED\tFREE\tUSE%")
	for _, f := range m.Filesystems {
		fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%.1f%%\n", f.Mount, f.Fstype, fmtBytes(f.TotalBytes), fmtBytes(f.UsedBytes), fmtBytes(f.FreeBytes), f.UsedPercent)
	}

	if len(m.Gpus) > 0 {
		fmt.Fprintln(w, "\nGPU\tNAME\tUTIL\tVRAM\tTEMP")
		for _, g := range m.Gpus {
			util, temp := "?", "?"
			if g.UtilizationPercent >= 0 {
				util = fmt.Sprintf("%.0f%%", g.UtilizationPercent)
			}
			if g.TemperatureCelsius >= 0 {
				temp = fmt.Sprintf("%.0fC", g.TemperatureCelsius)
			}
			fmt.Fprintf(w, "%d\t%s\t%s\t%s/%s\t%s\n", g.Index, g.Name, util, fmtBytes(g.MemoryUsedBytes), fmtBytes(g.MemoryTotalBytes), temp)
		}
	}

	if e := m.Engine; e != nil {
		fmt.Fprintf(w, "\ncontainer engine\t%s %s\n", e.Kind, e.Version)
		if d := e.Disk; d != nil {
			fmt.Fprintf(w, "\nENGINE DISK (measured %s ago)\tCOUNT\tSIZE\tRECLAIMABLE\n", fmtDur(time.Since(e.DiskMeasuredAt.AsTime())))
			for _, b := range []struct {
				name string
				v    *pb.DiskBucket
			}{{"images", d.Images}, {"containers", d.Containers}, {"volumes", d.Volumes}, {"build cache", d.BuildCache}} {
				fmt.Fprintf(w, "%s\t%d\t%s\t%s\n", b.name, b.v.Count, fmtBytes(b.v.SizeBytes), fmtBytes(b.v.ReclaimableBytes))
			}
		} else {
			fmt.Fprintln(w, "engine disk\tnot measured yet")
		}
		if len(e.Containers) > 0 {
			fmt.Fprintln(w, "\nCONTAINER\tIMAGE\tSTATE\tCPU\tMEM\tMEM%\tWRITABLE\tAGE")
			for _, c := range e.Containers {
				age := "-"
				if c.CreatedAt != nil {
					age = fmtDur(time.Since(c.CreatedAt.AsTime()))
				}
				if c.State != "running" {
					fmt.Fprintf(w, "%s\t%s\t%s\t-\t-\t-\t%s\t%s\n", c.Name, c.Image, c.State, fmtBytes(c.WritableLayerBytes), age)
					continue
				}
				if c.StatsError != "" {
					fmt.Fprintf(w, "%s\t%s\t%s\tstats unavailable: %s\n", c.Name, c.Image, c.State, c.StatsError)
					continue
				}
				fmt.Fprintf(w, "%s\t%s\t%s\t%.1f%%\t%s\t%.1f%%\t%s\t%s\n", c.Name, c.Image, c.State, c.CpuPercent, fmtBytes(c.MemoryUsedBytes), c.MemoryPercent, fmtBytes(c.WritableLayerBytes), age)
			}
		}
	}

	if len(m.WatchDirs) > 0 {
		fmt.Fprintln(w, "\nWATCHED DIR\tSIZE\tMEASURED\tNOTE")
		for _, d := range m.WatchDirs {
			note := ""
			switch {
			case d.Error != "":
				note = d.Error
			case !d.Complete:
				note = "lower bound (time budget)"
			}
			fmt.Fprintf(w, "%s\t%s\t%s ago\t%s\n", d.Path, fmtBytes(d.SizeBytes), fmtDur(time.Since(d.MeasuredAt.AsTime())), note)
		}
	}
	if len(m.Warnings) > 0 {
		fmt.Fprintln(w)
		for _, x := range m.Warnings {
			fmt.Fprintf(w, "warning\t%s\n", x)
		}
	}
}
