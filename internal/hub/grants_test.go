package hub

import (
	"context"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// grantEnv is a job env with a token store and an audit log; "peer" is care
// unless the test moves it.
type grantEnv struct {
	*jobEnv
	store    *auth.Store
	auditDir string
	url      string
}

func newGrantEnv(t *testing.T) *grantEnv {
	t.Helper()
	e := newJobEnv(t, 1)
	store, err := auth.OpenStore(filepath.Join(e.home, "tokens.json"))
	if err != nil {
		t.Fatal(err)
	}
	g := &grantEnv{jobEnv: e, store: store, auditDir: filepath.Join(e.home, "audit")}
	au := auth.NewAudit(g.auditDir)
	t.Cleanup(func() { au.Close() })
	e.h.SetAuth(auth.New(testToken, store, au))
	srv := httptest.NewServer(e.h.Handler())
	t.Cleanup(srv.Close)
	g.url = srv.URL
	return g
}

func (g *grantEnv) token(t *testing.T, name string, scopes ...string) string {
	t.Helper()
	_, secret, err := g.store.Create(name, scopes, time.Hour, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	return secret
}

func (g *grantEnv) grants(tok string) protectoratev1connect.GrantServiceClient {
	return protectoratev1connect.NewGrantServiceClient(&http.Client{Transport: bearer{tok}}, g.url)
}

func (g *grantEnv) jobs(tok string) protectoratev1connect.JobServiceClient {
	return protectoratev1connect.NewJobServiceClient(&http.Client{Transport: bearer{tok}}, g.url)
}

func TestGrantAdminMinting(t *testing.T) {
	g := newGrantEnv(t)
	if _, err := g.h.nodes.inv.SetTier("peer", TierCare); err != nil {
		t.Fatal(err)
	}
	orch := g.token(t, sleeve.OrchestratorTokenName, "grant_admin")
	star := g.token(t, "star", "*")
	ttl := func(d time.Duration) *durationpb.Duration { return durationpb.New(d) }
	tests := []struct {
		name       string
		tok        string
		orchGrants bool
		req        *pb.GrantServiceAdminRequest
		want       connect.Code
		wantBy     string
	}{
		{"owner, care node", testToken, false, &pb.GrantServiceAdminRequest{Node: "peer", Task: "update kids machine"}, 0, auth.MintedByOwner},
		{"owner, glob", testToken, false, &pb.GrantServiceAdminRequest{Node: "pe*", Task: "t", Ttl: ttl(time.Hour)}, 0, auth.MintedByOwner},
		{"owner, no task", testToken, false, &pb.GrantServiceAdminRequest{Node: "peer"}, connect.CodeInvalidArgument, ""},
		{"owner, ttl over 24h", testToken, false, &pb.GrantServiceAdminRequest{Node: "peer", Task: "t", Ttl: ttl(25 * time.Hour)}, connect.CodeInvalidArgument, ""},
		{"star token", star, true, &pb.GrantServiceAdminRequest{Node: "hubhost", Task: "t"}, connect.CodePermissionDenied, ""},
		{"orchestrator, setting off", orch, false, &pb.GrantServiceAdminRequest{Node: "hubhost", Task: "t"}, connect.CodePermissionDenied, ""},
		{"orchestrator, shared node", orch, true, &pb.GrantServiceAdminRequest{Node: "hubhost", Task: "rebuild cache"}, 0, sleeve.OrchestratorTokenName},
		{"orchestrator, care node", orch, true, &pb.GrantServiceAdminRequest{Node: "peer", Task: "t"}, connect.CodePermissionDenied, ""},
		{"orchestrator, glob", orch, true, &pb.GrantServiceAdminRequest{Node: "hub*", Task: "t"}, connect.CodeInvalidArgument, ""},
		{"orchestrator, unknown node", orch, true, &pb.GrantServiceAdminRequest{Node: "ghost", Task: "t"}, connect.CodeNotFound, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			g.h.OrchestratorGrants = tt.orchGrants
			res, err := g.grants(tt.tok).Admin(context.Background(), connect.NewRequest(tt.req))
			if code(err) != tt.want {
				t.Fatalf("code %v (%v), want %v", code(err), err, tt.want)
			}
			if err != nil {
				return
			}
			tok := res.Msg.Token
			want := []string{"job_*", "node_admin:" + tt.req.Node}
			if !slices.Equal(tok.Scopes, want) || tok.MintedBy != tt.wantBy || tok.Task != tt.req.Task || !strings.HasPrefix(res.Msg.Secret, auth.SecretPrefix) {
				t.Fatalf("grant %v", tok)
			}
			life := tok.ExpiresAt.AsTime().Sub(tok.CreatedAt.AsTime())
			if wantLife := cmpDur(tt.req.Ttl, defaultGrantTTL); life != wantLife {
				t.Fatalf("lifetime %s, want %s", life, wantLife)
			}
		})
	}
	g.h.OrchestratorGrants = false
}

func cmpDur(d *durationpb.Duration, def time.Duration) time.Duration {
	if d == nil {
		return def
	}
	return d.AsDuration()
}

func TestAdminJobs(t *testing.T) {
	g := newGrantEnv(t)
	if _, err := g.h.nodes.inv.SetTier("peer", TierCare); err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	mint := func(node string) string {
		res, err := g.grants(testToken).Admin(ctx, connect.NewRequest(&pb.GrantServiceAdminRequest{Node: node, Task: "update kids machine"}))
		if err != nil {
			t.Fatal(err)
		}
		return res.Msg.Secret
	}
	peerGrant, hubGrant := mint("peer"), mint("hubhost")
	plain := g.token(t, "plain", "job_*")
	tests := []struct {
		name     string
		tok      string
		target   *pb.JobTarget
		admin    bool
		want     connect.Code
		wantMode pb.JobMode
		wantIso  string
	}{
		{"grant, admin on care", peerGrant, onNode("peer"), true, 0, pb.JobMode_JOB_MODE_ADMIN, "root"},
		{"grant, plain job on care", peerGrant, onNode("peer"), false, 0, pb.JobMode_JOB_MODE_PLAIN, "plain"},
		{"owner, admin on care", testToken, onNode("peer"), true, 0, pb.JobMode_JOB_MODE_ADMIN, "root"},
		{"job token, admin on shared", plain, onNode("hubhost"), true, connect.CodePermissionDenied, 0, ""},
		{"job token, sandboxed on shared", plain, onNode("hubhost"), false, 0, pb.JobMode_JOB_MODE_SANDBOXED, "none: no admin jobs: test"},
		{"grant, node cannot elevate", hubGrant, onNode("hubhost"), true, connect.CodeFailedPrecondition, 0, ""},
		{"grant for another node", hubGrant, onNode("peer"), true, connect.CodePermissionDenied, 0, ""},
		{"admin with a selector", testToken, &pb.JobTarget{Target: &pb.JobTarget_Selector{Selector: &pb.NodeSelector{}}}, true, connect.CodeInvalidArgument, 0, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			res, err := g.jobs(tt.tok).SubmitJob(ctx, connect.NewRequest(&pb.SubmitJobRequest{Target: tt.target, Command: []string{"true"}, Admin: tt.admin}))
			if code(err) != tt.want {
				t.Fatalf("code %v (%v), want %v", code(err), err, tt.want)
			}
			if err != nil {
				return
			}
			j := g.wait(t, res.Msg.Id)
			if _, err := g.jobs(tt.tok).GetJob(ctx, connect.NewRequest(&pb.GetJobRequest{Id: j.Id})); err != nil {
				t.Fatal(err)
			}
			if j.State != pb.JobState_JOB_STATE_SUCCEEDED || j.Mode != tt.wantMode || j.Isolation != tt.wantIso {
				t.Fatalf("job %v %v %q (%s)", j.State, j.Mode, j.Isolation, j.Error)
			}
		})
	}

	page, err := auth.ReadPage(g.auditDir, auth.Query{Limit: 1000})
	if err != nil {
		t.Fatal(err)
	}
	var adminLine, readLine bool
	for _, en := range page.Entries {
		if en.Task != "update kids machine" {
			continue
		}
		if strings.HasSuffix(en.Procedure, "/SubmitJob") && en.Outcome == auth.OutcomeOK && jobIDRe.MatchString(en.Subject) && strings.Contains(en.Detail, "node peer, admin (elevated, unsandboxed), command \"true\"") {
			adminLine = true
		}
		if strings.HasSuffix(en.Procedure, "/GetJob") {
			readLine = true
		}
	}
	if !adminLine || !readLine {
		t.Fatalf("grant use not audited with its task (admin %v, read %v): %+v", adminLine, readLine, page.Entries)
	}
}
