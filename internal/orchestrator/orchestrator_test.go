package orchestrator

import (
	"context"
	"flag"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/mcpbridge"
)

var update = flag.Bool("update", false, "rewrite testdata/AGENTS.golden.md")

func realTools(t *testing.T) []Tool {
	t.Helper()
	cat, err := mcpbridge.Catalog()
	if err != nil {
		t.Fatal(err)
	}
	var out []Tool
	for _, c := range cat {
		out = append(out, Tool{Name: c.Name, Description: c.Description, ReadOnly: c.ReadOnly, Destructive: c.Destructive})
	}
	return out
}

func TestDefaultScopes(t *testing.T) {
	scopes := DefaultScopes(realTools(t))
	if _, err := auth.ValidateScopes(scopes); err != nil {
		t.Fatalf("default scopes are not valid: %v", err)
	}
	for _, tool := range []string{
		"quota_suggest", "quota_get_quota", "sleeve_list_sleeves", "session_list_sessions", "session_capture_pane",
		"sleeve_spawn_sleeve", "sleeve_resleeve_sleeve", "backlog_create_note", "backlog_update_note", "backlog_list_notes",
		"message_send", "message_reply", "message_ack", "message_inbox",
	} {
		if !auth.Grants(scopes, tool) {
			t.Errorf("default scopes must grant %s: %v", tool, scopes)
		}
	}
	for _, tool := range []string{
		"session_create_session", "session_respawn_session", "session_kill_session", "sleeve_kill_sleeve",
		"node_add_node", "node_remove_node", "token_create_token", "token_revoke_token", "token_list_tokens",
	} {
		if auth.Grants(scopes, tool) {
			t.Errorf("default scopes must not grant %s: %v", tool, scopes)
		}
	}
	for _, s := range scopes {
		if strings.ContainsAny(s, "*?[") {
			t.Errorf("scope %q is a glob; defaults name tools explicitly", s)
		}
	}
}

func TestDefaultScopesDropDestructive(t *testing.T) {
	tools := []Tool{
		{Name: "backlog_create_note"}, {Name: "backlog_purge", Destructive: true}, {Name: "backlog_list_notes", ReadOnly: true},
		{Name: "sleeve_kill_sleeve", Destructive: true}, {Name: "sleeve_spawn_sleeve"}, {Name: "lease_acquire"},
	}
	got := strings.Join(DefaultScopes(tools), ",")
	if want := "backlog_create_note,events,lease_acquire,read,sleeve_spawn_sleeve"; got != want {
		t.Errorf("scopes = %s, want %s", got, want)
	}
}

func fixtureInput() DocInput {
	return DocInput{
		Hub:   "http://127.0.0.1:7470",
		Token: TokenSummary{Name: TokenName, Scopes: []string{"backlog_create_note", "events", "read", "sleeve_spawn_sleeve"}},
		Tools: []Tool{
			{Name: "sleeve_spawn_sleeve", Description: "SpawnSleeve starts a harness in a new isolated sleeve on the hub. The workspace is a directory."},
			{Name: "sleeve_kill_sleeve", Description: "KillSleeve stops a sleeve.", Destructive: true},
			{Name: "quota_suggest", Description: "Suggest ranks the harnesses | by capacity.", ReadOnly: true},
			{Name: "backlog_create_note", Description: "CreateNote adds a note."},
		},
		Commands: []Command{
			{Path: "sleeve spawn", Use: "sleeve spawn <name> --workspace <dir>", Short: "Start a harness in an isolated sleeve",
				Flags: []Flag{{Name: "workspace", Type: "string", Usage: "directory on the hub"}, {Name: "cpu", Type: "uint32", Usage: "CPU limit", Default: "100"}}},
			{Path: "quota suggest", Use: "quota suggest [--for small|medium|large]", Short: "Rank harnesses"},
			{Path: "sleeve ls", Use: "sleeve ls", Short: "List sleeves"},
		},
	}
}

func TestRenderAgentsGolden(t *testing.T) {
	got := RenderAgents(fixtureInput())
	const golden = "testdata/AGENTS.golden.md"
	if *update {
		if err := os.WriteFile(golden, got, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	want, err := os.ReadFile(golden)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(want) {
		t.Errorf("AGENTS.md differs from %s; run go test ./internal/orchestrator -update and review:\n%s", golden, got)
	}
}

func TestRenderAgentsStableOrdering(t *testing.T) {
	a := fixtureInput()
	b := fixtureInput()
	b.Tools[0], b.Tools[3] = b.Tools[3], b.Tools[0]
	b.Commands[0], b.Commands[2] = b.Commands[2], b.Commands[0]
	b.Commands[2].Flags[0], b.Commands[2].Flags[1] = b.Commands[2].Flags[1], b.Commands[2].Flags[0]
	if string(RenderAgents(a)) != string(RenderAgents(b)) {
		t.Error("output depends on input order")
	}
	out := string(RenderAgents(a))
	if strings.Contains(out, "ptk_") {
		t.Error("document must never contain a token secret")
	}
	if !strings.Contains(out, "| `sleeve_kill_sleeve` | destructive | no |") || !strings.Contains(out, "| `sleeve_spawn_sleeve` | mutating | yes |") {
		t.Errorf("grant table wrong:\n%s", out)
	}
}

type fakeSleeves struct {
	protectoratev1connect.SleeveServiceClient
	state   pb.SleeveState
	exists  bool
	killed  int
	spawned []*pb.SpawnSleeveRequest
	failAt  bool
}

func (f *fakeSleeves) GetSleeve(context.Context, *connect.Request[pb.GetSleeveRequest]) (*connect.Response[pb.GetSleeveResponse], error) {
	if !f.exists {
		return nil, connect.NewError(connect.CodeNotFound, os.ErrNotExist)
	}
	return connect.NewResponse(&pb.GetSleeveResponse{Sleeve: &pb.Sleeve{Name: SleeveName, State: f.state}}), nil
}

func (f *fakeSleeves) KillSleeve(context.Context, *connect.Request[pb.KillSleeveRequest]) (*connect.Response[pb.KillSleeveResponse], error) {
	f.killed++
	f.exists = false
	return connect.NewResponse(&pb.KillSleeveResponse{}), nil
}

func (f *fakeSleeves) SpawnSleeve(_ context.Context, r *connect.Request[pb.SpawnSleeveRequest]) (*connect.Response[pb.SpawnSleeveResponse], error) {
	if f.failAt {
		return nil, connect.NewError(connect.CodeFailedPrecondition, os.ErrInvalid)
	}
	f.spawned = append(f.spawned, r.Msg)
	f.exists, f.state = true, pb.SleeveState_SLEEVE_STATE_RUNNING
	return connect.NewResponse(&pb.SpawnSleeveResponse{Sleeve: &pb.Sleeve{Name: r.Msg.Name, Harness: r.Msg.Harness, Workspace: r.Msg.Workspace}}), nil
}

type testHub struct {
	protectoratev1connect.TokenServiceClient
	*fakeSleeves
	protectoratev1connect.QuotaServiceClient
}

type fakeTokens struct {
	protectoratev1connect.TokenServiceClient
	store *auth.Store
}

func tokenInfo(t auth.Token) *pb.TokenInfo {
	info := &pb.TokenInfo{Id: t.ID, Name: t.Name, Scopes: t.Scopes, CreatedAt: timestamppb.New(t.Created)}
	if t.Expires != nil {
		info.ExpiresAt = timestamppb.New(*t.Expires)
	}
	if t.RevokedAt != nil {
		info.RevokedAt = timestamppb.New(*t.RevokedAt)
	}
	return info
}

func (f fakeTokens) CreateToken(_ context.Context, r *connect.Request[pb.CreateTokenRequest]) (*connect.Response[pb.CreateTokenResponse], error) {
	t, secret, err := f.store.Create(r.Msg.Name, r.Msg.Scopes, r.Msg.Ttl.AsDuration(), time.Now())
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.CreateTokenResponse{Token: tokenInfo(t), Secret: secret}), nil
}

func (f fakeTokens) ListTokens(context.Context, *connect.Request[pb.ListTokensRequest]) (*connect.Response[pb.ListTokensResponse], error) {
	res := &pb.ListTokensResponse{}
	for _, t := range f.store.List() {
		res.Tokens = append(res.Tokens, tokenInfo(t))
	}
	return connect.NewResponse(res), nil
}

func (f fakeTokens) RevokeToken(_ context.Context, r *connect.Request[pb.RevokeTokenRequest]) (*connect.Response[pb.RevokeTokenResponse], error) {
	t, err := f.store.Revoke(r.Msg.Id, time.Now())
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.RevokeTokenResponse{Token: tokenInfo(t)}), nil
}

type fakeQuota struct {
	protectoratev1connect.QuotaServiceClient
}

func (fakeQuota) Suggest(context.Context, *connect.Request[pb.SuggestRequest]) (*connect.Response[pb.SuggestResponse], error) {
	return connect.NewResponse(&pb.SuggestResponse{Suggestions: []*pb.HarnessSuggestion{
		{Harness: pb.Harness_HARNESS_PI, Recommended: true, Reason: "local LLM via pi: no quota"},
	}}), nil
}

func startHub(t *testing.T) (testHub, *auth.Store) {
	t.Helper()
	store := auth.NewMemoryStore()
	return testHub{fakeTokens{store: store}, &fakeSleeves{}, fakeQuota{}}, store
}

func opts(home string, tools []Tool) Options {
	return Options{Home: home, HubURL: "http://127.0.0.1:7470", Harness: "claude", TTL: time.Hour, Tools: tools}
}

func activeTokens(store *auth.Store) []auth.Token {
	var out []auth.Token
	for _, t := range store.List() {
		if t.Name == TokenName && t.RevokedAt == nil {
			out = append(out, t)
		}
	}
	return out
}

func TestSpawnMintsAndRevokesOnRespawn(t *testing.T) {
	h, store := startHub(t)
	home := t.TempDir()
	tools := realTools(t)

	first, err := Spawn(context.Background(), h, opts(home, tools))
	if err != nil {
		t.Fatal(err)
	}
	if first.Replaced || len(h.spawned) != 1 {
		t.Fatalf("first spawn: %+v spawned=%d", first, len(h.spawned))
	}
	req := h.spawned[0]
	ws := filepath.Join(home, DirName)
	if req.Name != SleeveName || req.Workspace != ws || req.Harness != pb.Harness_HARNESS_CLAUDE || req.Mode != pb.SleeveMode_SLEEVE_MODE_CHAT {
		t.Errorf("spawn request = %v", req)
	}
	tokenPath := filepath.Join(ws, TokenFile)
	if req.Env["PROTECTORATE_TOKEN_FILE"] != tokenPath || req.Env["PROTECTORATE_HUB"] != "http://127.0.0.1:7470" || len(req.Env) != 2 {
		t.Errorf("env = %v", req.Env)
	}
	fi, err := os.Stat(tokenPath)
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS != "windows" && fi.Mode().Perm() != 0o600 {
		t.Errorf("token file mode = %v", fi.Mode().Perm())
	}
	secret1, _ := os.ReadFile(tokenPath)
	act := activeTokens(store)
	if len(act) != 1 || act[0].ID != first.Token.Id {
		t.Fatalf("active tokens = %v", act)
	}
	if _, err := store.Lookup(strings.TrimSpace(string(secret1)), time.Now()); err != nil {
		t.Errorf("minted secret does not authenticate: %v", err)
	}
	if strings.Join(act[0].Scopes, ",") != strings.Join(DefaultScopes(tools), ",") {
		t.Errorf("scopes = %v", act[0].Scopes)
	}

	agents, err := os.ReadFile(filepath.Join(ws, "AGENTS.md"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(agents), first.Token.Id) || strings.Contains(string(agents), strings.TrimSpace(string(secret1))) {
		t.Error("AGENTS.md must name neither the token id nor the secret")
	}
	if target, err := os.Readlink(filepath.Join(ws, "CLAUDE.md")); err != nil || target != "AGENTS.md" {
		t.Errorf("CLAUDE.md link = %q, %v", target, err)
	}
	if hosts, _ := os.ReadFile(filepath.Join(ws, "hosts.md")); !strings.Contains(string(hosts), "Hosts") {
		t.Errorf("hosts.md = %q", hosts)
	}

	second, err := Spawn(context.Background(), h, opts(home, tools))
	if err != nil {
		t.Fatal(err)
	}
	if !second.Replaced || h.killed != 1 {
		t.Errorf("respawn must stop the running sleeve: %+v killed=%d", second, h.killed)
	}
	act = activeTokens(store)
	if len(act) != 1 || act[0].ID != second.Token.Id || act[0].ID == first.Token.Id {
		t.Fatalf("after respawn active tokens = %v (first %s)", act, first.Token.Id)
	}
	if _, err := store.Lookup(strings.TrimSpace(string(secret1)), time.Now()); err == nil {
		t.Error("previous envoy token still authenticates after respawn")
	}
	secret2, _ := os.ReadFile(tokenPath)
	if string(secret1) == string(secret2) {
		t.Error("token file was not replaced")
	}
}

func TestSpawnFailureRevokesNewToken(t *testing.T) {
	h, store := startHub(t)
	h.failAt = true
	if _, err := Spawn(context.Background(), h, opts(t.TempDir(), realTools(t))); err == nil {
		t.Fatal("expected spawn error")
	}
	if act := activeTokens(store); len(act) != 0 {
		t.Errorf("a failed spawn left tokens behind: %v", act)
	}
}

func TestSpawnCustomScopesAndHarness(t *testing.T) {
	h, store := startHub(t)
	o := opts(t.TempDir(), realTools(t))
	o.Scopes = MergeScopes(DefaultScopes(o.Tools), []string{"sleeve_kill_sleeve", "read"})
	o.Harness = "pi"
	res, err := Spawn(context.Background(), h, o)
	if err != nil {
		t.Fatal(err)
	}
	if !auth.Grants(activeTokens(store)[0].Scopes, "sleeve_kill_sleeve") {
		t.Error("widened scope not granted")
	}
	if h.spawned[0].Harness != pb.Harness_HARNESS_PI || res.Harness != "pi" {
		t.Errorf("harness = %v", h.spawned[0].Harness)
	}
	o.Harness = "gemini"
	if _, err := Spawn(context.Background(), h, o); err == nil {
		t.Error("unknown harness accepted")
	}
}

func TestSpawnAutoPicksSuggestion(t *testing.T) {
	h, _ := startHub(t)
	o := opts(t.TempDir(), realTools(t))
	o.Harness = "auto"
	res, err := Spawn(context.Background(), h, o)
	if err != nil {
		t.Fatal(err)
	}
	if res.Harness != "pi" || !strings.Contains(res.Reason, "local LLM") {
		t.Errorf("with no quota readers auto must fall back to pi: %+v", res)
	}
}

func TestWriteContextMovesForeignClaudeMdAside(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "CLAUDE.md"), []byte("mine"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := WriteContext(dir, []byte("a"), []byte("h")); err != nil {
		t.Fatal(err)
	}
	if err := WriteContext(dir, []byte("b"), []byte("h")); err != nil {
		t.Fatal(err)
	}
	if b, _ := os.ReadFile(filepath.Join(dir, "CLAUDE.md")); string(b) != "b" {
		t.Errorf("CLAUDE.md = %q", b)
	}
	aside, _ := filepath.Glob(filepath.Join(dir, "CLAUDE.md.orphaned-*"))
	if len(aside) != 1 {
		t.Fatalf("aside = %v", aside)
	}
	if b, _ := os.ReadFile(aside[0]); string(b) != "mine" {
		t.Errorf("user's file lost: %q", b)
	}
}

func TestSpawnContextIsCacheStable(t *testing.T) {
	h, _ := startHub(t)
	home := t.TempDir()
	ws := filepath.Join(home, DirName)
	tools := realTools(t)
	spawn := func(o Options) *Result {
		t.Helper()
		res, err := Spawn(context.Background(), h, o)
		if err != nil {
			t.Fatal(err)
		}
		return res
	}
	read := func(name string) []byte {
		t.Helper()
		b, err := os.ReadFile(filepath.Join(ws, name))
		if err != nil {
			t.Fatal(err)
		}
		return b
	}

	first := spawn(opts(home, tools))
	agents := read("AGENTS.md")
	old := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
	if err := os.Chtimes(filepath.Join(ws, "AGENTS.md"), old, old); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(filepath.Join(home, "hosts.md"), []byte("# Hosts\n\nkidsbox 10.0.0.9\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	time.Sleep(10 * time.Millisecond)
	second := spawn(opts(home, tools))
	if first.Token.Id == second.Token.Id {
		t.Fatal("respawn kept the token")
	}
	if got := read("AGENTS.md"); string(got) != string(agents) {
		t.Errorf("AGENTS.md changed across respawns:\n%s\nvs\n%s", agents, got)
	}
	fi, err := os.Stat(filepath.Join(ws, "AGENTS.md"))
	if err != nil || !fi.ModTime().Equal(old) {
		t.Errorf("AGENTS.md was rewritten although unchanged: %v, %v", fi, err)
	}
	if !strings.Contains(string(read("hosts.md")), "kidsbox") {
		t.Error("hosts.md did not follow the inventory")
	}
	if strings.Contains(string(read("AGENTS.md")), "kidsbox") {
		t.Error("volatile hosts.md content leaked into AGENTS.md")
	}

	changed := opts(home, tools)
	changed.Harness = "codex"
	spawn(changed)
	if string(read("AGENTS.md")) != string(agents) {
		t.Error("AGENTS.md depends on the harness, so a resleeve or a harness change busts the cache")
	}
	changed.Scopes = []string{"read"}
	spawn(changed)
	if string(read("AGENTS.md")) == string(agents) {
		t.Error("AGENTS.md ignored a scope change")
	}
}
