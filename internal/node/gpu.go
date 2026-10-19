package node

import (
	"bytes"
	"context"
	"encoding/json"
	"os/exec"
	"sort"
	"strconv"
	"strings"
	"time"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

const gpuToolTimeout = 8 * time.Second

// gpuMetrics queries vendor tools; a missing tool or any failure yields no
// GPUs of that vendor rather than an error.
func gpuMetrics(ctx context.Context) []*pb.GpuMetrics {
	var out []*pb.GpuMetrics
	if raw, ok := runTool(ctx, "nvidia-smi",
		"--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu",
		"--format=csv,noheader,nounits"); ok {
		out = append(out, parseNvidiaSMI(raw)...)
	}
	if raw, ok := runTool(ctx, "rocm-smi", "--showproductname", "--showuse", "--showmeminfo", "vram", "--showtemp", "--json"); ok {
		base := int32(len(out))
		for _, g := range parseROCmSMI(raw) {
			g.Index += base
			out = append(out, g)
		}
	}
	return out
}

func runTool(ctx context.Context, name string, args ...string) ([]byte, bool) {
	path, err := exec.LookPath(name)
	if err != nil {
		return nil, false
	}
	ctx, cancel := context.WithTimeout(ctx, gpuToolTimeout)
	defer cancel()
	var stdout bytes.Buffer
	cmd := exec.CommandContext(ctx, path, args...)
	cmd.Stdout = &stdout
	cmd.WaitDelay = time.Second
	if cmd.Run() != nil {
		return nil, false
	}
	return stdout.Bytes(), true
}

const mib = 1 << 20

func parseNvidiaSMI(raw []byte) []*pb.GpuMetrics {
	var out []*pb.GpuMetrics
	for _, line := range strings.Split(string(raw), "\n") {
		f := strings.Split(strings.TrimSpace(line), ",")
		if len(f) < 6 {
			continue
		}
		for i := range f {
			f[i] = strings.TrimSpace(f[i])
		}
		idx, err := strconv.Atoi(f[0])
		if err != nil {
			continue
		}
		out = append(out, &pb.GpuMetrics{
			Index:              int32(idx),
			Vendor:             "nvidia",
			Name:               f[1],
			UtilizationPercent: numOr(f[2], -1),
			MemoryUsedBytes:    uint64(numOr(f[3], 0) * mib),
			MemoryTotalBytes:   uint64(numOr(f[4], 0) * mib),
			TemperatureCelsius: numOr(f[5], -1),
		})
	}
	return out
}

// numOr parses a number, returning def for "[N/A]" and other non-numbers.
func numOr(s string, def float64) float64 {
	v, err := strconv.ParseFloat(strings.TrimSpace(s), 64)
	if err != nil || v < 0 {
		return def
	}
	return v
}

// parseROCmSMI reads `rocm-smi --json` output: {"card0": {"<label>": "<value>"}}.
// Labels vary between ROCm releases, so fields are matched by prefix.
func parseROCmSMI(raw []byte) []*pb.GpuMetrics {
	var cards map[string]map[string]any
	if json.Unmarshal(raw, &cards) != nil {
		return nil
	}
	names := make([]string, 0, len(cards))
	for k := range cards {
		if strings.HasPrefix(k, "card") {
			names = append(names, k)
		}
	}
	sort.Slice(names, func(i, j int) bool {
		a, _ := strconv.Atoi(strings.TrimPrefix(names[i], "card"))
		b, _ := strconv.Atoi(strings.TrimPrefix(names[j], "card"))
		return a < b
	})
	var out []*pb.GpuMetrics
	for i, n := range names {
		c := cards[n]
		g := &pb.GpuMetrics{
			Index: int32(i), Vendor: "amd",
			UtilizationPercent: -1, TemperatureCelsius: -1,
			Name: firstField(c, "Card Series", "Card series", "Card Model", "Card model"),
		}
		g.UtilizationPercent = numOr(firstField(c, "GPU use (%)"), -1)
		g.MemoryTotalBytes = uint64(numOr(firstField(c, "VRAM Total Memory (B)"), 0))
		g.MemoryUsedBytes = uint64(numOr(firstField(c, "VRAM Total Used Memory (B)"), 0))
		g.TemperatureCelsius = numOr(firstFieldPrefix(c, "Temperature (Sensor edge)"), -1)
		out = append(out, g)
	}
	return out
}

func firstField(m map[string]any, keys ...string) string {
	for _, k := range keys {
		if v, ok := m[k]; ok {
			return asString(v)
		}
	}
	return ""
}

func firstFieldPrefix(m map[string]any, prefix string) string {
	for k, v := range m {
		if strings.HasPrefix(k, prefix) {
			return asString(v)
		}
	}
	return ""
}

func asString(v any) string {
	switch t := v.(type) {
	case string:
		return t
	case float64:
		return strconv.FormatFloat(t, 'f', -1, 64)
	}
	return ""
}
