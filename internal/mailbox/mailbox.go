// Package mailbox is the hub's message store: an append-only JSONL log of
// everything that happens to a message (sent, addressed, delivered, read,
// acked, reminded, escalated), replayed on open. The vocabulary follows
// mail-style agent coordination: threads, per-recipient read and ack, an
// unread-only inbox, and ack timers. The store decides nothing about who a
// message goes to or how it is delivered; the hub does, and records the
// outcome here.
package mailbox

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

var (
	ErrNotFound     = errors.New("no such message")
	ErrNotRecipient = errors.New("not a recipient of this message")
)

const ConciseGuidance = "Mail: aim for 150 words; lead with the result or blocker. Include only actionable findings and essential evidence; reference a file, commit or note for details. Do not restate the request or prior context. No acknowledgements unless asked or ack-required; use msg ack when only confirmation is needed. Finish every brief with protectorate report --status complete|partial|blocked --summary-file F; use partial honestly when work remains."

type Kind string

const (
	Note    Kind = "note"
	Request Kind = "request"
	Notice  Kind = "notice"
)

// Delivery is one recipient's view of a message. Since starts the ack timer:
// it is set when the message is injected into the recipient's chat or, for a
// recipient that cannot be injected, when it was addressed to them.
type Delivery struct {
	Since     time.Time  `json:"since,omitempty"`
	Delivered *time.Time `json:"delivered,omitempty"`
	Read      *time.Time `json:"read,omitempty"`
	Acked     *time.Time `json:"acked,omitempty"`
	Reminders int        `json:"reminders,omitempty"`
}

// Message is one message with its delivery state.
type Message struct {
	ID          string               `json:"id"`
	Thread      string               `json:"thread"`
	From        string               `json:"from"`
	To          string               `json:"to"`
	Kind        Kind                 `json:"kind"`
	AckRequired bool                 `json:"ack_required,omitempty"`
	Body        string               `json:"body"`
	RequestID   string               `json:"request_id,omitempty"`
	ReplyTo     string               `json:"reply_to,omitempty"`
	FromProject string               `json:"from_project,omitempty"`
	Project     string               `json:"project,omitempty"`
	Created     time.Time            `json:"created"`
	Recipients  []string             `json:"recipients,omitempty"`
	Deliveries  map[string]*Delivery `json:"deliveries,omitempty"`
	Escalated   *time.Time           `json:"escalated,omitempty"`
	NoteID      string               `json:"note_id,omitempty"`
}

func (m *Message) clone() Message {
	c := *m
	c.Recipients = append([]string(nil), m.Recipients...)
	c.Deliveries = make(map[string]*Delivery, len(m.Deliveries))
	for k, d := range m.Deliveries {
		v := *d
		c.Deliveries[k] = &v
	}
	return c
}

// Open reports whether no recipient has acked the message yet.
func (m *Message) Open() bool {
	for _, d := range m.Deliveries {
		if d.Acked != nil {
			return false
		}
	}
	return true
}

// Unread reports whether rcpt has neither been shown, been injected nor acked
// the message.
func (m *Message) Unread(rcpt string) bool {
	d := m.Deliveries[rcpt]
	return d != nil && d.Delivered == nil && d.Read == nil && d.Acked == nil
}

// record is one log line.
type record struct {
	Op   string    `json:"op"`
	At   time.Time `json:"at"`
	ID   string    `json:"id,omitempty"`
	Rcpt string    `json:"rcpt,omitempty"`
	To   []string  `json:"to,omitempty"`
	Item string    `json:"item,omitempty"`
	Msg  *Message  `json:"msg,omitempty"`
}

// Store is the message store. All methods are safe for concurrent use.
type Store struct {
	now func() time.Time

	changed chan struct{}
	mu      sync.Mutex
	f       *os.File
	msgs    map[string]*Message
	order   []string
	byReq   map[string]string
	nextMsg int
	nextThr int
}

// Open opens or creates the log at path and replays it. A partial last line,
// left by a crash mid-write, is cut off. now may be nil.
func Open(path string, now func() time.Time) (*Store, error) {
	if now == nil {
		now = time.Now
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, fmt.Errorf("create mailbox dir: %w", err)
	}
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE|os.O_APPEND, 0o600)
	if err != nil {
		return nil, fmt.Errorf("open mailbox: %w", err)
	}
	s := &Store{changed: make(chan struct{}), now: now, f: f, msgs: map[string]*Message{}, byReq: map[string]string{}}
	if err := s.replay(path); err != nil {
		f.Close()
		return nil, err
	}
	return s, nil
}

func (s *Store) replay(path string) error {
	r := bufio.NewReaderSize(io.NewSectionReader(s.f, 0, 1<<62), 1<<20)
	var off int64
	for n := 1; ; n++ {
		line, err := r.ReadBytes('\n')
		if errors.Is(err, io.EOF) {
			if len(line) > 0 {
				if err := s.f.Truncate(off); err != nil {
					return fmt.Errorf("cut partial mailbox line: %w", err)
				}
			}
			return nil
		}
		if err != nil {
			return fmt.Errorf("read mailbox: %w", err)
		}
		var rec record
		if err := json.Unmarshal(line, &rec); err != nil {
			return fmt.Errorf("mailbox %s: line %d: %w", path, n, err)
		}
		s.apply(rec)
		off += int64(len(line))
	}
}

// Close closes the log.
func (s *Store) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.f.Close()
}

// commit appends rec durably and applies it. The state changes only if the
// write succeeded.
func (s *Store) commit(rec record) error {
	b, err := json.Marshal(rec)
	if err != nil {
		return fmt.Errorf("encode mailbox record: %w", err)
	}
	if _, err := s.f.Write(append(b, '\n')); err != nil {
		return fmt.Errorf("write mailbox: %w", err)
	}
	if err := s.f.Sync(); err != nil {
		return fmt.Errorf("sync mailbox: %w", err)
	}
	s.apply(rec)
	close(s.changed)
	s.changed = make(chan struct{})
	return nil
}

func idNumber(id string) int {
	n, _ := strconv.Atoi(strings.TrimLeft(id, "mt"))
	return n
}

func (s *Store) apply(rec record) {
	if rec.Op == "send" {
		m := rec.Msg
		m.Deliveries = map[string]*Delivery{}
		for _, r := range m.Recipients {
			m.Deliveries[r] = &Delivery{}
		}
		s.msgs[m.ID] = m
		s.order = append(s.order, m.ID)
		if m.RequestID != "" {
			s.byReq[reqKey(m.From, m.RequestID)] = m.ID
		}
		s.nextMsg = max(s.nextMsg, idNumber(m.ID))
		s.nextThr = max(s.nextThr, idNumber(m.Thread))
		return
	}
	m := s.msgs[rec.ID]
	if m == nil {
		return
	}
	at := rec.At
	d := m.Deliveries[rec.Rcpt]
	switch rec.Op {
	case "resolve":
		if len(m.Recipients) > 0 {
			return
		}
		for _, r := range rec.To {
			m.Recipients = append(m.Recipients, r)
			m.Deliveries[r] = &Delivery{}
		}
	case "arm":
		if d != nil && d.Since.IsZero() {
			d.Since = at
		}
	case "deliver":
		if d != nil {
			d.Delivered, d.Since = &at, at
		}
	case "read":
		if d != nil && d.Read == nil {
			d.Read = &at
		}
	case "ack":
		if d != nil && d.Acked == nil {
			d.Acked = &at
		}
	case "remind":
		if d != nil {
			d.Reminders++
			d.Since = at
			d.Delivered = &at
		}
	case "escalate":
		if m.Escalated == nil {
			m.Escalated = &at
		}
	case "backlog":
		m.NoteID = rec.Item
	}
}

func reqKey(from, id string) string { return from + "\x00" + id }

// NewMessage is a message to send. Recipients may be left empty for an
// address that resolves later (a project with no live sleeve yet).
type NewMessage struct {
	From        string
	To          string
	Kind        Kind
	AckRequired bool
	Body        string
	RequestID   string
	Thread      string
	ReplyTo     string
	FromProject string
	Project     string
	Recipients  []string
	// Footer, if set, is appended to the body once the thread id is known.
	Footer func(thread string) string
}

// Send stores a message. A retry from the same sender with the same
// RequestID returns the original and created=false. A message without a
// Thread starts a new one.
func (s *Store) Send(n NewMessage) (m Message, created bool, err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if n.RequestID != "" {
		if id, ok := s.byReq[reqKey(n.From, n.RequestID)]; ok {
			return s.msgs[id].clone(), false, nil
		}
	}
	msg := &Message{
		ID: "m" + strconv.Itoa(s.nextMsg+1), Thread: n.Thread, From: n.From, To: n.To, Kind: n.Kind,
		AckRequired: n.AckRequired, Body: n.Body, RequestID: n.RequestID, ReplyTo: n.ReplyTo,
		FromProject: n.FromProject, Project: n.Project, Created: s.now().UTC(),
		Recipients: append([]string(nil), n.Recipients...),
	}
	if msg.Thread == "" {
		msg.Thread = "t" + strconv.Itoa(s.nextThr+1)
	}
	if n.Footer != nil {
		msg.Body = strings.TrimRight(msg.Body, "\n") + "\n\n" + n.Footer(msg.Thread)
	}
	if err := s.commit(record{Op: "send", At: msg.Created, Msg: msg}); err != nil {
		return Message{}, false, err
	}
	return s.msgs[msg.ID].clone(), true, nil
}

func (s *Store) op(op, id, rcpt string, extra func(*record)) error {
	m := s.msgs[id]
	if m == nil {
		return fmt.Errorf("%w: %s", ErrNotFound, id)
	}
	if rcpt != "" && m.Deliveries[rcpt] == nil {
		return fmt.Errorf("%w: %s, message %s", ErrNotRecipient, rcpt, id)
	}
	rec := record{Op: op, At: s.now().UTC(), ID: id, Rcpt: rcpt}
	if extra != nil {
		extra(&rec)
	}
	return s.commit(rec)
}

// Resolve sets the recipients of a message that had none and starts the ack
// timer for each recipient that cannot be injected (armed).
func (s *Store) Resolve(id string, rcpts, armed []string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if m := s.msgs[id]; m != nil && len(m.Recipients) > 0 {
		return nil
	}
	if err := s.op("resolve", id, "", func(r *record) { r.To = rcpts }); err != nil {
		return err
	}
	for _, a := range armed {
		if err := s.op("arm", id, a, nil); err != nil {
			return err
		}
	}
	return nil
}

// Arm starts the ack timer of a recipient that is not injected.
func (s *Store) Arm(id, rcpt string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if m := s.msgs[id]; m != nil && m.Deliveries[rcpt] != nil && !m.Deliveries[rcpt].Since.IsZero() {
		return nil
	}
	return s.op("arm", id, rcpt, nil)
}

// Deliver records that the message was injected into rcpt's chat.
func (s *Store) Deliver(id, rcpt string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.op("deliver", id, rcpt, nil)
}

// Remind records a second delivery of an unacked message.
func (s *Store) Remind(id, rcpt string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.op("remind", id, rcpt, nil)
}

// Escalate records that an unacked message was escalated. It happens once.
func (s *Store) Escalate(id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if m := s.msgs[id]; m != nil && m.Escalated != nil {
		return nil
	}
	return s.op("escalate", id, "", nil)
}

// SetNoteID records the note a request was filed as.
func (s *Store) SetNoteID(id, note string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.op("backlog", id, "", func(r *record) { r.Item = note })
}

// Ack marks the message acked by rcpt. changed is false if it already was.
func (s *Store) Ack(id, rcpt string) (m Message, changed bool, err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	cur := s.msgs[id]
	if cur != nil && cur.Deliveries[rcpt] != nil && cur.Deliveries[rcpt].Acked != nil {
		return cur.clone(), false, nil
	}
	if err := s.op("ack", id, rcpt, nil); err != nil {
		return Message{}, false, err
	}
	return s.msgs[id].clone(), true, nil
}

// Get returns one message.
func (s *Store) Get(id string) (Message, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	m := s.msgs[id]
	if m == nil {
		return Message{}, false
	}
	return m.clone(), true
}

// Thread returns the messages of a thread in the order they were sent.
func (s *Store) Thread(thread string) []Message {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []Message
	for _, id := range s.order {
		if m := s.msgs[id]; m.Thread == thread {
			out = append(out, m.clone())
		}
	}
	return out
}

// Inbox returns the messages addressed to rcpt, oldest first, only those of
// thread unless it is empty. With unreadOnly it leaves out those already
// injected, shown or acked. Unless peek is set, the returned messages are
// marked read.
func (s *Store) Inbox(rcpt, thread string, unreadOnly, peek bool) ([]Message, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []Message
	for _, id := range s.order {
		m := s.msgs[id]
		if m.Deliveries[rcpt] == nil || (thread != "" && m.Thread != thread) || (unreadOnly && !m.Unread(rcpt)) {
			continue
		}
		c := m.clone()
		if !peek && m.Deliveries[rcpt].Read == nil {
			if err := s.op("read", id, rcpt, nil); err != nil {
				return nil, err
			}
		}
		out = append(out, c)
	}
	return out, nil
}

// ThreadInfo summarizes one thread.
type ThreadInfo struct {
	ID string
	// Participants are senders and recipients in order of first appearance;
	// a message with no recipients yet adds "project:<id>" for its project.
	Participants []string
	Messages     int
	Last         Message
	// Unread, OpenAcks and Escalated count messages: unread by the reader,
	// requiring an ack nobody gave, and escalated without an ack since.
	Unread    int
	OpenAcks  int
	Escalated int
	Projects  []string
}

// Threads summarizes every thread, most recently active first, counting
// unread messages for reader.
func (s *Store) Threads(reader string) []ThreadInfo {
	s.mu.Lock()
	defer s.mu.Unlock()
	byID := map[string]*ThreadInfo{}
	last := map[string]string{}
	var list []*ThreadInfo
	add := func(xs []string, v string) []string {
		for _, x := range xs {
			if x == v {
				return xs
			}
		}
		return append(xs, v)
	}
	for _, id := range s.order {
		m := s.msgs[id]
		ti := byID[m.Thread]
		if ti == nil {
			ti = &ThreadInfo{ID: m.Thread}
			byID[m.Thread] = ti
			list = append(list, ti)
		}
		ti.Messages++
		last[m.Thread] = m.ID
		ti.Participants = add(ti.Participants, m.From)
		for _, r := range m.Recipients {
			ti.Participants = add(ti.Participants, r)
		}
		if len(m.Recipients) == 0 && m.Project != "" {
			ti.Participants = add(ti.Participants, "project:"+m.Project)
		}
		for _, p := range []string{m.Project, m.FromProject} {
			if p != "" {
				ti.Projects = add(ti.Projects, p)
			}
		}
		if m.Unread(reader) {
			ti.Unread++
		}
		if m.Open() && m.AckRequired {
			ti.OpenAcks++
		}
		if m.Open() && m.Escalated != nil {
			ti.Escalated++
		}
	}
	out := make([]ThreadInfo, len(list))
	for i, ti := range list {
		ti.Last = s.msgs[last[ti.ID]].clone()
		out[i] = *ti
	}
	sort.SliceStable(out, func(i, j int) bool {
		return idNumber(out[i].Last.ID) > idNumber(out[j].Last.ID)
	})
	return out
}

// Unresolved returns the messages that have no recipients yet.
func (s *Store) Unresolved() []Message {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []Message
	for _, id := range s.order {
		if m := s.msgs[id]; len(m.Recipients) == 0 {
			out = append(out, m.clone())
		}
	}
	return out
}

// Pending returns, per recipient, the messages still to be injected: not yet
// delivered, not read and not acked. Armed mail also remains pending so a
// terminal recipient switched to chat can receive it.
func (s *Store) Pending() map[string][]Message {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := map[string][]Message{}
	for _, id := range s.order {
		m := s.msgs[id]
		for r, d := range m.Deliveries {
			if d.Delivered == nil && d.Read == nil && d.Acked == nil {
				out[r] = append(out[r], m.clone())
			}
		}
	}
	return out
}

// Involving returns the ack-required messages still open that name sent or
// that are addressed to name and unacked by it, oldest first.
func (s *Store) Involving(name string) []Message {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []Message
	for _, id := range s.order {
		m := s.msgs[id]
		if !m.AckRequired {
			continue
		}
		d := m.Deliveries[name]
		if (m.From == name && m.Open()) || (d != nil && d.Acked == nil) {
			out = append(out, m.clone())
		}
	}
	return out
}

// OpenRequests returns the unacked request messages addressed to project that
// were sent from fromProject, oldest first.
func (s *Store) OpenRequests(project, fromProject string) []Message {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []Message
	for _, id := range s.order {
		m := s.msgs[id]
		if m.Kind == Request && m.Project == project && m.FromProject == fromProject && m.Open() {
			out = append(out, m.clone())
		}
	}
	return out
}

// Action is what an overdue ack calls for.
type Action string

const (
	ActRemind   Action = "remind"
	ActEscalate Action = "escalate"
)

// Due is one overdue delivery.
type Due struct {
	ID     string
	Rcpt   string
	Action Action
}

// Overdue lists the unacked ack_required deliveries whose timer ran past
// timeout: the first time a reminder, the second time an escalation (once per
// message).
func (s *Store) Overdue(timeout time.Duration) []Due {
	s.mu.Lock()
	defer s.mu.Unlock()
	now := s.now()
	var out []Due
	for _, id := range s.order {
		m := s.msgs[id]
		if !m.AckRequired || m.Escalated != nil {
			continue
		}
		rcpts := make([]string, 0, len(m.Deliveries))
		for r := range m.Deliveries {
			rcpts = append(rcpts, r)
		}
		sort.Strings(rcpts)
		escalated := false
		for _, r := range rcpts {
			d := m.Deliveries[r]
			if d.Acked != nil || d.Since.IsZero() || now.Sub(d.Since) < timeout {
				continue
			}
			if d.Reminders == 0 {
				out = append(out, Due{ID: id, Rcpt: r, Action: ActRemind})
			} else if !escalated {
				out = append(out, Due{ID: id, Rcpt: r, Action: ActEscalate})
				escalated = true
			}
		}
	}
	return out
}

// Wait returns a reply or an acknowledged outgoing message, including outcomes
// already recorded before the call. Subscription and inspection share the lock.
func (s *Store) Wait(ctx context.Context, target, who string) (Message, error) {
	for {
		s.mu.Lock()
		thread := target
		original := s.msgs[target]
		if original != nil {
			thread = original.Thread
		}
		found := false
		for _, id := range s.order {
			m := s.msgs[id]
			if m.Thread != thread {
				continue
			}
			found = true
			if original != nil && m.ID != target && m.ReplyTo != target {
				continue
			}
			if m.From != who && m.Deliveries[who] != nil && (original == nil || m.ID != target) {
				out := m.clone()
				s.mu.Unlock()
				return out, nil
			}
			if m.From == who {
				for _, d := range m.Deliveries {
					if d.Acked != nil {
						out := m.clone()
						s.mu.Unlock()
						return out, nil
					}
				}
			}
		}
		changed := s.changed
		s.mu.Unlock()
		if !found {
			return Message{}, ErrNotFound
		}
		select {
		case <-ctx.Done():
			return Message{}, fmt.Errorf("wait for mail: %w", ctx.Err())
		case <-changed:
		}
	}
}
