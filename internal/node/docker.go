package node

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net"
	"net/http"
	"net/url"
	"os"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

const (
	maxAPIVersion    = "1.43"
	statsConcurrency = 8
	statsTimeout     = 10 * time.Second
	dfTimeout        = 3 * time.Minute
	maxAPIBody       = 64 << 20
)

// dockerClient talks to the Docker Engine API over its socket with the
// standard library only. A nil http client means no engine is configured on
// this platform.
type dockerClient struct {
	http   *http.Client
	base   string
	socket string

	mu      sync.Mutex
	prefix  string
	version string
}

func newDockerClient(host string) *dockerClient {
	if host == "" {
		host = os.Getenv("DOCKER_HOST")
	}
	if host == "" && runtime.GOOS != "windows" {
		host = "unix:///var/run/docker.sock"
	}
	u, err := url.Parse(host)
	if err != nil {
		return &dockerClient{}
	}
	switch u.Scheme {
	case "unix":
		sock := u.Path
		if runtime.GOOS == "windows" && len(sock) >= 3 && sock[0] == '/' && sock[2] == ':' {
			sock = sock[1:]
		}
		return &dockerClient{
			base:   "http://docker",
			socket: sock,
			http: &http.Client{Transport: &http.Transport{
				DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
					return (&net.Dialer{Timeout: 3 * time.Second}).DialContext(ctx, "unix", sock)
				},
				DisableKeepAlives: true,
			}},
		}
	case "tcp":
		return &dockerClient{base: "http://" + u.Host, http: &http.Client{Transport: &http.Transport{DisableKeepAlives: true}}}
	}
	return &dockerClient{}
}

func (d *dockerClient) enabled() bool { return d != nil && d.http != nil }

func (d *dockerClient) getRaw(ctx context.Context, path string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, d.base+path, nil)
	if err != nil {
		return nil, err
	}
	res, err := d.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer res.Body.Close()
	body, err := io.ReadAll(io.LimitReader(res.Body, maxAPIBody))
	if err != nil {
		return nil, err
	}
	if res.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("docker %s: %s: %s", path, res.Status, strings.TrimSpace(string(body)))
	}
	return body, nil
}

func (d *dockerClient) get(ctx context.Context, path string, v any) error {
	body, err := d.getRaw(ctx, path)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(body, v); err != nil {
		return fmt.Errorf("docker %s: decode: %w", path, err)
	}
	return nil
}

// api returns the versioned path prefix and engine version, negotiated once.
func (d *dockerClient) api(ctx context.Context) (string, string, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.prefix != "" {
		return d.prefix, d.version, nil
	}
	var v struct {
		Version       string
		APIVersion    string `json:"ApiVersion"`
		MinAPIVersion string `json:"MinAPIVersion"`
	}
	if err := d.get(ctx, "/version", &v); err != nil {
		return "", "", err
	}
	d.prefix = "/v" + negotiateAPI(v.APIVersion, v.MinAPIVersion)
	d.version = v.Version
	return d.prefix, d.version, nil
}

// negotiateAPI picks the newest API version up to maxAPIVersion that the
// engine accepts, because the df schema changed after it.
func negotiateAPI(server, min string) string {
	pick := maxAPIVersion
	if server != "" && cmpVersion(server, pick) < 0 {
		pick = server
	}
	if min != "" && cmpVersion(min, pick) > 0 {
		pick = min
	}
	return pick
}

func cmpVersion(a, b string) int {
	pa, pb := strings.Split(a, "."), strings.Split(b, ".")
	for i := 0; i < len(pa) || i < len(pb); i++ {
		var x, y int
		if i < len(pa) {
			x, _ = strconv.Atoi(pa[i])
		}
		if i < len(pb) {
			y, _ = strconv.Atoi(pb[i])
		}
		if x != y {
			if x < y {
				return -1
			}
			return 1
		}
	}
	return 0
}

// absent reports errors that mean "no engine here" rather than a fault.
func (d *dockerClient) absent(err error) bool {
	return errors.Is(err, fs.ErrNotExist) || errors.Is(err, syscall.ECONNREFUSED)
}

type dockerDF struct {
	usage      *pb.EngineDiskUsage
	containers map[string]containerSize
}

type containerSize struct{ rw, rootfs int64 }

type dfResponse struct {
	LayersSize int64
	Images     []struct {
		Size       int64
		Containers int64
	}
	Containers []struct {
		ID         string `json:"Id"`
		State      string
		SizeRw     int64
		SizeRootFs int64
	}
	Volumes []struct {
		UsageData *struct{ Size, RefCount int64 }
	}
	BuildCache []struct {
		Size  int64
		InUse bool
	}
}

func (d *dockerClient) diskUsage(ctx context.Context) *dockerDF {
	if !d.enabled() {
		return nil
	}
	ctx, cancel := context.WithTimeout(ctx, dfTimeout)
	defer cancel()
	prefix, _, err := d.api(ctx)
	if err != nil {
		return nil
	}
	body, err := d.getRaw(ctx, prefix+"/system/df")
	if err != nil {
		return nil
	}
	return parseDF(body)
}

func parseDF(body []byte) *dockerDF {
	var r dfResponse
	if json.Unmarshal(body, &r) != nil {
		return nil
	}
	out := &dockerDF{usage: &pb.EngineDiskUsage{
		Images: &pb.DiskBucket{}, Containers: &pb.DiskBucket{}, Volumes: &pb.DiskBucket{}, BuildCache: &pb.DiskBucket{},
	}, containers: map[string]containerSize{}}
	u := out.usage
	var imgSum int64
	for _, im := range r.Images {
		u.Images.Count++
		imgSum += im.Size
		if im.Containers == 0 {
			u.Images.ReclaimableBytes += im.Size
		}
	}
	u.Images.SizeBytes = r.LayersSize
	if u.Images.SizeBytes <= 0 {
		u.Images.SizeBytes = imgSum
	}
	if u.Images.ReclaimableBytes > u.Images.SizeBytes {
		u.Images.ReclaimableBytes = u.Images.SizeBytes
	}
	for _, c := range r.Containers {
		u.Containers.Count++
		u.Containers.SizeBytes += max(c.SizeRw, 0)
		if c.State != "running" {
			u.Containers.ReclaimableBytes += max(c.SizeRw, 0)
		}
		out.containers[c.ID] = containerSize{rw: max(c.SizeRw, 0), rootfs: max(c.SizeRootFs, 0)}
	}
	for _, v := range r.Volumes {
		u.Volumes.Count++
		if v.UsageData == nil || v.UsageData.Size < 0 {
			continue
		}
		u.Volumes.SizeBytes += v.UsageData.Size
		if v.UsageData.RefCount == 0 {
			u.Volumes.ReclaimableBytes += v.UsageData.Size
		}
	}
	for _, b := range r.BuildCache {
		u.BuildCache.Count++
		u.BuildCache.SizeBytes += b.Size
		if !b.InUse {
			u.BuildCache.ReclaimableBytes += b.Size
		}
	}
	return out
}

// engineMetrics returns nil without error when no engine is reachable.
func (c *Collector) engineMetrics(ctx context.Context) (*pb.ContainerEngine, error) {
	d := c.docker
	if !d.enabled() {
		return nil, nil
	}
	pctx, cancel := context.WithTimeout(ctx, 3*time.Second)
	prefix, version, err := d.api(pctx)
	cancel()
	if err != nil {
		if d.absent(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("docker: %w", err)
	}

	var list []struct {
		ID    string `json:"Id"`
		Names []string
		Image string
		State string
	}
	lctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	err = d.get(lctx, prefix+"/containers/json", &list)
	cancel()
	if err != nil {
		return nil, fmt.Errorf("docker: list containers: %w", err)
	}

	eng := &pb.ContainerEngine{Kind: "docker", Version: version}
	c.slowMu.RLock()
	df, dfAt := c.df, c.dfAt
	c.slowMu.RUnlock()
	if df != nil {
		eng.Disk, eng.DiskMeasuredAt = df.usage, timestamppb.New(dfAt)
	}

	eng.Containers = make([]*pb.ContainerMetrics, len(list))
	sem := make(chan struct{}, statsConcurrency)
	var wg sync.WaitGroup
	for i, ct := range list {
		cm := &pb.ContainerMetrics{Id: shortID(ct.ID), Image: ct.Image, State: ct.State}
		if len(ct.Names) > 0 {
			cm.Name = strings.TrimPrefix(ct.Names[0], "/")
		}
		if df != nil {
			sz := df.containers[ct.ID]
			cm.WritableLayerBytes, cm.RootfsBytes = sz.rw, sz.rootfs
		}
		eng.Containers[i] = cm
		wg.Add(1)
		sem <- struct{}{}
		go func() {
			defer wg.Done()
			defer func() { <-sem }()
			sctx, cancel := context.WithTimeout(ctx, statsTimeout)
			defer cancel()
			body, err := d.getRaw(sctx, prefix+"/containers/"+ct.ID+"/stats?stream=false")
			if err != nil {
				cm.StatsError = err.Error()
				return
			}
			if err := applyStats(cm, body); err != nil {
				cm.StatsError = err.Error()
			}
		}()
	}
	wg.Wait()
	sort.Slice(eng.Containers, func(i, j int) bool { return eng.Containers[i].Name < eng.Containers[j].Name })
	return eng, nil
}

func shortID(id string) string {
	if len(id) > 12 {
		return id[:12]
	}
	return id
}

type statsJSON struct {
	CPUStats    cpuStatsJSON `json:"cpu_stats"`
	PreCPUStats cpuStatsJSON `json:"precpu_stats"`
	MemoryStats struct {
		Usage uint64
		Limit uint64
		Stats map[string]uint64
	} `json:"memory_stats"`
}

type cpuStatsJSON struct {
	CPUUsage struct {
		TotalUsage  uint64   `json:"total_usage"`
		PercpuUsage []uint64 `json:"percpu_usage"`
	} `json:"cpu_usage"`
	SystemUsage uint64 `json:"system_cpu_usage"`
	OnlineCPUs  uint32 `json:"online_cpus"`
}

// applyStats fills CPU and memory the way `docker stats` does: page cache
// (inactive file) is not counted as used memory.
func applyStats(cm *pb.ContainerMetrics, body []byte) error {
	var s statsJSON
	if err := json.Unmarshal(body, &s); err != nil {
		return fmt.Errorf("decode stats: %w", err)
	}
	used := s.MemoryStats.Usage
	cache := s.MemoryStats.Stats["inactive_file"]
	if cache == 0 {
		cache = s.MemoryStats.Stats["total_inactive_file"]
	}
	if cache < used {
		used -= cache
	}
	cm.MemoryUsedBytes, cm.MemoryLimitBytes = used, s.MemoryStats.Limit
	if s.MemoryStats.Limit > 0 {
		cm.MemoryPercent = float64(used) / float64(s.MemoryStats.Limit) * 100
	}
	cpuDelta := float64(s.CPUStats.CPUUsage.TotalUsage) - float64(s.PreCPUStats.CPUUsage.TotalUsage)
	sysDelta := float64(s.CPUStats.SystemUsage) - float64(s.PreCPUStats.SystemUsage)
	cpus := float64(s.CPUStats.OnlineCPUs)
	if cpus == 0 {
		cpus = float64(len(s.CPUStats.CPUUsage.PercpuUsage))
	}
	if cpuDelta > 0 && sysDelta > 0 && cpus > 0 {
		cm.CpuPercent = cpuDelta / sysDelta * cpus * 100
	}
	return nil
}
