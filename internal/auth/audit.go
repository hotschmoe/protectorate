package auth

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strconv"
	"strings"
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

// Page reads the log this Audit writes; a nil Audit has no entries.
func (a *Audit) Page(q Query) (Page, error) {
	if a == nil {
		return Page{}, nil
	}
	return ReadPage(a.dir, q)
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

// Query selects audit entries. Text filters match a substring of the field,
// ignoring case; Outcome matches exactly.
type Query struct {
	Limit     int
	Before    string
	Actor     string
	Procedure string
	Subject   string
	Outcome   string
}

func (q Query) match(e Entry) bool {
	has := func(field, want string) bool {
		return want == "" || strings.Contains(strings.ToLower(field), strings.ToLower(want))
	}
	return has(e.Actor, q.Actor) && has(e.Procedure, q.Procedure) && has(e.Subject, q.Subject) && (q.Outcome == "" || e.Outcome == q.Outcome)
}

// Page is a run of entries, newest first. Next continues with older entries
// (Query.Before); it is empty once the start of the log is reached.
type Page struct {
	Entries []Entry
	Next    string
}

// ErrCursor marks a Before cursor this log did not hand out.
var ErrCursor = errors.New("invalid audit cursor")

// pageScanBudget bounds the lines one call reads: a filter that matches
// rarely returns a short page and a cursor instead of reading every month.
const pageScanBudget = 200_000

// ReadPage returns up to q.Limit entries under dir matching q, newest first,
// older than q.Before. A cursor is "<file>:<line>", the entries before that
// line of that monthly file, or "<file>:*" for the whole file.
func ReadPage(dir string, q Query) (Page, error) {
	if q.Limit <= 0 {
		return Page{}, nil
	}
	files, err := filepath.Glob(filepath.Join(dir, "audit-*.jsonl"))
	if err != nil {
		return Page{}, err
	}
	sort.Strings(files)
	i, bound := len(files)-1, -1
	if q.Before != "" {
		name, line, ok := strings.Cut(q.Before, ":")
		i = slices.Index(files, filepath.Join(dir, name))
		if !ok || i < 0 || filepath.Base(name) != name {
			return Page{}, fmt.Errorf("%w: %q", ErrCursor, q.Before)
		}
		if line != "*" {
			if bound, err = strconv.Atoi(line); err != nil || bound < 0 {
				return Page{}, fmt.Errorf("%w: %q", ErrCursor, q.Before)
			}
		}
	}
	var out Page
	scanned := 0
	for ; i >= 0; i-- {
		need := q.Limit - len(out.Entries)
		got, lines, more, err := scanFile(files[i], bound, need, q)
		if err != nil {
			return Page{}, err
		}
		for j := len(got) - 1; j >= 0; j-- {
			out.Entries = append(out.Entries, got[j].e)
		}
		if more {
			out.Next = fmt.Sprintf("%s:%d", filepath.Base(files[i]), got[0].line)
			return out, nil
		}
		scanned += lines
		bound = -1
		if i > 0 && (len(out.Entries) == q.Limit || scanned >= pageScanBudget) {
			out.Next = filepath.Base(files[i-1]) + ":*"
			return out, nil
		}
	}
	return out, nil
}

type lineEntry struct {
	e    Entry
	line int
}

// scanFile returns the last n matching entries of path before line bound
// (all lines when bound is negative), oldest first, how many lines it read,
// and whether earlier lines held more matches. Only complete lines count, so
// a line being appended is left for the next read.
func scanFile(path string, bound, n int, q Query) ([]lineEntry, int, bool, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, 0, false, err
	}
	defer f.Close()
	r := bufio.NewReaderSize(f, 64<<10)
	var keep []lineEntry
	dropped := false
	line := 0
	for ; bound < 0 || line < bound; line++ {
		b, err := r.ReadBytes('\n')
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, line, false, err
		}
		var e Entry
		if json.Unmarshal(b, &e) != nil || !q.match(e) {
			continue
		}
		keep = append(keep, lineEntry{e, line})
		if len(keep) > n {
			keep = keep[1:]
			dropped = true
		}
	}
	return keep, line, dropped, nil
}
