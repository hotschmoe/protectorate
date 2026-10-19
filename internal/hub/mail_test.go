//go:build linux

package hub

import (
	"bufio"
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/encoding/protojson"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/mailbox"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

type mailRig struct {
	*chatRig
	home  string
	url   string
	clock *clock
	owner protectoratev1connect.MessageServiceClient
}

func newMailRig(t *testing.T) *mailRig {
	t.Helper()
	r := newChatRig(t)
	t.Setenv("PROTECTORATE_HOME", t.TempDir())
	r.h.SetBacklog(backlog.NewStore(filepath.Join(t.TempDir(), "backlog.md")))
	r.h.SelfURL = "http://127.0.0.1:1"
	m := &mailRig{chatRig: r, home: t.TempDir(), clock: &clock{t: time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)}}
	if err := r.h.EnableCoordination(m.home); err != nil {
		t.Fatal(err)
	}
	r.h.coord.now = m.clock.now
	t.Cleanup(r.h.coord.close)
	srv := httptest.NewServer(r.h.Handler())
	t.Cleanup(srv.Close)
	m.url = srv.URL
	m.owner = protectoratev1connect.NewMessageServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL)
	return m
}

// spawnNamed starts another sleeve in the same workspace layout.
func (m *mailRig) spawnNamed(name string, mode pb.SleeveMode, ws string) {
	m.t.Helper()
	m.t.Cleanup(func() {
		exec.Command("systemctl", "--user", "kill", "--signal=SIGKILL", sleeve.UnitName(name)+".scope").Run()
	})
	if ws == "" {
		ws = m.t.TempDir()
	}
	if _, err := m.sl.SpawnSleeve(m.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
		Name: name, Workspace: ws, Harness: pb.Harness_HARNESS_CLAUDE, Mode: mode,
		Limits: &pb.SleeveLimits{CpuPercent: 100, MemoryBytes: 512 << 20},
	})); err != nil {
		m.t.Fatal(err)
	}
}

// as returns a MessageService client that authenticates with the sleeve's own
// token file, the way the CLI inside the sleeve does.
func (m *mailRig) as(name string) protectoratev1connect.MessageServiceClient {
	m.t.Helper()
	raw, err := os.ReadFile(filepath.Join(m.h.sleeves.HomeDir(name), sleeveTokenFile))
	if err != nil {
		m.t.Fatal(err)
	}
	return protectoratev1connect.NewMessageServiceClient(&http.Client{Transport: bearer{strings.TrimSpace(string(raw))}}, m.url)
}

func (m *mailRig) post(c protectoratev1connect.MessageServiceClient, to, body string, mut ...func(*pb.MessageServiceSendRequest)) *pb.MessageServiceSendResponse {
	m.t.Helper()
	req := &pb.MessageServiceSendRequest{To: to, Body: body}
	for _, f := range mut {
		f(req)
	}
	res, err := c.Send(m.ctx, connect.NewRequest(req))
	if err != nil {
		m.t.Fatalf("send to %s: %v", to, err)
	}
	return res.Msg
}

func (m *mailRig) pass() { m.h.coord.pass(m.ctx) }

type userMsg struct{ from, text string }

// users reads the user messages in a sleeve's chat log.
func (m *mailRig) users(name string) []userMsg {
	m.t.Helper()
	f, err := os.Open(m.h.sleeves.ChatLogPath(name))
	if err != nil {
		m.t.Fatal(err)
	}
	defer f.Close()
	var out []userMsg
	sc := bufio.NewScanner(f)
	sc.Buffer(nil, 1<<20)
	for sc.Scan() {
		var ev pb.ChatEvent
		if err := protojson.Unmarshal(sc.Bytes(), &ev); err != nil {
			m.t.Fatal(err)
		}
		if u := ev.GetUser(); u != nil {
			out = append(out, userMsg{u.From, u.Text})
		}
	}
	return out
}

func (m *mailRig) waitIdle(name string) {
	m.t.Helper()
	waitFor(m.t, name+" idle", func() bool {
		res, err := m.sl.GetSleeve(m.ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: name}))
		return err == nil && res.Msg.Sleeve.Chat.GetState() == pb.ChatState_CHAT_STATE_IDLE
	})
}

func TestRenderMessage(t *testing.T) {
	tests := []struct {
		name     string
		m        mailbox.Message
		project  string
		reminder bool
		want     string
	}{
		{
			name: "request from a sleeve",
			m:    mailbox.Message{ID: "m42", Thread: "t7", From: "api", Kind: mailbox.Request, AckRequired: true, Body: "please export Foo\n"},
			want: "[request m42 from sleeve api, thread t7, ack required]\nplease export Foo\n\n" +
				"Reply: protectorate msg reply m42 \"...\"\nAck: protectorate msg ack m42",
		},
		{
			name:    "note to a project",
			m:       mailbox.Message{ID: "m3", Thread: "t1", From: "owner", Kind: mailbox.Note, Body: "heads up"},
			project: "acme/lib",
			want: "[message m3 from owner, thread t1, to project acme/lib]\nheads up\n\n" +
				"Reply: protectorate msg reply m3 \"...\"\nAck: protectorate msg ack m3",
		},
		{
			name: "hub notice needs no reply",
			m:    mailbox.Message{ID: "m9", Thread: "t4", From: "hub", Kind: mailbox.Notice, Body: "lib v1.2.0"},
			want: "[notice m9 from hub, thread t4]\nlib v1.2.0",
		},
		{
			name: "hub notice that needs an ack",
			m:    mailbox.Message{ID: "m9", Thread: "t4", From: "hub", Kind: mailbox.Notice, AckRequired: true, Body: "x"},
			want: "[notice m9 from hub, thread t4, ack required]\nx\n\nAck: protectorate msg ack m9",
		},
		{
			name:     "reminder",
			m:        mailbox.Message{ID: "m1", Thread: "t1", From: "api", Kind: mailbox.Note, AckRequired: true, Body: "b"},
			reminder: true,
			want: "[reminder, still unacked: message m1 from sleeve api, thread t1, ack required]\nb\n\n" +
				"Reply: protectorate msg reply m1 \"...\"\nAck: protectorate msg ack m1",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := renderMessage(tt.m, tt.project, tt.reminder); got != tt.want {
				t.Fatalf("got:\n%s\nwant:\n%s", got, tt.want)
			}
		})
	}
}

func TestMailWaitsForIdleAndIsAuthenticated(t *testing.T) {
	r := newMailRig(t)
	r.spawnNamed("sender", pb.SleeveMode_SLEEVE_MODE_TERMINAL, "")
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)

	// the fake claude holds a "slow" turn until it is interrupted
	if _, err := r.send("slow one", ""); err != nil {
		t.Fatal(err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_WORKING)

	res := r.post(r.as("sender"), r.name, "ping from the sleeve", func(q *pb.MessageServiceSendRequest) { q.AckRequired = true })
	if res.Message.From != "sender" || res.Message.Id != "m1" || res.Message.ThreadId != "t1" || res.Waiting {
		t.Fatalf("sent = %v", res)
	}
	r.pass()
	if got := r.users(r.name); len(got) != 1 {
		t.Fatalf("a message was injected into a running turn: %v", got)
	}

	if _, err := r.chat.Interrupt(r.ctx, connect.NewRequest(&pb.InterruptRequest{Sleeve: r.name})); err != nil {
		t.Fatal(err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_IDLE)
	r.pass()
	got := r.users(r.name)
	if len(got) != 2 || got[1].from != "sleeve:sender" {
		t.Fatalf("user messages = %v", got)
	}
	for _, want := range []string{"[message m1 from sleeve sender, thread t1, ack required]", "ping from the sleeve", "protectorate msg reply m1", "protectorate msg ack m1"} {
		if !strings.Contains(got[1].text, want) {
			t.Errorf("injected text lacks %q:\n%s", want, got[1].text)
		}
	}
	m, _ := r.h.coord.mail.Get("m1")
	if m.Deliveries[r.name].Delivered == nil {
		t.Fatal("delivery not recorded")
	}

	// the delivery made the chat work; the next message waits for its turn to end
	r.waitIdle(r.name)
	r.post(r.owner, r.name, "second")
	r.pass()
	r.waitIdle(r.name)
	if got := r.users(r.name); len(got) != 3 || got[2].from != "owner" {
		t.Fatalf("user messages = %v", got)
	}
}

func TestMailReplyKeepsThreadAndAcks(t *testing.T) {
	r := newMailRig(t)
	r.spawnNamed("alice", pb.SleeveMode_SLEEVE_MODE_TERMINAL, "")
	r.spawnNamed("bob", pb.SleeveMode_SLEEVE_MODE_TERMINAL, "")
	alice, bob := r.as("alice"), r.as("bob")

	asked := r.post(alice, "bob", "can you review?", func(q *pb.MessageServiceSendRequest) { q.Kind = pb.MessageKind_MESSAGE_KIND_REQUEST })
	if !asked.Message.AckRequired && asked.Message.Kind != pb.MessageKind_MESSAGE_KIND_REQUEST {
		t.Fatalf("request = %v", asked.Message)
	}
	if _, err := alice.Reply(r.ctx, connect.NewRequest(&pb.ReplyRequest{Id: asked.Message.Id, Body: "me?"})); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("a non-recipient replied: %v", err)
	}
	rep, err := bob.Reply(r.ctx, connect.NewRequest(&pb.ReplyRequest{Id: asked.Message.Id, Body: "done"}))
	if err != nil {
		t.Fatal(err)
	}
	if rep.Msg.Message.ThreadId != asked.Message.ThreadId || rep.Msg.Message.To != "alice" || rep.Msg.Message.ReplyTo != asked.Message.Id || rep.Msg.Message.From != "bob" {
		t.Fatalf("reply = %v", rep.Msg.Message)
	}
	th, err := alice.Thread(r.ctx, connect.NewRequest(&pb.ThreadRequest{ThreadId: asked.Message.ThreadId}))
	if err != nil || len(th.Msg.Messages) != 2 {
		t.Fatalf("thread = %v %v", th, err)
	}
	if d := th.Msg.Messages[0].Deliveries[0]; d.Recipient != "bob" || d.AckedAt == nil {
		t.Fatalf("reply did not ack the original: %v", th.Msg.Messages[0])
	}
	in, err := alice.Inbox(r.ctx, connect.NewRequest(&pb.InboxRequest{UnreadOnly: true}))
	if err != nil || len(in.Msg.Messages) != 1 || in.Msg.Messages[0].Body != "done" {
		t.Fatalf("alice's inbox = %v %v", in, err)
	}
	if in2, _ := alice.Inbox(r.ctx, connect.NewRequest(&pb.InboxRequest{UnreadOnly: true})); len(in2.Msg.Messages) != 0 {
		t.Fatalf("read messages came back as unread: %v", in2.Msg.Messages)
	}
	if _, err := alice.Inbox(r.ctx, connect.NewRequest(&pb.InboxRequest{As: "bob"})); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("a sleeve read another mailbox: %v", err)
	}
	if as, err := r.owner.Inbox(r.ctx, connect.NewRequest(&pb.InboxRequest{As: "bob", Peek: true})); err != nil || len(as.Msg.Messages) != 1 {
		t.Fatalf("owner reading bob's mailbox: %v %v", as, err)
	}
	if _, err := bob.Reply(r.ctx, connect.NewRequest(&pb.ReplyRequest{Id: "m99", Body: "x"})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("reply to unknown: %v", err)
	}
}

func TestMailSendValidationAndDedupe(t *testing.T) {
	r := newMailRig(t)
	r.spawnNamed("alice", pb.SleeveMode_SLEEVE_MODE_TERMINAL, "")
	alice := r.as("alice")

	first := r.post(alice, "owner", "hello", func(q *pb.MessageServiceSendRequest) { q.RequestId = "req-aaaaaaaa" })
	again := r.post(alice, "owner", "hello again", func(q *pb.MessageServiceSendRequest) { q.RequestId = "req-aaaaaaaa" })
	if !again.Duplicate || again.Message.Id != first.Message.Id || again.Message.Body != "hello" {
		t.Fatalf("dedupe: %v vs %v", again, first)
	}
	if n := len(r.h.coord.mail.Thread(first.Message.ThreadId)); n != 1 {
		t.Fatalf("thread has %d messages", n)
	}

	tests := []struct {
		name string
		req  *pb.MessageServiceSendRequest
		want connect.Code
	}{
		{"unknown sleeve", &pb.MessageServiceSendRequest{To: "nobody", Body: "x"}, connect.CodeNotFound},
		{"unknown project", &pb.MessageServiceSendRequest{To: "project:nothing", Body: "x"}, connect.CodeNotFound},
		{"to self", &pb.MessageServiceSendRequest{To: "alice", Body: "x"}, connect.CodeInvalidArgument},
		{"empty body", &pb.MessageServiceSendRequest{To: "owner", Body: " \n"}, connect.CodeInvalidArgument},
		{"no address", &pb.MessageServiceSendRequest{Body: "x"}, connect.CodeInvalidArgument},
		{"huge body", &pb.MessageServiceSendRequest{To: "owner", Body: strings.Repeat("x", maxBodyBytes+1)}, connect.CodeInvalidArgument},
		{"short request id", &pb.MessageServiceSendRequest{To: "owner", Body: "x", RequestId: "abc"}, connect.CodeInvalidArgument},
		{"unknown thread", &pb.MessageServiceSendRequest{To: "owner", Body: "x", ThreadId: "t99"}, connect.CodeNotFound},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := alice.Send(r.ctx, connect.NewRequest(tt.req)); connect.CodeOf(err) != tt.want {
				t.Fatalf("err = %v, want code %v", err, tt.want)
			}
		})
	}
}

func TestMailSurvivesRestartAndTerminalSleevesUseInbox(t *testing.T) {
	r := newMailRig(t)
	r.spawnNamed("term", pb.SleeveMode_SLEEVE_MODE_TERMINAL, "")
	sent := r.post(r.owner, "term", "read me", func(q *pb.MessageServiceSendRequest) { q.AckRequired = true })
	r.pass()
	m, _ := r.h.coord.mail.Get(sent.Message.Id)
	if d := m.Deliveries["term"]; d.Delivered != nil || d.Since.IsZero() {
		t.Fatalf("a terminal sleeve must not be injected, and its ack timer must run: %+v", d)
	}

	r.h.coord.close()
	if err := r.h.EnableCoordination(r.home); err != nil {
		t.Fatal(err)
	}
	r.h.coord.now = r.clock.now
	t.Cleanup(r.h.coord.close)
	srv := httptest.NewServer(r.h.Handler())
	t.Cleanup(srv.Close)
	r.url = srv.URL
	r.owner = protectoratev1connect.NewMessageServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL)
	m, ok := r.h.coord.mail.Get(sent.Message.Id)
	if !ok || m.Body != "read me" || m.Deliveries["term"] == nil {
		t.Fatalf("message lost across restart: %+v", m)
	}
	in, err := r.as("term").Inbox(r.ctx, connect.NewRequest(&pb.InboxRequest{UnreadOnly: true}))
	if err != nil || len(in.Msg.Messages) != 1 {
		t.Fatalf("inbox after restart = %v %v", in, err)
	}
	next := r.post(r.owner, "term", "again")
	if next.Message.Id != "m2" {
		t.Fatalf("ids restarted: %s", next.Message.Id)
	}
}

func TestMailEscalation(t *testing.T) {
	tests := []struct {
		name    string
		withPoe bool
	}{
		{"orchestrator running", true},
		{"no orchestrator", false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := newMailRig(t)
			r.h.AckTimeout = 30 * time.Minute
			r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
			if tt.withPoe {
				r.spawnNamed(sleeve.OrchestratorName, pb.SleeveMode_SLEEVE_MODE_TERMINAL, "")
			}
			events := make(chan string, 16)
			evCtx, stop := context.WithCancel(r.ctx)
			t.Cleanup(stop)
			stream, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, r.url).
				Subscribe(evCtx, connect.NewRequest(&pb.SubscribeRequest{}))
			if err != nil {
				t.Fatal(err)
			}
			go func() {
				for stream.Receive() {
					if e := stream.Msg(); strings.HasPrefix(e.Kind, "msg.") {
						events <- e.Kind + " " + e.Subject
					}
				}
			}()

			sent := r.post(r.owner, r.name, "please confirm", func(q *pb.MessageServiceSendRequest) { q.AckRequired = true })
			id := sent.Message.Id
			r.pass()
			r.waitIdle(r.name)
			if m, _ := r.h.coord.mail.Get(id); m.Deliveries[r.name].Delivered == nil {
				t.Fatal("not delivered")
			}

			steps := []struct {
				advance   time.Duration
				reminders int
				escalated bool
			}{
				{29 * time.Minute, 0, false},
				{time.Minute, 1, false},
				{29 * time.Minute, 1, false},
				{time.Minute, 1, true},
				{5 * time.Hour, 1, true},
			}
			for i, st := range steps {
				r.clock.advance(st.advance)
				r.pass()
				r.waitIdle(r.name)
				m, _ := r.h.coord.mail.Get(id)
				if got := m.Deliveries[r.name].Reminders; got != st.reminders || (m.Escalated != nil) != st.escalated {
					t.Fatalf("step %d: reminders %d escalated %v, want %d %v", i, got, m.Escalated != nil, st.reminders, st.escalated)
				}
			}
			users := r.users(r.name)
			if len(users) != 2 || !strings.HasPrefix(users[1].text, "[reminder, still unacked: message "+id) {
				t.Fatalf("the reminder was not injected once: %v", users)
			}

			waitFor(t, "escalation event", func() bool {
				for len(events) > 0 {
					if <-events == "msg.escalated "+id {
						return true
					}
				}
				return false
			})
			if tt.withPoe {
				box, _ := r.h.coord.mail.Inbox(sleeve.OrchestratorName, false, true)
				if len(box) != 1 || box[0].From != "hub" || box[0].Kind != mailbox.Notice || box[0].Thread != sent.Message.ThreadId || !strings.Contains(box[0].Body, id) {
					t.Fatalf("poe's inbox = %+v", box)
				}
			} else if n := len(r.h.coord.mail.Thread(sent.Message.ThreadId)); n != 1 {
				t.Fatalf("a notice was sent to an orchestrator that is not running: %d messages", n)
			}
		})
	}
}

func TestMailProjectAddressing(t *testing.T) {
	r := newMailRig(t)
	libWS := t.TempDir()
	if err := os.WriteFile(filepath.Join(libWS, "go.mod"), []byte("module github.com/acme/lib\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, libWS, "init", "-q")
	gitIn(t, libWS, "remote", "add", "origin", "git@github.com:acme/lib.git")
	apiWS := t.TempDir()
	if err := os.WriteFile(filepath.Join(apiWS, "go.mod"), []byte("module github.com/acme/api\n\nrequire github.com/acme/lib v1.0.0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, apiWS, "init", "-q")
	gitIn(t, apiWS, "remote", "add", "origin", "git@github.com:acme/api.git")

	r.spawnNamed("apidev", pb.SleeveMode_SLEEVE_MODE_CHAT, apiWS)
	r.spawnNamed("libdev", pb.SleeveMode_SLEEVE_MODE_CHAT, libWS)
	r.h.coord.refresh(r.ctx)
	r.waitIdle("apidev")
	r.waitIdle("libdev")
	apidev := r.as("apidev")

	got := r.post(apidev, "project:acme/lib", "please export Foo", func(q *pb.MessageServiceSendRequest) { q.Kind = pb.MessageKind_MESSAGE_KIND_REQUEST })
	if got.Waiting || got.BacklogItem != "" || len(got.Message.Deliveries) != 1 || got.Message.Deliveries[0].Recipient != "libdev" {
		t.Fatalf("project address did not resolve to its sleeve: %v", got)
	}
	r.pass()
	r.waitIdle("libdev")
	us := r.users("libdev")
	if len(us) != 1 || us[0].from != "sleeve:apidev" || !strings.Contains(us[0].text, "to project acme/lib") {
		t.Fatalf("libdev got %v", us)
	}

	// no live owner: the request waits and is filed as a backlog item
	if _, err := r.sl.KillSleeve(r.ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: "libdev"})); err != nil {
		t.Fatal(err)
	}
	r.h.coord.refresh(r.ctx)
	waiting := r.post(apidev, "project:lib", "and also Bar", func(q *pb.MessageServiceSendRequest) { q.Kind = pb.MessageKind_MESSAGE_KIND_REQUEST })
	if !waiting.Waiting || waiting.BacklogItem == "" {
		t.Fatalf("waiting request = %v", waiting)
	}
	items, err := r.h.backlog.List(backlog.Filter{Project: "acme/lib"})
	if err != nil || len(items) != 1 || !strings.Contains(items[0].Title, "apidev asks: and also Bar") || items[0].ID != waiting.BacklogItem {
		t.Fatalf("backlog = %v %v", items, err)
	}
	note := r.post(apidev, "project:lib", "fyi")
	if !note.Waiting || note.BacklogItem != "" {
		t.Fatalf("a note must wait without a backlog item: %v", note)
	}

	// a sleeve for the project appears: the waiting messages go to it
	r.spawnNamed("libdev2", pb.SleeveMode_SLEEVE_MODE_CHAT, libWS)
	r.h.coord.refresh(r.ctx)
	r.pass()
	r.waitIdle("libdev2")
	us = r.users("libdev2")
	if len(us) != 1 || !strings.Contains(us[0].text, "and also Bar") || !strings.Contains(us[0].text, "fyi") || us[0].from != "mailbox" {
		t.Fatalf("libdev2 got %v", us)
	}
	if m, _ := r.h.coord.mail.Get(waiting.Message.Id); len(m.Recipients) != 1 || m.Recipients[0] != "libdev2" {
		t.Fatalf("recipients = %v", m.Recipients)
	}
}

func TestLeaseGrantSendsNotice(t *testing.T) {
	r := newMailRig(t)
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
	tb, err := OpenLeases(filepath.Join(t.TempDir(), "leases.yaml"), nil)
	if err != nil {
		t.Fatal(err)
	}
	tb.onGrant = r.h.leaseGranted
	r.h.leases = tb
	busy := func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
		if l := blocked("gpubox", nil); l != nil {
			return "", nil, busyErr("leased by %s", l.Holder)
		}
		return "gpubox", nil, nil
	}
	held, err := tb.Acquire("someone", time.Hour, busy)
	if err != nil {
		t.Fatal(err)
	}
	ticket, err := tb.AcquireWait(Wait{Holder: r.name, Notify: r.name, Resource: "gpubox", TTL: 45 * time.Minute}, busy)
	if err != nil || ticket.State != pb.LeaseState_LEASE_STATE_QUEUED {
		t.Fatalf("ticket = %v %v", ticket, err)
	}
	if got := r.users(r.name); len(got) != 0 {
		t.Fatalf("notified before the grant: %v", got)
	}
	if err := tb.Release(held.Id); err != nil {
		t.Fatal(err)
	}
	r.pass()
	r.waitIdle(r.name)
	us := r.users(r.name)
	if len(us) != 1 || us[0].from != "hub" {
		t.Fatalf("user messages = %v", us)
	}
	granted := tb.List()
	if len(granted) != 1 || granted[0].Holder != r.name {
		t.Fatalf("leases = %v", granted)
	}
	for _, want := range []string{"[notice m1 from hub", "gpubox", "protectorate lease renew " + granted[0].Id, "45m0s"} {
		if !strings.Contains(us[0].text, want) {
			t.Errorf("notice lacks %q:\n%s", want, us[0].text)
		}
	}
	if strings.Contains(us[0].text, "msg reply") {
		t.Errorf("a hub notice must not offer a reply:\n%s", us[0].text)
	}
}
