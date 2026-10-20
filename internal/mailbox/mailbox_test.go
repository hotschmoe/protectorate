package mailbox

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"
)

type clock struct{ t time.Time }

func (c *clock) now() time.Time          { return c.t }
func (c *clock) advance(d time.Duration) { c.t = c.t.Add(d) }

func newStore(t *testing.T) (*Store, *clock, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "mailbox.jsonl")
	c := &clock{t: time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)}
	s, err := Open(path, c.now)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s, c, path
}

func mustSend(t *testing.T, s *Store, n NewMessage) Message {
	t.Helper()
	m, _, err := s.Send(n)
	if err != nil {
		t.Fatal(err)
	}
	return m
}

func TestSendDedupeAndThreads(t *testing.T) {
	s, _, _ := newStore(t)
	a := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Request, Body: "hi", RequestID: "req-1", Recipients: []string{"web"}})
	again, created, err := s.Send(NewMessage{From: "api", To: "web", Kind: Request, Body: "changed", RequestID: "req-1", Recipients: []string{"web"}})
	if err != nil || created || again.ID != a.ID || again.Body != "hi" {
		t.Fatalf("retry = %+v created=%v err=%v", again, created, err)
	}
	other := mustSend(t, s, NewMessage{From: "cli", To: "web", Kind: Note, Body: "x", RequestID: "req-1", Recipients: []string{"web"}})
	if other.ID == a.ID {
		t.Fatal("request ids must be scoped to the sender")
	}
	reply := mustSend(t, s, NewMessage{From: "web", To: "api", Kind: Note, Body: "re", Thread: a.Thread, ReplyTo: a.ID, Recipients: []string{"api"}})
	if a.ID != "m1" || a.Thread != "t1" || other.Thread != "t2" || reply.Thread != "t1" || reply.ID != "m3" {
		t.Fatalf("ids: %s/%s %s/%s %s/%s", a.ID, a.Thread, other.ID, other.Thread, reply.ID, reply.Thread)
	}
	var ids []string
	for _, m := range s.Thread("t1") {
		ids = append(ids, m.ID)
	}
	if !reflect.DeepEqual(ids, []string{"m1", "m3"}) {
		t.Fatalf("thread t1 = %v", ids)
	}
}

func TestInboxAndAck(t *testing.T) {
	s, _, _ := newStore(t)
	m1 := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Note, Body: "one", Recipients: []string{"web"}})
	m2 := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Note, Body: "two", Recipients: []string{"web"}})
	m3 := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Note, Body: "three", Recipients: []string{"web"}})
	mustSend(t, s, NewMessage{From: "api", To: "cli", Kind: Note, Body: "other", Recipients: []string{"cli"}})
	if err := s.Deliver(m2.ID, "web"); err != nil {
		t.Fatal(err)
	}

	ids := func(ms []Message) (out []string) {
		for _, m := range ms {
			out = append(out, m.ID)
		}
		return
	}
	peek, err := s.Inbox("web", "", true, true)
	if err != nil || !reflect.DeepEqual(ids(peek), []string{m1.ID, m3.ID}) {
		t.Fatalf("unread peek = %v %v", ids(peek), err)
	}
	again, _ := s.Inbox("web", "", true, true)
	if len(again) != 2 {
		t.Fatalf("peek must not mark read: %v", ids(again))
	}
	shown, _ := s.Inbox("web", "", true, false)
	if len(shown) != 2 {
		t.Fatalf("unread = %v", ids(shown))
	}
	if left, _ := s.Inbox("web", "", true, false); len(left) != 0 {
		t.Fatalf("reading marks read, still unread: %v", ids(left))
	}
	all, _ := s.Inbox("web", "", false, true)
	if len(all) != 3 {
		t.Fatalf("full inbox = %v", ids(all))
	}

	if _, _, err := s.Ack(m1.ID, "cli"); !errors.Is(err, ErrNotRecipient) {
		t.Fatalf("ack by a stranger: %v", err)
	}
	if _, _, err := s.Ack("m99", "web"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("ack unknown: %v", err)
	}
	got, changed, err := s.Ack(m1.ID, "web")
	if err != nil || !changed || got.Deliveries["web"].Acked == nil {
		t.Fatalf("ack = %+v %v %v", got, changed, err)
	}
	if _, changed, _ := s.Ack(m1.ID, "web"); changed {
		t.Fatal("second ack reported a change")
	}
}

func TestPersistenceAcrossRestart(t *testing.T) {
	s, c, path := newStore(t)
	a := mustSend(t, s, NewMessage{From: "api", To: "project:lib", Kind: Request, AckRequired: true, Body: "need x", RequestID: "req-1", Project: "github.com/a/lib", FromProject: "github.com/a/api"})
	if err := s.Resolve(a.ID, []string{"libdev", "libdev2"}, []string{"libdev2"}); err != nil {
		t.Fatal(err)
	}
	if err := s.Deliver(a.ID, "libdev"); err != nil {
		t.Fatal(err)
	}
	c.advance(time.Minute)
	if _, _, err := s.Ack(a.ID, "libdev2"); err != nil {
		t.Fatal(err)
	}
	if err := s.SetNoteID(a.ID, "b7f3"); err != nil {
		t.Fatal(err)
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	// a crash mid-append leaves half a line
	f, _ := os.OpenFile(path, os.O_WRONLY|os.O_APPEND, 0)
	f.WriteString(`{"op":"ack","id":"m1","rc`)
	f.Close()

	re, err := Open(path, c.now)
	if err != nil {
		t.Fatal(err)
	}
	defer re.Close()
	got, ok := re.Get(a.ID)
	if !ok {
		t.Fatal("message lost")
	}
	d := got.Deliveries
	if !reflect.DeepEqual(got.Recipients, []string{"libdev", "libdev2"}) || d["libdev"].Delivered == nil || d["libdev"].Acked != nil ||
		d["libdev2"].Acked == nil || d["libdev2"].Since.IsZero() || got.NoteID != "b7f3" || got.Open() {
		t.Fatalf("replayed message = %+v", got)
	}
	if again, created, _ := re.Send(NewMessage{From: "api", RequestID: "req-1", To: "project:lib", Kind: Request}); created || again.ID != a.ID {
		t.Fatalf("dedupe lost across restart: %+v", again)
	}
	next := mustSend(t, re, NewMessage{From: "x", To: "y", Kind: Note, Recipients: []string{"y"}})
	if next.ID != "m2" || next.Thread != "t2" {
		t.Fatalf("counters after restart: %s %s", next.ID, next.Thread)
	}
	raw, _ := os.ReadFile(path)
	if raw[len(raw)-1] != '\n' {
		t.Fatal("partial line was not cut off")
	}
}

func TestResolveOnce(t *testing.T) {
	s, _, _ := newStore(t)
	m := mustSend(t, s, NewMessage{From: "a", To: "project:p", Kind: Note, Project: "p"})
	if got := s.Unresolved(); len(got) != 1 {
		t.Fatalf("unresolved = %v", got)
	}
	if err := s.Resolve(m.ID, []string{"x"}, nil); err != nil {
		t.Fatal(err)
	}
	if err := s.Resolve(m.ID, []string{"y"}, nil); err != nil {
		t.Fatal(err)
	}
	got, _ := s.Get(m.ID)
	if !reflect.DeepEqual(got.Recipients, []string{"x"}) || len(s.Unresolved()) != 0 {
		t.Fatalf("recipients = %v", got.Recipients)
	}
	if p := s.Pending()["x"]; len(p) != 1 {
		t.Fatalf("pending = %v", p)
	}
	if err := s.Arm(m.ID, "x"); err != nil {
		t.Fatal(err)
	}
	if p := s.Pending()["x"]; len(p) != 0 {
		t.Fatalf("an armed recipient is not injected: %v", p)
	}
}

func TestOverdueTimeline(t *testing.T) {
	const timeout = 30 * time.Minute
	s, c, _ := newStore(t)
	req := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Request, AckRequired: true, Recipients: []string{"web"}})
	plain := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Note, Recipients: []string{"web"}})
	terminal := mustSend(t, s, NewMessage{From: "api", To: "shell", Kind: Request, AckRequired: true, Recipients: []string{"shell"}})
	acked := mustSend(t, s, NewMessage{From: "api", To: "web", Kind: Request, AckRequired: true, Recipients: []string{"web"}})
	for _, m := range []Message{req, plain, acked} {
		if err := s.Deliver(m.ID, "web"); err != nil {
			t.Fatal(err)
		}
	}
	if err := s.Arm(terminal.ID, "shell"); err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.Ack(acked.ID, "web"); err != nil {
		t.Fatal(err)
	}

	type step struct {
		advance time.Duration
		want    []Due
		do      func()
	}
	steps := []step{
		{advance: 29 * time.Minute, want: nil},
		{advance: time.Minute, want: []Due{{req.ID, "web", ActRemind}, {terminal.ID, "shell", ActRemind}},
			do: func() { s.Remind(req.ID, "web"); s.Remind(terminal.ID, "shell") }},
		{advance: 29 * time.Minute, want: nil},
		{advance: time.Minute, want: []Due{{req.ID, "web", ActEscalate}, {terminal.ID, "shell", ActEscalate}},
			do: func() { s.Escalate(req.ID) }},
		{advance: time.Hour, want: []Due{{terminal.ID, "shell", ActEscalate}}, do: func() { s.Escalate(terminal.ID) }},
		{advance: time.Hour, want: nil},
	}
	for i, st := range steps {
		c.advance(st.advance)
		got := s.Overdue(timeout)
		if !reflect.DeepEqual(got, st.want) {
			t.Fatalf("step %d: overdue = %v, want %v", i, got, st.want)
		}
		if st.do != nil {
			st.do()
		}
	}
}

func TestOpenRequests(t *testing.T) {
	s, _, _ := newStore(t)
	req := func(from, fromProj, proj string) Message {
		return mustSend(t, s, NewMessage{From: from, To: "project:" + proj, Kind: Request, Project: proj, FromProject: fromProj, Recipients: []string{"owner-of-" + proj}})
	}
	a := req("api", "api-proj", "lib")
	b := req("api", "api-proj", "lib")
	req("web", "web-proj", "lib")
	req("api", "api-proj", "other")
	mustSend(t, s, NewMessage{From: "api", To: "project:lib", Kind: Note, Project: "lib", FromProject: "api-proj"})
	if _, _, err := s.Ack(b.ID, "owner-of-lib"); err != nil {
		t.Fatal(err)
	}
	got := s.OpenRequests("lib", "api-proj")
	if len(got) != 1 || got[0].ID != a.ID {
		t.Fatalf("open requests = %v", got)
	}
}

func TestInvolving(t *testing.T) {
	s, _, _ := newStore(t)
	toAlice := mustSend(t, s, NewMessage{From: "bob", To: "alice", Kind: Request, AckRequired: true, Body: "do x", Recipients: []string{"alice"}})
	acked := mustSend(t, s, NewMessage{From: "bob", To: "alice", Kind: Request, AckRequired: true, Body: "done", Recipients: []string{"alice"}})
	mustSend(t, s, NewMessage{From: "bob", To: "alice", Kind: Note, Body: "fyi", Recipients: []string{"alice"}})
	fromAlice := mustSend(t, s, NewMessage{From: "alice", To: "lib", Kind: Request, AckRequired: true, Body: "export Foo", Recipients: []string{"lib"}})
	answered := mustSend(t, s, NewMessage{From: "alice", To: "lib", Kind: Request, AckRequired: true, Body: "old", Recipients: []string{"lib"}})
	mustSend(t, s, NewMessage{From: "bob", To: "carol", Kind: Request, AckRequired: true, Body: "other", Recipients: []string{"carol"}})
	for _, a := range [][2]string{{acked.ID, "alice"}, {answered.ID, "lib"}} {
		if _, _, err := s.Ack(a[0], a[1]); err != nil {
			t.Fatal(err)
		}
	}
	var ids []string
	for _, m := range s.Involving("alice") {
		ids = append(ids, m.ID)
	}
	if want := []string{toAlice.ID, fromAlice.ID}; !reflect.DeepEqual(ids, want) {
		t.Fatalf("involving alice = %v, want %v", ids, want)
	}
}

func TestThreadsSummary(t *testing.T) {
	s, c, _ := newStore(t)
	ask := mustSend(t, s, NewMessage{From: "api", To: "owner", Kind: Request, AckRequired: true, Body: "deploy?", Project: "", FromProject: "github.com/acme/api", Recipients: []string{"owner"}})
	c.advance(time.Minute)
	mustSend(t, s, NewMessage{From: "owner", To: "api", Kind: Note, Body: "later", Thread: ask.Thread, Recipients: []string{"api"}})
	c.advance(time.Minute)
	wait := mustSend(t, s, NewMessage{From: "web", To: "project:lib", Kind: Request, AckRequired: true, Body: "export Foo", Project: "github.com/acme/lib"})
	if err := s.Escalate(wait.ID); err != nil {
		t.Fatal(err)
	}
	c.advance(time.Minute)
	note := mustSend(t, s, NewMessage{From: "hub", To: "web", Kind: Notice, Body: "lease granted", Recipients: []string{"web"}})

	tests := []struct {
		name   string
		reader string
		thread string
		want   ThreadInfo
	}{
		{"owner sees its unread request", "owner", ask.Thread, ThreadInfo{ID: ask.Thread, Participants: []string{"api", "owner"}, Messages: 2, Unread: 1, OpenAcks: 1, Projects: []string{"github.com/acme/api"}}},
		{"waiting project request", "owner", wait.Thread, ThreadInfo{ID: wait.Thread, Participants: []string{"web", "project:github.com/acme/lib"}, Messages: 1, OpenAcks: 1, Escalated: 1, Projects: []string{"github.com/acme/lib"}}},
		{"recipient unread", "web", note.Thread, ThreadInfo{ID: note.Thread, Participants: []string{"hub", "web"}, Messages: 1, Unread: 1}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			all := s.Threads(tt.reader)
			if len(all) != 3 || all[0].ID != note.Thread || all[2].ID != ask.Thread {
				t.Fatalf("order = %+v", all)
			}
			var got ThreadInfo
			for _, ti := range all {
				if ti.ID == tt.thread {
					got = ti
				}
			}
			last := got.Last
			got.Last = Message{}
			if !reflect.DeepEqual(got, tt.want) {
				t.Fatalf("summary = %+v\nwant %+v", got, tt.want)
			}
			if want := s.Thread(tt.thread); last.ID != want[len(want)-1].ID {
				t.Fatalf("last = %s", last.ID)
			}
		})
	}

	if _, _, err := s.Ack(ask.ID, "owner"); err != nil {
		t.Fatal(err)
	}
	if ti := s.Threads("owner")[2]; ti.Unread != 0 || ti.OpenAcks != 0 {
		t.Fatalf("after ack: %+v", ti)
	}
	if got, _ := s.Inbox("web", wait.Thread, false, false); len(got) != 0 {
		t.Fatalf("thread filter leaked other threads: %v", got)
	}
	if got, _ := s.Inbox("web", note.Thread, false, false); len(got) != 1 || s.Threads("web")[0].Unread != 0 {
		t.Fatalf("reading a thread did not mark it read: %v", got)
	}
}
