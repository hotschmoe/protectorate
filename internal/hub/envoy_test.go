//go:build linux

package hub

import (
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/orchestrator"
)

func envoyConfig() EnvoyConfig {
	return EnvoyConfig{
		Commands: []orchestrator.Command{{Path: "sleeve ls", Use: "sleeve ls", Short: "List sleeves"}},
		Tools: []orchestrator.Tool{
			{Name: "sleeve_spawn_sleeve", Description: "SpawnSleeve starts a harness."},
			{Name: "sleeve_list_sleeves", Description: "ListSleeves lists.", ReadOnly: true},
		},
	}
}

func newEnvoyRig(t *testing.T) *chatRig {
	t.Helper()
	r := newChatRig(t)
	r.h.SelfURL = "http://127.0.0.1:1"
	r.h.SetEnvoy(envoyConfig())
	t.Cleanup(func() {
		exec.Command("systemctl", "--user", "kill", "--signal=SIGKILL", r.cfg.UnitName(orchestrator.SleeveName)+".scope").Run()
	})
	return r
}

func (r *chatRig) spawnEnvoy(req *pb.SpawnEnvoyRequest) (*pb.SpawnEnvoyResponse, error) {
	r.t.Helper()
	res, err := r.sl.SpawnEnvoy(r.ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

func envoyTokens(st *auth.Store, name string) (live []auth.Token) {
	for _, t := range st.List() {
		if t.Name == name && t.RevokedAt == nil {
			live = append(live, t)
		}
	}
	return live
}

func TestSpawnEnvoyOwnerOnly(t *testing.T) {
	r := newEnvoyRig(t)
	_, secret, err := r.h.authz.Store().Create("agent", []string{"*", "read"}, time.Hour, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	c := protectoratev1connect.NewSleeveServiceClient(&http.Client{Transport: bearer{secret}}, r.srv.URL)
	_, err = c.SpawnEnvoy(r.ctx, connect.NewRequest(&pb.SpawnEnvoyRequest{Harness: pb.Harness_HARNESS_CLAUDE}))
	if code(err) != connect.CodePermissionDenied {
		t.Fatalf("scoped spawn err = %v", err)
	}
	if _, err := r.h.sleeves.Get(orchestrator.SleeveName); err == nil {
		t.Fatal("a denied call started Envoy")
	}
	_, err = r.spawnEnvoy(&pb.SpawnEnvoyRequest{Harness: pb.Harness_HARNESS_SHELL})
	if code(err) != connect.CodeInvalidArgument {
		t.Fatalf("shell err = %v", err)
	}
}

func TestSpawnEnvoyAndRenew(t *testing.T) {
	r := newEnvoyRig(t)
	st := r.h.authz.Store()
	ws := filepath.Join(r.cfg.Home, orchestrator.DirName)
	tokenPath := filepath.Join(ws, orchestrator.TokenFile)

	first, err := r.spawnEnvoy(&pb.SpawnEnvoyRequest{Harness: pb.Harness_HARNESS_CLAUDE, Ttl: durationpb.New(time.Hour), AddScopes: []string{"events"}})
	if err != nil {
		t.Fatal(err)
	}
	if first.Sleeve.Name != "envoy" || first.Workspace != ws || first.Replaced || first.Sleeve.Mode != pb.SleeveMode_SLEEVE_MODE_CHAT {
		t.Fatalf("response = %v", first)
	}
	agents1, err := os.ReadFile(filepath.Join(ws, "AGENTS.md"))
	if err != nil || !strings.Contains(string(agents1), "sleeve_spawn_sleeve") {
		t.Fatalf("AGENTS.md = %q, %v", agents1, err)
	}
	secret1, _ := os.ReadFile(tokenPath)
	live := envoyTokens(st, orchestrator.TokenName)
	if len(live) != 1 || live[0].ID != first.Token.Id {
		t.Fatalf("live tokens = %v", live)
	}
	if !auth.Grants(live[0].Scopes, "sleeve_spawn_sleeve") || auth.Grants(live[0].Scopes, "sleeve_spawn_envoy") {
		t.Fatalf("scopes = %v", live[0].Scopes)
	}

	// a respawn on the same inputs keeps AGENTS.md byte for byte
	second, err := r.spawnEnvoy(&pb.SpawnEnvoyRequest{Harness: pb.Harness_HARNESS_CLAUDE, Ttl: durationpb.New(time.Hour), AddScopes: []string{"events"}})
	if err != nil {
		t.Fatal(err)
	}
	if !second.Replaced || second.Token.Id == first.Token.Id {
		t.Fatalf("second = %v", second)
	}
	if agents2, _ := os.ReadFile(filepath.Join(ws, "AGENTS.md")); string(agents2) != string(agents1) {
		t.Fatalf("AGENTS.md changed across respawns:\n%s\n---\n%s", agents1, agents2)
	}
	secret2, _ := os.ReadFile(tokenPath)
	if _, err := st.Lookup(strings.TrimSpace(string(secret1)), time.Now()); err == nil {
		t.Error("first token survived the respawn")
	}

	before, err := r.h.sleeves.Get(orchestrator.SleeveName)
	if err != nil {
		t.Fatal(err)
	}
	// renewal: nothing early on, then a new token in the same file, the old
	// one revoked only after the grace period
	created := second.Token.CreatedAt.AsTime()
	steps := []struct {
		name     string
		at       time.Duration
		wantLive int
		rotated  bool
	}{
		{"early", 10 * time.Minute, 1, false},
		{"near the end", 50 * time.Minute, 2, true},
		{"within grace", 52 * time.Minute, 2, false},
		{"after grace", 56 * time.Minute, 1, false},
	}
	var cur = strings.TrimSpace(string(secret2))
	for _, s := range steps {
		if err := r.h.renewEnvoy(created.Add(s.at)); err != nil {
			t.Fatalf("%s: %v", s.name, err)
		}
		if n := len(envoyTokens(st, orchestrator.TokenName)); n != s.wantLive {
			t.Fatalf("%s: %d live tokens, want %d", s.name, n, s.wantLive)
		}
		b, _ := os.ReadFile(tokenPath)
		now := strings.TrimSpace(string(b))
		if (now != cur) != s.rotated {
			t.Fatalf("%s: token file changed = %t, want %t", s.name, now != cur, s.rotated)
		}
		cur = now
		if _, err := st.Lookup(cur, created.Add(s.at)); err != nil {
			t.Fatalf("%s: the token in the file does not authenticate: %v", s.name, err)
		}
	}
	if _, err := st.Lookup(strings.TrimSpace(string(secret2)), time.Now()); err == nil {
		t.Error("replaced token still authenticates")
	}
	if got, _ := os.ReadFile(filepath.Join(ws, "AGENTS.md")); string(got) != string(agents1) {
		t.Error("renewal changed AGENTS.md")
	}
	if after, err := r.h.sleeves.Get(orchestrator.SleeveName); err != nil || !after.Created.Equal(before.Created) || after.State != before.State {
		t.Fatalf("renewal touched the sleeve: %+v, %v", after, err)
	}
}

func TestPlanEnvoyRenewal(t *testing.T) {
	base := time.Date(2026, 10, 20, 12, 0, 0, 0, time.UTC)
	at := func(d time.Duration) *time.Time { v := base.Add(d); return &v }
	tok := func(id string, created time.Duration, exp *time.Time, revoked bool) auth.Token {
		t := auth.Token{ID: id, Name: orchestrator.TokenName, Created: base.Add(created), Expires: exp}
		if revoked {
			t.RevokedAt = at(created)
		}
		return t
	}
	other := auth.Token{ID: "x", Name: "someone", Created: base, Expires: at(time.Minute)}
	tests := []struct {
		name       string
		tokens     []auth.Token
		now        time.Duration
		wantRotate string
		wantRevoke []string
	}{
		{"none", []auth.Token{other}, 0, "", nil},
		{"fresh", []auth.Token{tok("a", 0, at(100*time.Hour), false)}, 10 * time.Hour, "", nil},
		{"last quarter", []auth.Token{tok("a", 0, at(100*time.Hour), false)}, 75 * time.Hour, "a", nil},
		{"expired but unrevoked", []auth.Token{tok("a", 0, at(100*time.Hour), false)}, 200 * time.Hour, "a", nil},
		{"never expires", []auth.Token{tok("a", 0, nil, false)}, 9999 * time.Hour, "", nil},
		{"revoked is left alone", []auth.Token{tok("a", 0, at(100*time.Hour), true)}, 99 * time.Hour, "", nil},
		{"replaced inside grace", []auth.Token{tok("a", 0, at(100*time.Hour), false), tok("b", 76*time.Hour, at(176*time.Hour), false)}, 76*time.Hour + time.Minute, "", nil},
		{"replaced past grace", []auth.Token{tok("a", 0, at(100*time.Hour), false), tok("b", 76*time.Hour, at(176*time.Hour), false)}, 76*time.Hour + envoyGrace, "", []string{"a"}},
		{"newest nears its end too", []auth.Token{tok("a", 0, at(100*time.Hour), false), tok("b", 76*time.Hour, at(80*time.Hour), false)}, 79 * time.Hour, "b", []string{"a"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rotate, revoke := planEnvoyRenewal(tt.tokens, base.Add(tt.now))
			got := ""
			if rotate != nil {
				got = rotate.ID
			}
			if got != tt.wantRotate {
				t.Errorf("rotate = %q, want %q", got, tt.wantRotate)
			}
			var ids []string
			for _, r := range revoke {
				ids = append(ids, r.ID)
			}
			if strings.Join(ids, ",") != strings.Join(tt.wantRevoke, ",") {
				t.Errorf("revoke = %v, want %v", ids, tt.wantRevoke)
			}
		})
	}
}

func TestMigrateEnvoyTokens(t *testing.T) {
	tests := []struct {
		name       string
		sleeve     bool
		haveNew    bool
		legacyTTL  time.Duration
		wantLive   int
		wantFile   bool
		wantScopes []string
	}{
		{name: "no sleeve record", legacyTTL: time.Hour},
		{name: "sleeve without a token of its own", sleeve: true, legacyTTL: time.Hour, wantLive: 1, wantFile: true, wantScopes: []string{"read", "sleeve_spawn_sleeve"}},
		{name: "never expiring token carries over", sleeve: true, wantLive: 1, wantFile: true, wantScopes: []string{"read", "sleeve_spawn_sleeve"}},
		{name: "sleeve with a new token", sleeve: true, haveNew: true, legacyTTL: time.Hour, wantLive: 1},
		{name: "legacy token expired", sleeve: true, legacyTTL: -time.Hour},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := newEnvoyRig(t)
			st := r.h.authz.Store()
			ws := filepath.Join(r.cfg.Home, orchestrator.DirName)
			if tt.sleeve {
				if _, err := r.spawnEnvoy(&pb.SpawnEnvoyRequest{Harness: pb.Harness_HARNESS_CLAUDE}); err != nil {
					t.Fatal(err)
				}
				if !tt.haveNew {
					for _, tk := range envoyTokens(st, orchestrator.TokenName) {
						if _, err := st.Revoke(tk.ID, time.Now()); err != nil {
							t.Fatal(err)
						}
					}
					if err := os.WriteFile(filepath.Join(ws, orchestrator.TokenFile), []byte("stale\n"), 0o600); err != nil {
						t.Fatal(err)
					}
				}
			}
			now := time.Now()
			ttl := tt.legacyTTL
			create := ttl
			if ttl < 0 {
				create = time.Hour
			}
			_, _, err := st.Create(previousEnvoyToken, []string{"read", "sleeve_spawn_sleeve"}, create, now)
			if err != nil {
				t.Fatal(err)
			}
			at := now
			if ttl < 0 {
				at = now.Add(2 * time.Hour)
			}
			r.h.migrateEnvoyTokens(at)
			if n := len(envoyTokens(st, previousEnvoyToken)); n != 0 {
				t.Errorf("%d previous-name tokens still live", n)
			}
			live := envoyTokens(st, orchestrator.TokenName)
			if len(live) != tt.wantLive {
				t.Fatalf("live new-name tokens = %d, want %d", len(live), tt.wantLive)
			}
			if tt.wantFile {
				b, _ := os.ReadFile(filepath.Join(ws, orchestrator.TokenFile))
				if _, err := st.Lookup(strings.TrimSpace(string(b)), at); err != nil {
					t.Errorf("the token file does not hold a working token: %v", err)
				}
				if strings.Join(live[0].Scopes, ",") != strings.Join(tt.wantScopes, ",") {
					t.Errorf("scopes = %v", live[0].Scopes)
				}
			}
		})
	}
}
