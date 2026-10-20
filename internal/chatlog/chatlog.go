// Package chatlog is the append-only event log behind ChatService: one JSONL
// file per sleeve, seq from 1, with assistant text coalesced into whole
// messages once they complete and a Watch that replays the file and then
// follows live appends without a gap or a duplicate.
//
// A text message reserves its seq at its first delta. Live watchers receive
// the deltas, all carrying that seq; the stored event is the whole message
// under the same seq. A watcher that attaches while a message is still open
// first receives the text so far as one event, then the following deltas.
package chatlog

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

var (
	ErrClosed       = errors.New("chat log closed")
	ErrSlowConsumer = errors.New("watcher fell too far behind; resume from the last seq")
)

// maxQueue bounds the live events held for one watcher.
const maxQueue = 4096

type openText struct {
	seq      uint64
	id       string
	auto     bool
	thinking bool
	at       time.Time
	text     strings.Builder
}

type watcher struct {
	queue    []*pb.ChatEvent
	notify   chan struct{}
	overflow bool
	closed   bool
}

type Log struct {
	path string

	mu       sync.Mutex
	f        *os.File
	offsets  []int64
	size     int64
	last     uint64
	open     *openText
	watchers map[*watcher]struct{}
	closed   bool
}

// Open opens or creates the log at path and recovers its state. A partial
// last line, left by a crash mid-write, is cut off.
func Open(path string) (*Log, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, fmt.Errorf("create chat log dir: %w", err)
	}
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE|os.O_APPEND, 0o600)
	if err != nil {
		return nil, fmt.Errorf("open chat log: %w", err)
	}
	l := &Log{path: path, f: f, watchers: map[*watcher]struct{}{}}
	if err := l.recover(); err != nil {
		f.Close()
		return nil, err
	}
	return l, nil
}

func (l *Log) recover() error {
	r := bufio.NewReaderSize(io.NewSectionReader(l.f, 0, 1<<62), 1<<20)
	var off int64
	for {
		line, err := r.ReadBytes('\n')
		if errors.Is(err, io.EOF) {
			if len(line) > 0 {
				if err := l.f.Truncate(off); err != nil {
					return fmt.Errorf("cut partial chat log line: %w", err)
				}
			}
			break
		}
		if err != nil {
			return fmt.Errorf("read chat log: %w", err)
		}
		var ev pb.ChatEvent
		if err := (protojson.UnmarshalOptions{DiscardUnknown: true}).Unmarshal(line, &ev); err != nil {
			return fmt.Errorf("chat log %s: line %d: %w", l.path, len(l.offsets)+1, err)
		}
		if ev.Seq != uint64(len(l.offsets))+1 {
			return fmt.Errorf("chat log %s: line %d has seq %d", l.path, len(l.offsets)+1, ev.Seq)
		}
		l.offsets = append(l.offsets, off)
		off += int64(len(line))
	}
	l.size, l.last = off, uint64(len(l.offsets))
	return nil
}

// Last is the highest seq allocated, including an open text message.
func (l *Log) Last() uint64 {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.last
}

func (l *Log) persistLocked(ev *pb.ChatEvent) error {
	b, err := protojson.Marshal(ev)
	if err != nil {
		return fmt.Errorf("encode chat event: %w", err)
	}
	b = append(b, '\n')
	if _, err := l.f.Write(b); err != nil {
		return fmt.Errorf("write chat log: %w", err)
	}
	l.offsets = append(l.offsets, l.size)
	l.size += int64(len(b))
	return nil
}

func (l *Log) flushLocked() error {
	o := l.open
	if o == nil {
		return nil
	}
	l.open = nil
	return l.persistLocked(wholeText(o))
}

func wholeText(o *openText) *pb.ChatEvent {
	return &pb.ChatEvent{Seq: o.seq, Time: timestamppb.New(o.at), Body: &pb.ChatEvent_Text{Text: &pb.AssistantText{
		MessageId: o.id, Delta: o.text.String(), Thinking: o.thinking,
	}}}
}

func (l *Log) fanoutLocked(ev *pb.ChatEvent) {
	for w := range l.watchers {
		if len(w.queue) >= maxQueue {
			w.overflow = true
		} else {
			w.queue = append(w.queue, ev)
		}
		select {
		case w.notify <- struct{}{}:
		default:
		}
	}
}

// Append stores one non-text event, closing any open text message first, and
// returns its seq. A zero Time is set to now.
func (l *Log) Append(ev *pb.ChatEvent) (uint64, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.closed {
		return 0, ErrClosed
	}
	if err := l.flushLocked(); err != nil {
		return 0, err
	}
	ev.Seq = l.last + 1
	if ev.Time == nil {
		ev.Time = timestamppb.Now()
	}
	if r := ev.GetReport(); r != nil {
		r.ChatSeq, r.Time = ev.Seq, ev.Time
	}
	if err := l.persistLocked(ev); err != nil {
		return 0, err
	}
	l.last = ev.Seq
	l.fanoutLocked(ev)
	return ev.Seq, nil
}

// AppendText adds a delta to the open text message, or starts a new one when
// the message id or the thinking flag changes. An empty id continues the open
// message, and names a new one after its seq.
func (l *Log) AppendText(id string, thinking bool, delta string) error {
	if delta == "" {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.closed {
		return ErrClosed
	}
	if o := l.open; o != nil && (o.thinking != thinking || (id != "" && (o.auto || o.id != id))) {
		if err := l.flushLocked(); err != nil {
			return err
		}
	}
	if l.open == nil {
		l.last++
		o := &openText{seq: l.last, id: id, thinking: thinking, at: time.Now()}
		if id == "" {
			o.id, o.auto = "m"+strconv.FormatUint(o.seq, 10), true
		}
		l.open = o
	}
	l.open.text.WriteString(delta)
	l.fanoutLocked(&pb.ChatEvent{Seq: l.open.seq, Time: timestamppb.Now(), Body: &pb.ChatEvent_Text{Text: &pb.AssistantText{
		MessageId: l.open.id, Delta: delta, Thinking: thinking,
	}}})
	return nil
}

// Close stores any open message, ends every watch with ErrClosed and closes
// the file. The file stays on disk.
func (l *Log) Close() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.closed {
		return nil
	}
	err := l.flushLocked()
	l.closed = true
	for w := range l.watchers {
		w.closed = true
		select {
		case w.notify <- struct{}{}:
		default:
		}
	}
	if cerr := l.f.Close(); err == nil {
		err = cerr
	}
	return err
}

// Watch calls send for every stored event with seq > after, then for each
// event as it is appended, until ctx ends (nil), the log closes (ErrClosed),
// send fails, or the watcher falls maxQueue events behind (ErrSlowConsumer).
//
// A client that last saw a text delta resumes with that delta's seq minus
// one, so the replay re-sends the whole message.
func (l *Log) Watch(ctx context.Context, after uint64, send func(*pb.ChatEvent) error) error {
	w := &watcher{notify: make(chan struct{}, 1)}
	l.mu.Lock()
	if l.closed {
		l.mu.Unlock()
		return ErrClosed
	}
	boundary := l.last
	var partial *pb.ChatEvent
	if l.open != nil {
		boundary = l.open.seq - 1
		partial = wholeText(l.open)
	}
	var start int64 = -1
	if after < boundary {
		start = l.offsets[after]
	}
	l.watchers[w] = struct{}{}
	l.mu.Unlock()
	defer func() {
		l.mu.Lock()
		delete(l.watchers, w)
		l.mu.Unlock()
	}()

	if start >= 0 {
		if err := l.replay(ctx, start, boundary, send); err != nil {
			return err
		}
	}
	if partial != nil && partial.Seq > after {
		if err := send(partial); err != nil {
			return err
		}
	}
	for {
		l.mu.Lock()
		batch, overflow, closed := w.queue, w.overflow, w.closed
		w.queue = nil
		l.mu.Unlock()
		for _, ev := range batch {
			if ev.Seq <= after {
				continue
			}
			if err := send(ev); err != nil {
				return err
			}
		}
		switch {
		case overflow:
			return ErrSlowConsumer
		case closed && len(batch) == 0:
			return ErrClosed
		case len(batch) > 0:
			continue
		}
		select {
		case <-ctx.Done():
			return nil
		case <-w.notify:
		}
	}
}

func (l *Log) replay(ctx context.Context, start int64, boundary uint64, send func(*pb.ChatEvent) error) error {
	f, err := os.Open(l.path)
	if err != nil {
		return fmt.Errorf("open chat log: %w", err)
	}
	defer f.Close()
	r := bufio.NewReaderSize(io.NewSectionReader(f, start, 1<<62), 256<<10)
	for {
		if ctx.Err() != nil {
			return nil
		}
		line, err := r.ReadBytes('\n')
		if err != nil {
			return fmt.Errorf("read chat log: %w", err)
		}
		ev := new(pb.ChatEvent)
		if err := (protojson.UnmarshalOptions{DiscardUnknown: true}).Unmarshal(line, ev); err != nil {
			return fmt.Errorf("decode chat log: %w", err)
		}
		if err := send(ev); err != nil {
			return err
		}
		if ev.Seq >= boundary {
			return nil
		}
	}
}
