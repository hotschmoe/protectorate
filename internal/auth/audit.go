package auth

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"
)

// Entry is one audit line. It names who did what to which subject and how it
// ended. Secrets and request bodies are never recorded.
type Entry struct {
	Time      time.Time `json:"time"`
	Actor     string    `json:"actor"`
	TokenID   string    `json:"token_id,omitempty"`
	Tailnet   string    `json:"tailnet,omitempty"`
	Procedure string    `json:"procedure"`
	Subject   string    `json:"subject,omitempty"`
	Outcome   string    `json:"outcome"`
	Code      string    `json:"code,omitempty"`
	// Task is the task of the admin grant that made the call.
	Task string `json:"task,omitempty"`
	// Detail is what the handler added, such as a job's node and mode.
	Detail string `json:"detail,omitempty"`
}

const (
	OutcomeOK     = "ok"
	OutcomeDenied = "denied"
	OutcomeError  = "error"
)

// Audit appends JSON lines to $PROTECTORATE_HOME/audit/audit-YYYY-MM.jsonl,
// opened O_APPEND with mode 0600. The file is append-only by convention of this
// process, not by enforcement: anything running as the same OS user can edit
// it, so true tamper resistance needs shipping the log to another host. Sleeves
// see only their workspace mounts and cannot reach it. A nil *Audit discards.
type Audit struct {
	dir     string
	OnError func(error)

	mu    sync.Mutex
	f     *os.File
	month string
}

func NewAudit(dir string) *Audit { return &Audit{dir: dir} }

func auditFile(dir string, t time.Time) string {
	return filepath.Join(dir, "audit-"+t.UTC().Format("2006-01")+".jsonl")
}

// Write appends e. Failures go to OnError; the caller's operation is not undone.
func (a *Audit) Write(e Entry) {
	if a == nil {
		return
	}
	if e.Time.IsZero() {
		e.Time = time.Now()
	}
	e.Time = e.Time.UTC()
	if err := a.append(e); err != nil && a.OnError != nil {
		a.OnError(fmt.Errorf("audit log: %w", err))
	}
}

// MarshalEntry encodes e as one audit line without the newline.
func MarshalEntry(e Entry) ([]byte, error) { return json.Marshal(e) }

func (a *Audit) append(e Entry) error {
	line, err := MarshalEntry(e)
	if err != nil {
		return err
	}
	line = append(line, '\n')
	a.mu.Lock()
	defer a.mu.Unlock()
	month := e.Time.Format("2006-01")
	if a.f == nil || a.month != month {
		if a.f != nil {
			a.f.Close()
			a.f = nil
		}
		if err := os.MkdirAll(a.dir, 0o700); err != nil {
			return err
		}
		f, err := os.OpenFile(auditFile(a.dir, e.Time), os.O_WRONLY|os.O_APPEND|os.O_CREATE, 0o600)
		if err != nil {
			return err
		}
		a.f, a.month = f, month
	}
	_, err = a.f.Write(line)
	return err
}

// Close releases the open log file.
func (a *Audit) Close() error {
	if a == nil {
		return nil
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.f == nil {
		return nil
	}
	err := a.f.Close()
	a.f = nil
	return err
}

// Tail returns the last n entries under dir, oldest first.
func Tail(dir string, n int) ([]Entry, error) {
	files, err := filepath.Glob(filepath.Join(dir, "audit-*.jsonl"))
	if err != nil {
		return nil, err
	}
	sort.Strings(files)
	if n <= 0 {
		return nil, nil
	}
	var out []Entry
	for i := len(files) - 1; i >= 0 && len(out) < n; i-- {
		es, err := readEntries(files[i], n)
		if err != nil {
			return nil, err
		}
		out = append(es, out...)
	}
	if len(out) > n {
		out = out[len(out)-n:]
	}
	return out, nil
}

// readEntries returns the last n entries of path, holding at most 2n at a time.
func readEntries(path string, n int) ([]Entry, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	var out []Entry
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64<<10), 1<<20)
	for sc.Scan() {
		var e Entry
		if json.Unmarshal(sc.Bytes(), &e) != nil {
			continue
		}
		out = append(out, e)
		if len(out) >= 2*n {
			out = append(out[:0], out[len(out)-n:]...)
		}
	}
	if len(out) > n {
		out = append(out[:0], out[len(out)-n:]...)
	}
	return out, sc.Err()
}
