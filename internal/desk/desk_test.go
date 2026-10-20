package desk

import (
	"bytes"
	"errors"
	"image"
	"image/color"
	"image/png"
	"strings"
	"testing"
)

func TestParseChord(t *testing.T) {
	tests := []struct {
		in      string
		mods    int
		vk      uint16
		ext     bool
		wantErr bool
	}{
		{"enter", 0, 0x0D, false, false},
		{"ctrl+s", 1, 'S', false, false},
		{"Ctrl+Shift+Esc", 2, 0x1B, false, false},
		{"alt+f4", 1, 0x73, false, false},
		{"f12", 0, 0x7B, false, false},
		{"win+left", 1, 0x25, true, false},
		{"ctrl+plus", 1, 0xBB, false, false},
		{"ctrl+5", 1, '5', false, false},
		{"", 0, 0, false, true},
		{"ctrl+", 0, 0, false, true},
		{"s+ctrl", 0, 0, false, true},
		{"ctrl+nonsense", 0, 0, false, true},
		{"f25", 0, 0, false, true},
		{"ctrl+s+a", 0, 0, false, true},
	}
	for _, tc := range tests {
		t.Run(tc.in, func(t *testing.T) {
			c, err := ParseChord(tc.in)
			if tc.wantErr {
				if !errors.Is(err, ErrInvalid) {
					t.Fatalf("err = %v, want ErrInvalid", err)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if len(c.Modifiers) != tc.mods || c.Key.VK != tc.vk || c.Key.Extended != tc.ext {
				t.Fatalf("chord = %+v", c)
			}
		})
	}
}

func TestParseButton(t *testing.T) {
	tests := []struct {
		in      string
		want    Button
		wantErr bool
	}{
		{"", Left, false}, {"left", Left, false}, {"RIGHT", Right, false}, {"middle", Middle, false}, {"back", 0, true},
	}
	for _, tc := range tests {
		got, err := ParseButton(tc.in)
		if (err != nil) != tc.wantErr || (err == nil && got != tc.want) {
			t.Errorf("ParseButton(%q) = %v, %v", tc.in, got, err)
		}
	}
}

func TestEncodePNG(t *testing.T) {
	img := image.NewRGBA(image.Rect(0, 0, 3, 2))
	img.SetRGBA(1, 1, color.RGBA{R: 10, G: 20, B: 30, A: 255})
	b, err := EncodePNG(img)
	if err != nil {
		t.Fatal(err)
	}
	got, err := png.Decode(bytes.NewReader(b))
	if err != nil {
		t.Fatal(err)
	}
	if got.Bounds() != img.Bounds() {
		t.Fatalf("bounds = %v", got.Bounds())
	}
	if r, g, bl, _ := got.At(1, 1).RGBA(); r>>8 != 10 || g>>8 != 20 || bl>>8 != 30 {
		t.Fatalf("pixel = %v %v %v", r, g, bl)
	}
}

func TestDownscale(t *testing.T) {
	src := image.NewRGBA(image.Rect(0, 0, 8, 4))
	for y := range 4 {
		for x := range 8 {
			v := uint8(0)
			if x >= 4 {
				v = 200
			}
			src.SetRGBA(x, y, color.RGBA{R: v, G: v / 2, B: 10, A: 255})
		}
	}
	tests := []struct {
		name     string
		maxWidth int
		w, h     int
	}{
		{"off", 0, 8, 4},
		{"no wider", 8, 8, 4},
		{"half", 4, 4, 2},
		{"odd", 3, 3, 2},
		{"one pixel", 1, 1, 1},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := Downscale(src, tt.maxWidth)
			if got.Bounds().Dx() != tt.w || got.Bounds().Dy() != tt.h {
				t.Fatalf("size = %v", got.Bounds())
			}
			b, err := EncodePNG(src)
			if err != nil {
				t.Fatal(err)
			}
			scaled, err := DownscalePNG(b, tt.maxWidth)
			if err != nil {
				t.Fatal(err)
			}
			cfg, err := png.DecodeConfig(bytes.NewReader(scaled))
			if err != nil || cfg.Width != tt.w || cfg.Height != tt.h {
				t.Fatalf("png = %+v %v", cfg, err)
			}
		})
	}
	half := Downscale(src, 4)
	if c := half.RGBAAt(0, 0); c.R != 0 || c.B != 10 {
		t.Fatalf("left = %v", c)
	}
	if c := half.RGBAAt(3, 1); c.R != 200 || c.G != 100 {
		t.Fatalf("right = %v", c)
	}
	if c := Downscale(src, 1).RGBAAt(0, 0); c.R != 100 || c.A != 255 {
		t.Fatalf("average = %v", c)
	}
	if _, err := DownscalePNG([]byte("not a png"), 4); err == nil {
		t.Fatal("garbage accepted")
	}
}

func TestCheckInteractive(t *testing.T) {
	tests := []struct {
		name     string
		s        Session
		probeErr error
		want     string
	}{
		{"console session", Session{ID: 1, WindowStation: "WinSta0"}, nil, ""},
		{"rdp session lowercase", Session{ID: 3, WindowStation: "winsta0"}, nil, ""},
		{"session 0", Session{ID: 0, WindowStation: "Service-0x0-3e7$"}, nil, "session 0"},
		{"user session, service window station", Session{ID: 2, WindowStation: "Service-0x0-3e7$"}, nil, "window station"},
		{"probe failed", Session{}, errors.New("boom"), "cannot inspect"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			err := CheckInteractive(tc.s, tc.probeErr)
			if tc.want == "" {
				if err != nil {
					t.Fatal(err)
				}
				return
			}
			if !errors.Is(err, ErrNotInteractive) || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v", err)
			}
			if !strings.Contains(err.Error(), "protectorate node install") && tc.name != "probe failed" {
				t.Fatalf("error does not name the fix: %v", err)
			}
		})
	}
}

func TestClassifyInputDesktop(t *testing.T) {
	tests := []struct {
		name string
		in   string
		err  error
		want ScreenState
	}{
		{"default desktop", "Default", nil, ScreenUnlocked},
		{"case folded", "default", nil, ScreenUnlocked},
		{"logon desktop", "Winlogon", nil, ScreenLocked},
		{"screen saver", "Screen-saver", nil, ScreenLocked},
		{"access denied", "", ErrInputDesktopDenied, ScreenLocked},
		{"wrapped denied", "", errors.Join(errors.New("x"), ErrInputDesktopDenied), ScreenLocked},
		{"other error", "", errors.New("boom"), ScreenUnknown},
		{"unknown name", "Other", nil, ScreenUnknown},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := ClassifyInputDesktop(tt.in, tt.err); got != tt.want {
				t.Errorf("got %d, want %d", got, tt.want)
			}
		})
	}
}

func TestCheckReach(t *testing.T) {
	tests := []struct {
		name    string
		f       Foreground
		blocked bool
	}{
		{"no window", Foreground{AgentIntegrity: IntegrityMedium}, false},
		{"same level", Foreground{PID: 1, Integrity: IntegrityMedium, AgentIntegrity: IntegrityMedium}, false},
		{"lower level", Foreground{PID: 1, Integrity: IntegrityLow, AgentIntegrity: IntegrityMedium}, false},
		{"higher level", Foreground{PID: 1, Title: "Windows Security", Process: "consent.exe", Integrity: IntegrityHigh, AgentIntegrity: IntegrityMedium}, true},
		{"unreadable", Foreground{PID: 1, Unreadable: true, AgentIntegrity: IntegrityMedium}, true},
		{"elevated agent, high window", Foreground{PID: 1, Integrity: IntegrityHigh, AgentIntegrity: IntegrityHigh}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := CheckReach(tt.f, "foreground window")
			if (err != nil) != tt.blocked || (err != nil && !errors.Is(err, ErrBlocked)) {
				t.Errorf("err = %v, want blocked %v", err, tt.blocked)
			}
			if err != nil && !strings.Contains(err.Error(), "--elevated") {
				t.Errorf("error lacks the fix: %v", err)
			}
		})
	}
}
