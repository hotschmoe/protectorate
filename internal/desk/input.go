package desk

import (
	"sync/atomic"
	"time"
	"unicode/utf16"
)

const (
	inputKeyboard = 1

	keyExtended = 0x0001
	keyUp       = 0x0002
	keyUnicode  = 0x0004
)

// keyInput is the keyboard INPUT of SendInput on the 64-bit targets.
type keyInput struct {
	typ   uint32
	_     uint32
	vk    uint16
	scan  uint16
	flags uint32
	time  uint32
	extra uintptr
	_     [8]byte
}

func vkEvent(k Key, up bool) keyInput {
	e := keyInput{typ: inputKeyboard, vk: k.VK}
	if k.Extended {
		e.flags |= keyExtended
	}
	if up {
		e.flags |= keyUp
	}
	return e
}

// typeGroups turns text into key presses, each a down and an up event sent in
// its own call. A rune outside the BMP is two presses, one per UTF-16 unit.
// Newline is Enter, tab is Tab, carriage return is dropped.
func typeGroups(text string) [][]keyInput {
	var out [][]keyInput
	for _, r := range text {
		switch r {
		case '\r':
		case '\n':
			k := Key{VK: 0x0D}
			out = append(out, []keyInput{vkEvent(k, false), vkEvent(k, true)})
		case '\t':
			k := Key{VK: 0x09}
			out = append(out, []keyInput{vkEvent(k, false), vkEvent(k, true)})
		default:
			for _, u := range utf16.Encode([]rune{r}) {
				out = append(out, []keyInput{
					{typ: inputKeyboard, scan: u, flags: keyUnicode},
					{typ: inputKeyboard, scan: u, flags: keyUnicode | keyUp},
				})
			}
		}
	}
	return out
}

// Flags of the low-level keyboard and mouse hook structures. Input that
// SendInput (or any other program) injected carries them; only input that did
// not is the user at the machine.
const (
	llkhfInjected        = 0x10
	llkhfLowerILInjected = 0x02
	llmhfInjected        = 0x01
	llmhfLowerILInjected = 0x02
	keyboardInjectedMask = llkhfInjected | llkhfLowerILInjected
	mouseInjectedMask    = llmhfInjected | llmhfLowerILInjected
)

func keyInjected(flags uint32) bool   { return flags&keyboardInjectedMask != 0 }
func mouseInjected(flags uint32) bool { return flags&mouseInjectedMask != 0 }

// Tracker keeps the time of the last physical input. Hook callbacks call
// Event; any goroutine may call Last.
type Tracker struct {
	now  func() time.Time
	last atomic.Int64
}

// NewTracker starts the clock at the current time: until input is seen, the
// user may have left a moment before the monitor started.
func NewTracker(now func() time.Time) *Tracker {
	t := &Tracker{now: now}
	t.last.Store(now().UnixNano())
	return t
}

// Event records an input event; injected events are ignored.
func (t *Tracker) Event(injected bool) {
	if !injected {
		t.last.Store(t.now().UnixNano())
	}
}

// Last is the time of the last physical input, or of the tracker's start.
func (t *Tracker) Last() time.Time { return time.Unix(0, t.last.Load()) }
