package chatlog

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func tool(id string) *pb.ChatEvent {
	return &pb.ChatEvent{Body: &pb.ChatEvent_ToolCall{ToolCall: &pb.ToolCall{Id: id, Name: "Bash"}}}
}

func openLog(t *testing.T) *Log {
	t.Helper()
	l, err := Open(filepath.Join(t.TempDir(), "sub", "chat.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { l.Close() })
	return l
}

// collect watches until n distinct events were seen or the timeout hits.
type collector struct {
	mu   sync.Mutex
	evs  []*pb.ChatEvent
	done chan error
}

func watch(ctx context.Context, l *Log, after uint64) *collector {
	c := &collector{done: make(chan error, 1)}
	go func() {
		c.done <- l.Watch(ctx, after, func(ev *pb.ChatEvent) error {
			c.mu.Lock()
			c.evs = append(c.evs, ev)
			c.mu.Unlock()
			return nil
		})
	}()
	return c
}

func (c *collector) snapshot() []*pb.ChatEvent {
	c.mu.Lock()
	defer c.mu.Unlock()
	return append([]*pb.ChatEvent(nil), c.evs...)
}

// merge folds deltas that share a seq into one string per seq, as a client does.
func merge(evs []*pb.ChatEvent) (seqs []uint64, text map[uint64]string) {
	text = map[uint64]string{}
	for _, e := range evs {
		if len(seqs) == 0 || seqs[len(seqs)-1] != e.Seq {
			seqs = append(seqs, e.Seq)
		}
		if t := e.GetText(); t != nil {
			text[e.Seq] += t.Delta
		}
	}
	return
}

func TestSeqAndCoalescing(t *testing.T) {
	l := openLog(t)
	if s, _ := l.Append(tool("a")); s != 1 {
		t.Fatalf("first seq %d", s)
	}
	for _, d := range []string{"hel", "lo ", "world"} {
		if err := l.AppendText("msg1", false, d); err != nil {
			t.Fatal(err)
		}
	}
	if err := l.AppendText("", true, "thinking"); err != nil {
		t.Fatal(err)
	}
	if err := l.AppendText("", true, " more"); err != nil {
		t.Fatal(err)
	}
	if s, _ := l.Append(tool("b")); s != 4 {
		t.Fatalf("seq after two messages = %d", s)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	c := watch(ctx, l, 0)
	waitFor(t, func() bool { return len(c.snapshot()) == 4 })
	evs := c.snapshot()
	var got []string
	for _, e := range evs {
		switch b := e.Body.(type) {
		case *pb.ChatEvent_Text:
			got = append(got, fmt.Sprintf("%d:%s:%q:%v", e.Seq, b.Text.MessageId, b.Text.Delta, b.Text.Thinking))
		default:
			got = append(got, fmt.Sprint(e.Seq))
		}
	}
	want := []string{"1", `2:msg1:"hello world":false`, `3:m3:"thinking more":true`, "4"}
	if l.Last() != 4 {
		t.Fatalf("last = %d", l.Last())
	}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("replay\n got %v\nwant %v", got, want)
	}
}

func TestSeqIsContiguous(t *testing.T) {
	l := openLog(t)
	l.AppendText("a", false, "x")
	l.Append(tool("1"))
	l.AppendText("b", false, "y")
	l.AppendText("c", false, "z")
	l.Append(tool("2"))
	if l.Last() != 5 {
		t.Fatalf("last = %d, want 5", l.Last())
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	c := watch(ctx, l, 2)
	waitFor(t, func() bool { return len(c.snapshot()) == 3 })
	seqs, _ := merge(c.snapshot())
	if fmt.Sprint(seqs) != "[3 4 5]" {
		t.Fatalf("seqs after 2: %v", seqs)
	}
}

func TestReopenKeepsEventsAndCutsPartialLine(t *testing.T) {
	path := filepath.Join(t.TempDir(), "chat.jsonl")
	l, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	l.Append(tool("a"))
	l.AppendText("m", false, "open at close")
	if err := l.Close(); err != nil {
		t.Fatal(err)
	}
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString(`{"seq":"3","time":"2026-`)
	f.Close()

	l2, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer l2.Close()
	if l2.Last() != 2 {
		t.Fatalf("last after reopen = %d", l2.Last())
	}
	if s, _ := l2.Append(tool("b")); s != 3 {
		t.Fatalf("next seq = %d", s)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	c := watch(ctx, l2, 0)
	waitFor(t, func() bool { return len(c.snapshot()) == 3 })
	if _, text := merge(c.snapshot()); text[2] != "open at close" {
		t.Fatalf("message lost across reopen: %v", text)
	}
}

func TestCorruptLogRefused(t *testing.T) {
	path := filepath.Join(t.TempDir(), "chat.jsonl")
	if err := os.WriteFile(path, []byte("{\"seq\":\"2\"}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Open(path); err == nil {
		t.Fatal("log starting at seq 2 accepted")
	}
}

func TestWatchWhileMessageOpen(t *testing.T) {
	l := openLog(t)
	l.Append(tool("a"))
	l.AppendText("m", false, "one ")
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	c := watch(ctx, l, 0)
	waitFor(t, func() bool { return len(c.snapshot()) == 2 })
	l.AppendText("m", false, "two")
	l.Append(tool("b"))
	waitFor(t, func() bool { return len(c.snapshot()) == 4 })
	seqs, text := merge(c.snapshot())
	if fmt.Sprint(seqs) != "[1 2 3]" || text[2] != "one two" {
		t.Fatalf("seqs %v text %v", seqs, text)
	}

	// a client that already holds seq 2 gets no partial tail of it
	l.AppendText("n", false, "p")
	c2 := watch(ctx, l, 4)
	l.AppendText("n", false, "q")
	l.Append(tool("c"))
	waitFor(t, func() bool { return len(c2.snapshot()) >= 1 })
	for _, e := range c2.snapshot() {
		if e.Seq <= 4 {
			t.Fatalf("event %d at or before after_seq", e.Seq)
		}
	}
}

func TestWatchEndsOnCloseAndContext(t *testing.T) {
	l := openLog(t)
	ctx, cancel := context.WithCancel(context.Background())
	c := watch(ctx, l, 0)
	cancel()
	if err := <-c.done; err != nil {
		t.Fatalf("canceled watch: %v", err)
	}
	c = watch(context.Background(), l, 0)
	waitFor(t, func() bool {
		l.mu.Lock()
		defer l.mu.Unlock()
		return len(l.watchers) == 1
	})
	l.Append(tool("a"))
	l.AppendText("m", false, "tail")
	l.Close()
	if err := <-c.done; !errors.Is(err, ErrClosed) {
		t.Fatalf("watch on close: %v", err)
	}
	if _, text := merge(c.snapshot()); text[2] != "tail" {
		t.Fatalf("open message not delivered before close: %v", text)
	}
	if err := l.Watch(context.Background(), 0, nil); !errors.Is(err, ErrClosed) {
		t.Fatalf("watch on closed log: %v", err)
	}
	if _, err := l.Append(tool("x")); !errors.Is(err, ErrClosed) {
		t.Fatalf("append on closed log: %v", err)
	}
}

func TestSlowWatcherIsCut(t *testing.T) {
	l := openLog(t)
	block := make(chan struct{})
	started := make(chan struct{})
	done := make(chan error, 1)
	go func() {
		first := true
		done <- l.Watch(context.Background(), 0, func(*pb.ChatEvent) error {
			if first {
				first = false
				close(started)
				<-block
			}
			return nil
		})
	}()
	l.Append(tool("a"))
	<-started
	for i := 0; i < maxQueue+10; i++ {
		l.Append(tool("x"))
	}
	close(block)
	if err := <-done; !errors.Is(err, ErrSlowConsumer) {
		t.Fatalf("slow watcher: %v", err)
	}
}

// TestNoGapNoDuplicateAcrossReplayLiveBoundary has watchers join at random
// points while a writer appends mixed events, and checks every watcher sees
// each seq exactly once, in order, with intact message text.
func TestNoGapNoDuplicateAcrossReplayLiveBoundary(t *testing.T) {
	l := openLog(t)
	const rounds = 400
	want := map[uint64]string{}
	var wmu sync.Mutex

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var watchers []*collector
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		for i := 0; i < rounds; i++ {
			switch i % 3 {
			case 0:
				l.Append(tool(fmt.Sprint(i)))
			default:
				id := fmt.Sprintf("msg%d", i/3)
				var all strings.Builder
				for j := 0; j < 3; j++ {
					d := fmt.Sprintf("<%d.%d>", i, j)
					all.WriteString(d)
					l.AppendText(id, false, d)
				}
				wmu.Lock()
				want[l.Last()] += all.String()
				wmu.Unlock()
			}
			if i%25 == 0 {
				watchers = append(watchers, watch(ctx, l, 0))
				time.Sleep(time.Millisecond)
			}
		}
		l.Append(tool("end"))
	}()
	wg.Wait()
	final := l.Last()
	for i, c := range watchers {
		c := c
		waitFor(t, func() bool {
			evs := c.snapshot()
			return len(evs) > 0 && evs[len(evs)-1].Seq == final
		})
		seqs, text := merge(c.snapshot())
		for j, s := range seqs {
			if s != uint64(j)+1 {
				t.Fatalf("watcher %d: position %d has seq %d (gap or duplicate)", i, j, s)
			}
		}
		wmu.Lock()
		for s, w := range want {
			if text[s] != w {
				t.Fatalf("watcher %d: seq %d text %q, want %q", i, s, text[s], w)
			}
		}
		wmu.Unlock()
	}
}

func waitFor(t *testing.T, f func() bool) {
	t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	for !f() {
		if time.Now().After(deadline) {
			t.Fatal("timed out")
		}
		time.Sleep(2 * time.Millisecond)
	}
}
