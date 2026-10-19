package desk

import (
	"fmt"
	"strings"
	"unicode"
)

// Key is a Windows virtual-key code plus whether it needs the extended-key
// flag.
type Key struct {
	VK       uint16
	Extended bool
}

// Chord is modifiers held while Key is pressed.
type Chord struct {
	Modifiers []Key
	Key       Key
}

var modifierKeys = map[string]Key{
	"ctrl": {VK: 0x11}, "control": {VK: 0x11},
	"alt": {VK: 0x12}, "shift": {VK: 0x10},
	"win": {VK: 0x5B, Extended: true}, "super": {VK: 0x5B, Extended: true}, "meta": {VK: 0x5B, Extended: true},
}

var namedKeys = map[string]Key{
	"enter": {VK: 0x0D}, "return": {VK: 0x0D}, "tab": {VK: 0x09}, "esc": {VK: 0x1B}, "escape": {VK: 0x1B},
	"space": {VK: 0x20}, "backspace": {VK: 0x08},
	"delete": {VK: 0x2E, Extended: true}, "del": {VK: 0x2E, Extended: true},
	"insert": {VK: 0x2D, Extended: true}, "ins": {VK: 0x2D, Extended: true},
	"home": {VK: 0x24, Extended: true}, "end": {VK: 0x23, Extended: true},
	"pageup": {VK: 0x21, Extended: true}, "pgup": {VK: 0x21, Extended: true},
	"pagedown": {VK: 0x22, Extended: true}, "pgdn": {VK: 0x22, Extended: true},
	"left": {VK: 0x25, Extended: true}, "up": {VK: 0x26, Extended: true},
	"right": {VK: 0x27, Extended: true}, "down": {VK: 0x28, Extended: true},
	"printscreen": {VK: 0x2C, Extended: true}, "capslock": {VK: 0x14},
	"-": {VK: 0xBD}, "=": {VK: 0xBB}, ",": {VK: 0xBC}, ".": {VK: 0xBE}, "/": {VK: 0xBF},
	";": {VK: 0xBA}, "'": {VK: 0xDE}, "[": {VK: 0xDB}, "]": {VK: 0xDD}, "\\": {VK: 0xDC}, "`": {VK: 0xC0},
}

// ParseChord parses "ctrl+s", "alt+f4", "ctrl+shift+esc" or "enter": zero or
// more modifiers, then exactly one key (a letter, digit, F1 to F24 or a name).
// A literal plus key is written "plus".
func ParseChord(s string) (Chord, error) {
	parts := strings.Split(strings.ToLower(strings.TrimSpace(s)), "+")
	var c Chord
	for i, raw := range parts {
		part := strings.TrimSpace(raw)
		if part == "" {
			return Chord{}, fmt.Errorf("%w: empty element in key chord %q", ErrInvalid, s)
		}
		last := i == len(parts)-1
		if m, ok := modifierKeys[part]; ok && !last {
			c.Modifiers = append(c.Modifiers, m)
			continue
		}
		if !last {
			return Chord{}, fmt.Errorf("%w: %q in key chord %q is not a modifier (ctrl, alt, shift, win)", ErrInvalid, part, s)
		}
		k, err := parseKey(part)
		if err != nil {
			return Chord{}, fmt.Errorf("key chord %q: %w", s, err)
		}
		c.Key = k
	}
	return c, nil
}

func parseKey(s string) (Key, error) {
	if s == "plus" {
		return Key{VK: 0xBB}, nil
	}
	if k, ok := namedKeys[s]; ok {
		return k, nil
	}
	if len(s) >= 2 && s[0] == 'f' {
		n := 0
		for _, r := range s[1:] {
			if !unicode.IsDigit(r) {
				n = 0
				break
			}
			n = n*10 + int(r-'0')
		}
		if n >= 1 && n <= 24 {
			return Key{VK: uint16(0x70 + n - 1)}, nil
		}
	}
	if len(s) == 1 {
		switch r := rune(s[0]); {
		case r >= 'a' && r <= 'z':
			return Key{VK: uint16(unicode.ToUpper(r))}, nil
		case r >= '0' && r <= '9':
			return Key{VK: uint16(r)}, nil
		}
	}
	return Key{}, fmt.Errorf("%w: unknown key %q", ErrInvalid, s)
}
