package hub

import (
	"cmp"
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"github.com/coder/websocket"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/session"
)

type authEnv struct {
	h     *Hub
	srv   *httptest.Server
	home  string
	audit string
	store *auth.Store
}

func newAuthEnv(t *testing.T) *authEnv {
	t.Helper()
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	h := New("test", testToken)
	h.Heartbeat = 20 * time.Millisecond
	h.PingInterval = 50 * time.Millisecond
	store, err := auth.OpenStore(filepath.Join(home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	e := &authEnv{h: h, home: home, audit: filepath.Join(home, "audit"), store: store}
	au := auth.NewAudit(e.audit)
	t.Cleanup(func() { au.Close() })
	h.SetAuth(auth.New(testToken, store, au))
	e.srv = httptest.NewServer(h.Handler())
	t.Cleanup(e.srv.Close)
	return e
}

func (e *authEnv) mint(t *testing.T, name string, ttl time.Duration, scopes ...string) string {
	t.Helper()
	_, secret, err := e.store.Create(name, scopes, ttl, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	return secret
}

func (e *authEnv) entries(t *testing.T) []auth.Entry {
	t.Helper()
	p, err := auth.ReadPage(e.audit, auth.Query{Limit: 1000})
	if err != nil {
		t.Fatal(err)
	}
	slices.Reverse(p.Entries)
	return p.Entries
}

func code(err error) connect.Code {
	if err == nil {
		return 0
	}
	return connect.CodeOf(err)
}

func TestUnaryScopeEnforcement(t *testing.T) {
	e := newAuthEnv(t)
	tokens := map[string]string{
		"owner":    testToken,
		"none":     "not-a-token",
		"read":     e.mint(t, "read", time.Hour, "read"),
		"backlog":  e.mint(t, "backlog", time.Hour, "backlog_*"),
		"add":      e.mint(t, "add", time.Hour, "backlog_create_note"),
		"star":     e.mint(t, "star", time.Hour, "*"),
		"sessions": e.mint(t, "sessions", time.Hour, "session_*"),
		"expired":  e.mint(t, "expired", time.Nanosecond, "*"),
		"revoked":  e.mint(t, "revoked", time.Hour, "*"),
	}
	for _, tk := range e.store.List() {
		if tk.Name == "revoked" {
			if _, err := e.store.Revoke(tk.ID, time.Now()); err != nil {
				t.Fatal(err)
			}
		}
	}
	time.Sleep(2 * time.Millisecond)

	status := func(c *http.Client) error {
		_, err := protectoratev1connect.NewHubServiceClient(c, e.srv.URL).GetStatus(context.Background(), connect.NewRequest(&pb.GetStatusRequest{}))
		return err
	}
	add := func(c *http.Client) error {
		_, err := protectoratev1connect.NewBacklogServiceClient(c, e.srv.URL).CreateNote(context.Background(),
			connect.NewRequest(&pb.CreateNoteRequest{Title: "t"}))
		return err
	}
	kill := func(c *http.Client) error {
		_, err := protectoratev1connect.NewSessionServiceClient(c, e.srv.URL).KillSession(context.Background(),
			connect.NewRequest(&pb.KillSessionRequest{Name: "ghost"}))
		return err
	}
	create := func(c *http.Client) error {
		_, err := protectoratev1connect.NewSessionServiceClient(c, e.srv.URL).CreateSession(context.Background(),
			connect.NewRequest(&pb.CreateSessionRequest{Name: "x", Command: []string{"true"}}))
		return err
	}
	respawn := func(c *http.Client) error {
		_, err := protectoratev1connect.NewSessionServiceClient(c, e.srv.URL).RespawnSession(context.Background(),
			connect.NewRequest(&pb.RespawnSessionRequest{Name: "ghost"}))
		return err
	}
	tokenLs := func(c *http.Client) error {
		_, err := protectoratev1connect.NewTokenServiceClient(c, e.srv.URL).ListTokens(context.Background(), connect.NewRequest(&pb.ListTokensRequest{}))
		return err
	}
	tests := []struct {
		name  string
		token string
		call  func(*http.Client) error
		want  connect.Code
	}{
		{"owner reads", "owner", status, 0},
		{"owner mutates", "owner", add, 0},
		{"owner kills (not found is past auth)", "owner", kill, connect.CodeNotFound},
		{"owner lists tokens", "owner", tokenLs, 0},
		{"garbage token", "none", status, connect.CodeUnauthenticated},
		{"expired", "expired", status, connect.CodeUnauthenticated},
		{"revoked", "revoked", status, connect.CodeUnauthenticated},
		{"read reads", "read", status, 0},
		{"read cannot mutate", "read", add, connect.CodePermissionDenied},
		{"backlog cannot read status", "backlog", status, connect.CodePermissionDenied},
		{"backlog mutates backlog", "backlog", add, 0},
		{"backlog cannot kill", "backlog", kill, connect.CodePermissionDenied},
		{"exact tool mutates", "add", add, 0},
		{"star kills", "star", kill, connect.CodeNotFound},
		{"star cannot create session", "star", create, connect.CodePermissionDenied},
		{"star cannot respawn", "star", respawn, connect.CodePermissionDenied},
		{"session glob cannot create session", "sessions", create, connect.CodePermissionDenied},
		{"session glob still kills", "sessions", kill, connect.CodeNotFound},
		{"owner respawn (not found is past auth)", "owner", respawn, connect.CodeNotFound},
		{"star cannot list tokens", "star", tokenLs, connect.CodePermissionDenied},
		{"read cannot list tokens", "read", tokenLs, connect.CodePermissionDenied},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.call(&http.Client{Transport: bearer{tokens[tt.token]}})
			if code(err) != tt.want {
				t.Fatalf("err = %v, want code %v", err, tt.want)
			}
		})
	}

	_, err := protectoratev1connect.NewBacklogServiceClient(&http.Client{Transport: bearer{tokens["read"]}}, e.srv.URL).
		CreateNote(context.Background(), connect.NewRequest(&pb.CreateNoteRequest{Title: "t"}))
	if err == nil || !strings.Contains(err.Error(), "backlog_create_note") {
		t.Fatalf("denial does not name the scope: %v", err)
	}
}

func TestStreamScopeEnforcement(t *testing.T) {
	e := newAuthEnv(t)
	tests := []struct {
		name  string
		token string
		want  connect.Code
	}{
		{"owner", testToken, 0},
		{"events scope", e.mint(t, "ev", time.Hour, "events"), 0},
		{"glob over event tool", e.mint(t, "ev2", time.Hour, "event_*"), 0},
		{"read is not events", e.mint(t, "rd", time.Hour, "read"), connect.CodePermissionDenied},
		{"star-less glob elsewhere", e.mint(t, "bk", time.Hour, "backlog_*"), connect.CodePermissionDenied},
		{"bad token", "nope", connect.CodeUnauthenticated},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			st, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{tt.token}}, e.srv.URL).
				Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
			if err == nil {
				defer st.Close()
				if st.Receive() {
					if tt.want != 0 {
						t.Fatal("stream delivered to an unauthorized token")
					}
					return
				}
				err = st.Err()
			}
			if code(err) != tt.want {
				t.Fatalf("err = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestTokenServiceOwnerOnly(t *testing.T) {
	e := newAuthEnv(t)
	owner := protectoratev1connect.NewTokenServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL)
	ctx := context.Background()

	created, err := owner.CreateToken(ctx, connect.NewRequest(&pb.CreateTokenRequest{
		Name: "phone", Scopes: []string{"read", "session_*"}, Ttl: durationpb.New(time.Hour),
	}))
	if err != nil {
		t.Fatal(err)
	}
	secret := created.Msg.Secret
	if !strings.HasPrefix(secret, auth.SecretPrefix) || created.Msg.Token.Name != "phone" || created.Msg.Token.ExpiresAt == nil {
		t.Fatalf("created: %v", created.Msg)
	}

	bad := []*pb.CreateTokenRequest{
		{Name: "", Scopes: []string{"read"}},
		{Name: "x"},
		{Name: "x", Scopes: []string{"nosuch_*"}},
		{Name: "x", Scopes: []string{"read"}, Ttl: durationpb.New(-time.Hour)},
	}
	for _, r := range bad {
		if _, err := owner.CreateToken(ctx, connect.NewRequest(r)); code(err) != connect.CodeInvalidArgument {
			t.Errorf("%v: err = %v", r, err)
		}
	}

	list, err := owner.ListTokens(ctx, connect.NewRequest(&pb.ListTokensRequest{}))
	if err != nil || len(list.Msg.Tokens) != 1 || list.Msg.Tokens[0].Id != created.Msg.Token.Id {
		t.Fatalf("list: %v %v", list, err)
	}

	scoped := protectoratev1connect.NewTokenServiceClient(&http.Client{Transport: bearer{secret}}, e.srv.URL)
	star := protectoratev1connect.NewTokenServiceClient(&http.Client{Transport: bearer{e.mint(t, "star", time.Hour, "*", "read")}}, e.srv.URL)
	for name, c := range map[string]protectoratev1connect.TokenServiceClient{"scoped": scoped, "star": star} {
		if _, err := c.CreateToken(ctx, connect.NewRequest(&pb.CreateTokenRequest{Name: "esc", Scopes: []string{"*"}})); code(err) != connect.CodePermissionDenied {
			t.Errorf("%s create: %v", name, err)
		}
		if _, err := c.ListTokens(ctx, connect.NewRequest(&pb.ListTokensRequest{})); code(err) != connect.CodePermissionDenied {
			t.Errorf("%s list: %v", name, err)
		}
		if _, err := c.RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: created.Msg.Token.Id})); code(err) != connect.CodePermissionDenied {
			t.Errorf("%s revoke: %v", name, err)
		}
	}
	if len(e.store.List()) != 2 {
		t.Fatalf("a non-owner changed the store: %d tokens", len(e.store.List()))
	}

	statusAs := func(tok string) error {
		_, err := protectoratev1connect.NewHubServiceClient(&http.Client{Transport: bearer{tok}}, e.srv.URL).
			GetStatus(ctx, connect.NewRequest(&pb.GetStatusRequest{}))
		return err
	}
	if err := statusAs(secret); err != nil {
		t.Fatal(err)
	}
	rev, err := owner.RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: created.Msg.Token.Id}))
	if err != nil || rev.Msg.Token.RevokedAt == nil {
		t.Fatalf("revoke: %v %v", rev, err)
	}
	if err := statusAs(secret); code(err) != connect.CodeUnauthenticated {
		t.Fatalf("revoked token still works: %v", err)
	}
	if _, err := owner.RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: "nope"})); code(err) != connect.CodeNotFound {
		t.Fatalf("revoke unknown: %v", err)
	}

	raw, err := os.ReadFile(filepath.Join(e.home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), secret) || strings.Contains(string(raw), strings.TrimPrefix(secret, auth.SecretPrefix)) {
		t.Fatal("secret persisted in tokens.yaml")
	}
}

func TestAuditCoversMutationsNotReads(t *testing.T) {
	e := newAuthEnv(t)
	read := e.mint(t, "reader", time.Hour, "read")
	ctx := context.Background()
	ownerHC := &http.Client{Transport: bearer{testToken}}
	readHC := &http.Client{Transport: bearer{read}}

	protectoratev1connect.NewHubServiceClient(ownerHC, e.srv.URL).GetStatus(ctx, connect.NewRequest(&pb.GetStatusRequest{}))
	protectoratev1connect.NewBacklogServiceClient(ownerHC, e.srv.URL).ListNotes(ctx, connect.NewRequest(&pb.ListNotesRequest{}))
	protectoratev1connect.NewHubServiceClient(readHC, e.srv.URL).GetStatus(ctx, connect.NewRequest(&pb.GetStatusRequest{}))
	protectoratev1connect.NewHubServiceClient(&http.Client{Transport: bearer{"bad"}}, e.srv.URL).GetStatus(ctx, connect.NewRequest(&pb.GetStatusRequest{}))
	st, err := protectoratev1connect.NewEventServiceClient(ownerHC, e.srv.URL).Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err == nil {
		st.Receive()
		st.Close()
	}
	protectoratev1connect.NewTokenServiceClient(ownerHC, e.srv.URL).ListTokens(ctx, connect.NewRequest(&pb.ListTokensRequest{}))
	if got := e.entries(t); len(got) != 0 {
		t.Fatalf("reads were audited: %+v", got)
	}

	protectoratev1connect.NewBacklogServiceClient(ownerHC, e.srv.URL).CreateNote(ctx, connect.NewRequest(&pb.CreateNoteRequest{Title: "do thing"}))
	protectoratev1connect.NewBacklogServiceClient(readHC, e.srv.URL).CreateNote(ctx, connect.NewRequest(&pb.CreateNoteRequest{Title: "sneaky"}))
	protectoratev1connect.NewSessionServiceClient(&http.Client{Transport: bearer{"bad"}}, e.srv.URL).KillSession(ctx, connect.NewRequest(&pb.KillSessionRequest{Name: "victim"}))
	protectoratev1connect.NewSessionServiceClient(ownerHC, e.srv.URL).KillSession(ctx, connect.NewRequest(&pb.KillSessionRequest{Name: "ghost"}))
	created, _ := protectoratev1connect.NewTokenServiceClient(ownerHC, e.srv.URL).CreateToken(ctx, connect.NewRequest(&pb.CreateTokenRequest{Name: "dev", Scopes: []string{"read"}}))
	protectoratev1connect.NewTokenServiceClient(ownerHC, e.srv.URL).RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: created.Msg.Token.Id}))

	got := e.entries(t)
	type row struct{ actor, proc, subject, outcome, code string }
	var rows []row
	for _, g := range got {
		rows = append(rows, row{g.Actor, g.Procedure, g.Subject, g.Outcome, g.Code})
	}
	want := []row{
		{"owner", "/protectorate.v1.BacklogService/CreateNote", "", "ok", ""},
		{"token:reader", "/protectorate.v1.BacklogService/CreateNote", "", "denied", "permission_denied"},
		{"unauthenticated", "/protectorate.v1.SessionService/KillSession", "victim", "denied", "unauthenticated"},
		{"owner", "/protectorate.v1.SessionService/KillSession", "ghost", "error", "not_found"},
		{"owner", "/protectorate.v1.TokenService/CreateToken", "dev", "ok", ""},
		{"owner", "/protectorate.v1.TokenService/RevokeToken", created.Msg.Token.Id, "ok", ""},
	}
	if len(rows) != len(want) {
		t.Fatalf("rows = %+v", rows)
	}
	for i := range want {
		if rows[i] != want[i] {
			t.Errorf("row %d = %+v, want %+v", i, rows[i], want[i])
		}
	}
	if got[1].TokenID == "" {
		t.Error("token id missing for token actor")
	}

	for _, f := range mustGlob(t, filepath.Join(e.audit, "*")) {
		b, _ := os.ReadFile(f)
		for _, secret := range []string{testToken, read, created.Msg.Secret, "do thing", "sneaky"} {
			if strings.Contains(string(b), secret) {
				t.Errorf("audit log contains %q", secret)
			}
		}
	}
}

func mustGlob(t *testing.T, pat string) []string {
	t.Helper()
	m, err := filepath.Glob(pat)
	if err != nil || len(m) == 0 {
		t.Fatalf("glob %s: %v %v", pat, m, err)
	}
	return m
}

func TestTerminalScopeEnforcement(t *testing.T) {
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("tmux not installed")
	}
	e := newAuthEnv(t)
	sock := "protectorate-authtest-" + time.Now().Format("150405.000000")
	e.h.Sessions = session.New(sock)
	t.Cleanup(func() { exec.Command("tmux", "-L", sock, "kill-server").Run() })
	if _, err := e.h.Sessions.Create(context.Background(), session.CreateOptions{Name: "box", Command: []string{"cat"}}); err != nil {
		t.Fatal(err)
	}
	if _, err := e.h.Sessions.Create(context.Background(), session.CreateOptions{Name: "sleeve-box", Command: []string{"cat"}}); err != nil {
		t.Fatal(err)
	}
	term := e.mint(t, "term", time.Hour, "terminal")
	ro := e.mint(t, "ro", time.Hour, "terminal:readonly")
	star := e.mint(t, "star", time.Hour, "*", "read", "events")
	expired := e.mint(t, "old", time.Nanosecond, "terminal")
	time.Sleep(2 * time.Millisecond)
	hdr := func(tok string) *websocket.DialOptions {
		return &websocket.DialOptions{HTTPHeader: http.Header{"Authorization": {"Bearer " + tok}}}
	}
	tests := []struct {
		name    string
		session string
		query   string
		opts    *websocket.DialOptions
		want    int
	}{
		{"owner", "", "", hdr(testToken), 101},
		{"owner rw sleeve", "sleeve-box", "", hdr(testToken), 101},
		{"terminal rw host session denied", "", "", hdr(term), 403},
		{"terminal rw sleeve", "sleeve-box", "", hdr(term), 101},
		{"terminal ro sleeve", "sleeve-box", "?readonly=1", hdr(term), 101},
		{"terminal ro flag", "", "?readonly=1", hdr(term), 101},
		{"terminal via subprotocol", "sleeve-box", "", &websocket.DialOptions{Subprotocols: []string{TerminalProtocol, "token." + term}}, 101},
		{"terminal via query", "sleeve-box", "?token=" + term, &websocket.DialOptions{}, 101},
		{"readonly scope with flag", "", "?readonly=1", hdr(ro), 101},
		{"readonly scope without flag", "", "", hdr(ro), 403},
		{"star has no terminal", "", "", hdr(star), 403},
		{"star has no readonly terminal", "", "?readonly=1", hdr(star), 403},
		{"expired", "", "", hdr(expired), 401},
		{"bad header good query falls through", "sleeve-box", "?token=" + term, hdr("nope"), 101},
		{"no token", "", "", &websocket.DialOptions{}, 401},
		{"scoped token in wrong place", "", "", &websocket.DialOptions{Subprotocols: []string{TerminalProtocol, "token."}}, 401},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			name := cmp.Or(tt.session, "box")
			conn, res, err := websocket.Dial(ctx, wsURL(e.srv.URL, name, tt.query), tt.opts)
			if tt.want == 101 {
				if err != nil {
					t.Fatal(err)
				}
				conn.Close(websocket.StatusNormalClosure, "")
				return
			}
			if err == nil {
				conn.Close(websocket.StatusNormalClosure, "")
				t.Fatal("attach allowed")
			}
			if res == nil || res.StatusCode != tt.want {
				t.Fatalf("got %v, want %d", res, tt.want)
			}
		})
	}

	var attaches, denied int
	for _, en := range e.entries(t) {
		if !strings.HasPrefix(en.Procedure, "terminal.attach") || (en.Subject != "box" && en.Subject != "sleeve-box") {
			t.Errorf("unexpected entry %+v", en)
		}
		switch en.Outcome {
		case auth.OutcomeOK:
			attaches++
		case auth.OutcomeDenied:
			denied++
		}
	}
	if attaches != 9 || denied != 7 {
		t.Fatalf("attaches=%d denied=%d entries=%+v", attaches, denied, e.entries(t))
	}
	b, _ := os.ReadFile(mustGlob(t, filepath.Join(e.audit, "*"))[0])
	for _, secret := range []string{testToken, term, ro, star} {
		if strings.Contains(string(b), secret) {
			t.Errorf("audit log contains a secret")
		}
	}
}

func TestTailnetPathComposesIdentityAndToken(t *testing.T) {
	e := newAuthEnv(t)
	scoped := e.mint(t, "phone", time.Hour, "read", "backlog_*")
	admit := true
	ts := httptest.NewServer(e.h.tailnetHandler(func(*http.Request) (string, error) {
		if !admit {
			return "", errors.New("forbidden")
		}
		return "phone.tail.net", nil
	}))
	t.Cleanup(ts.Close)

	call := func(token string, add bool) (int, error) {
		hc := &http.Client{Transport: bearer{token}}
		ctx := context.Background()
		if add {
			_, err := protectoratev1connect.NewBacklogServiceClient(hc, ts.URL).CreateNote(ctx, connect.NewRequest(&pb.CreateNoteRequest{Title: "t"}))
			return 0, err
		}
		_, err := protectoratev1connect.NewHubServiceClient(hc, ts.URL).GetStatus(ctx, connect.NewRequest(&pb.GetStatusRequest{}))
		return 0, err
	}
	if _, err := call(testToken, false); err != nil {
		t.Fatalf("identified device with owner token: %v", err)
	}
	if _, err := call(scoped, false); err != nil {
		t.Fatalf("identified device with scoped token: %v", err)
	}
	if _, err := call("", false); code(err) != connect.CodeUnauthenticated {
		t.Fatalf("identified device without token must still be refused: %v", err)
	}
	if _, err := call("bad", true); code(err) != connect.CodeUnauthenticated {
		t.Fatalf("bad token: %v", err)
	}
	if _, err := call(scoped, true); err != nil {
		t.Fatalf("in-scope mutation: %v", err)
	}
	narrow := e.mint(t, "narrow", time.Hour, "read")
	if _, err := call(narrow, true); code(err) != connect.CodePermissionDenied {
		t.Fatalf("out-of-scope mutation over tailnet: %v", err)
	}

	admit = false
	for _, tok := range []string{testToken, scoped, ""} {
		req, _ := http.NewRequest(http.MethodPost, ts.URL+"/protectorate.v1.HubService/GetStatus", strings.NewReader("{}"))
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Authorization", "Bearer "+tok)
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != http.StatusForbidden {
			t.Fatalf("unidentified peer with token %q: status %d", tok, res.StatusCode)
		}
	}

	for _, en := range e.entries(t) {
		if en.Tailnet != "phone.tail.net" {
			t.Errorf("tailnet identity missing from %+v", en)
		}
	}
	if len(e.entries(t)) != 3 {
		t.Fatalf("entries = %+v", e.entries(t))
	}
}

func TestDefaultHubHasOwnerAuthAndNoDiskWrites(t *testing.T) {
	_, srv := newTestHub(t)
	hc := &http.Client{Transport: bearer{"x"}}
	_, err := protectoratev1connect.NewHubServiceClient(hc, srv.URL).GetStatus(context.Background(), connect.NewRequest(&pb.GetStatusRequest{}))
	if code(err) != connect.CodeUnauthenticated {
		t.Fatalf("err = %v", err)
	}
}

func TestSpawnSleeveReservesOrchestratorName(t *testing.T) {
	h := New("test", testToken)
	tests := []struct {
		name string
		ctx  context.Context
	}{
		{"no principal", context.Background()},
		{"scoped token", auth.WithPrincipal(context.Background(), &auth.Principal{Name: "phone", Scopes: []string{"sleeve_*"}})},
		{"owner outside SpawnEnvoy", auth.WithPrincipal(context.Background(), &auth.Principal{Owner: true})},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := h.SpawnSleeve(tt.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{Name: "envoy", Workspace: "/tmp"}))
			if code(err) != connect.CodePermissionDenied {
				t.Fatalf("err = %v, want permission_denied", err)
			}
		})
	}
}

func TestChatScopes(t *testing.T) {
	a := auth.New(testToken, nil, nil)
	read := &auth.Principal{Name: "r", Scopes: []string{"read"}}
	chat := &auth.Principal{Name: "c", Scopes: []string{"chat_*"}}
	tests := []struct {
		method string
		p      *auth.Principal
		ok     bool
	}{
		{"Watch", read, true},
		{"Watch", chat, true},
		{"Send", read, false},
		{"Send", chat, true},
		{"Interrupt", chat, true},
		{"Respond", read, false},
		{"Compact", chat, true},
		{"Compact", read, false},
	}
	for _, tc := range tests {
		err := a.AuthorizeProcedure(tc.p, "/protectorate.v1.ChatService/"+tc.method)
		if (err == nil) != tc.ok {
			t.Errorf("%s as %s: err = %v, want ok=%v", tc.method, tc.p.Name, err, tc.ok)
		}
	}
}

func TestListAuditEntriesOwnerOnly(t *testing.T) {
	e := newAuthEnv(t)
	ctx := context.Background()
	owner := protectoratev1connect.NewAuditServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL)
	tok := protectoratev1connect.NewTokenServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL)
	for _, name := range []string{"a", "b", "c"} {
		if _, err := tok.CreateToken(ctx, connect.NewRequest(&pb.CreateTokenRequest{Name: name, Scopes: []string{"read"}})); err != nil {
			t.Fatal(err)
		}
	}
	reader := protectoratev1connect.NewTokenServiceClient(&http.Client{Transport: bearer{e.mint(t, "reader", time.Hour, "read")}}, e.srv.URL)
	reader.RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: "x"}))

	first, err := owner.ListAuditEntries(ctx, connect.NewRequest(&pb.ListAuditEntriesRequest{Limit: 2}))
	if err != nil {
		t.Fatal(err)
	}
	es := first.Msg.Entries
	if len(es) != 2 || es[0].Actor != "token:reader" || es[0].Outcome != auth.OutcomeDenied || es[1].Subject != "c" || first.Msg.NextPageToken == "" {
		t.Fatalf("first page = %v", first.Msg)
	}
	rest, err := owner.ListAuditEntries(ctx, connect.NewRequest(&pb.ListAuditEntriesRequest{PageToken: first.Msg.NextPageToken}))
	if err != nil || len(rest.Msg.Entries) != 2 || rest.Msg.Entries[1].Subject != "a" || rest.Msg.NextPageToken != "" {
		t.Fatalf("second page = %v %v", rest, err)
	}
	denied, err := owner.ListAuditEntries(ctx, connect.NewRequest(&pb.ListAuditEntriesRequest{Outcome: auth.OutcomeDenied, Actor: "READER"}))
	if err != nil || len(denied.Msg.Entries) != 1 || !strings.HasSuffix(denied.Msg.Entries[0].Procedure, "/RevokeToken") {
		t.Fatalf("filtered = %v %v", denied, err)
	}

	tests := []struct {
		name string
		c    protectoratev1connect.AuditServiceClient
		req  *pb.ListAuditEntriesRequest
		want connect.Code
	}{
		{"read scope", protectoratev1connect.NewAuditServiceClient(&http.Client{Transport: bearer{e.mint(t, "r2", time.Hour, "read")}}, e.srv.URL), &pb.ListAuditEntriesRequest{}, connect.CodePermissionDenied},
		{"star scope", protectoratev1connect.NewAuditServiceClient(&http.Client{Transport: bearer{e.mint(t, "star", time.Hour, "*", "read")}}, e.srv.URL), &pb.ListAuditEntriesRequest{}, connect.CodePermissionDenied},
		{"bad outcome", owner, &pb.ListAuditEntriesRequest{Outcome: "maybe"}, connect.CodeInvalidArgument},
		{"bad cursor", owner, &pb.ListAuditEntriesRequest{PageToken: "../token:1"}, connect.CodeInvalidArgument},
		{"negative limit", owner, &pb.ListAuditEntriesRequest{Limit: -1}, connect.CodeInvalidArgument},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := tt.c.ListAuditEntries(ctx, connect.NewRequest(tt.req)); code(err) != tt.want {
				t.Fatalf("err = %v, want %v", err, tt.want)
			}
		})
	}
}
