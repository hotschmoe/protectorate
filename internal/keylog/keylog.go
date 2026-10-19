// Package keylog persists the request ids a service remembers for a while:
// an append-only JSONL file of entries, each with an expiry, reloaded when the
// service starts. Expired and superseded entries are dropped when the file is
// opened (it is rewritten atomically if that changes anything) and again when
// the file has grown well past the live set.
package keylog

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// Entry is what a service remembers about one request id. Value is what the
// first call produced (a job id, a chat seq); Code and Err describe a
// remembered refusal instead.
type Entry struct {
	Key     string    `json:"key"`
	Value   string    `json:"value,omitempty"`
	Code    uint32    `json:"code,omitempty"`
	Err     string    `json:"err,omitempty"`
	Expires time.Time `json:"expires"`
}

// slack is how many dead lines the file may carry beyond twice the live set.
const slack = 1024

type Log struct {
	path string
	max  int
	now  func() time.Time

	mu    sync.Mutex
	f     *os.File
	live  map[string]Entry
	lines int
}

// Open loads the entries at path that have not expired, newest line per key,
// keeping at most max (the latest to expire win), and returns them. A torn
// last line and lines that do not parse are dropped.
func Open(path string, max int, now func() time.Time) (*Log, []Entry, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, nil, fmt.Errorf("create request id log dir: %w", err)
	}
	l := &Log{path: path, max: max, now: now, live: map[string]Entry{}}
	lines, err := l.load()
	if err != nil {
		return nil, nil, err
	}
	if dropped := lines - len(l.live); dropped > 0 || lines == 0 {
		if err := l.rewriteLocked(); err != nil {
			return nil, nil, err
		}
	} else if l.f, err = os.OpenFile(path, os.O_WRONLY|os.O_APPEND, 0o600); err != nil {
		return nil, nil, fmt.Errorf("open request id log: %w", err)
	}
	l.lines = len(l.live)
	return l, l.entriesLocked(), nil
}

func (l *Log) load() (int, error) {
	f, err := os.Open(l.path)
	if errors.Is(err, os.ErrNotExist) {
		return 0, nil
	}
	if err != nil {
		return 0, fmt.Errorf("read request id log: %w", err)
	}
	defer f.Close()
	r := bufio.NewReaderSize(f, 1<<20)
	now := l.now()
	lines := 0
	for {
		line, err := r.ReadBytes('\n')
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return 0, fmt.Errorf("read request id log: %w", err)
		}
		lines++
		var e Entry
		if json.Unmarshal(line, &e) != nil || e.Key == "" {
			log.Printf("request ids: skip unreadable line %d of %s", lines, l.path)
			continue
		}
		if e.Expires.After(now) {
			l.live[e.Key] = e
		} else {
			delete(l.live, e.Key)
		}
	}
	l.trimLocked()
	return lines, nil
}

// trimLocked keeps the max entries that expire last.
func (l *Log) trimLocked() {
	if len(l.live) <= l.max {
		return
	}
	all := l.entriesLocked()
	sort.Slice(all, func(i, j int) bool { return all[i].Expires.After(all[j].Expires) })
	for _, e := range all[l.max:] {
		delete(l.live, e.Key)
	}
}

func (l *Log) entriesLocked() []Entry {
	out := make([]Entry, 0, len(l.live))
	for _, e := range l.live {
		out = append(out, e)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Key < out[j].Key })
	return out
}

// rewriteLocked replaces the file with the live entries.
func (l *Log) rewriteLocked() error {
	now := l.now()
	for k, e := range l.live {
		if !e.Expires.After(now) {
			delete(l.live, k)
		}
	}
	var buf []byte
	for _, e := range l.entriesLocked() {
		b, err := json.Marshal(e)
		if err != nil {
			return err
		}
		buf = append(append(buf, b...), '\n')
	}
	if err := fsutil.WriteFile(l.path, buf, 0o600); err != nil {
		return fmt.Errorf("rewrite request id log: %w", err)
	}
	if l.f != nil {
		l.f.Close()
	}
	f, err := os.OpenFile(l.path, os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return fmt.Errorf("open request id log: %w", err)
	}
	l.f, l.lines = f, len(l.live)
	return nil
}

// Append makes e durable. Call it once the outcome of a request is known.
func (l *Log) Append(e Entry) error {
	b, err := json.Marshal(e)
	if err != nil {
		return err
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if _, err := l.f.Write(append(b, '\n')); err != nil {
		return fmt.Errorf("write request id log: %w", err)
	}
	if err := l.f.Sync(); err != nil {
		return fmt.Errorf("sync request id log: %w", err)
	}
	l.live[e.Key] = e
	l.lines++
	l.trimLocked()
	if l.lines > 2*len(l.live)+slack {
		return l.rewriteLocked()
	}
	return nil
}

func (l *Log) Close() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.f == nil {
		return nil
	}
	err := l.f.Close()
	l.f = nil
	return err
}
