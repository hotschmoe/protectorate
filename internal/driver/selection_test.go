package driver

import (
	"errors"
	"os"
	"testing"
	"time"
)

func TestCodexSelectionProtocol(t *testing.T) {
	opts := Options{Bin: os.Args[0], Env: fakeEnv("codex"), StopTimeout: time.Second}
	s, err := Open(ctx5(t), Codex, t.TempDir(), "", opts)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	id := s.SessionID()
	models, err := s.Models(ctx5(t))
	if err != nil || len(models) != 2 || models[1].ID != "gpt-other" {
		t.Fatalf("paginated catalog: %+v %v", models, err)
	}
	for _, tc := range []struct {
		name      string
		requested Selection
		want      string
	}{
		{"untouched", Selection{}, "native"},
		{"explicit", Selection{Model: "gpt-other", Effort: "xhigh"}, "gpt-other/xhigh"},
		{"model default effort", Selection{Model: "gpt-other"}, "gpt-other/medium"},
		{"restore original", Selection{}, "gpt-fake/low"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if tc.name != "untouched" {
				if err := s.SetSelection(ctx5(t), tc.requested); err != nil {
					t.Fatal(err)
				}
			}
			if err := s.Send(ctx5(t), "selection"); err != nil {
				t.Fatal(err)
			}
			if got := joinText(until(t, s, KindSettled)); got != tc.want {
				t.Fatalf("native received %q, want %q", got, tc.want)
			}
			if s.SessionID() != id {
				t.Fatal("selection replaced native session")
			}
		})
	}
	before := s.Selection()
	for _, bad := range []Selection{{Model: "missing"}, {Model: "gpt-other", Effort: "invented"}} {
		if err := s.SetSelection(ctx5(t), bad); err == nil {
			t.Fatalf("accepted %+v", bad)
		}
		if s.Selection() != before {
			t.Fatal("failed validation changed settings")
		}
	}
	if err := s.Send(ctx5(t), "slow"); err != nil {
		t.Fatal(err)
	}
	if err := s.SetSelection(ctx5(t), Selection{Effort: "xhigh"}); !errors.Is(err, ErrBusy) {
		t.Fatalf("busy update: %v", err)
	}
	if err := s.Interrupt(ctx5(t)); err != nil {
		t.Fatal(err)
	}
	until(t, s, KindSettled)
}

func TestSelectionUnsupportedAndNativeDefaults(t *testing.T) {
	for _, h := range []Harness{Claude, Grok, Pi} {
		if _, err := Open(ctx5(t), h, t.TempDir(), "", Options{Selection: Selection{Model: "x"}}); !errors.Is(err, ErrUnsupported) {
			t.Fatalf("%s: %v", h, err)
		}
	}
	opts := Options{Bin: os.Args[0], Env: append(fakeEnv("codex"), "DRIVER_FAKE_NO_SETTINGS=1"), StopTimeout: time.Second}
	s, err := Open(ctx5(t), Codex, t.TempDir(), "", opts)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err := s.SetSelection(ctx5(t), Selection{Effort: "xhigh"}); err == nil {
		t.Fatal("old native protocol silently accepted settings")
	}
	if s.Selection().Requested != (Selection{}) {
		t.Fatal("failed update changed requested selection")
	}
}

func TestCodexSelectionAtOpenAndResume(t *testing.T) {
	opts := Options{Bin: os.Args[0], Env: fakeEnv("codex"), StopTimeout: time.Second,
		Selection: Selection{Model: "gpt-other", Effort: "xhigh"}, Defaults: Selection{Model: "gpt-fake", Effort: "low"}}
	for _, id := range []string{"", "saved-thread"} {
		s, err := Open(ctx5(t), Codex, t.TempDir(), id, opts)
		if err != nil {
			t.Fatal(err)
		}
		if s.Selection().Requested != opts.Selection || s.Selection().Defaults != opts.Defaults {
			t.Fatalf("settings lost: %+v", s.Selection())
		}
		if id != "" && s.SessionID() != id {
			t.Fatal("resume lost native ID")
		}
		if err := s.Send(ctx5(t), "selection"); err != nil {
			t.Fatal(err)
		}
		if got := joinText(until(t, s, KindSettled)); got != "gpt-other/xhigh" {
			t.Fatalf("open did not apply settings: %q", got)
		}
		s.Close()
	}
}

func TestSelectionUsesNativeConfirmation(t *testing.T) {
	s, err := Open(ctx5(t), Codex, t.TempDir(), "", Options{Bin: os.Args[0], Env: append(fakeEnv("codex"), "DRIVER_FAKE_EFFECTIVE_EFFORT=low"), StopTimeout: time.Second})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err := s.SetSelection(ctx5(t), Selection{Model: "gpt-other", Effort: "xhigh"}); err != nil {
		t.Fatal(err)
	}
	st := s.Selection()
	if st.Requested.Effort != "xhigh" || st.Effective.Effort != "low" {
		t.Fatalf("request substituted for native confirmation: %+v", st)
	}
}
