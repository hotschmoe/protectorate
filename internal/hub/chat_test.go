//go:build linux

package hub

import (
	"context"
	"fmt"
	"math/rand"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// fakeClaude speaks enough of Claude's stream-json protocol for the chat
// layer, and sleeps when started as a TUI.
const fakeClaude = `#!/bin/sh
sid=s1
prev=
for a in "$@"; do
  case "$prev" in --resume|--session-id) sid="$a";; esac
  prev="$a"
done
echo "$@" >> "$PWD/args.log"
case " $* " in
  *" -p "*) ;;
  *) exec sleep 600;;
esac
compacts=0
tdir="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects/-w"
mkdir -p "$tdir"
echo '{"type":"system","subtype":"init","session_id":"'$sid'"}'
while read -r line; do
  case "$line" in
    *'"/compact"'*)
      compacts=$((compacts+1))
      if [ $compacts -eq 1 ]; then
        echo '{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual","pre_tokens":900,"post_tokens":90}}'
      fi
      echo '{"type":"result","subtype":"success","session_id":"'$sid'"}'
      ;;
    *approve*)
      echo '{"type":"control_request","request_id":"req1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/x"}}}'
      read -r resp
      echo '{"type":"assistant","message":{"id":"m2","model":"fake","content":[{"type":"text","text":"approved"}],"usage":{"input_tokens":1,"output_tokens":1}}}'
      echo '{"type":"result","subtype":"success","session_id":"'$sid'"}'
      ;;
    *slow*)
      read -r intr
      echo '{"type":"result","subtype":"error_during_execution","stop_reason":"cancelled","session_id":"'$sid'"}'
      ;;
    *)
      printf '%s\n' '{"type":"user","message":{"role":"user","content":"please answer ping"}}' \
        '{"type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"text","text":"pong"}]}}' >> "$tdir/$sid.jsonl"
      echo '{"type":"assistant","message":{"id":"m1","model":"fake-1","content":[{"type":"text","text":"po"}],"usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":40}}}'
      echo '{"type":"assistant","message":{"id":"m1","model":"fake-1","content":[{"type":"text","text":"ng"}],"usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":40}}}'
      echo '{"type":"result","subtype":"success","session_id":"'$sid'","modelUsage":{"fake-1":{"contextWindow":1000}}}'
      ;;
  esac
done
`

type chatRig struct {
	t    *testing.T
	ctx  context.Context
	h    *Hub
	srv  *httptest.Server
	sl   protectoratev1connect.SleeveServiceClient
	chat protectoratev1connect.ChatServiceClient
	ws   string
	name string
	cfg  sleeve.Config
	// harness is what spawn runs; zero means Claude.
	harness pb.Harness
}

func newChatRig(t *testing.T) *chatRig {
	t.Helper()
	t.Setenv("TMUX_TMPDIR", t.TempDir())
	bin := t.TempDir()
	for _, h := range []string{"claude", "codex", "grok", "pi"} {
		if err := os.WriteFile(filepath.Join(bin, h), []byte(fakeClaude), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("PATH", bin+":"+os.Getenv("PATH"))
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	for _, key := range []string{"PROTECTORATE_TOKEN_FILE", "PROTECTORATE_HUB", "PROTECTORATE_TOKEN"} {
		t.Setenv(key, "")
	}
	cfg, err := sleeve.DefaultConfig(home, []string{bin})
	if err != nil {
		t.Fatal(err)
	}
	cfg.UnitPrefix = fmt.Sprintf("prot-test-%06d-", rand.Intn(1000000))
	if err := cfg.Preflight(); err != nil {
		t.Skipf("sleeves unavailable: %v", err)
	}
	h := New("test", testToken)
	h.Heartbeat = 20 * time.Millisecond
	sock := "chat"
	h.Sessions = session.New(sock)
	h.SessionPoll = 20 * time.Millisecond
	name := fmt.Sprintf("chat%d", time.Now().UnixNano()%1000000)
	opts := h.SleeveOptions(cfg)
	opts.UsageInterval = 100 * time.Millisecond
	m, err := sleeve.Open(context.Background(), opts)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		for _, info := range m.List() {
			_ = m.Kill(ctx, info.Name)
		}
		exec.CommandContext(ctx, "tmux", "-L", sock, "kill-server").Run()
	})
	h.SetSleeves(m)
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); m.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done; h.chat.shutdown() })
	hc := &http.Client{Transport: bearer{testToken}}
	return &chatRig{
		t: t, ctx: ctx, h: h, cfg: cfg, srv: srv, name: name, ws: t.TempDir(),
		sl:   protectoratev1connect.NewSleeveServiceClient(hc, srv.URL),
		chat: protectoratev1connect.NewChatServiceClient(hc, srv.URL),
	}
}

func (r *chatRig) spawn(mode pb.SleeveMode) *pb.Sleeve {
	r.t.Helper()
	harness := r.harness
	if harness == pb.Harness_HARNESS_UNSPECIFIED {
		harness = pb.Harness_HARNESS_CLAUDE
	}
	res, err := r.sl.SpawnSleeve(r.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
		Name: r.name, Workspace: r.ws, Harness: harness, Mode: mode,
		Limits: &pb.SleeveLimits{CpuPercent: 100, MemoryBytes: 512 << 20},
	}))
	if err != nil {
		r.t.Fatal(err)
	}
	return res.Msg.Sleeve
}

func (r *chatRig) get() *pb.Sleeve {
	r.t.Helper()
	res, err := r.sl.GetSleeve(r.ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: r.name}))
	if err != nil {
		r.t.Fatal(err)
	}
	return res.Msg.Sleeve
}

func (r *chatRig) send(text, id string) (*pb.SendResponse, error) {
	res, err := r.chat.Send(r.ctx, connect.NewRequest(&pb.SendRequest{Sleeve: r.name, Text: text, RequestId: id}))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

func (r *chatRig) waitState(want pb.ChatState) {
	r.t.Helper()
	waitFor(r.t, "chat state "+want.String(), func() bool { return r.get().Chat.GetState() == want })
}

type feed struct {
	mu   sync.Mutex
	evs  []*pb.ChatEvent
	done chan error
}

func (r *chatRig) watch(after uint64) *feed {
	f := &feed{done: make(chan error, 1)}
	stream, err := r.chat.Watch(r.ctx, connect.NewRequest(&pb.WatchRequest{Sleeve: r.name, AfterSeq: after}))
	if err != nil {
		r.t.Fatal(err)
	}
	go func() {
		for stream.Receive() {
			f.mu.Lock()
			f.evs = append(f.evs, stream.Msg())
			f.mu.Unlock()
		}
		f.done <- stream.Err()
	}()
	return f
}

func (f *feed) all() []*pb.ChatEvent {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]*pb.ChatEvent(nil), f.evs...)
}

// describe renders events compactly, joining text deltas of one seq.
func describe(evs []*pb.ChatEvent) string {
	var out []string
	for _, e := range evs {
		switch b := e.Body.(type) {
		case *pb.ChatEvent_Text:
			if n := len(out); n > 0 && strings.HasPrefix(out[n-1], fmt.Sprintf("%d:text:", e.Seq)) {
				out[n-1] += b.Text.Delta
			} else {
				out = append(out, fmt.Sprintf("%d:text:%s", e.Seq, b.Text.Delta))
			}
		case *pb.ChatEvent_User:
			out = append(out, fmt.Sprintf("%d:user:%s:%s", e.Seq, b.User.From, b.User.Text))
		case *pb.ChatEvent_State:
			out = append(out, fmt.Sprintf("%d:state:%s", e.Seq, strings.TrimPrefix(b.State.State.String(), "CHAT_STATE_")))
		case *pb.ChatEvent_Settled:
			out = append(out, fmt.Sprintf("%d:settled", e.Seq))
		case *pb.ChatEvent_Usage:
			out = append(out, fmt.Sprintf("%d:usage:%d/%d", e.Seq, b.Usage.ContextTokens, b.Usage.ContextWindow))
		case *pb.ChatEvent_Compaction:
			out = append(out, fmt.Sprintf("%d:compaction:%d>%d:%v", e.Seq, b.Compaction.TokensBefore, b.Compaction.TokensAfter, b.Compaction.Native))
		case *pb.ChatEvent_Approval:
			out = append(out, fmt.Sprintf("%d:approval:%s", e.Seq, b.Approval.Id))
		case *pb.ChatEvent_ApprovalResolved:
			out = append(out, fmt.Sprintf("%d:resolved:%s:%v:%s", e.Seq, b.ApprovalResolved.Id, b.ApprovalResolved.Allow, b.ApprovalResolved.By))
		case *pb.ChatEvent_Error:
			out = append(out, fmt.Sprintf("%d:error:%s", e.Seq, b.Error.Text))
		default:
			out = append(out, fmt.Sprintf("%d:other", e.Seq))
		}
	}
	return strings.Join(out, " ")
}

func TestChatEndToEnd(t *testing.T) {
	r := newChatRig(t)

	evCtx, evCancel := context.WithCancel(r.ctx)
	defer evCancel()
	stream, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, r.srv.URL).
		Subscribe(evCtx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	states := make(chan string, 64)
	go func() {
		for stream.Receive() {
			if e := stream.Msg(); e.Kind == "chat.state" && e.Subject == r.name {
				states <- e.Payload.Fields["state"].GetStringValue()
			}
		}
	}()

	s := r.spawn(pb.SleeveMode_SLEEVE_MODE_UNSPECIFIED)
	if s.Mode != pb.SleeveMode_SLEEVE_MODE_CHAT || s.Session != "" || s.Chat.GetState() != pb.ChatState_CHAT_STATE_IDLE {
		t.Fatalf("spawned %v", s)
	}
	live := r.watch(0)

	first, err := r.send("hello", "req-0000001")
	if err != nil {
		t.Fatal(err)
	}
	again, err := r.send("hello", "req-0000001")
	if err != nil || again.Seq != first.Seq {
		t.Fatalf("request id dedupe: %v %v vs %v", err, again, first)
	}
	if _, err := r.send("x", "short"); code(err) != connect.CodeInvalidArgument {
		t.Fatalf("bad request id: %v", err)
	}
	waitFor(t, "first turn settles", func() bool { return strings.Contains(describe(live.all()), ":settled") })
	st := r.get().Chat
	if st.State != pb.ChatState_CHAT_STATE_IDLE || st.ContextTokens != 41 || st.ContextWindow != 1000 || st.Model != "fake-1" ||
		st.CacheExpiresAt == nil || st.HarnessSessionId == "" || st.LastSeq == 0 {
		t.Fatalf("chat status %v", st)
	}
	d := describe(live.all())
	for _, want := range []string{":user:owner:hello", ":text:pong", ":usage:41/", "state:WORKING", "state:IDLE"} {
		if !strings.Contains(d, want) {
			t.Fatalf("log lacks %q: %s", want, d)
		}
	}
	if strings.Count(d, ":user:") != 1 {
		t.Fatalf("deduped send was logged twice: %s", d)
	}
	waitFor(t, "chat.state event", func() bool { return len(states) > 0 })
	if got := <-states; got == "" {
		t.Fatal("empty chat.state payload")
	}

	// a late watcher replays the same log, and after_seq resumes without repeats
	late := r.watch(0)
	waitFor(t, "replay", func() bool { return describe(late.all()) == describe(live.all()) })
	mid := first.Seq
	resumed := r.watch(mid)
	waitFor(t, "resume", func() bool { return len(resumed.all()) > 0 })
	for _, e := range resumed.all() {
		if e.Seq <= mid {
			t.Fatalf("resume after %d replayed seq %d", mid, e.Seq)
		}
	}

	// approval
	if _, err := r.send("please approve", ""); err != nil {
		t.Fatal(err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_AWAITING_APPROVAL)
	if _, err := r.chat.Compact(r.ctx, connect.NewRequest(&pb.CompactRequest{Sleeve: r.name})); code(err) != connect.CodeFailedPrecondition {
		t.Fatalf("compact while awaiting approval: %v", err)
	}
	if _, err := r.chat.Respond(r.ctx, connect.NewRequest(&pb.RespondRequest{Sleeve: r.name, ApprovalId: "nope", Allow: true})); err == nil {
		t.Fatal("unknown approval accepted")
	}
	if _, err := r.chat.Respond(r.ctx, connect.NewRequest(&pb.RespondRequest{Sleeve: r.name, ApprovalId: "req1", Allow: true})); err != nil {
		t.Fatal(err)
	}
	waitFor(t, "approval turn settles", func() bool { return strings.Count(describe(live.all()), ":settled") == 2 })
	if d := describe(live.all()); !strings.Contains(d, ":approval:req1") || !strings.Contains(d, ":resolved:req1:true:owner") {
		t.Fatalf("approval not logged: %s", d)
	}

	// interrupt; claude takes no message mid-turn
	if _, err := r.send("slow one", ""); err != nil {
		t.Fatal(err)
	}
	if _, err := r.send("more", ""); code(err) != connect.CodeFailedPrecondition {
		t.Fatalf("send mid-turn on a harness without steering: %v", err)
	}
	if _, err := r.chat.Interrupt(r.ctx, connect.NewRequest(&pb.InterruptRequest{Sleeve: r.name})); err != nil {
		t.Fatal(err)
	}
	waitFor(t, "interrupted turn settles", func() bool { return strings.Count(describe(live.all()), ":settled") == 3 })
	r.waitState(pb.ChatState_CHAT_STATE_IDLE)

	// compaction
	res, err := r.chat.Compact(r.ctx, connect.NewRequest(&pb.CompactRequest{Sleeve: r.name}))
	if err != nil {
		t.Fatal(err)
	}
	if c := res.Msg.Compaction; c.TokensBefore != 900 || c.TokensAfter != 90 || c.Native {
		t.Fatalf("compaction %v", c)
	}
	waitFor(t, "compaction logged", func() bool { return strings.Contains(describe(live.all()), ":compaction:900>90:false") })
	if got := r.get().Chat; got.ContextTokens != 90 || got.State != pb.ChatState_CHAT_STATE_IDLE {
		t.Fatalf("status after compaction %v", got)
	}
	if _, err := r.chat.Compact(r.ctx, connect.NewRequest(&pb.CompactRequest{Sleeve: r.name})); code(err) != connect.CodeFailedPrecondition {
		t.Fatalf("second compaction: %v", err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_IDLE)

	// a hub restart: the driver goes, the log and the session id stay, and the
	// next send resumes the harness's own session
	sid := r.get().Chat.HarnessSessionId
	lastBefore := r.get().Chat.LastSeq
	r.h.chat.remove(r.name)
	if err := <-live.done; err != nil {
		t.Fatalf("watch ended by driver shutdown with %v", err)
	}
	if got := r.get().Chat; got.State != pb.ChatState_CHAT_STATE_IDLE || got.LastSeq != lastBefore {
		t.Fatalf("parked status %v", got)
	}
	live = r.watch(0)
	waitFor(t, "replay after restart", func() bool { return strings.Contains(describe(live.all()), ":compaction:900>90") })
	if _, err := r.send("back", ""); err != nil {
		t.Fatal(err)
	}
	waitFor(t, "turn after restart", func() bool { return strings.Count(describe(live.all()), ":settled") == 4 })
	args, _ := os.ReadFile(filepath.Join(r.ws, "args.log"))
	if lines := strings.Split(strings.TrimSpace(string(args)), "\n"); !strings.Contains(lines[len(lines)-1], "-p") || !strings.Contains(lines[len(lines)-1], "--resume "+sid) {
		t.Fatalf("chat did not resume %s: %q", sid, args)
	}
	seqs := map[uint64]bool{}
	for _, e := range live.all() {
		if e.Seq > lastBefore {
			seqs[e.Seq] = true
		}
	}
	if len(seqs) == 0 || !seqs[lastBefore+1] {
		t.Fatalf("seq did not continue after %d: %v", lastBefore, seqs)
	}

	// the sandbox accounts for a chat sleeve like a terminal one
	waitFor(t, "cgroup usage", func() bool { return r.get().Usage.GetMemoryCurrentBytes() > 0 })

	if _, err := r.sl.KillSleeve(r.ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: r.name})); err != nil {
		t.Fatal(err)
	}
	if err := <-live.done; err != nil {
		t.Fatalf("watch after kill ended with %v", err)
	}
	if _, err := r.send("dead", ""); code(err) != connect.CodeNotFound {
		t.Fatalf("send to killed sleeve: %v", err)
	}
}

func TestChatShellAndMissing(t *testing.T) {
	r := newChatRig(t)
	stream, err := r.chat.Watch(r.ctx, connect.NewRequest(&pb.WatchRequest{Sleeve: "nosuch"}))
	if err != nil {
		t.Fatal(err)
	}
	for stream.Receive() {
	}
	if code(stream.Err()) != connect.CodeNotFound {
		t.Fatalf("watch missing sleeve: %v", stream.Err())
	}
	if _, err := r.sl.SpawnSleeve(r.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
		Name: r.name, Workspace: r.ws, Harness: pb.Harness_HARNESS_SHELL, Mode: pb.SleeveMode_SLEEVE_MODE_CHAT,
	})); code(err) != connect.CodeInvalidArgument {
		t.Fatalf("shell chat spawn: %v", err)
	}
}

func TestChatSupervisedPermissionMode(t *testing.T) {
	tests := []struct {
		name       string
		supervised bool
		mode       pb.SleeveMode
		wantArg    string
		wantCode   connect.Code
	}{
		{"autonomous by default", false, pb.SleeveMode_SLEEVE_MODE_CHAT, "--permission-mode bypassPermissions", 0},
		{"supervised asks", true, pb.SleeveMode_SLEEVE_MODE_CHAT, "--permission-mode default", 0},
		{"supervised terminal refused", true, pb.SleeveMode_SLEEVE_MODE_TERMINAL, "", connect.CodeInvalidArgument},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			r := newChatRig(t)
			res, err := r.sl.SpawnSleeve(r.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
				Name: r.name, Workspace: r.ws, Harness: pb.Harness_HARNESS_CLAUDE, Mode: tc.mode, Supervised: tc.supervised,
			}))
			if tc.wantCode != 0 {
				if code(err) != tc.wantCode {
					t.Fatalf("spawn: %v, want %v", err, tc.wantCode)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if res.Msg.Sleeve.Supervised != tc.supervised {
				t.Fatalf("supervised = %v", res.Msg.Sleeve.Supervised)
			}
			waitFor(t, "harness args with "+tc.wantArg, func() bool {
				args, _ := os.ReadFile(filepath.Join(r.ws, "args.log"))
				return strings.Contains(string(args), tc.wantArg)
			})
		})
	}
}

func TestChatTerminalSleeveHasNoChat(t *testing.T) {
	r := newChatRig(t)
	if s := r.spawn(pb.SleeveMode_SLEEVE_MODE_TERMINAL); s.Mode != pb.SleeveMode_SLEEVE_MODE_TERMINAL || s.Session == "" || s.Chat != nil {
		t.Fatalf("terminal sleeve = %v", s)
	}
	if _, err := r.send("hi", ""); code(err) != connect.CodeFailedPrecondition {
		t.Fatalf("send to a terminal sleeve: %v", err)
	}
	args, _ := os.ReadFile(filepath.Join(r.ws, "args.log"))
	if strings.Contains(string(args), "-p") {
		t.Fatalf("terminal sleeve ran headless: %q", args)
	}
}

func TestSleeveTokenReadsOnlyItself(t *testing.T) {
	r := newChatRig(t)
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
	mint := func(sleeveName string) (protectoratev1connect.ChatServiceClient, protectoratev1connect.SessionServiceClient) {
		_, secret, err := r.h.authz.Store().Create(sleeveTokenName(sleeveName), sleeveScopes, 0, time.Now())
		if err != nil {
			t.Fatal(err)
		}
		hc := &http.Client{Transport: bearer{secret}}
		return protectoratev1connect.NewChatServiceClient(hc, r.srv.URL), protectoratev1connect.NewSessionServiceClient(hc, r.srv.URL)
	}
	otherChat, otherSess := mint("someoneelse")
	ownChat, _ := mint(r.name)

	watch := func(c protectoratev1connect.ChatServiceClient) error {
		ctx, cancel := context.WithTimeout(r.ctx, 300*time.Millisecond)
		defer cancel()
		stream, err := c.Watch(ctx, connect.NewRequest(&pb.WatchRequest{Sleeve: r.name}))
		if err != nil {
			return err
		}
		stream.Receive()
		return stream.Err()
	}
	if err := watch(otherChat); code(err) != connect.CodePermissionDenied {
		t.Fatalf("another sleeve's token watched this chat: %v", err)
	}
	if err := watch(ownChat); code(err) == connect.CodePermissionDenied {
		t.Fatalf("a sleeve was refused its own chat: %v", err)
	}
	if _, err := otherSess.CapturePane(r.ctx, connect.NewRequest(&pb.CapturePaneRequest{Name: sleeve.SessionName(r.name)})); code(err) != connect.CodePermissionDenied {
		t.Fatalf("another sleeve's token captured this session: %v", err)
	}
	list, err := otherSess.ListSessions(r.ctx, connect.NewRequest(&pb.ListSessionsRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	if len(list.Msg.Sessions) != 0 {
		t.Fatalf("another sleeve's token listed sessions: %v", list.Msg.Sessions)
	}
}

func TestChatSendRequestIDsSurviveHubRestart(t *testing.T) {
	r := newChatRig(t)
	r.spawn(pb.SleeveMode_SLEEVE_MODE_UNSPECIFIED)
	live := r.watch(0)
	first, err := r.send("hello", "req-persist-1")
	if err != nil {
		t.Fatal(err)
	}
	waitFor(t, "turn settles", func() bool { return strings.Contains(describe(live.all()), ":settled") })
	r.waitState(pb.ChatState_CHAT_STATE_IDLE)

	r.h.chat.remove(r.name)
	again, err := r.send("hello", "req-persist-1")
	if err != nil || again.Seq != first.Seq {
		t.Fatalf("send after restart: %v %v, want seq %d", err, again, first.Seq)
	}
	replay := r.watch(0)
	waitFor(t, "replay", func() bool { return strings.Contains(describe(replay.all()), ":settled") })
	if n := strings.Count(describe(replay.all()), ":user:"); n != 1 {
		t.Fatalf("%d user messages after a deduped resend: %s", n, describe(replay.all()))
	}
}
