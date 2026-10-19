package hub

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/jobtest"
	"github.com/hotschmoe/protectorate/internal/node"
)

// recordingAdmin is a tailnet that hands out numbered single-use keys and
// records expiries and revocations; pool "gpu" is configured.
type recordingAdmin struct {
	mu       sync.Mutex
	next     uint64
	invites  map[uint64]string
	expired  []uint64
	revoked  []string
	enrolled map[string]bool
}

func newRecordingAdmin() *recordingAdmin {
	return &recordingAdmin{invites: map[uint64]string{}, enrolled: map[string]bool{}}
}

func (a *recordingAdmin) ControlURL() string { return "http://hs.lan:7480" }

func (a *recordingAdmin) InviteNode(_ context.Context, name, tier string, pools []string, _ time.Duration) (string, uint64, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.enrolled[name] {
		return "", 0, fmt.Errorf("name %q is already in use (revoke it first)", name)
	}
	for _, p := range pools {
		if p != "gpu" {
			return "", 0, fmt.Errorf("pool %q is not configured", p)
		}
	}
	a.next++
	a.invites[a.next] = tier + " " + strings.Join(pools, ",")
	return fmt.Sprintf("hskey-auth-%d-0123456789abcdef", a.next), a.next, nil
}

func (a *recordingAdmin) ExpireKey(_ context.Context, id uint64) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.expired = append(a.expired, id)
	return nil
}

func (a *recordingAdmin) Revoke(_ context.Context, name string) (bool, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if !a.enrolled[name] {
		return false, nil
	}
	delete(a.enrolled, name)
	a.revoked = append(a.revoked, name)
	return true, nil
}

func (a *recordingAdmin) enroll(name string) {
	a.mu.Lock()
	a.enrolled[name] = true
	a.mu.Unlock()
}

func (a *recordingAdmin) snapshot() (expired []uint64, revoked []string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	return slices.Clone(a.expired), slices.Clone(a.revoked)
}

type inviteEnv struct {
	h     *Hub
	srv   *httptest.Server
	tn    *recordingAdmin
	store *auth.Store
	peers atomic.Pointer[[]Peer]
	// up makes the fake node agents answer polls with version.
	up      atomic.Bool
	version atomic.Pointer[string]
}

// newInviteEnv is a hub with a tailnet fake, a token store for scoped tokens
// and node polls that answer only once up is set.
func newInviteEnv(t *testing.T, withTailnet bool) *inviteEnv {
	t.Helper()
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	e := &inviteEnv{tn: newRecordingAdmin()}
	e.peers.Store(&[]Peer{})
	v := "test"
	e.version.Store(&v)
	store, err := auth.OpenStore(filepath.Join(home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	e.store = store
	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "peer", Address: "peer"}); err != nil {
		t.Fatal(err)
	}
	e.h = New("test", testToken)
	e.h.setupPoll = 10 * time.Millisecond
	e.h.SetAuth(auth.New(testToken, store, nil))
	m := NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}), NodeOptions{
		Interval: time.Hour,
		Peers:    func(context.Context) ([]Peer, error) { return *e.peers.Load(), nil },
	})
	m.fetchRemote = func(_ context.Context, n *pb.NodeInfo) (*pb.NodeMetrics, error) {
		if !e.up.Load() {
			return nil, connect.NewError(connect.CodeUnavailable, errors.New("no route"))
		}
		return &pb.NodeMetrics{Os: "linux", Arch: "arm64", AgentVersion: *e.version.Load()}, nil
	}
	e.h.SetNodes(m)
	if withTailnet {
		e.h.SetTailnetAdmin(e.tn)
	}
	e.srv = httptest.NewServer(e.h.Handler())
	t.Cleanup(e.srv.Close)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); m.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	return e
}

func (e *inviteEnv) client(tok string) protectoratev1connect.NodeServiceClient {
	return protectoratev1connect.NewNodeServiceClient(&http.Client{Transport: bearer{tok}}, e.srv.URL)
}

func (e *inviteEnv) mint(t *testing.T, scopes ...string) string {
	t.Helper()
	_, secret, err := e.store.Create("t", scopes, time.Hour, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	return secret
}

func TestInviteNodeChecks(t *testing.T) {
	e := newInviteEnv(t, true)
	star, read := e.mint(t, "*"), e.mint(t, "read")
	tests := []struct {
		name string
		tok  string
		req  *pb.InviteNodeRequest
		want connect.Code
	}{
		{"owner", testToken, &pb.InviteNodeRequest{Name: "office-arm"}, 0},
		{"star token is not the owner", star, &pb.InviteNodeRequest{Name: "x1"}, connect.CodePermissionDenied},
		{"read token", read, &pb.InviteNodeRequest{Name: "x2"}, connect.CodePermissionDenied},
		{"quote in name", testToken, &pb.InviteNodeRequest{Name: "a'b"}, connect.CodeInvalidArgument},
		{"command substitution", testToken, &pb.InviteNodeRequest{Name: "$(id)"}, connect.CodeInvalidArgument},
		{"space", testToken, &pb.InviteNodeRequest{Name: "a b"}, connect.CodeInvalidArgument},
		{"upper case", testToken, &pb.InviteNodeRequest{Name: "Office"}, connect.CodeInvalidArgument},
		{"dot", testToken, &pb.InviteNodeRequest{Name: "a.b"}, connect.CodeInvalidArgument},
		{"underscore", testToken, &pb.InviteNodeRequest{Name: "a_b"}, connect.CodeInvalidArgument},
		{"leading dash", testToken, &pb.InviteNodeRequest{Name: "-x"}, connect.CodeInvalidArgument},
		{"too long", testToken, &pb.InviteNodeRequest{Name: strings.Repeat("a", 64)}, connect.CodeInvalidArgument},
		{"empty", testToken, &pb.InviteNodeRequest{}, connect.CodeInvalidArgument},
		{"hub's own name", testToken, &pb.InviteNodeRequest{Name: testSelf.Name}, connect.CodeAlreadyExists},
		{"joined node", testToken, &pb.InviteNodeRequest{Name: "peer"}, connect.CodeAlreadyExists},
		{"unknown tier", testToken, &pb.InviteNodeRequest{Name: "x3", Tier: "root"}, connect.CodeInvalidArgument},
		{"unknown pool", testToken, &pb.InviteNodeRequest{Name: "x4", Pools: []string{"nope"}}, connect.CodeFailedPrecondition},
		{"ttl too long", testToken, &pb.InviteNodeRequest{Name: "x5", Ttl: durationpb.New(48 * time.Hour)}, connect.CodeInvalidArgument},
		{"install url with a path", testToken, &pb.InviteNodeRequest{Name: "x6", InstallUrl: "http://hub/x"}, connect.CodeInvalidArgument},
		{"name already on the tailnet", testToken, &pb.InviteNodeRequest{Name: "taken"}, connect.CodeFailedPrecondition},
	}
	e.tn.enroll("taken")
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := e.client(tt.tok).InviteNode(context.Background(), connect.NewRequest(tt.req))
			if code(err) != tt.want {
				t.Fatalf("code %v (%v), want %v", code(err), err, tt.want)
			}
		})
	}
	if _, err := e.h.nodes.inv.Get("x1"); err == nil {
		t.Fatal("a refused invite reached the inventory")
	}

	t.Run("no tailnet", func(t *testing.T) {
		e := newInviteEnv(t, false)
		_, err := e.client(testToken).InviteNode(context.Background(), connect.NewRequest(&pb.InviteNodeRequest{Name: "box"}))
		if code(err) != connect.CodeFailedPrecondition {
			t.Fatalf("code %v", code(err))
		}
	})
}

func TestInviteNodeRecordsAndReissues(t *testing.T) {
	e := newInviteEnv(t, true)
	c := e.client(testToken)
	ctx := context.Background()
	res, err := c.InviteNode(ctx, connect.NewRequest(&pb.InviteNodeRequest{Name: "office", Tier: "family", Pools: []string{"gpu"}}))
	if err != nil {
		t.Fatal(err)
	}
	r := res.Msg
	if r.Key != "hskey-auth-1-0123456789abcdef" || r.ControlUrl != "http://hs.lan:7480" || r.InstallUrl != e.srv.URL || len(r.Commands) != 3 {
		t.Fatalf("response %v", r)
	}
	for _, cmd := range r.Commands {
		if !strings.Contains(cmd.Command, r.Key) || !strings.Contains(cmd.Command, e.srv.URL+"/install.") {
			t.Errorf("%s command lacks the key or the request's URL: %s", cmd.Platform, cmd.Command)
		}
	}
	if d := time.Until(r.ExpiresAt.AsTime()); d < 59*time.Minute || d > time.Hour {
		t.Errorf("expires in %s", d)
	}
	if got := e.tn.invites[1]; got != "family gpu" {
		t.Errorf("invited as %q", got)
	}
	info, err := e.h.nodes.inv.Get("office")
	if err != nil || !info.Invited || info.InviteKeyId != 1 || info.Tier != "family" || !slices.Equal(info.Tags, []string{"pool-gpu"}) {
		t.Fatalf("inventory %v %v", info, err)
	}

	res, err = c.InviteNode(ctx, connect.NewRequest(&pb.InviteNodeRequest{Name: "office", InstallUrl: "https://hub.example/"}))
	if err != nil {
		t.Fatal(err)
	}
	if res.Msg.Key == r.Key || res.Msg.InstallUrl != "https://hub.example" {
		t.Fatalf("reissue %v", res.Msg)
	}
	if expired, _ := e.tn.snapshot(); !slices.Equal(expired, []uint64{1}) {
		t.Fatalf("expired %v, want the first key", expired)
	}
	if info, _ := e.h.nodes.inv.Get("office"); info.InviteKeyId != 2 {
		t.Fatalf("key id %d", info.InviteKeyId)
	}

	e.h.PublicURL = "http://public.lan:7470"
	res, err = c.InviteNode(ctx, connect.NewRequest(&pb.InviteNodeRequest{Name: "other"}))
	if err != nil || res.Msg.InstallUrl != "http://public.lan:7470" {
		t.Fatalf("public url: %v %v", res, err)
	}
}

func TestWatchNodeSetup(t *testing.T) {
	e := newInviteEnv(t, true)
	c := e.client(testToken)
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	inv, err := c.InviteNode(ctx, connect.NewRequest(&pb.InviteNodeRequest{Name: "box"}))
	if err != nil {
		t.Fatal(err)
	}
	stream, err := c.WatchNodeSetup(ctx, connect.NewRequest(&pb.WatchNodeSetupRequest{Name: "box"}))
	if err != nil {
		t.Fatal(err)
	}
	next := func() *pb.NodeSetupProgress {
		t.Helper()
		if !stream.Receive() {
			t.Fatalf("stream ended: %v", stream.Err())
		}
		return stream.Msg()
	}
	if p := next(); p.Stage != pb.NodeSetupStage_NODE_SETUP_STAGE_INVITED {
		t.Fatalf("first %v", p)
	}
	e.peers.Store(&[]Peer{{Name: "box", KeyID: inv.Msg.Node.InviteKeyId}})
	if p := next(); p.Stage != pb.NodeSetupStage_NODE_SETUP_STAGE_JOINED {
		t.Fatalf("after enroll %v", p)
	}
	old := "v0.9"
	e.version.Store(&old)
	e.up.Store(true)
	p := next()
	if p.Stage != pb.NodeSetupStage_NODE_SETUP_STAGE_ATTENTION || len(p.Problems) != 1 || !strings.Contains(p.Problems[0], "v0.9") {
		t.Fatalf("old agent %v", p)
	}
	cur := "test"
	e.version.Store(&cur)
	e.h.nodes.kick(p.Status.Node)
	if p := next(); p.Stage != pb.NodeSetupStage_NODE_SETUP_STAGE_READY || p.Status.Node.Invited {
		t.Fatalf("ready %v", p)
	}
	if stream.Receive() {
		t.Fatalf("stream goes on after READY: %v", stream.Msg())
	}
	if err := stream.Err(); err != nil {
		t.Fatal(err)
	}
}

func TestSetupChecks(t *testing.T) {
	fresh := pb.NodeFreshness_NODE_FRESHNESS_FRESH
	win := &pb.NodeMetrics{Os: "windows", AgentVersion: "v1"}
	tests := []struct {
		name          string
		st            *pb.NodeStatus
		desk          bool
		problems      int
		notes         int
		pending       bool
		problemSubstr string
	}{
		{"linux ready", &pb.NodeStatus{Freshness: fresh, Metrics: &pb.NodeMetrics{Os: "linux", AgentVersion: "v1"}}, true, 0, 0, false, ""},
		{"version skew", &pb.NodeStatus{Freshness: fresh, Metrics: &pb.NodeMetrics{Os: "linux", AgentVersion: "v0"}}, true, 1, 0, false, "update the node"},
		{"stale", &pb.NodeStatus{Freshness: pb.NodeFreshness_NODE_FRESHNESS_STALE, Error: "timeout", Metrics: &pb.NodeMetrics{AgentVersion: "v1"}}, true, 1, 0, false, "timeout"},
		{"windows desk not reported", &pb.NodeStatus{Freshness: fresh, Metrics: win}, true, 0, 0, true, ""},
		{"windows desk check off", &pb.NodeStatus{Freshness: fresh, Metrics: win}, false, 0, 0, false, ""},
		{"windows not interactive", &pb.NodeStatus{Freshness: fresh, Metrics: win, Desk: &pb.DeskState{Reason: pb.DeskReason_DESK_REASON_NOT_INTERACTIVE}}, true, 1, 0, false, "logon task"},
		{"windows desk unknown", &pb.NodeStatus{Freshness: fresh, Metrics: win, Desk: &pb.DeskState{Reason: pb.DeskReason_DESK_REASON_UNKNOWN, Detail: "x"}}, true, 0, 1, false, ""},
		{"warnings and alerts are notes", &pb.NodeStatus{Freshness: fresh, Metrics: &pb.NodeMetrics{AgentVersion: "v1", Warnings: []string{"no gpu tool"}},
			Alerts: []*pb.NodeAlert{{Kind: pb.AlertKind_ALERT_KIND_DISK_PERCENT, Message: "/ is 95% full"}}}, true, 0, 2, false, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			problems, notes, pending := setupChecks(tt.st, "v1", tt.desk)
			if len(problems) != tt.problems || len(notes) != tt.notes || (pending != "") != tt.pending {
				t.Fatalf("problems %q notes %q pending %q", problems, notes, pending)
			}
			if tt.problemSubstr != "" && !strings.Contains(problems[0], tt.problemSubstr) {
				t.Fatalf("problem %q lacks %q", problems[0], tt.problemSubstr)
			}
		})
	}
}

func removeAll(t *testing.T, c protectoratev1connect.NodeServiceClient, req *pb.RemoveNodeRequest) ([]*pb.RemoveNodeProgress, error) {
	t.Helper()
	stream, err := c.RemoveNode(context.Background(), connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	defer stream.Close()
	var out []*pb.RemoveNodeProgress
	for stream.Receive() {
		out = append(out, stream.Msg())
	}
	return out, stream.Err()
}

func TestRemoveNodeOwnerOnly(t *testing.T) {
	e := newInviteEnv(t, true)
	for _, tok := range []string{e.mint(t, "*"), e.mint(t, "read")} {
		if _, err := removeAll(t, e.client(tok), &pb.RemoveNodeRequest{Name: "peer"}); code(err) != connect.CodePermissionDenied {
			t.Fatalf("code %v", code(err))
		}
	}
	if _, err := e.h.nodes.inv.Get("peer"); err != nil {
		t.Fatal("peer removed by a scoped token")
	}
	if _, err := removeAll(t, e.client(testToken), &pb.RemoveNodeRequest{Name: testSelf.Name}); code(err) != connect.CodeFailedPrecondition {
		t.Fatalf("hub's own node: %v", err)
	}
	if _, err := removeAll(t, e.client(testToken), &pb.RemoveNodeRequest{Name: "ghost"}); code(err) != connect.CodeNotFound {
		t.Fatalf("unknown node: %v", err)
	}
}

func TestRemoveNodeCancelsPendingInvite(t *testing.T) {
	e := newInviteEnv(t, true)
	c := e.client(testToken)
	inv, err := c.InviteNode(context.Background(), connect.NewRequest(&pb.InviteNodeRequest{Name: "box"}))
	if err != nil {
		t.Fatal(err)
	}
	steps, err := removeAll(t, c, &pb.RemoveNodeRequest{Name: "box", Uninstall: true})
	if err != nil {
		t.Fatal(err)
	}
	var msgs []string
	for _, s := range steps {
		msgs = append(msgs, s.Phase.String()+": "+s.Message)
	}
	all := strings.Join(msgs, "\n")
	for _, w := range []string{"not reachable", "expired the invite key", "not enrolled in the tailnet", "REMOVE_PHASE_REMOVED"} {
		if !strings.Contains(all, w) {
			t.Errorf("steps lack %q:\n%s", w, all)
		}
	}
	if expired, _ := e.tn.snapshot(); !slices.Equal(expired, []uint64{inv.Msg.Node.InviteKeyId}) {
		t.Fatalf("expired %v", expired)
	}
	if _, err := e.h.nodes.inv.Get("box"); !errors.Is(err, ErrNodeNotFound) {
		t.Fatalf("box still in the inventory: %v", err)
	}
}

func TestRemoveNodeDrains(t *testing.T) {
	tests := []struct {
		name      string
		jobs      pb.RemoveJobs
		uninstall bool
		wantCode  connect.Code
		wantState pb.JobState
	}{
		{"refused while a job runs", pb.RemoveJobs_REMOVE_JOBS_UNSPECIFIED, false, connect.CodeFailedPrecondition, pb.JobState_JOB_STATE_SUCCEEDED},
		{"wait lets the job finish", pb.RemoveJobs_REMOVE_JOBS_WAIT, true, 0, pb.JobState_JOB_STATE_SUCCEEDED},
		{"cancel stops the job", pb.RemoveJobs_REMOVE_JOBS_CANCEL, false, 0, pb.JobState_JOB_STATE_CANCELED},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			e := newJobEnv(t, 2)
			e.tn.enroll("peer")
			nc := protectoratev1connect.NewNodeServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL)
			job := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:started", "sleep:1.5")})
			waitUntil(t, 10*time.Second, "job running", func() bool { return e.get(t, job.Id).State == pb.JobState_JOB_STATE_RUNNING })

			type result struct {
				steps []*pb.RemoveNodeProgress
				err   error
			}
			done := make(chan result, 1)
			go func() {
				s, err := removeAll(t, nc, &pb.RemoveNodeRequest{Name: "peer", Jobs: tt.jobs, Uninstall: tt.uninstall})
				done <- result{s, err}
			}()
			if tt.jobs == pb.RemoveJobs_REMOVE_JOBS_WAIT {
				waitUntil(t, 5*time.Second, "drain", func() bool { s, _ := e.h.nodes.Status("peer"); return s.GetDraining() })
				_, err := e.jc.SubmitJob(context.Background(), connect.NewRequest(&pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:late")}))
				if code(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), "being removed") {
					t.Errorf("job submitted to a draining node: %v", err)
				}
				sel := &pb.JobTarget{Target: &pb.JobTarget_Selector{Selector: &pb.NodeSelector{Os: runtime.GOOS}}}
				if j, err := e.jc.SubmitJob(context.Background(), connect.NewRequest(&pb.SubmitJobRequest{Target: sel, Command: jobtest.Script("say:any")})); err == nil && j.Msg.Node == "peer" {
					t.Error("a selector placed a job on the draining node")
				}
				_, err = e.lc.AcquireLease(context.Background(), connect.NewRequest(&pb.AcquireLeaseRequest{Holder: "x", Ttl: durationpb.New(time.Minute), Target: &pb.AcquireLeaseRequest_Resource{Resource: "peer"}}))
				if err == nil {
					t.Error("leased a draining node")
				}
			}
			r := <-done
			if code(r.err) != tt.wantCode {
				t.Fatalf("code %v (%v)", code(r.err), r.err)
			}
			if j := e.wait(t, job.Id); j.State != tt.wantState {
				t.Fatalf("job %v", j.State)
			}
			_, revoked := e.tn.snapshot()
			if tt.wantCode != 0 {
				s, err := e.h.nodes.Status("peer")
				if err != nil || s.Draining || len(revoked) != 0 {
					t.Fatalf("refused removal left peer draining=%v revoked=%v (%v)", s.GetDraining(), revoked, err)
				}
				return
			}
			last := r.steps[len(r.steps)-1]
			if last.Phase != pb.RemovePhase_REMOVE_PHASE_REMOVED || !slices.Equal(revoked, []string{"peer"}) {
				t.Fatalf("last %v revoked %v", last, revoked)
			}
			if _, err := e.h.nodes.Status("peer"); err == nil {
				t.Fatal("peer still listed")
			}
			if tt.uninstall {
				waitUntil(t, 5*time.Second, "agent uninstall", e.uninstalled.Load)
				var drained bool
				for _, s := range r.steps {
					drained = drained || (s.Phase == pb.RemovePhase_REMOVE_PHASE_DRAINING && s.RunningJobs == 1)
				}
				if !drained {
					t.Errorf("no draining step with the running job: %v", r.steps)
				}
			}
		})
	}
}
