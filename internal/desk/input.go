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

const (
	vkShift  = 0x10
	vkReturn = 0x0D
	vkTab    = 0x09
)

// layout maps characters to keys of one keyboard layout. key reports the
// virtual key that produces r with at most Shift held; scan gives a key's
// hardware scan code.
type layout struct {
	key  func(r rune) (vk uint16, shift, ok bool)
	scan func(vk uint16) uint16
}

func (l layout) press(vk uint16, up bool) keyInput {
	e := keyInput{typ: inputKeyboard, vk: vk, scan: l.scan(vk)}
	if up {
		e.flags |= keyUp
	}
	return e
}

// typeGroups plans the key events for text, one group per character, each sent
// in its own call. Characters the layout produces are typed with real virtual
// keys and Shift, as a keyboard would: applications such as Windows 11 Notepad
// mangle runs of KEYEVENTF_UNICODE packets. Only characters the layout cannot
// produce fall back to Unicode packets (a rune outside the BMP is two UTF-16
// units). Newline is Enter, tab is Tab, carriage return is dropped.
func typeGroups(text string, l layout) [][]keyInput {
	var out [][]keyInput
	for _, r := range text {
		switch r {
		case '\r':
			continue
		case '\n':
			out = append(out, []keyInput{l.press(vkReturn, false), l.press(vkReturn, true)})
			continue
		case '\t':
			out = append(out, []keyInput{l.press(vkTab, false), l.press(vkTab, true)})
			continue
		}
		if vk, shift, ok := l.key(r); ok {
			var g []keyInput
			if shift {
				g = append(g, l.press(vkShift, false))
			}
			g = append(g, l.press(vk, false), l.press(vk, true))
			if shift {
				g = append(g, l.press(vkShift, true))
			}
			out = append(out, g)
			continue
		}
		for _, u := range utf16.Encode([]rune{r}) {
			out = append(out, []keyInput{
				{typ: inputKeyboard, scan: u, flags: keyUnicode},
				{typ: inputKeyboard, scan: u, flags: keyUnicode | keyUp},
			})
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
