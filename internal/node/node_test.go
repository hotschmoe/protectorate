package node

import (
	"context"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"
	"github.com/shirou/gopsutil/v4/cpu"
	"github.com/shirou/gopsutil/v4/disk"
	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestParseNvidiaSMI(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want []*pb.GpuMetrics
	}{
		{"empty", "", nil},
		{"garbage", "NVIDIA-SMI has failed\n", nil},
		{"two gpus", "0, NVIDIA GeForce RTX 4090, 35, 1024, 24564, 51\n1, NVIDIA GeForce RTX 3060, 0, 0, 12288, 40\n", []*pb.GpuMetrics{
			{Index: 0, Name: "NVIDIA GeForce RTX 4090", UtilizationPercent: 35, MemoryUsedBytes: proto.Uint64(1024 * mib), MemoryTotalBytes: 24564 * mib, TemperatureCelsius: 51},
			{Index: 1, Name: "NVIDIA GeForce RTX 3060", UtilizationPercent: 0, MemoryUsedBytes: proto.Uint64(0), MemoryTotalBytes: 12288 * mib, TemperatureCelsius: 40},
		}},
		{"not available", "0, Tesla T4, [N/A], 10, 15360, [N/A]\n", []*pb.GpuMetrics{
			{Index: 0, Name: "Tesla T4", UtilizationPercent: -1, MemoryUsedBytes: proto.Uint64(10 * mib), MemoryTotalBytes: 15360 * mib, TemperatureCelsius: -1},
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := parseNvidiaSMI([]byte(tt.in))
			if len(got) != len(tt.want) {
				t.Fatalf("got %d gpus, want %d", len(got), len(tt.want))
			}
			for i, w := range tt.want {
				g := got[i]
				if g.Vendor != "nvidia" || g.Index != w.Index || g.Name != w.Name || g.UtilizationPercent != w.UtilizationPercent ||
					g.GetMemoryUsedBytes() != w.GetMemoryUsedBytes() || g.MemoryTotalBytes != w.MemoryTotalBytes || g.TemperatureCelsius != w.TemperatureCelsius {
					t.Fatalf("gpu %d = %v, want %v", i, g, w)
				}
			}
		})
	}
}

func TestParseROCmSMI(t *testing.T) {
	raw := `{"card1": {"Card Series": "Radeon RX 7900 XTX", "GPU use (%)": "12", "VRAM Total Memory (B)": "25753026560", "VRAM Total Used Memory (B)": "1048576", "Temperature (Sensor edge) (C)": "44.0"},
	"card0": {"Card Series": "Radeon Pro", "GPU use (%)": "0"}, "system": {"Driver version": "6.1"}}`
	got := parseROCmSMI([]byte(raw))
	if len(got) != 2 {
		t.Fatalf("got %d gpus", len(got))
	}
	if got[0].Name != "Radeon Pro" || got[0].UtilizationPercent != 0 || got[0].TemperatureCelsius != -1 {
		t.Fatalf("card0 = %v", got[0])
	}
	if got[1].Name != "Radeon RX 7900 XTX" || got[1].MemoryTotalBytes != 25753026560 || got[1].GetMemoryUsedBytes() != 1048576 || got[1].TemperatureCelsius != 44 || got[1].Vendor != "amd" {
		t.Fatalf("card1 = %v", got[1])
	}
	if parseROCmSMI([]byte("not json")) != nil {
		t.Fatal("invalid json must yield no gpus")
	}
}

func TestParseDF(t *testing.T) {
	body := `{"LayersSize": 1000,
	"Images": [{"Size": 600, "Containers": 1}, {"Size": 500, "Containers": 0}],
	"Containers": [{"Id": "aaa", "State": "running", "SizeRw": 10, "SizeRootFs": 600}, {"Id": "bbb", "State": "exited", "SizeRw": 30, "SizeRootFs": 500}],
	"Volumes": [{"UsageData": {"Size": 100, "RefCount": 0}}, {"UsageData": {"Size": 50, "RefCount": 2}}, {"UsageData": {"Size": -1, "RefCount": -1}}],
	"BuildCache": [{"Size": 70, "InUse": true}, {"Size": 30, "InUse": false}]}`
	df := parseDF([]byte(body))
	if df == nil {
		t.Fatal("nil df")
	}
	u := df.usage
	got := map[string][3]int64{
		"images":     {u.Images.Count, u.Images.SizeBytes, u.Images.ReclaimableBytes},
		"containers": {u.Containers.Count, u.Containers.SizeBytes, u.Containers.ReclaimableBytes},
		"volumes":    {u.Volumes.Count, u.Volumes.SizeBytes, u.Volumes.ReclaimableBytes},
		"build":      {u.BuildCache.Count, u.BuildCache.SizeBytes, u.BuildCache.ReclaimableBytes},
	}
	exp := map[string][3]int64{
		"images": {2, 1000, 500}, "containers": {2, 40, 30}, "volumes": {3, 150, 100}, "build": {2, 100, 30},
	}
	for k, e := range exp {
		if got[k] != e {
			t.Errorf("%s = %v, want %v (count, size, reclaimable)", k, got[k], e)
		}
	}
	if df.containers["bbb"].rw != 30 || df.containers["aaa"].rootfs != 600 {
		t.Fatalf("container sizes = %v", df.containers)
	}
	if parseDF([]byte("nope")) != nil {
		t.Fatal("invalid json must yield nil")
	}
}

func TestApplyStats(t *testing.T) {
	body := `{"cpu_stats": {"cpu_usage": {"total_usage": 2000}, "system_cpu_usage": 20000, "online_cpus": 4},
	"precpu_stats": {"cpu_usage": {"total_usage": 1000}, "system_cpu_usage": 10000},
	"memory_stats": {"usage": 1000, "limit": 4000, "stats": {"inactive_file": 200}}}`
	cm := &pb.ContainerMetrics{}
	if err := applyStats(cm, []byte(body)); err != nil {
		t.Fatal(err)
	}
	if cm.CpuPercent != 40 || cm.MemoryUsedBytes != 800 || cm.MemoryLimitBytes != 4000 || cm.MemoryPercent != 20 {
		t.Fatalf("got %v", cm)
	}
	if applyStats(&pb.ContainerMetrics{}, []byte("{")) == nil {
		t.Fatal("expected decode error")
	}
	zero := &pb.ContainerMetrics{}
	if err := applyStats(zero, []byte(`{}`)); err != nil || zero.CpuPercent != 0 || zero.MemoryPercent != 0 {
		t.Fatalf("empty stats = %v, %v", zero, err)
	}
}

func TestNegotiateAPI(t *testing.T) {
	tests := []struct{ server, min, want string }{
		{"1.41", "1.12", "1.41"},
		{"1.47", "1.24", "1.43"},
		{"1.52", "1.44", "1.44"},
		{"", "", "1.43"},
		{"1.9", "", "1.9"},
	}
	for _, tt := range tests {
		if got := negotiateAPI(tt.server, tt.min); got != tt.want {
			t.Errorf("negotiateAPI(%q,%q) = %q, want %q", tt.server, tt.min, got, tt.want)
		}
	}
}

func TestCPUBusyPercent(t *testing.T) {
	tests := []struct {
		name      string
		prev, cur cpu.TimesStat
		want      float64
	}{
		{"half busy", cpu.TimesStat{User: 10, Idle: 10}, cpu.TimesStat{User: 20, Idle: 20}, 50},
		{"iowait is idle", cpu.TimesStat{}, cpu.TimesStat{User: 10, Iowait: 10}, 50},
		{"no elapsed", cpu.TimesStat{User: 5}, cpu.TimesStat{User: 5}, 0},
		{"counter reset", cpu.TimesStat{User: 100, Idle: 100}, cpu.TimesStat{User: 1, Idle: 1}, 0},
		{"all busy", cpu.TimesStat{}, cpu.TimesStat{System: 10}, 100},
	}
	for _, tt := range tests {
		if got := cpuBusyPercent(tt.prev, tt.cur); got != tt.want {
			t.Errorf("%s: got %v, want %v", tt.name, got, tt.want)
		}
	}
}

func TestKeepPartition(t *testing.T) {
	tests := []struct {
		fstype, mount string
		want          bool
	}{
		{"ext4", "/", true},
		{"ext4", "/home", true},
		{"NTFS", `C:`, true},
		{"tmpfs", "/tmp", false},
		{"squashfs", "/snap/core/1", false},
		{"overlay", "/", true},
		{"overlay", "/var/lib/docker/overlay2/x/merged", false},
		{"ext4", "/etc/hosts", false},
		{"ext4", "/var/lib/docker/volumes", false},
		{"", "/mnt/x", false},
		{"iso9660", "/media/cd", false},
	}
	for _, tt := range tests {
		if got := keepPartition(disk.PartitionStat{Fstype: tt.fstype, Mountpoint: tt.mount}); got != tt.want {
			t.Errorf("keepPartition(%s %s) = %v, want %v", tt.fstype, tt.mount, got, tt.want)
		}
	}
}

func TestDedupeFilesystems(t *testing.T) {
	in := []*pb.FilesystemMetrics{
		{Mount: "/mnt/data/bind", Device: "/dev/sda1", TotalBytes: 100},
		{Mount: "/mnt/data", Device: "/dev/sda1", TotalBytes: 100},
		{Mount: "/", Device: "/dev/sdb1", TotalBytes: 50},
		{Mount: "/empty", Device: "/dev/sdc1", TotalBytes: 0},
	}
	got := dedupeFilesystems(in)
	if len(got) != 2 || got[0].Mount != "/" || got[1].Mount != "/mnt/data" {
		t.Fatalf("got %v", got)
	}
}

func TestDirSize(t *testing.T) {
	root := t.TempDir()
	must := func(err error) {
		t.Helper()
		if err != nil {
			t.Fatal(err)
		}
	}
	must(os.MkdirAll(filepath.Join(root, "a", "b"), 0o755))
	must(os.WriteFile(filepath.Join(root, "x"), make([]byte, 100), 0o644))
	must(os.WriteFile(filepath.Join(root, "a", "b", "y"), make([]byte, 400), 0o644))

	tests := []struct {
		name         string
		path         string
		budget       time.Duration
		wantSize     int64
		wantComplete bool
		wantErr      bool
	}{
		{"tree", root, time.Minute, 500, true, false},
		{"missing", filepath.Join(root, "nope"), time.Minute, 0, false, true},
		{"expired budget", root, time.Nanosecond, -1, false, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ds := dirSize(context.Background(), tt.path, tt.budget)
			if tt.wantSize >= 0 && ds.SizeBytes != tt.wantSize {
				t.Errorf("size = %d, want %d", ds.SizeBytes, tt.wantSize)
			}
			if tt.name != "expired budget" && ds.Complete != tt.wantComplete {
				t.Errorf("complete = %v, want %v", ds.Complete, tt.wantComplete)
			}
			if (ds.Error != "") != tt.wantErr {
				t.Errorf("error = %q", ds.Error)
			}
		})
	}
}

func fakeDocker(t *testing.T) string {
	t.Helper()
	mux := http.NewServeMux()
	mux.HandleFunc("/version", func(w http.ResponseWriter, _ *http.Request) {
		w.Write([]byte(`{"Version":"27.0.0","ApiVersion":"1.46","MinAPIVersion":"1.24"}`))
	})
	mux.HandleFunc("/v1.43/containers/json", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("all") != "1" {
			http.Error(w, "stopped containers not requested", http.StatusBadRequest)
			return
		}
		w.Write([]byte(`[{"Id":"aaaaaaaaaaaaaaaa","Names":["/web"],"Image":"nginx","State":"running","Created":1700000000},{"Id":"bbbbbbbbbbbbbbbb","Names":["/broken"],"Image":"x","State":"running"},{"Id":"cccccccccccccccc","Names":["/dead"],"Image":"old","State":"exited","Created":1600000000}]`))
	})
	mux.HandleFunc("/v1.43/containers/aaaaaaaaaaaaaaaa/stats", func(w http.ResponseWriter, _ *http.Request) {
		w.Write([]byte(`{"cpu_stats":{"cpu_usage":{"total_usage":200},"system_cpu_usage":2000,"online_cpus":2},"precpu_stats":{"cpu_usage":{"total_usage":100},"system_cpu_usage":1000},"memory_stats":{"usage":500,"limit":1000}}`))
	})
	mux.HandleFunc("/v1.43/containers/bbbbbbbbbbbbbbbb/stats", func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "boom", http.StatusInternalServerError)
	})
	mux.HandleFunc("/v1.43/system/df", func(w http.ResponseWriter, _ *http.Request) {
		w.Write([]byte(`{"LayersSize":900,"Images":[{"Size":900,"Containers":0}],"Containers":[{"Id":"aaaaaaaaaaaaaaaa","State":"running","SizeRw":7,"SizeRootFs":900},{"Id":"cccccccccccccccc","State":"exited","SizeRw":5000,"SizeRootFs":5900}],"Volumes":[],"BuildCache":[]}`))
	})
	sock := filepath.Join(t.TempDir(), "d.sock")
	l, err := net.Listen("unix", sock)
	if err != nil {
		t.Skipf("unix sockets unavailable: %v", err)
	}
	srv := httptest.NewUnstartedServer(mux)
	srv.Listener = l
	srv.Start()
	t.Cleanup(srv.Close)
	p := filepath.ToSlash(sock)
	if !strings.HasPrefix(p, "/") {
		p = "/" + p
	}
	return "unix://" + p
}

func TestEngineMetrics(t *testing.T) {
	c := NewCollector(Options{DockerHost: fakeDocker(t)})
	ctx := context.Background()
	c.Refresh(ctx)
	eng, err := c.engineMetrics(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if eng.Kind != "docker" || eng.Version != "27.0.0" || len(eng.Containers) != 3 {
		t.Fatalf("engine = %v", eng)
	}
	if eng.Disk == nil || eng.Disk.Images.SizeBytes != 900 || eng.DiskMeasuredAt == nil {
		t.Fatalf("disk = %v", eng.Disk)
	}
	var web, broken *pb.ContainerMetrics
	for _, cm := range eng.Containers {
		switch cm.Name {
		case "web":
			web = cm
		case "broken":
			broken = cm
		}
	}
	if web == nil || web.CpuPercent != 20 || web.MemoryPercent != 50 || web.WritableLayerBytes != 7 || web.Id != "aaaaaaaaaaaa" || web.StatsError != "" {
		t.Fatalf("web = %v", web)
	}
	if broken == nil || broken.StatsError == "" {
		t.Fatalf("broken = %v", broken)
	}
	var dead *pb.ContainerMetrics
	for _, cm := range eng.Containers {
		if cm.Name == "dead" {
			dead = cm
		}
	}
	if dead == nil || dead.State != "exited" || dead.Image != "old" || dead.WritableLayerBytes != 5000 || dead.CreatedAt.AsTime().Unix() != 1600000000 || dead.StatsError != "" || dead.CpuPercent != 0 || dead.MemoryUsedBytes != 0 {
		t.Fatalf("dead = %v", dead)
	}
}

func TestEngineAbsent(t *testing.T) {
	tests := []struct{ name, host string }{
		{"missing socket", "unix://" + filepath.Join(t.TempDir(), "none.sock")},
		{"unsupported scheme", "npipe:////./pipe/docker_engine"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			eng, err := NewCollector(Options{DockerHost: tt.host}).engineMetrics(context.Background())
			if eng != nil || err != nil {
				t.Fatalf("got %v, %v; want no engine and no error", eng, err)
			}
		})
	}
}

func TestCollectLocal(t *testing.T) {
	c := NewCollector(Options{Version: "t", DockerHost: "unix:///nonexistent.sock"})
	m, err := c.Collect(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if m.Hostname == "" || m.Os == "" || m.Arch == "" || m.AgentVersion != "t" || m.Cpu == nil || m.Cpu.LogicalCores < 1 || m.Memory == nil || m.Memory.TotalBytes == 0 {
		t.Fatalf("incomplete metrics: %v", m)
	}
	if len(m.Filesystems) == 0 {
		t.Fatalf("no filesystems: %v", m.Warnings)
	}
	again, _ := c.Collect(context.Background())
	if again != m {
		t.Fatal("rapid second Collect should reuse the result")
	}
}

func TestAgent(t *testing.T) {
	a := &Agent{
		Collector: NewCollector(Options{DockerHost: "unix:///nonexistent.sock", MinAge: time.Millisecond}),
		Authorize: TokenAuthorizer("tok"),
	}
	srv := httptest.NewServer(a.Handler())
	defer srv.Close()

	tests := []struct {
		name    string
		token   string
		wantErr connect.Code
	}{
		{"ok", "tok", 0},
		{"wrong token", "bad", connect.CodeUnauthenticated},
		{"no token", "", connect.CodeUnauthenticated},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			hc := &http.Client{Transport: roundTripFunc(func(r *http.Request) (*http.Response, error) {
				if tt.token != "" {
					r.Header.Set("Authorization", "Bearer "+tt.token)
				}
				return http.DefaultTransport.RoundTrip(r)
			})}
			cl := protectoratev1connect.NewNodeAgentServiceClient(hc, srv.URL, connect.WithProtoJSON())
			res, err := cl.GetMetrics(context.Background(), connect.NewRequest(&pb.GetMetricsRequest{}))
			if tt.wantErr != 0 {
				if connect.CodeOf(err) != tt.wantErr {
					t.Fatalf("err = %v, want code %v", err, tt.wantErr)
				}
				return
			}
			if err != nil || res.Msg.Hostname == "" {
				t.Fatalf("res=%v err=%v", res, err)
			}
		})
	}

	t.Run("nil authorizer rejects", func(t *testing.T) {
		s := httptest.NewServer((&Agent{Collector: a.Collector}).Handler())
		defer s.Close()
		res, err := http.Post(s.URL+"/protectorate.v1.NodeAgentService/GetMetrics", "application/json", nil)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != http.StatusForbidden {
			t.Fatalf("status = %d", res.StatusCode)
		}
	})

	t.Run("watch streams", func(t *testing.T) {
		hc := &http.Client{Transport: roundTripFunc(func(r *http.Request) (*http.Response, error) {
			r.Header.Set("Authorization", "Bearer tok")
			return http.DefaultTransport.RoundTrip(r)
		})}
		cl := protectoratev1connect.NewNodeAgentServiceClient(hc, srv.URL, connect.WithProtoJSON())
		ctx, cancel := context.WithCancel(context.Background())
		defer cancel()
		st, err := cl.WatchMetrics(ctx, connect.NewRequest(&pb.WatchMetricsRequest{IntervalSeconds: 1}))
		if err != nil {
			t.Fatal(err)
		}
		if !st.Receive() || st.Msg().Hostname == "" {
			t.Fatalf("no first message: %v", st.Err())
		}
	})
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestAndroidSystemMount(t *testing.T) {
	for mount, want := range map[string]bool{
		"/apex/com.android.art@340":        true,
		"/system":                          true,
		"/system_ext":                      true,
		"/mnt/pass_through/0/emulated":     true,
		"/data":                            false,
		"/data/data/com.termux/files/home": false,
		"/storage/emulated":                false,
		"/systemx":                         false,
	} {
		if got := androidSystemMount(mount); got != want {
			t.Errorf("androidSystemMount(%q) = %v, want %v", mount, got, want)
		}
	}
}

func TestFSProberBlockedMount(t *testing.T) {
	release := make(chan struct{})
	var mu sync.Mutex
	calls := map[string]int{}
	pr := newFSProber()
	pr.timeout = 20 * time.Millisecond
	pr.run = func(ctx context.Context, p disk.PartitionStat) fsResult {
		mu.Lock()
		calls[p.Mountpoint]++
		mu.Unlock()
		if p.Mountpoint == "/dead" {
			<-release
		}
		return fsResult{fs: &pb.FilesystemMetrics{Mount: p.Mountpoint}}
	}
	dead := disk.PartitionStat{Mountpoint: "/dead"}
	live := disk.PartitionStat{Mountpoint: "/live"}
	ctx := context.Background()

	if r := pr.probe(ctx, dead); r.err == nil || !strings.Contains(r.err.Error(), "timed out") {
		t.Fatalf("first probe = %+v, want timeout", r)
	}
	for i := 0; i < 3; i++ {
		if r := pr.probe(ctx, dead); r.err == nil || !strings.Contains(r.err.Error(), "stale") {
			t.Fatalf("repeat probe = %+v, want stale", r)
		}
		if r := pr.probe(ctx, live); r.err != nil || r.fs == nil {
			t.Fatalf("live probe = %+v", r)
		}
	}
	mu.Lock()
	if calls["/dead"] != 1 || calls["/live"] != 3 {
		t.Fatalf("calls = %v, want one dead and three live", calls)
	}
	mu.Unlock()

	close(release)
	deadline := time.Now().Add(2 * time.Second)
	for {
		if r := pr.probe(ctx, dead); r.err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("mount stayed stale after the probe returned")
		}
		time.Sleep(5 * time.Millisecond)
	}
}
