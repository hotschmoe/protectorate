package auth

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

var t0 = time.Date(2026, 10, 19, 12, 0, 0, 0, time.UTC)

func TestSnake(t *testing.T) {
	for in, want := range map[string]string{"GetQuota": "get_quota", "ListSessions": "list_sessions", "GetURLPath": "get_url_path", "Hub": "hub"} {
		if got := snake(in); got != want {
			t.Errorf("snake(%q)=%q want %q", in, got, want)
		}
	}
}

func TestCatalog(t *testing.T) {
	ms, err := Methods()
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]struct{ read, owner, streaming bool }{
		"/protectorate.v1.HubService/GetStatus":          {read: true},
		"/protectorate.v1.SessionService/KillSession":    {},
		"/protectorate.v1.EventService/Subscribe":        {streaming: true},
		"/protectorate.v1.TokenService/CreateToken":      {owner: true},
		"/protectorate.v1.TokenService/ListTokens":       {read: true, owner: true},
		"/protectorate.v1.AuditService/ListAuditEntries": {read: true, owner: true},
		pCreate:  {owner: true},
		pRespawn: {owner: true},
		pAddNode: {owner: true},
		pRmNode:  {owner: true, streaming: true},
	}
	for proc, w := range want {
		m, ok := ms[proc]
		if !ok {
			t.Errorf("%s missing", proc)
			continue
		}
		if m.ReadOnly != w.read || m.OwnerOnly != w.owner || m.Streaming != w.streaming {
			t.Errorf("%s = %+v", proc, m)
		}
	}
	for proc := range ms {
		if strings.Contains(proc, "NodeAgentService") {
			t.Errorf("node_local service in hub catalog: %s", proc)
		}
	}
}

func TestValidateScopes(t *testing.T) {
	tests := []struct {
		name    string
		in      []string
		want    []string
		wantErr bool
	}{
		{"empty", nil, nil, true},
		{"special", []string{"terminal", "read", "events", "terminal:readonly", "terminal:host"}, []string{"events", "read", "terminal", "terminal:host", "terminal:readonly"}, false},
		{"glob and tool", []string{"session_*", "backlog_add_item"}, []string{"backlog_add_item", "session_*"}, false},
		{"dedup", []string{"read", "read"}, []string{"read"}, false},
		{"star", []string{"*"}, []string{"*"}, false},
		{"typo matches nothing", []string{"sesion_*"}, nil, true},
		{"owner-only only", []string{"token_*"}, nil, true},
		{"owner-only rpc by name", []string{"session_create_session"}, nil, true},
		{"owner-only rpc add node", []string{"node_add_node"}, nil, true},
		{"bad glob", []string{"["}, nil, true},
		{"empty scope", []string{""}, nil, true},
		{"procedure form rejected", []string{"/protectorate.v1.HubService/GetStatus"}, nil, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ValidateScopes(tt.in)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			if strings.Join(got, ",") != strings.Join(tt.want, ",") {
				t.Fatalf("got %v want %v", got, tt.want)
			}
		})
	}
}

const (
	pStatus  = "/protectorate.v1.HubService/GetStatus"
	pList    = "/protectorate.v1.SessionService/ListSessions"
	pCreate  = "/protectorate.v1.SessionService/CreateSession"
	pKill    = "/protectorate.v1.SessionService/KillSession"
	pRespawn = "/protectorate.v1.SessionService/RespawnSession"
	pAddNode = "/protectorate.v1.NodeService/AddNode"
	pRmNode  = "/protectorate.v1.NodeService/RemoveNode"
	pPutFile = "/protectorate.v1.NodeService/PutFile"
	pAdd     = "/protectorate.v1.BacklogService/AddItem"
	pListIt  = "/protectorate.v1.BacklogService/ListItems"
	pSub     = "/protectorate.v1.EventService/Subscribe"
	pTokList = "/protectorate.v1.TokenService/ListTokens"
	pTokNew  = "/protectorate.v1.TokenService/CreateToken"
	pUnknown = "/protectorate.v1.Nope/Nope"
)

func TestAuthorizeProcedure(t *testing.T) {
	a := New("owner-secret", nil, nil)
	tests := []struct {
		name   string
		scopes []string
		owner  bool
		proc   string
		ok     bool
		need   string
	}{
		{"owner anything", nil, true, pKill, true, ""},
		{"owner token service", nil, true, pTokNew, true, ""},
		{"owner unknown procedure", nil, true, pUnknown, true, ""},
		{"no scopes denies read", nil, false, pStatus, false, "read or hub_get_status"},
		{"read covers every read rpc", []string{"read"}, false, pList, true, ""},
		{"read covers other service read", []string{"read"}, false, pListIt, true, ""},
		{"read excludes mutation", []string{"read"}, false, pCreate, false, "owner token (session_create_session is owner-only)"},
		{"read excludes streaming", []string{"read"}, false, pSub, false, "events or event_subscribe"},
		{"events covers stream", []string{"events"}, false, pSub, true, ""},
		{"events not reads", []string{"events"}, false, pStatus, false, "read or hub_get_status"},
		{"glob", []string{"session_*"}, false, pKill, true, ""},
		{"glob other service", []string{"session_*"}, false, pAdd, false, "backlog_add_item"},
		{"exact tool", []string{"backlog_add_item"}, false, pAdd, true, ""},
		{"exact tool not sibling", []string{"backlog_add_item"}, false, pListIt, false, "read or backlog_list_items"},
		{"star everything but owner-only", []string{"*"}, false, pKill, true, ""},
		{"star not create session", []string{"*"}, false, pCreate, false, "session_create_session is owner-only"},
		{"exact tool not create session", []string{"session_create_session"}, false, pCreate, false, "session_create_session is owner-only"},
		{"glob not respawn", []string{"session_*"}, false, pRespawn, false, "session_respawn_session is owner-only"},
		{"star not add node", []string{"*", "node_add_node"}, false, pAddNode, false, "node_add_node is owner-only"},
		{"owner create session", nil, true, pCreate, true, ""},
		{"glob covers its service", []string{"node_*"}, false, pPutFile, true, ""},
		{"glob not remove node", []string{"node_*"}, false, pRmNode, false, "node_remove_node is owner-only"},
		{"star not token service", []string{"*"}, false, pTokNew, false, "owner token (token_create_token is owner-only)"},
		{"star not token list", []string{"*", "read"}, false, pTokList, false, "owner token (token_list_tokens is owner-only)"},
		{"terminal scope grants no rpc", []string{"terminal", "terminal:readonly"}, false, pStatus, false, "read or hub_get_status"},
		{"unknown procedure denied", []string{"*", "read"}, false, pUnknown, false, "unknown procedure"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := &Principal{Name: "t", Owner: tt.owner, Scopes: tt.scopes}
			err := a.AuthorizeProcedure(p, tt.proc)
			if tt.ok {
				if err != nil {
					t.Fatal(err)
				}
				return
			}
			var d *DeniedError
			if !errors.As(err, &d) || !strings.Contains(d.Need, tt.need) {
				t.Fatalf("err = %v, want denial mentioning %q", err, tt.need)
			}
		})
	}
}

func TestAuthorizeTerminal(t *testing.T) {
	a := New("o", nil, nil)
	tests := []struct {
		name     string
		p        *Principal
		readOnly bool
		host     bool
		ok       bool
	}{
		{"owner rw", &Principal{Owner: true}, false, false, true},
		{"owner rw host session", &Principal{Owner: true}, false, true, true},
		{"terminal rw sleeve", &Principal{Scopes: []string{"terminal"}}, false, false, true},
		{"terminal rw host session denied", &Principal{Scopes: []string{"terminal"}}, false, true, false},
		{"terminal ro host session", &Principal{Scopes: []string{"terminal"}}, true, true, true},
		{"terminal ro", &Principal{Scopes: []string{"terminal"}}, true, false, true},
		{"host scope rw host session", &Principal{Scopes: []string{"terminal:host"}}, false, true, true},
		{"host scope rw sleeve", &Principal{Scopes: []string{"terminal:host"}}, false, false, true},
		{"readonly scope rw host denied", &Principal{Scopes: []string{"terminal:readonly", "terminal"}}, false, true, false},
		{"readonly scope ro", &Principal{Scopes: []string{"terminal:readonly"}}, true, true, true},
		{"readonly scope rw denied", &Principal{Scopes: []string{"terminal:readonly"}}, false, false, false},
		{"star is not terminal", &Principal{Scopes: []string{"*", "read", "events"}}, true, false, false},
		{"none", &Principal{}, true, false, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := a.AuthorizeTerminal(tt.p, tt.readOnly, tt.host)
			if (err == nil) != tt.ok {
				t.Fatalf("err = %v", err)
			}
		})
	}
}

func TestAuthenticateLifecycle(t *testing.T) {
	now := t0
	a := New("owner-secret", NewMemoryStore(), nil)
	a.Now = func() time.Time { return now }
	st := a.Store()

	tok, secret, err := st.Create("phone", []string{"read"}, time.Hour, now)
	if err != nil {
		t.Fatal(err)
	}
	forever, foreverSecret, err := st.Create("forever", []string{"read"}, 0, now)
	if err != nil {
		t.Fatal(err)
	}
	if forever.Expires != nil {
		t.Fatal("zero ttl must not expire")
	}

	p, err := a.Authenticate("owner-secret")
	if err != nil || !p.Owner || p.Actor() != "owner" {
		t.Fatalf("owner: %+v %v", p, err)
	}
	p, err = a.Authenticate(secret)
	if err != nil || p.Owner || p.ID != tok.ID || p.Actor() != "token:phone" {
		t.Fatalf("token: %+v %v", p, err)
	}
	for _, bad := range []string{"", "nope", secret + "x", strings.ToUpper(secret), tok.Hash, tok.ID} {
		if _, err := a.Authenticate(bad); !errors.Is(err, ErrUnauthenticated) {
			t.Errorf("%q: err = %v", bad, err)
		}
	}

	now = t0.Add(time.Hour - time.Second)
	if _, err := a.Authenticate(secret); err != nil {
		t.Fatalf("before expiry: %v", err)
	}
	now = t0.Add(time.Hour)
	if _, err := a.Authenticate(secret); !errors.Is(err, ErrUnauthenticated) {
		t.Fatalf("at expiry: %v", err)
	}
	if _, err := st.Lookup(secret, now); !errors.Is(err, ErrExpired) {
		t.Fatalf("store lookup at expiry: %v", err)
	}
	now = t0.Add(10000 * time.Hour)
	if _, err := a.Authenticate(foreverSecret); err != nil {
		t.Fatalf("never-expiring token: %v", err)
	}
	if _, err := a.Authenticate("owner-secret"); err != nil {
		t.Fatalf("owner never expires: %v", err)
	}

	if _, err := st.Revoke(forever.ID, now); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Authenticate(foreverSecret); !errors.Is(err, ErrUnauthenticated) {
		t.Fatalf("revoked: %v", err)
	}
	if _, err := st.Lookup(foreverSecret, now); !errors.Is(err, ErrRevoked) {
		t.Fatalf("store lookup revoked: %v", err)
	}
	first := st.List()[1].RevokedAt
	again, err := st.Revoke(forever.ID, now.Add(time.Hour))
	if err != nil || !again.RevokedAt.Equal(*first) {
		t.Fatalf("second revoke changed time: %v %v", again.RevokedAt, err)
	}
	if _, err := st.Revoke("missing", now); !errors.Is(err, ErrUnknownToken) {
		t.Fatalf("revoke missing: %v", err)
	}
}

func TestEmptyOwnerTokenNeverMatches(t *testing.T) {
	a := New("", nil, nil)
	if _, err := a.Authenticate(""); err == nil {
		t.Fatal("empty credential accepted")
	}
}

func TestCreateValidation(t *testing.T) {
	st := NewMemoryStore()
	tests := []struct {
		name   string
		tname  string
		scopes []string
		ttl    time.Duration
	}{
		{"no name", "", []string{"read"}, time.Hour},
		{"no scopes", "x", nil, time.Hour},
		{"negative ttl", "x", []string{"read"}, -time.Hour},
		{"bad scope", "x", []string{"zzz"}, time.Hour},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, _, err := st.Create(tt.tname, tt.scopes, tt.ttl, t0); err == nil {
				t.Fatal("accepted")
			}
		})
	}
	if len(st.List()) != 0 {
		t.Fatal("failed create left a token behind")
	}
}

func TestStorePersistenceAndNoSecrets(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sub", "tokens.yaml")
	st, err := OpenStore(path)
	if err != nil {
		t.Fatal(err)
	}
	tok, secret, err := st.Create("phone", []string{"read", "session_*"}, time.Hour, t0)
	if err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), secret) || strings.Contains(string(raw), strings.TrimPrefix(secret, SecretPrefix)) {
		t.Fatalf("secret stored in tokens.yaml:\n%s", raw)
	}
	if !strings.Contains(string(raw), tok.Hash) || !strings.Contains(string(raw), "phone") {
		t.Fatalf("hash or name missing:\n%s", raw)
	}
	if runtime.GOOS != "windows" {
		fi, _ := os.Stat(path)
		if fi.Mode().Perm() != 0o600 {
			t.Fatalf("mode %v", fi.Mode().Perm())
		}
	}
	entries, _ := os.ReadDir(filepath.Dir(path))
	if len(entries) != 1 {
		t.Fatalf("temp files left behind: %v", entries)
	}

	if _, err := st.Lookup(secret, t0); err != nil {
		t.Fatal(err)
	}
	if err := st.Flush(); err != nil {
		t.Fatal(err)
	}
	re, err := OpenStore(path)
	if err != nil {
		t.Fatal(err)
	}
	got := re.List()
	if len(got) != 1 || got[0].ID != tok.ID || got[0].LastUsed == nil || strings.Join(got[0].Scopes, ",") != "read,session_*" {
		t.Fatalf("reloaded: %+v", got)
	}
	if _, err := re.Lookup(secret, t0); err != nil {
		t.Fatalf("secret no longer authenticates after reload: %v", err)
	}
	if _, err := re.Revoke(tok.ID, t0); err != nil {
		t.Fatal(err)
	}
	re2, _ := OpenStore(path)
	if _, err := re2.Lookup(secret, t0); !errors.Is(err, ErrRevoked) {
		t.Fatalf("revocation not persisted: %v", err)
	}
}

func TestStoreCorruptFileFailsClosed(t *testing.T) {
	path := filepath.Join(t.TempDir(), "tokens.yaml")
	if err := os.WriteFile(path, []byte("tokens: [: nope"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := OpenStore(path); err == nil {
		t.Fatal("corrupt store opened")
	}
}

func TestStoreConcurrent(t *testing.T) {
	st, err := OpenStore(filepath.Join(t.TempDir(), "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	_, secret, err := st.Create("c", []string{"read"}, time.Hour, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			st.Lookup(secret, time.Now().Add(time.Duration(i)*time.Minute))
			st.List()
			st.Create("x", []string{"read"}, 0, time.Now())
		}()
	}
	wg.Wait()
	if len(st.List()) != 17 {
		t.Fatalf("tokens = %d", len(st.List()))
	}
}

func TestAuditAppendAndTail(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "audit")
	a := NewAudit(dir)
	t.Cleanup(func() { a.Close() })
	a.Write(Entry{Time: time.Date(2026, 9, 30, 23, 0, 0, 0, time.UTC), Actor: "owner", Procedure: pKill, Subject: "old", Outcome: OutcomeOK})
	a.Write(Entry{Time: time.Date(2026, 10, 1, 1, 0, 0, 0, time.UTC), Actor: "token:phone", TokenID: "ab12", Tailnet: "phone.ts.net", Procedure: pCreate, Subject: "s1", Outcome: OutcomeDenied, Code: "permission_denied"})
	a.Write(Entry{Time: time.Date(2026, 10, 2, 1, 0, 0, 0, time.UTC), Actor: "owner", Procedure: pKill, Subject: "s2", Outcome: OutcomeOK})

	files, _ := filepath.Glob(filepath.Join(dir, "*"))
	if len(files) != 2 || filepath.Base(files[0]) != "audit-2026-09.jsonl" || filepath.Base(files[1]) != "audit-2026-10.jsonl" {
		t.Fatalf("files = %v", files)
	}
	if runtime.GOOS != "windows" {
		for _, f := range files {
			if fi, _ := os.Stat(f); fi.Mode().Perm() != 0o600 {
				t.Fatalf("%s mode %v", f, fi.Mode().Perm())
			}
		}
	}
	got := oldestFirst(t, dir, 2)
	if len(got) != 2 || got[0].Subject != "s1" || got[1].Subject != "s2" || got[0].Tailnet != "phone.ts.net" {
		t.Fatalf("tail 2: %+v", got)
	}
	if got := oldestFirst(t, dir, 50); len(got) != 3 || got[0].Subject != "old" {
		t.Fatalf("tail 50: %+v", got)
	}

	b, _ := os.ReadFile(files[1])
	lines := strings.Split(strings.TrimSpace(string(b)), "\n")
	if len(lines) != 2 {
		t.Fatalf("lines: %q", b)
	}

	a2 := NewAudit(dir)
	a2.Write(Entry{Time: time.Date(2026, 10, 3, 0, 0, 0, 0, time.UTC), Actor: "owner", Procedure: pKill, Outcome: OutcomeOK})
	a2.Close()
	if got := oldestFirst(t, dir, 50); len(got) != 4 || got[3].Time.Day() != 3 {
		t.Fatalf("reopen did not append: %+v", got)
	}
}

func TestAuditFailureReported(t *testing.T) {
	f := filepath.Join(t.TempDir(), "file")
	if err := os.WriteFile(f, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	var got error
	a := NewAudit(filepath.Join(f, "audit"))
	a.OnError = func(err error) { got = err }
	a.Write(Entry{Actor: "owner", Procedure: pKill})
	if got == nil {
		t.Fatal("write failure not reported")
	}
	var nilAudit *Audit
	nilAudit.Write(Entry{})
}

func TestRecordOutcomes(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "audit")
	au := NewAudit(dir)
	t.Cleanup(func() { au.Close() })
	a := New("o", nil, au)
	ctx := WithTailnet(context.Background(), "laptop.ts.net")
	p := &Principal{ID: "id1", Name: "bridge"}
	a.Record(ctx, p, pKill, "s", nil, "")
	a.Record(ctx, p, pKill, "s", &DeniedError{Name: "bridge", Need: "x"}, "permission_denied")
	a.Record(ctx, nil, pKill, "s", ErrUnauthenticated, "unauthenticated")
	a.Record(ctx, p, pKill, "s", errors.New("boom"), "internal")
	got := oldestFirst(t, dir, 10)
	var outcomes []string
	for _, e := range got {
		outcomes = append(outcomes, e.Actor+":"+e.Outcome)
		if e.Tailnet != "laptop.ts.net" {
			t.Errorf("tailnet identity missing: %+v", e)
		}
	}
	want := "token:bridge:ok,token:bridge:denied,unauthenticated:denied,token:bridge:error"
	if strings.Join(outcomes, ",") != want {
		t.Fatalf("outcomes = %v", outcomes)
	}
}

func TestSubject(t *testing.T) {
	tests := []struct {
		name string
		msg  any
		want string
	}{
		{"name", &pb.KillSessionRequest{Name: "alpha"}, "alpha"},
		{"id", &pb.RevokeTokenRequest{Id: "ab12"}, "ab12"},
		{"nested", &pb.AddNodeRequest{Node: &pb.NodeInfo{Name: "gpu1", Address: "secret-host"}}, "gpu1"},
		{"none", &pb.GetStatusRequest{}, ""},
		{"nil", nil, ""},
		{"body is not logged", &pb.CreateSessionRequest{Name: "n", Env: map[string]string{"K": "v"}, Command: []string{"x"}}, "n"},
	}
	for _, tt := range tests {
		if got := Subject(tt.msg); got != tt.want {
			t.Errorf("%s: %q want %q", tt.name, got, tt.want)
		}
	}
}
