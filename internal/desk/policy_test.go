package desk

import (
	"errors"
	"strings"
	"testing"
	"time"
)

func TestParseWindow(t *testing.T) {
	tests := []struct {
		in         string
		start, end int
		wantErr    bool
	}{
		{"19:00-07:00", 19 * 60, 7 * 60, false},
		{" 08:30-17:45 ", 8*60 + 30, 17*60 + 45, false},
		{"00:00-23:59", 0, 23*60 + 59, false},
		{"07:00-07:00", 0, 0, true},
		{"7:00-19:00", 7 * 60, 19 * 60, false},
		{"19:00", 0, 0, true},
		{"24:00-07:00", 0, 0, true},
		{"19:60-07:00", 0, 0, true},
		{"x-y", 0, 0, true},
	}
	for _, tc := range tests {
		s, e, err := ParseWindow(tc.in)
		if (err != nil) != tc.wantErr || (err != nil && !errors.Is(err, ErrInvalid)) || s != tc.start || e != tc.end {
			t.Errorf("ParseWindow(%q) = %d %d %v", tc.in, s, e, err)
		}
	}
}

func TestInWindow(t *testing.T) {
	berlin, err := time.LoadLocation("Europe/Berlin")
	if err != nil {
		t.Fatal(err)
	}
	night, _ := NewPolicy("19:00-07:00", time.Minute, "Europe/Berlin", false)
	day, _ := NewPolicy("08:00-17:00", time.Minute, "UTC", false)
	at := func(loc *time.Location, h, m int) time.Time { return time.Date(2026, 10, 20, h, m, 0, 0, loc) }
	tests := []struct {
		name string
		p    *Policy
		t    time.Time
		want bool
	}{
		{"night evening", night, at(berlin, 19, 0), true},
		{"night before midnight", night, at(berlin, 23, 59), true},
		{"night after midnight", night, at(berlin, 3, 0), true},
		{"night end is exclusive", night, at(berlin, 7, 0), false},
		{"night midday", night, at(berlin, 12, 0), false},
		{"night, instant given in UTC", night, at(time.UTC, 17, 30), true},
		{"night, UTC clock reads inside but Berlin is outside", night, at(time.UTC, 5, 30), false},
		{"day inside", day, at(time.UTC, 9, 0), true},
		{"day start inclusive", day, at(time.UTC, 8, 0), true},
		{"day end exclusive", day, at(time.UTC, 17, 0), false},
	}
	for _, tc := range tests {
		if got := tc.p.InWindow(tc.t); got != tc.want {
			t.Errorf("%s: InWindow = %v, want %v", tc.name, got, tc.want)
		}
	}
	if _, err := NewPolicy("19:00-07:00", time.Minute, "Mars/Olympus", false); !errors.Is(err, ErrInvalid) {
		t.Errorf("bad zone: %v", err)
	}
	if _, err := NewPolicy("19:00-07:00", 0, "", false); !errors.Is(err, ErrInvalid) {
		t.Errorf("zero idle: %v", err)
	}
}

func TestEvaluate(t *testing.T) {
	p, err := NewPolicy("19:00-07:00", 15*time.Minute, "UTC", false)
	if err != nil {
		t.Fatal(err)
	}
	at := func(h, m int) time.Time { return time.Date(2026, 10, 20, h, m, 0, 0, time.UTC) }
	tests := []struct {
		name   string
		p      *Policy
		in     Inputs
		avail  bool
		reason Reason
		detail string
	}{
		{"no policy", nil, Inputs{Now: at(12, 0), LastInput: at(12, 0)}, true, Available, "always"},
		{"no policy not interactive", nil, Inputs{Now: at(12, 0), Ready: ErrNotInteractive}, false, NotInteractive, "not an interactive session"},
		{"unsupported os", p, Inputs{Now: at(23, 0), Ready: ErrUnsupported}, false, Unsupported, ""},
		{"away", p, Inputs{Now: at(23, 0), LastInput: at(21, 0), Screen: ScreenUnlocked}, true, Available, "away"},
		{"idle just long enough", p, Inputs{Now: at(23, 0), LastInput: at(22, 45)}, true, Available, ""},
		{"user active", p, Inputs{Now: at(23, 0), LastInput: at(22, 50)}, false, UserActive, "10m0s ago"},
		{"outside window wins over active", p, Inputs{Now: at(12, 0), LastInput: at(11, 59)}, false, OutsideWindow, "19:00-07:00"},
		{"outside window", p, Inputs{Now: at(12, 0), LastInput: at(1, 0)}, false, OutsideWindow, ""},
		{"locked screen", p, Inputs{Now: at(23, 0), LastInput: at(21, 0), Screen: ScreenLocked}, false, Locked, "unlock"},
		{"not interactive with policy", p, Inputs{Now: at(23, 0), LastInput: at(21, 0), Ready: ErrNotInteractive}, false, NotInteractive, ""},
	}
	for _, tc := range tests {
		v := Evaluate(tc.p, tc.in)
		if v.Available != tc.avail || v.Reason != tc.reason || !strings.Contains(v.Detail, tc.detail) {
			t.Errorf("%s: got %+v, want available=%v reason=%v detail~%q", tc.name, v, tc.avail, tc.reason, tc.detail)
		}
	}
}

func TestInjectedFlags(t *testing.T) {
	tests := []struct {
		name  string
		flags uint32
		key   bool
		mouse bool
	}{
		{"physical", 0, false, false},
		{"key injected", 0x10, true, false},
		{"key lower IL injected", 0x02, true, true},
		{"mouse injected", 0x01, false, true},
		{"extended key bit only", 0x01 << 7, false, false},
	}
	for _, tc := range tests {
		if got := keyInjected(tc.flags); got != tc.key {
			t.Errorf("%s: keyInjected = %v", tc.name, got)
		}
		if got := mouseInjected(tc.flags); got != tc.mouse {
			t.Errorf("%s: mouseInjected = %v", tc.name, got)
		}
	}
}

func TestTracker(t *testing.T) {
	now := time.Date(2026, 10, 20, 22, 0, 0, 0, time.UTC)
	tr := NewTracker(func() time.Time { return now })
	steps := []struct {
		name     string
		advance  time.Duration
		injected *bool
		want     time.Time
	}{
		{"starts at creation", 0, nil, now},
		{"injected input is ignored", time.Minute, ptr(true), now},
		{"physical input counts", time.Minute, ptr(false), now.Add(2 * time.Minute)},
		{"later injected input is ignored", time.Hour, ptr(true), now.Add(2 * time.Minute)},
	}
	for _, s := range steps {
		now = now.Add(s.advance)
		if s.injected != nil {
			tr.Event(*s.injected)
		}
		if got := tr.Last(); !got.Equal(s.want) {
			t.Errorf("%s: Last = %v, want %v", s.name, got, s.want)
		}
	}
}

func ptr(b bool) *bool { return &b }
