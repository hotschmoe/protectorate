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
			if !strings.Contains(err.Error(), "protectorate service windows-task") && tc.name != "probe failed" {
				t.Fatalf("error does not name the fix: %v", err)
			}
		})
	}
}
