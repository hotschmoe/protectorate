// Package backlog reads and edits the markdown backlog (see docs/backlog.md).
// Documents round-trip byte for byte: edits touch only the characters of the
// field being changed.
package backlog

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"regexp"
	"strconv"
	"strings"
	"time"
)

type State int

const (
	StateTodo State = iota
	StateClaimed
	StateDone
)

func (s State) String() string {
	switch s {
	case StateClaimed:
		return "claimed"
	case StateDone:
		return "done"
	}
	return "todo"
}

var (
	ErrNotFound    = errors.New("item not found")
	ErrAmbiguous   = errors.New("item id is not unique")
	ErrUnavailable = errors.New("item is not available")
	ErrInvalid     = errors.New("invalid argument")
)

type Claim struct {
	By string
	At time.Time
}

type Item struct {
	ID       string
	Project  string
	Title    string
	IssueURL string
	Priority int
	Harness  string
	State    State
	Claim    *Claim
	Notes    string
	Donate   bool
	Line     int
}

type Filter struct {
	Project string
	State   *State
	// Harness keeps items that harness may run: its own plus "any"/unset.
	Harness string
}

func (f Filter) match(it Item) bool {
	if f.Project != "" && it.Project != f.Project {
		return false
	}
	if f.State != nil && it.State != *f.State {
		return false
	}
	if f.Harness != "" && it.Harness != "" && it.Harness != "any" && it.Harness != f.Harness {
		return false
	}
	return true
}

type NewItem struct {
	Project  string
	Title    string
	IssueURL string
	Priority int
	Harness  string
}

const donatePrefix = "donate:"

// Doc is a parsed backlog. lines holds the raw text, so String is lossless.
type Doc struct {
	lines []string
	nl    bool
}

func Parse(text string) *Doc {
	if text == "" {
		return &Doc{nl: true}
	}
	nl := strings.HasSuffix(text, "\n")
	return &Doc{lines: strings.Split(strings.TrimSuffix(text, "\n"), "\n"), nl: nl}
}

func (d *Doc) String() string {
	if len(d.lines) == 0 {
		return ""
	}
	s := strings.Join(d.lines, "\n")
	if d.nl {
		s += "\n"
	}
	return s
}

var (
	headingRe = regexp.MustCompile(`^##[ \t]+(\S.*?)[ \t]*$`)
	itemRe    = regexp.MustCompile(`^[-*] \[([ xX~])\] +`)
	tokenRe   = regexp.MustCompile(`\S+`)
	prioRe    = regexp.MustCompile(`^#p[1-9]$`)
	harnessRe = regexp.MustCompile(`^@[A-Za-z0-9_-]+$`)
	idRe      = regexp.MustCompile(`^id:[A-Za-z0-9_-]+$`)
	claimRe   = regexp.MustCompile(`^claim:[^\s@]+@\S+$`)
	urlRe     = regexp.MustCompile(`^https?://\S+$`)
)

type span struct{ start, end int }

type parsedLine struct {
	state  State
	title  string
	spans  map[string]span
	toks   map[string]string
	stateI int
}

func tokenKind(tok string) string {
	switch {
	case prioRe.MatchString(tok):
		return "prio"
	case harnessRe.MatchString(tok):
		return "harness"
	case idRe.MatchString(tok):
		return "id"
	case claimRe.MatchString(tok):
		return "claim"
	case urlRe.MatchString(tok):
		return "url"
	}
	return ""
}

func splitEOL(line string) (string, string) {
	if strings.HasSuffix(line, "\r") {
		return line[:len(line)-1], "\r"
	}
	return line, ""
}

// parseItemLine reads metadata tokens from the right; a repeated kind or any
// other word ends the run and everything before it is the title.
func parseItemLine(content string) (parsedLine, bool) {
	m := itemRe.FindStringSubmatchIndex(content)
	if m == nil {
		return parsedLine{}, false
	}
	p := parsedLine{spans: map[string]span{}, toks: map[string]string{}, stateI: m[2]}
	switch content[m[2]] {
	case '~':
		p.state = StateClaimed
	case 'x', 'X':
		p.state = StateDone
	}
	bodyStart := m[1]
	locs := tokenRe.FindAllStringIndex(content[bodyStart:], -1)
	titleEnd := bodyStart
	if len(locs) > 0 {
		titleEnd = bodyStart + locs[0][0]
	}
	for i := len(locs) - 1; i >= 0; i-- {
		s := span{bodyStart + locs[i][0], bodyStart + locs[i][1]}
		tok := content[s.start:s.end]
		kind := tokenKind(tok)
		if _, dup := p.spans[kind]; kind == "" || dup {
			titleEnd = s.end
			break
		}
		p.spans[kind], p.toks[kind] = s, tok
		titleEnd = bodyStart + locs[i][0]
	}
	p.title = strings.TrimSpace(content[bodyStart:titleEnd])
	return p, true
}

func (p parsedLine) item() Item {
	it := Item{Title: p.title, State: p.state}
	if t, ok := p.toks["id"]; ok {
		it.ID = strings.TrimPrefix(t, "id:")
	}
	it.IssueURL = p.toks["url"]
	if t, ok := p.toks["prio"]; ok {
		it.Priority, _ = strconv.Atoi(t[2:])
	}
	it.Harness = strings.TrimPrefix(p.toks["harness"], "@")
	if t, ok := p.toks["claim"]; ok {
		body := strings.TrimPrefix(t, "claim:")
		i := strings.LastIndexByte(body, '@')
		c := &Claim{By: body[:i]}
		if at, err := time.Parse(time.RFC3339, body[i+1:]); err == nil {
			c.At = at.UTC()
		}
		it.Claim = c
	}
	return it
}

// Items returns every item in file order.
func (d *Doc) Items() []Item {
	var items []Item
	project := ""
	for i := 0; i < len(d.lines); i++ {
		content, _ := splitEOL(d.lines[i])
		if m := headingRe.FindStringSubmatch(content); m != nil {
			project = m[1]
			continue
		}
		p, ok := parseItemLine(content)
		if !ok {
			continue
		}
		it := p.item()
		it.Project = project
		it.Donate = strings.HasPrefix(project, donatePrefix)
		it.Line = i
		var notes []string
		for i+1 < len(d.lines) && isNote(d.lines[i+1]) {
			i++
			notes = append(notes, strings.TrimSpace(d.lines[i]))
		}
		it.Notes = strings.Join(notes, "\n")
		items = append(items, it)
	}
	return items
}

func isNote(line string) bool {
	content, _ := splitEOL(line)
	return (strings.HasPrefix(content, " ") || strings.HasPrefix(content, "\t")) && strings.TrimSpace(content) != ""
}

func (d *Doc) Query(f Filter) []Item {
	var out []Item
	for _, it := range d.Items() {
		if f.match(it) {
			out = append(out, it)
		}
	}
	return out
}

// missingIDs reports whether any item lacks an id.
func (d *Doc) missingIDs() bool {
	for _, it := range d.Items() {
		if it.ID == "" {
			return true
		}
	}
	return false
}

// EnsureIDs appends a fresh id to each item without one.
func (d *Doc) EnsureIDs() (bool, error) {
	items := d.Items()
	used := map[string]bool{}
	for _, it := range items {
		if it.ID != "" {
			used[it.ID] = true
		}
	}
	changed := false
	for _, it := range items {
		if it.ID != "" {
			continue
		}
		id, err := newID(used)
		if err != nil {
			return changed, err
		}
		used[id] = true
		d.lines[it.Line] = setField(d.lines[it.Line], "id", "id:"+id)
		changed = true
	}
	return changed, nil
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

// setField replaces, appends or (tok == "") removes the metadata token of kind.
func setField(line, kind, tok string) string {
	content, eol := splitEOL(line)
	p, ok := parseItemLine(content)
	if !ok {
		return line
	}
	sp, has := p.spans[kind]
	switch {
	case has && tok != "":
		content = content[:sp.start] + tok + content[sp.end:]
	case has:
		start := sp.start
		for start > 0 && (content[start-1] == ' ' || content[start-1] == '\t') {
			start--
		}
		content = content[:start] + content[sp.end:]
	case tok != "":
		content = strings.TrimRight(content, " \t") + " " + tok
	}
	return content + eol
}

func setStateChar(line string, st State) string {
	content, eol := splitEOL(line)
	p, ok := parseItemLine(content)
	if !ok {
		return line
	}
	c := byte(' ')
	switch st {
	case StateClaimed:
		c = '~'
	case StateDone:
		c = 'x'
	}
	return content[:p.stateI] + string(c) + content[p.stateI+1:] + eol
}

func claimToken(c Claim) string {
	return "claim:" + c.By + "@" + c.At.UTC().Format(time.RFC3339)
}

var (
	projectRe = regexp.MustCompile(`^\S(.*\S)?$`)
	byRe      = regexp.MustCompile(`^[A-Za-z0-9_./:-]+$`)
	harnessNm = regexp.MustCompile(`^[A-Za-z0-9_-]+$`)
)

func (n NewItem) validate() error {
	if !projectRe.MatchString(n.Project) || strings.ContainsAny(n.Project, "\r\n") {
		return fmt.Errorf("%w: project must be one non-empty line", ErrInvalid)
	}
	if strings.HasPrefix(n.Project, donatePrefix) && strings.TrimSpace(strings.TrimPrefix(n.Project, donatePrefix)) == "" {
		return fmt.Errorf("%w: donate project needs a repo", ErrInvalid)
	}
	if strings.ContainsAny(n.Title, "\r\n") {
		return fmt.Errorf("%w: title must be one line", ErrInvalid)
	}
	if n.Title == "" && n.IssueURL == "" {
		return fmt.Errorf("%w: need a title or an issue url", ErrInvalid)
	}
	if n.IssueURL != "" && !urlRe.MatchString(n.IssueURL) {
		return fmt.Errorf("%w: issue url must be an http(s) url without spaces", ErrInvalid)
	}
	if strings.HasPrefix(n.Project, donatePrefix) && n.IssueURL == "" {
		return fmt.Errorf("%w: donated items need an issue url", ErrInvalid)
	}
	if n.Priority < 0 || n.Priority > 9 {
		return fmt.Errorf("%w: priority must be 1-9", ErrInvalid)
	}
	if n.Harness != "" && !harnessNm.MatchString(n.Harness) {
		return fmt.Errorf("%w: bad harness %q", ErrInvalid, n.Harness)
	}
	if f := strings.Fields(n.Title); len(f) > 0 && tokenKind(f[len(f)-1]) != "" {
		return fmt.Errorf("%w: title must not end with a metadata-like word (%s)", ErrInvalid, f[len(f)-1])
	}
	return nil
}

// Add appends a new item under its project heading, creating the heading at
// the end of the file when absent.
func (d *Doc) Add(n NewItem) (Item, error) {
	if err := n.validate(); err != nil {
		return Item{}, err
	}
	used := map[string]bool{}
	for _, it := range d.Items() {
		used[it.ID] = true
	}
	id, err := newID(used)
	if err != nil {
		return Item{}, err
	}
	line := "- [ ]"
	if n.Title != "" {
		line += " " + n.Title
	}
	var meta []string
	if n.IssueURL != "" {
		meta = append(meta, n.IssueURL)
	}
	if n.Priority > 0 {
		meta = append(meta, "#p"+strconv.Itoa(n.Priority))
	}
	if n.Harness != "" {
		meta = append(meta, "@"+n.Harness)
	}
	meta = append(meta, "id:"+id)
	sep := "  "
	if n.Title == "" {
		sep = " "
	}
	line += sep + strings.Join(meta, " ")

	at := d.insertAt(n.Project)
	if at < 0 {
		if len(d.lines) > 0 && d.lines[len(d.lines)-1] != "" {
			d.lines = append(d.lines, "")
		}
		d.lines = append(d.lines, "## "+n.Project, line)
	} else {
		d.lines = append(d.lines[:at], append([]string{line}, d.lines[at:]...)...)
	}
	if !d.nl && (at < 0 || at == len(d.lines)-1) {
		d.nl = true
	}
	return d.find(id)
}

// insertAt returns the line index after the project's last item (or its
// heading), or -1 when the project has no heading.
func (d *Doc) insertAt(project string) int {
	head := -1
	for i, l := range d.lines {
		content, _ := splitEOL(l)
		m := headingRe.FindStringSubmatch(content)
		if m == nil {
			continue
		}
		if head >= 0 {
			break
		}
		if m[1] == project {
			head = i
		}
	}
	if head < 0 {
		return -1
	}
	at := head + 1
	for i := head + 1; i < len(d.lines); i++ {
		content, _ := splitEOL(d.lines[i])
		if headingRe.MatchString(content) {
			break
		}
		if _, ok := parseItemLine(content); ok {
			at = i + 1
			for at < len(d.lines) && isNote(d.lines[at]) {
				at++
			}
		}
	}
	return at
}

func (d *Doc) find(id string) (Item, error) {
	var found []Item
	for _, it := range d.Items() {
		if it.ID == id {
			found = append(found, it)
		}
	}
	switch len(found) {
	case 0:
		return Item{}, fmt.Errorf("%w: %q", ErrNotFound, id)
	case 1:
		return found[0], nil
	}
	return Item{}, fmt.Errorf("%w: %q", ErrAmbiguous, id)
}

// SetState moves item id to st. Claiming needs by and fails unless the item
// is todo or already held by the same claimant. Done keeps an existing claim
// as the record of who attempted it. It returns the updated item and the
// previous state.
func (d *Doc) SetState(id string, st State, by string, now time.Time) (Item, State, error) {
	if by != "" && !byRe.MatchString(by) {
		return Item{}, 0, fmt.Errorf("%w: claimant %q may use letters, digits and _ . / : -", ErrInvalid, by)
	}
	it, err := d.find(id)
	if err != nil {
		return Item{}, 0, err
	}
	prev := it.State
	line := d.lines[it.Line]
	switch st {
	case StateClaimed:
		if by == "" {
			return Item{}, prev, fmt.Errorf("%w: claim needs a claimant", ErrInvalid)
		}
		if prev == StateClaimed && it.Claim != nil && it.Claim.By == by {
			return it, prev, nil
		}
		if prev != StateTodo {
			return Item{}, prev, unavailable(it)
		}
		line = setField(line, "claim", claimToken(Claim{By: by, At: now}))
	case StateDone:
		if prev == StateDone {
			return it, prev, nil
		}
		switch {
		case it.Claim != nil && by != "" && it.Claim.By != by:
			return Item{}, prev, unavailable(it)
		case it.Claim == nil && by != "":
			line = setField(line, "claim", claimToken(Claim{By: by, At: now}))
		}
	case StateTodo:
		if prev == StateTodo {
			return it, prev, nil
		}
		line = setField(line, "claim", "")
	default:
		return Item{}, prev, fmt.Errorf("%w: unknown state", ErrInvalid)
	}
	d.lines[it.Line] = setStateChar(line, st)
	out, err := d.find(id)
	return out, prev, err
}

func unavailable(it Item) error {
	if it.State == StateDone {
		return fmt.Errorf("%w: %s is done", ErrUnavailable, it.ID)
	}
	if it.Claim != nil {
		return fmt.Errorf("%w: %s is claimed by %s since %s", ErrUnavailable, it.ID, it.Claim.By, it.Claim.At.Format(time.RFC3339))
	}
	return fmt.Errorf("%w: %s is claimed", ErrUnavailable, it.ID)
}
