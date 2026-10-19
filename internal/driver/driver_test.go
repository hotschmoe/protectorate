package driver

import (
	"errors"
	"os"
	"strings"
	"testing"
	"time"
)

func TestNewSession(t *testing.T) {
	for _, fx := range fixtures() {
		t.Run(string(fx.h), func(t *testing.T) {
			s := fx.open(t, "")
			if s.SessionID() == "" {
				t.Fatal("no session id after Open")
			}
			if err := s.Send(ctx5(t), "hello"); err != nil {
				t.Fatal(err)
			}
			evs := until(t, s, KindSettled)
			if txt := joinText(evs); !strings.Contains(txt, "ok id="+s.SessionID()) {
				t.Fatalf("text %q lacks session id %q", txt, s.SessionID())
			}
			last := evs[len(evs)-1]
			if last.StopReason == "" {
				t.Fatal("empty stop reason")
			}
			var settled int
			for _, e := range evs {
				if e.Kind == KindSettled {
					settled++
				}
			}
			if settled != 1 {
				t.Fatalf("settled count %d", settled)
			}
			// a second turn on the same session works after settling
			if err := s.Send(ctx5(t), "again"); err != nil {
				t.Fatal(err)
			}
			until(t, s, KindSettled)
			if err := s.Close(); err != nil {
				t.Fatal(err)
			}
			tail := until(t, s, KindExited)
			if tail[len(tail)-1].Kind != KindExited {
				t.Fatal("exited not last")
			}
			if _, ok := <-s.Events(); ok {
				t.Fatal("events not closed after exited")
			}
			if err := s.Send(ctx5(t), "x"); err == nil {
				t.Fatal("send after close succeeded")
			}
		})
	}
}

func TestEventDetails(t *testing.T) {
	for _, fx := range fixtures() {
		t.Run(string(fx.h), func(t *testing.T) {
			s := fx.open(t, "")
			if err := s.Send(ctx5(t), "hello"); err != nil {
				t.Fatal(err)
			}
			evs := until(t, s, KindSettled)
			var usage bool
			var tool bool
			for _, e := range evs {
				usage = usage || e.Kind == KindUsage && e.Usage != nil && e.Usage.InputTokens > 0
				tool = tool || e.Kind == KindToolCall && e.Tool != nil && e.Tool.Name != ""
			}
			wantTool := fx.h == Codex || fx.h == Pi
			if tool != wantTool {
				t.Fatalf("tool call seen=%v want %v (%v)", tool, wantTool, kinds(evs))
			}
			if !usage {
				t.Fatalf("no usage event: %v", kinds(evs))
			}
		})
	}
}

func TestResume(t *testing.T) {
	for _, fx := range fixtures() {
		t.Run(string(fx.h), func(t *testing.T) {
			s := fx.open(t, "sess-123")
			if s.SessionID() != "sess-123" {
				t.Fatalf("id %q", s.SessionID())
			}
			if err := s.Send(ctx5(t), "hello"); err != nil {
				t.Fatal(err)
			}
			txt := joinText(until(t, s, KindSettled))
			if !strings.Contains(txt, "id=sess-123") {
				t.Fatalf("harness did not resume sess-123: %q", txt)
			}
			if strings.Contains(txt, "REPLAY") {
				t.Fatalf("history replay leaked into events: %q", txt)
			}
		})
	}
}

func TestApproval(t *testing.T) {
	cases := []struct {
		name  string
		allow bool
		want  string
	}{
		{"allow", true, "allowed"},
		{"deny", false, "denied"},
	}
	for _, fx := range fixtures() {
		for _, tc := range cases {
			t.Run(string(fx.h)+"/"+tc.name, func(t *testing.T) {
				s := fx.open(t, "")
				if !s.Caps().Approvals {
					if err := s.Respond(ctx5(t), "x", true, ""); !errors.Is(err, ErrUnsupported) {
						t.Fatalf("Respond on ungated harness: %v", err)
					}
					return
				}
				if err := s.Send(ctx5(t), "please approve"); err != nil {
					t.Fatal(err)
				}
				evs := until(t, s, KindApproval)
				ap := evs[len(evs)-1].Approval
				if ap == nil || ap.ID == "" || ap.Kind == "" || ap.Summary == "" || len(ap.Raw) == 0 {
					t.Fatalf("incomplete approval: %+v", ap)
				}
				if err := s.Respond(ctx5(t), ap.ID, tc.allow, "nope"); err != nil {
					t.Fatal(err)
				}
				if err := s.Respond(ctx5(t), ap.ID, tc.allow, ""); !errors.Is(err, ErrUnknownApproval) {
					t.Fatalf("second Respond: %v", err)
				}
				txt := joinText(until(t, s, KindSettled))
				if !strings.Contains(txt, tc.want) {
					t.Fatalf("got %q want %q", txt, tc.want)
				}
				if fx.h == Grok && tc.allow && !strings.Contains(txt, "gate=true") {
					t.Fatalf("/always-approve off not sent before the turn: %q", txt)
				}
				if fx.h == Claude && !tc.allow && !strings.Contains(txt, "denied:nope") {
					t.Fatalf("deny note not forwarded: %q", txt)
				}
			})
		}
	}
}

func TestInterruptAndSteer(t *testing.T) {
	for _, fx := range fixtures() {
		t.Run(string(fx.h)+"/interrupt", func(t *testing.T) {
			s := fx.open(t, "")
			if !s.Caps().Interrupt {
				t.Skip("no interrupt")
			}
			if err := s.Send(ctx5(t), "slow"); err != nil {
				t.Fatal(err)
			}
			if err := s.Interrupt(ctx5(t)); err != nil {
				t.Fatal(err)
			}
			evs := until(t, s, KindSettled)
			if got := evs[len(evs)-1].StopReason; got != "cancelled" {
				t.Fatalf("stop reason %q", got)
			}
			if err := s.Interrupt(ctx5(t)); err != nil {
				t.Fatalf("idle interrupt: %v", err)
			}
		})
		t.Run(string(fx.h)+"/send-while-busy", func(t *testing.T) {
			s := fx.open(t, "")
			if err := s.Send(ctx5(t), "slow"); err != nil {
				t.Fatal(err)
			}
			err := s.Send(ctx5(t), "more")
			if s.Caps().Steer {
				if err != nil {
					t.Fatal(err)
				}
				if txt := joinText(until(t, s, KindSettled)); !strings.Contains(txt, "steered") {
					t.Fatalf("steer not delivered: %q", txt)
				}
				return
			}
			if !errors.Is(err, ErrBusy) {
				t.Fatalf("want ErrBusy, got %v", err)
			}
			if err := s.Interrupt(ctx5(t)); err != nil {
				t.Fatal(err)
			}
			until(t, s, KindSettled)
		})
	}
}

func TestExitMidTurn(t *testing.T) {
	for _, fx := range fixtures() {
		t.Run(string(fx.h), func(t *testing.T) {
			s := fx.open(t, "")
			if err := s.Send(ctx5(t), "die now"); err != nil {
				t.Fatal(err)
			}
			evs := until(t, s, KindExited)
			ks := kinds(evs)
			if len(ks) < 2 || ks[len(ks)-2] != KindError {
				t.Fatalf("want error then exited, got %v", ks)
			}
			for _, k := range ks {
				if k == KindSettled {
					t.Fatalf("settled after death: %v", ks)
				}
			}
			if fx.h != Codex && evs[len(evs)-1].ExitCode != 3 {
				t.Fatalf("exit code %d", evs[len(evs)-1].ExitCode)
			}
			if _, ok := <-s.Events(); ok {
				t.Fatal("stream not closed")
			}
			if err := s.Send(ctx5(t), "x"); err == nil {
				t.Fatal("send after death succeeded")
			}
		})
	}
}

func TestCodexResumeLocked(t *testing.T) {
	_, err := Open(ctx5(t), Codex, t.TempDir(), "locked", Options{CodexSocket: fakeCodex(t)})
	if err == nil || !strings.Contains(err.Error(), "active writer") {
		t.Fatalf("got %v", err)
	}
}

func TestCodexNoDaemon(t *testing.T) {
	_, err := Open(ctx5(t), Codex, t.TempDir(), "", Options{CodexSocket: t.TempDir() + "/none.sock"})
	if err == nil {
		t.Fatal("expected connect error")
	}
}

func TestOpenErrors(t *testing.T) {
	if _, err := Open(ctx5(t), "nope", t.TempDir(), "", Options{}); err == nil {
		t.Fatal("unknown harness accepted")
	}
	if _, err := Open(ctx5(t), Claude, t.TempDir(), "", Options{Bin: "/nonexistent/claude"}); err == nil {
		t.Fatal("missing binary accepted")
	}
}

func TestCaps(t *testing.T) {
	want := map[Harness]Caps{
		Claude: {Approvals: true, Interrupt: true},
		Codex:  {Approvals: true, Interrupt: true, Steer: true},
		Grok:   {Approvals: true, Interrupt: true},
		Pi:     {Interrupt: true, Steer: true},
	}
	for h, w := range want {
		got, err := CapsFor(h)
		if err != nil || got != w {
			t.Errorf("%s: %+v %v", h, got, err)
		}
	}
	if _, err := CapsFor("x"); err == nil {
		t.Error("unknown harness has caps")
	}
}

func TestStderrBounded(t *testing.T) {
	s, err := Open(ctx5(t), Claude, t.TempDir(), "", Options{
		Bin: os.Args[0], Env: fakeEnv("claude", "DRIVER_FAKE_STDERR=1"), StderrLimit: 64, StopTimeout: time.Second,
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Send(ctx5(t), "hello"); err != nil {
		t.Fatal(err)
	}
	until(t, s, KindSettled)
	s.Close()
	evs := until(t, s, KindExited)
	if n := len(evs[len(evs)-1].Text); n != 64 {
		t.Fatalf("stderr tail %d bytes, want 64", n)
	}
}

func TestCloseEscalates(t *testing.T) {
	for _, mode := range []string{"hang", "stubborn"} {
		t.Run(mode, func(t *testing.T) {
			s, err := startHang(t, mode)
			if err != nil {
				t.Fatal(err)
			}
			start := time.Now()
			if err := s.Close(); err != nil {
				t.Fatal(err)
			}
			if d := time.Since(start); d > 6*time.Second {
				t.Fatalf("close took %v", d)
			}
			ev := until(t, s, KindExited)
			if len(ev) == 0 {
				t.Fatal("no exited event")
			}
		})
	}
}

func startHang(t *testing.T, mode string) (Session, error) {
	t.Helper()
	return Open(ctx5(t), Claude, t.TempDir(), "", Options{
		Bin: os.Args[0], Env: fakeEnv(mode), StopTimeout: 200 * time.Millisecond,
	})
}

func TestOwners(t *testing.T) {
	o := NewOwners()
	rel, err := o.Acquire(Claude, "s1", OwnerTUI, "tmux:a")
	if err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name string
		h    Harness
		id   string
		kind OwnerKind
		ok   bool
	}{
		{"same session driver vs tui", Claude, "s1", OwnerDriver, false},
		{"same session tui vs tui", Claude, "s1", OwnerTUI, false},
		{"other id", Claude, "s2", OwnerDriver, true},
		{"other harness same id", Pi, "s1", OwnerDriver, true},
		{"empty id", Claude, "", OwnerDriver, false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			r, err := o.Acquire(tc.h, tc.id, tc.kind, "x")
			if (err == nil) != tc.ok {
				t.Fatalf("err=%v want ok=%v", err, tc.ok)
			}
			if err == nil {
				r()
				return
			}
			var ce *ConflictError
			if tc.id != "" && (!errors.As(err, &ce) || ce.Current.Holder != "tmux:a" || !strings.Contains(err.Error(), "already owned")) {
				t.Fatalf("not a conflict error: %v", err)
			}
		})
	}
	rel()
	rel()
	if _, ok := o.Holder(Claude, "s1"); ok {
		t.Fatal("still held after release")
	}
	r2, err := o.Acquire(Claude, "s1", OwnerDriver, "d")
	if err != nil {
		t.Fatal(err)
	}
	rel() // stale release must not drop the new owner
	if _, ok := o.Holder(Claude, "s1"); !ok {
		t.Fatal("stale release freed new owner")
	}
	r2()
}

func TestOwnersOpenDriver(t *testing.T) {
	o := NewOwners()
	fresh := Options{Bin: os.Args[0], Env: fakeEnv("pi"), StopTimeout: time.Second}

	s, err := o.OpenDriver(ctx5(t), Pi, t.TempDir(), "", "job1", fresh)
	if err != nil {
		t.Fatal(err)
	}
	id := s.SessionID()
	if h, ok := o.Holder(Pi, id); !ok || h.Kind != OwnerDriver || h.Holder != "job1" {
		t.Fatalf("new session not claimed: %+v %v", h, ok)
	}
	_, err = o.OpenDriver(ctx5(t), Pi, t.TempDir(), id, "job2", fresh)
	var ce *ConflictError
	if !errors.As(err, &ce) {
		t.Fatalf("want conflict, got %v", err)
	}
	if _, err := o.Acquire(Pi, id, OwnerTUI, "tmux"); err == nil {
		t.Fatal("TUI attach allowed while driver live")
	}
	s.Close()
	if _, ok := o.Holder(Pi, id); ok {
		t.Fatal("not released on Close")
	}
	rel, err := o.Acquire(Pi, id, OwnerTUI, "tmux")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := o.OpenDriver(ctx5(t), Pi, t.TempDir(), id, "job3", fresh); err == nil {
		t.Fatal("driver opened while TUI owns the session")
	}
	rel()

	s2, err := o.OpenDriver(ctx5(t), Pi, t.TempDir(), id, "job4", fresh)
	if err != nil {
		t.Fatal(err)
	}
	if err := s2.Send(ctx5(t), "die now"); err != nil {
		t.Fatal(err)
	}
	until(t, s2, KindExited)
	deadline := time.Now().Add(2 * time.Second)
	for {
		if _, ok := o.Holder(Pi, id); !ok {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("not released after process death")
		}
		time.Sleep(10 * time.Millisecond)
	}

	if _, err := o.OpenDriver(ctx5(t), Claude, t.TempDir(), "", "bad", Options{Bin: "/nonexistent"}); err == nil {
		t.Fatal("expected open failure")
	}
}

func TestCommandWrapsHarness(t *testing.T) {
	var gotBin string
	var gotArgs []string
	s, err := Open(ctx5(t), Claude, t.TempDir(), "sess-9", Options{
		Env: fakeEnv("claude"), StopTimeout: time.Second,
		Command: func(bin string, args []string) ([]string, error) {
			gotBin, gotArgs = bin, args
			return append([]string{os.Args[0]}, args...), nil
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if gotBin != "claude" || !strings.Contains(strings.Join(gotArgs, " "), "--resume sess-9") {
		t.Fatalf("command saw bin=%q args=%v", gotBin, gotArgs)
	}
	if err := s.Send(ctx5(t), "hello"); err != nil {
		t.Fatal(err)
	}
	if txt := joinText(until(t, s, KindSettled)); !strings.Contains(txt, "id=sess-9") {
		t.Fatalf("wrapped harness did not run: %q", txt)
	}
	if _, err := Open(ctx5(t), Claude, t.TempDir(), "", Options{
		Command: func(string, []string) ([]string, error) { return nil, errors.New("no sandbox") },
	}); err == nil || !strings.Contains(err.Error(), "no sandbox") {
		t.Fatalf("command error not surfaced: %v", err)
	}
}
