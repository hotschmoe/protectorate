// Package backlog stores the owner's notes on the hub (see docs/backlog.md).
// The notes live in one JSON file; every change takes an OS file lock, so hub
// processes and the CLI never overwrite each other.
package backlog

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
	"unicode/utf8"
)

var (
	ErrNotFound = errors.New("note not found")
	ErrInvalid  = errors.New("invalid argument")
)

const (
	maxTitle   = 200
	maxProject = 100
	maxSource  = 100
	maxBody    = 256 << 10
)

// Note is one note. Body is markdown text.
type Note struct {
	ID      string    `json:"id"`
	Title   string    `json:"title,omitempty"`
	Body    string    `json:"body,omitempty"`
	Created time.Time `json:"created_at"`
	Updated time.Time `json:"updated_at"`
	Pinned  bool      `json:"pinned,omitempty"`
	Project string    `json:"project,omitempty"`
	Source  string    `json:"source,omitempty"`
}

// Filter narrows List. Query matches title and body, ignoring case.
type Filter struct {
	Project string
	Query   string
}

func (f Filter) match(n Note) bool {
	if f.Project != "" && n.Project != f.Project {
		return false
	}
	if q := strings.ToLower(f.Query); q != "" {
		return strings.Contains(strings.ToLower(n.Title), q) || strings.Contains(strings.ToLower(n.Body), q)
	}
	return true
}

// NewNote is a note to create.
type NewNote struct {
	Title   string
	Body    string
	Pinned  bool
	Project string
	Source  string
}

// Patch changes the fields that are non-nil.
type Patch struct {
	Title   *string
	Body    *string
	Pinned  *bool
	Project *string
}

func cleanTitle(s string) (string, error) {
	s = strings.TrimSpace(s)
	if strings.ContainsAny(s, "\r\n") || !utf8.ValidString(s) {
		return "", fmt.Errorf("%w: title must be one line of text", ErrInvalid)
	}
	if utf8.RuneCountInString(s) > maxTitle {
		return "", fmt.Errorf("%w: title is longer than %d characters", ErrInvalid, maxTitle)
	}
	return s, nil
}

func cleanBody(s string) (string, error) {
	s = strings.ReplaceAll(s, "\r\n", "\n")
	if !utf8.ValidString(s) || strings.ContainsRune(s, 0) {
		return "", fmt.Errorf("%w: body must be text", ErrInvalid)
	}
	if len(s) > maxBody {
		return "", fmt.Errorf("%w: body is larger than %d KiB", ErrInvalid, maxBody>>10)
	}
	return s, nil
}

func cleanTag(what, s string, max int) (string, error) {
	s = strings.TrimSpace(s)
	if strings.ContainsAny(s, "\r\n") || !utf8.ValidString(s) {
		return "", fmt.Errorf("%w: %s must be one line of text", ErrInvalid, what)
	}
	if utf8.RuneCountInString(s) > max {
		return "", fmt.Errorf("%w: %s is longer than %d characters", ErrInvalid, what, max)
	}
	return s, nil
}

func (n NewNote) build(id string, now time.Time) (Note, error) {
	var out Note
	var err error
	if out.Title, err = cleanTitle(n.Title); err != nil {
		return Note{}, err
	}
	if out.Body, err = cleanBody(n.Body); err != nil {
		return Note{}, err
	}
	if out.Project, err = cleanTag("project", n.Project, maxProject); err != nil {
		return Note{}, err
	}
	if out.Source, err = cleanTag("source", n.Source, maxSource); err != nil {
		return Note{}, err
	}
	if out.Title == "" && strings.TrimSpace(out.Body) == "" {
		return Note{}, fmt.Errorf("%w: a note needs a title or a body", ErrInvalid)
	}
	out.ID, out.Pinned, out.Created, out.Updated = id, n.Pinned, now, now
	return out, nil
}

// apply returns n with p applied and whether anything changed.
func (p Patch) apply(n Note, now time.Time) (Note, bool, error) {
	out := n
	var err error
	if p.Title != nil {
		if out.Title, err = cleanTitle(*p.Title); err != nil {
			return n, false, err
		}
	}
	if p.Body != nil {
		if out.Body, err = cleanBody(*p.Body); err != nil {
			return n, false, err
		}
	}
	if p.Project != nil {
		if out.Project, err = cleanTag("project", *p.Project, maxProject); err != nil {
			return n, false, err
		}
	}
	if p.Pinned != nil {
		out.Pinned = *p.Pinned
	}
	if out.Title == "" && strings.TrimSpace(out.Body) == "" {
		return n, false, fmt.Errorf("%w: a note needs a title or a body", ErrInvalid)
	}
	if out == n {
		return n, false, nil
	}
	out.Updated = now
	return out, true, nil
}

// sortNotes puts pinned notes first, then the most recently updated.
func sortNotes(notes []Note) {
	sort.SliceStable(notes, func(i, j int) bool {
		a, b := notes[i], notes[j]
		if a.Pinned != b.Pinned {
			return a.Pinned
		}
		if !a.Updated.Equal(b.Updated) {
			return a.Updated.After(b.Updated)
		}
		return a.Created.After(b.Created)
	})
}

func newID(used map[string]bool) (string, error) {
	for n := 2; n <= 8; n++ {
		for try := 0; try < 64; try++ {
			b := make([]byte, n)
			if _, err := rand.Read(b); err != nil {
				return "", fmt.Errorf("generate id: %w", err)
			}
			if id := hex.EncodeToString(b); !used[id] {
				return id, nil
			}
		}
	}
	return "", errors.New("generate id: no free id")
}
