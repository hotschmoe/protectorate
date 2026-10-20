package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/chatlog"
	"github.com/hotschmoe/protectorate/internal/driver"
	"github.com/hotschmoe/protectorate/internal/keylog"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	driverOpenTimeout = 60 * time.Second
	sendTimeout       = 30 * time.Second
	compactTimeout    = 5 * time.Minute
	persistEvery      = time.Second
)

type openFunc func(ctx context.Context, h driver.Harness, cwd, sessionID, holder string, opts driver.Options) (driver.Session, error)

// chatManager owns the chat side of every sleeve: the event log, the driver
// session and the state machine. A driver is opened lazily and stays open
// until the sleeve is killed, switched to terminal mode, or the hub stops.
type chatManager struct {
	h    *Hub
	open openFunc

	mu      sync.Mutex
	sleeves map[string]*chatSleeve
	stopped bool
}

func newChatManager(h *Hub) *chatManager {
	m := &chatManager{h: h, sleeves: map[string]*chatSleeve{}}
	m.open = driver.NewOwners().OpenDriver
	return m
}

type chatSleeve struct {
	m    *chatManager
	name string
	log  *chatlog.Log
	keys sendKeys

	// opMu serializes lifecycle operations (open, send, compact, mode,
	// close); pmu orders the event pump against a send or respond so the
	// user message and approval answer land in the log before the harness's
	// reaction to them.
	opMu sync.Mutex
	pmu  sync.Mutex

	mu         sync.Mutex
	sess       driver.Session
	pumpDone   chan struct{}
	closing    bool
	releaseTUI func()
	state      pb.ChatState
	sessionID  string
	model      string
	effort     string
	requested  driver.Selection
	defaults   driver.Selection
	provider   string
	local      bool
	context    uint64
	window     uint64
	cacheExp   time.Time
	activity   time.Time
	turn       bool
	// used is set once a turn was sent: the harness stores a session only
	// then, so only then is its id worth resuming.
	used       bool
	compacting bool
	// hubOwned is set while a resleeve has taken the chat over: no sender
	// may start a turn or compact.
	hubOwned    bool
	approvals   map[string]bool
	lastPersist time.Time
	lastPublish time.Time
}

// get returns the chat state of a sleeve, creating it on first use. fresh
// starts a new log, moving a previous sleeve's aside.
func (m *chatManager) get(name string, fresh bool) (*chatSleeve, error) {
	info, err := m.h.sleeves.Get(name)
	if err != nil {
		return nil, sleeveErr(err)
	}
	if info.Mode != sleeve.ModeChat {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s runs in terminal mode, which has no chat", name))
	}
	if info.Helper != "" {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s is the hub's own (%s) and has no chat", name, info.Helper))
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.stopped {
		return nil, connect.NewError(connect.CodeUnavailable, errors.New("hub is shutting down"))
	}
	if s := m.sleeves[name]; s != nil {
		return s, nil
	}
	path := m.h.sleeves.ChatLogPath(name)
	keysPath := path + ".request-ids"
	if fresh {
		for _, p := range []string{path, keysPath} {
			if fi, err := os.Stat(p); err == nil && fi.Size() > 0 {
				if err := os.Rename(p, fmt.Sprintf("%s.%d", p, time.Now().Unix())); err != nil {
					return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("move old chat log aside: %w", err))
				}
			}
		}
	}
	lg, err := chatlog.Open(path)
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	keys, saved, err := keylog.Open(keysPath, maxRequestIDs, time.Now)
	if err != nil {
		lg.Close()
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	s := &chatSleeve{m: m, name: name, log: lg, approvals: map[string]bool{}, keys: sendKeys{entries: map[string]*sendEntry{}}}
	s.keys.restore(keys, saved)
	if c := info.Chat; c != nil {
		s.sessionID, s.model, s.context, s.window = c.SessionID, c.Model, c.Context, c.Window
		s.provider, s.local = c.Provider, c.LocalModel
		s.effort, s.requested, s.defaults = c.Effort, c.Requested, c.Defaults
		s.cacheExp, s.activity = c.CacheExpires, c.LastActivity
		s.used = c.SessionID != ""
	}
	if info.Chat != nil {
		s.state = pb.ChatState(pb.ChatState_value[info.Chat.State])
	}
	// no driver runs after a hub restart: a harness that was in a turn is
	// reported failed, anything else is idle
	if prev := s.state; prev != pb.ChatState_CHAT_STATE_IDLE {
		s.mu.Lock()
		switch prev {
		case pb.ChatState_CHAT_STATE_WORKING, pb.ChatState_CHAT_STATE_AWAITING_APPROVAL, pb.ChatState_CHAT_STATE_COMPACTING:
			what := "turn"
			if prev == pb.ChatState_CHAT_STATE_COMPACTING {
				what = "compaction"
			}
			s.appendLocked(func(e *pb.ChatEvent) {
				e.Body = &pb.ChatEvent_Error{Error: &pb.ChatError{Text: "hub restarted mid-" + what + "; the harness was stopped. Send again to resume its session."}}
			})
			s.setStateLocked(pb.ChatState_CHAT_STATE_FAILED)
		default:
			s.setStateLocked(pb.ChatState_CHAT_STATE_IDLE)
		}
		s.mu.Unlock()
	}
	m.sleeves[name] = s
	return s, nil
}

// recover loads the chat state of every chat sleeve that was not idle when
// the previous hub stopped, so a harness caught mid-turn shows FAILED at once
// instead of on first use.
func (m *chatManager) recover() {
	for _, info := range m.h.sleeves.List() {
		if info.Mode != sleeve.ModeChat || info.Chat == nil || info.Chat.State == "" || info.Chat.State == pb.ChatState_CHAT_STATE_IDLE.String() {
			continue
		}
		if _, err := m.get(info.Name, false); err != nil {
			log.Printf("chat %s: recover state: %v", info.Name, err)
		}
	}
}

// start attaches a freshly spawned chat sleeve and opens its driver, so a
// harness that cannot start fails the spawn. fresh moves an earlier sleeve's
// log of the same name aside; otherwise the log continues. hold keeps the
// chat the hub's until giveBack, so no other sender gets in first.
func (m *chatManager) start(ctx context.Context, name string, fresh, hold bool) error {
	s, err := m.get(name, fresh)
	if err != nil {
		return err
	}
	if hold {
		s.takeOver()
	}
	s.opMu.Lock()
	defer s.opMu.Unlock()
	return s.ensureOpen(ctx)
}

// remove closes a sleeve's driver and log before the sleeve is killed.
func (m *chatManager) remove(name string) {
	m.mu.Lock()
	s := m.sleeves[name]
	delete(m.sleeves, name)
	m.mu.Unlock()
	if s == nil {
		return
	}
	// stop the driver first so a long compaction or send releases opMu
	if sess := s.liveSession(); sess != nil {
		s.mu.Lock()
		s.closing = true
		s.mu.Unlock()
		_ = sess.Close()
	}
	s.opMu.Lock()
	defer s.opMu.Unlock()
	s.closeDriver()
	if err := s.log.Close(); err != nil {
		log.Printf("chat %s: close log: %v", name, err)
	}
	if err := s.keys.log.Close(); err != nil {
		log.Printf("chat %s: close request ids: %v", name, err)
	}
}

// shutdown closes every driver and log when the hub stops.
func (m *chatManager) shutdown() {
	m.mu.Lock()
	m.stopped = true
	names := make([]string, 0, len(m.sleeves))
	for n := range m.sleeves {
		names = append(names, n)
	}
	m.mu.Unlock()
	for _, n := range names {
		m.remove(n)
	}
}

// overlay replaces the persisted chat summary with the live one.
func (m *chatManager) overlay(s *pb.Sleeve) {
	if m == nil || s.Chat == nil {
		return
	}
	m.mu.Lock()
	cs := m.sleeves[s.Name]
	m.mu.Unlock()
	if cs != nil {
		s.Chat = cs.status()
	}
}

func (s *chatSleeve) statusLocked() *pb.ChatStatus {
	st := &pb.ChatStatus{
		State: s.state, HarnessSessionId: s.sessionID, Model: s.model, Provider: s.provider, LocalModel: s.local,
		ReasoningEffort: s.effort,
		Settings:        settingsToProto(driver.SelectionState{Requested: s.requested, Effective: driver.Selection{Model: s.model, Effort: s.effort}, Defaults: s.defaults}),
		ContextTokens:   s.context, ContextWindow: s.window, LastSeq: s.log.Last(),
	}
	if !s.cacheExp.IsZero() {
		st.CacheExpiresAt = timestamppb.New(s.cacheExp)
	}
	if !s.activity.IsZero() {
		st.LastActivityAt = timestamppb.New(s.activity)
	}
	return st
}

func (s *chatSleeve) status() *pb.ChatStatus {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.statusLocked()
}

// persistLocked writes the summary into the sleeve record; usage-only
// updates are rate limited.
func (s *chatSleeve) persistLocked(force bool) error {
	now := time.Now()
	if !force && now.Sub(s.lastPersist) < persistEvery {
		return nil
	}
	s.lastPersist = now
	st := s.statusLocked()
	_, err := s.m.h.sleeves.SetChat(s.name, func(c *sleeve.Chat) {
		c.State, c.Model, c.Context, c.Window = st.State.String(), st.Model, st.ContextTokens, st.ContextWindow
		c.Provider, c.LocalModel = st.Provider, st.LocalModel
		c.Effort, c.Requested, c.Defaults = s.effort, s.requested, s.defaults
		c.CacheExpires, c.LastActivity, c.LastSeq = s.cacheExp, s.activity, st.LastSeq
		if c.SessionID == "" && s.used {
			c.SessionID = s.sessionID
		}
	})
	if err != nil && !errors.Is(err, sleeve.ErrNotFound) {
		log.Printf("chat %s: persist status: %v", s.name, err)
	}
	return err
}

func (s *chatSleeve) publishLocked(force bool) {
	now := time.Now()
	if !force && now.Sub(s.lastPublish) < persistEvery {
		return
	}
	s.lastPublish = now
	st := s.statusLocked()
	p, err := structpb.NewStruct(map[string]any{
		"state": st.State.String(), "model": st.Model, "provider": st.Provider, "local_model": st.LocalModel, "context_tokens": float64(st.ContextTokens),
		"reasoning_effort": st.ReasoningEffort,
		"context_window":   float64(st.ContextWindow), "last_seq": float64(st.LastSeq),
	})
	if err == nil {
		s.m.h.emit("chat.state", s.name, p)
	}
}

// computeLocked derives the single state shown: a pending approval outranks a
// running turn, which outranks compaction, which outranks idle.
func (s *chatSleeve) computeLocked() pb.ChatState {
	switch {
	case s.sess == nil:
		return s.state
	case len(s.approvals) > 0:
		return pb.ChatState_CHAT_STATE_AWAITING_APPROVAL
	case s.turn:
		return pb.ChatState_CHAT_STATE_WORKING
	case s.compacting:
		return pb.ChatState_CHAT_STATE_COMPACTING
	}
	return pb.ChatState_CHAT_STATE_IDLE
}

func (s *chatSleeve) refreshLocked() { s.setStateLocked(s.computeLocked()) }

func (s *chatSleeve) setStateLocked(st pb.ChatState) {
	if st == s.state {
		return
	}
	s.state = st
	if _, err := s.log.Append(&pb.ChatEvent{Body: &pb.ChatEvent_State{State: &pb.StateChange{State: st}}}); err != nil {
		log.Printf("chat %s: log state: %v", s.name, err)
	}
	s.persistLocked(true)
	s.publishLocked(true)
}

func (s *chatSleeve) appendLocked(body func(*pb.ChatEvent)) {
	ev := &pb.ChatEvent{}
	body(ev)
	if _, err := s.log.Append(ev); err != nil && !errors.Is(err, chatlog.ErrClosed) {
		log.Printf("chat %s: log event: %v", s.name, err)
	}
	s.activity = time.Now()
}

// ensureOpen starts the driver if it is not running. opMu must be held.
func (s *chatSleeve) ensureOpen(ctx context.Context) error {
	s.mu.Lock()
	if s.sess != nil {
		s.mu.Unlock()
		return nil
	}
	requested, defaults := s.requested, s.defaults
	s.mu.Unlock()
	info, err := s.m.h.sleeves.Get(s.name)
	if err != nil {
		return sleeveErr(err)
	}
	cmd, err := s.m.h.sleeves.Command(s.name)
	if err != nil {
		return sleeveErr(err)
	}
	var resume string
	if info.Chat != nil {
		resume = info.Chat.SessionID
	}
	octx, cancel := context.WithTimeout(ctx, driverOpenTimeout)
	defer cancel()
	sess, err := s.m.open(octx, driver.Harness(info.Harness), info.Workspace, resume, "chat:"+s.name,
		driver.Options{Command: cmd, Autonomous: !info.Supervised, Selection: requested, Defaults: defaults})
	if err != nil {
		s.mu.Lock()
		s.appendLocked(func(e *pb.ChatEvent) {
			e.Body = &pb.ChatEvent_Error{Error: &pb.ChatError{Text: "open harness: " + err.Error()}}
		})
		s.sess = nil
		s.setStateLocked(pb.ChatState_CHAT_STATE_FAILED)
		s.mu.Unlock()
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("open %s in sleeve %s: %w", info.Harness, s.name, err))
	}
	done := make(chan struct{})
	s.mu.Lock()
	s.sess, s.pumpDone, s.closing = sess, done, false
	s.sessionID = sess.SessionID()
	if sess.Caps().Selection {
		sel := sess.Selection()
		s.requested, s.defaults = sel.Requested, sel.Defaults
		s.model, s.effort = sel.Effective.Model, sel.Effective.Effort
	}
	s.appendLocked(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Settings{Settings: s.statusLocked().Settings} })
	s.turn, s.compacting = false, false
	s.approvals = map[string]bool{}
	s.refreshLocked()
	s.persistLocked(true)
	s.mu.Unlock()
	go s.pump(sess, done)
	return nil
}

// closeDriver stops the driver and waits for its events to drain. opMu must
// be held.
func (s *chatSleeve) closeDriver() {
	s.mu.Lock()
	sess, done := s.sess, s.pumpDone
	s.closing = true
	s.mu.Unlock()
	if sess == nil {
		return
	}
	_ = sess.Close()
	<-done
}

func (s *chatSleeve) pump(sess driver.Session, done chan struct{}) {
	defer close(done)
	for ev := range sess.Events() {
		s.pmu.Lock()
		s.handle(sess, ev)
		s.pmu.Unlock()
	}
}

func (s *chatSleeve) handle(sess driver.Session, ev driver.Event) {
	switch ev.Kind {
	case driver.KindText:
		if err := s.log.AppendText(ev.MessageID, ev.Thinking, ev.Text); err != nil && !errors.Is(err, chatlog.ErrClosed) {
			log.Printf("chat %s: log text: %v", s.name, err)
		}
		s.mu.Lock()
		s.activity = time.Now()
		s.mu.Unlock()
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	switch ev.Kind {
	case driver.KindSelection:
		if ev.Selection != nil {
			st := ev.Selection
			s.requested, s.defaults, s.model, s.effort = st.Requested, st.Defaults, st.Effective.Model, st.Effective.Effort
			s.appendLocked(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Settings{Settings: settingsToProto(*st)} })
			s.persistLocked(true)
			s.publishLocked(true)
		}
	case driver.KindToolCall:
		s.appendLocked(func(e *pb.ChatEvent) {
			e.Body = &pb.ChatEvent_ToolCall{ToolCall: &pb.ToolCall{Id: ev.Tool.ID, Name: ev.Tool.Name, InputJson: string(ev.Tool.Input)}}
		})
	case driver.KindToolResult:
		r := ev.ToolResult
		s.appendLocked(func(e *pb.ChatEvent) {
			e.Body = &pb.ChatEvent_ToolResult{ToolResult: &pb.ToolResult{ToolCallId: r.CallID, Output: r.Output, IsError: r.IsError, Truncated: r.Truncated}}
		})
	case driver.KindApproval:
		a := ev.Approval
		s.approvals[a.ID] = true
		s.appendLocked(func(e *pb.ChatEvent) {
			e.Body = &pb.ChatEvent_Approval{Approval: &pb.ApprovalRequest{Id: a.ID, Kind: a.Kind, Summary: a.Summary}}
		})
		s.refreshLocked()
	case driver.KindUsage:
		u := ev.Usage
		if u.Model != "" {
			s.model = u.Model
		}
		if u.Provider != "" {
			s.provider = u.Provider
			s.local = servedLocally(u.BaseURL, s.m.h.nodeHostnames())
		}
		if u.ContextWindow > 0 {
			s.window = uint64(u.ContextWindow)
		}
		s.context = uint64(max(u.ContextTokens, 0))
		s.cacheExp = u.CacheExpires
		out := &pb.UsageUpdate{
			ContextTokens: s.context, ContextWindow: s.window, InputTokens: uint64(max(u.InputTokens, 0)),
			OutputTokens: uint64(max(u.OutputTokens, 0)), CacheReadTokens: uint64(max(u.CacheReadTokens, 0)),
			CacheWriteTokens: uint64(max(u.CacheWriteTokens, 0)),
		}
		if !u.CacheExpires.IsZero() {
			out.CacheExpiresAt = timestamppb.New(u.CacheExpires)
		}
		s.appendLocked(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Usage{Usage: out} })
		s.persistLocked(false)
		s.publishLocked(false)
	case driver.KindCompaction:
		c := ev.Compaction
		s.context = uint64(max(c.TokensAfter, 0))
		s.appendLocked(func(e *pb.ChatEvent) {
			e.Body = &pb.ChatEvent_Compaction{Compaction: &pb.Compaction{
				TokensBefore: uint64(max(c.TokensBefore, 0)), TokensAfter: s.context, Native: c.Native,
			}}
		})
		s.persistLocked(true)
		s.publishLocked(true)
	case driver.KindError:
		s.appendLocked(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Error{Error: &pb.ChatError{Text: ev.Text}} })
	case driver.KindSettled:
		s.appendLocked(func(e *pb.ChatEvent) {
			e.Body = &pb.ChatEvent_Settled{Settled: &pb.TurnSettled{StopReason: ev.StopReason}}
		})
		s.turn = false
		s.approvals = map[string]bool{}
		s.refreshLocked()
		s.persistLocked(true)
		s.publishLocked(true)
	case driver.KindExited:
		intentional := s.closing
		if s.sess == sess {
			s.sess = nil
		}
		s.turn, s.compacting = false, false
		s.approvals = map[string]bool{}
		st := pb.ChatState_CHAT_STATE_EXITED
		if intentional {
			st = pb.ChatState_CHAT_STATE_IDLE
		} else {
			text := fmt.Sprintf("harness exited (code %d)", ev.ExitCode)
			if t := strings.TrimSpace(ev.Text); t != "" {
				text += ": " + t
			}
			s.appendLocked(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Error{Error: &pb.ChatError{Text: text}} })
			if ev.ExitCode != 0 {
				st = pb.ChatState_CHAT_STATE_FAILED
			}
		}
		s.setStateLocked(st)
		s.persistLocked(true)
	}
}

func (s *chatSleeve) liveSession() driver.Session {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.sess
}

func (s *chatSleeve) send(ctx context.Context, from, text, requestID string) (uint64, error) {
	if requestID == "" {
		return s.sendOnce(ctx, from, text, false, false)
	}
	return s.keys.do(ctx, requestID, func() (uint64, error) { return s.sendOnce(ctx, from, text, false, false) })
}

func (s *chatSleeve) sendMailbox(ctx context.Context, from, text, requestID string) (uint64, error) {
	return s.keys.do(ctx, requestID, func() (uint64, error) {
		return s.sendOnce(ctx, from, text, false, true)
	})
}

// sendAsOwner is the hub's own send into a chat it has taken over.
func (s *chatSleeve) sendAsOwner(ctx context.Context, text string) (uint64, error) {
	return s.sendOnce(ctx, hubName, text, true, false)
}

// takeOver claims the chat for a resleeve; false if one already has it.
func (s *chatSleeve) takeOver() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.hubOwned {
		return false
	}
	s.hubOwned = true
	return true
}

func (s *chatSleeve) giveBack() {
	s.mu.Lock()
	s.hubOwned = false
	s.mu.Unlock()
}

var errResleeving = errors.New("the hub is resleeving this sleeve onto another harness; send again once it is done")

func (s *chatSleeve) owned() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.hubOwned
}

func (s *chatSleeve) sendOnce(ctx context.Context, from, text string, owner, idleOnly bool) (uint64, error) {
	s.opMu.Lock()
	defer s.opMu.Unlock()
	if s.owned() && !owner {
		return 0, connect.NewError(connect.CodeFailedPrecondition, errResleeving)
	}
	if st := s.status().State; st == pb.ChatState_CHAT_STATE_COMPACTING {
		return 0, connect.NewError(connect.CodeFailedPrecondition, errors.New("the harness is compacting; retry when it is idle"))
	}
	if err := s.ensureOpen(ctx); err != nil {
		return 0, err
	}
	s.pmu.Lock()
	defer s.pmu.Unlock()
	s.mu.Lock()
	sess, busy := s.sess, s.turn
	s.mu.Unlock()
	if sess == nil {
		return 0, connect.NewError(connect.CodeFailedPrecondition, errors.New("the harness exited; send again to restart it"))
	}
	if idleOnly && s.status().State != pb.ChatState_CHAT_STATE_IDLE {
		return 0, connect.NewError(connect.CodeFailedPrecondition, errors.New("mail delivery requires an idle chat"))
	}
	if busy && !sess.Caps().Steer {
		return 0, connect.NewError(connect.CodeFailedPrecondition, errors.New("a turn is running and this harness cannot take a message during one; interrupt it first"))
	}
	sctx, cancel := context.WithTimeout(ctx, sendTimeout)
	defer cancel()
	if err := sess.Send(sctx, text); err != nil {
		if errors.Is(err, driver.ErrBusy) {
			return 0, connect.NewError(connect.CodeFailedPrecondition, err)
		}
		return 0, connect.NewError(connect.CodeInternal, err)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	seq, err := s.log.Append(&pb.ChatEvent{Body: &pb.ChatEvent_User{User: &pb.UserMessage{Text: text, From: from}}})
	if err != nil {
		return 0, connect.NewError(connect.CodeInternal, err)
	}
	s.activity = time.Now()
	s.used = true
	if !busy {
		s.turn = true
	}
	s.refreshLocked()
	s.persistLocked(true)
	return seq, nil
}

func (s *chatSleeve) interrupt(ctx context.Context) error {
	sess := s.liveSession()
	if sess == nil {
		return nil
	}
	if err := sess.Interrupt(ctx); err != nil {
		return connect.NewError(connect.CodeInternal, err)
	}
	return nil
}

func (s *chatSleeve) respond(ctx context.Context, by, approvalID string, allow bool, note string) error {
	sess := s.liveSession()
	if sess == nil {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("the harness is not running; no approval is pending"))
	}
	s.pmu.Lock()
	defer s.pmu.Unlock()
	if err := sess.Respond(ctx, approvalID, allow, note); err != nil {
		switch {
		case errors.Is(err, driver.ErrUnknownApproval):
			return connect.NewError(connect.CodeNotFound, err)
		case errors.Is(err, driver.ErrUnsupported):
			return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("this harness asks for no approvals: %w", err))
		}
		return connect.NewError(connect.CodeInternal, err)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.appendLocked(func(e *pb.ChatEvent) {
		e.Body = &pb.ChatEvent_ApprovalResolved{ApprovalResolved: &pb.ApprovalResolved{Id: approvalID, Allow: allow, By: by}}
	})
	delete(s.approvals, approvalID)
	s.refreshLocked()
	return nil
}

func (s *chatSleeve) compact(ctx context.Context) (*pb.Compaction, error) {
	s.opMu.Lock()
	defer s.opMu.Unlock()
	if s.owned() {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errResleeving)
	}
	if st := s.status().State; st != pb.ChatState_CHAT_STATE_IDLE {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("compaction runs only while the chat is idle; it is %s", st))
	}
	if err := s.ensureOpen(ctx); err != nil {
		return nil, err
	}
	sess := s.liveSession()
	if sess == nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("the harness exited"))
	}
	s.mu.Lock()
	s.compacting = true
	s.refreshLocked()
	s.mu.Unlock()
	cctx, cancel := context.WithTimeout(ctx, compactTimeout)
	defer cancel()
	c, err := sess.Compact(cctx)
	s.mu.Lock()
	s.compacting = false
	s.refreshLocked()
	s.mu.Unlock()
	switch {
	case err == nil:
		return &pb.Compaction{TokensBefore: uint64(max(c.TokensBefore, 0)), TokensAfter: uint64(max(c.TokensAfter, 0)), Native: c.Native}, nil
	case errors.Is(err, driver.ErrUnsupported):
		return nil, connect.NewError(connect.CodeUnimplemented, fmt.Errorf("native compaction is not supported for this harness: %w", err))
	case errors.Is(err, driver.ErrBusy), errors.Is(err, driver.ErrNothingToCompact):
		return nil, connect.NewError(connect.CodeFailedPrecondition, err)
	}
	return nil, connect.NewError(connect.CodeInternal, err)
}

// sendKeys dedupes Send by request id for a bounded window, like SubmitJob.
type sendKeys struct {
	log *keylog.Log

	mu      sync.Mutex
	entries map[string]*sendEntry
}

// restore adopts the entries a previous hub persisted and records the rest in
// log.
func (k *sendKeys) restore(log *keylog.Log, saved []keylog.Entry) {
	k.log = log
	for _, p := range saved {
		seq, err := strconv.ParseUint(p.Value, 10, 64)
		if err != nil {
			continue
		}
		e := &sendEntry{done: make(chan struct{}), seq: seq, expires: p.Expires}
		close(e.done)
		k.entries[p.Key] = e
	}
}

type sendEntry struct {
	done    chan struct{}
	seq     uint64
	err     error
	expires time.Time
}

func (k *sendKeys) do(ctx context.Context, key string, send func() (uint64, error)) (uint64, error) {
	if err := checkRequestID(key); err != nil {
		return 0, err
	}
	k.mu.Lock()
	now := time.Now()
	for id, e := range k.entries {
		select {
		case <-e.done:
			if !e.expires.After(now) {
				delete(k.entries, id)
			}
		default:
		}
	}
	if e := k.entries[key]; e != nil {
		k.mu.Unlock()
		select {
		case <-e.done:
		case <-ctx.Done():
			return 0, connect.NewError(connect.CodeCanceled, ctx.Err())
		}
		return e.seq, e.err
	}
	if len(k.entries) >= maxRequestIDs {
		k.mu.Unlock()
		return 0, connect.NewError(connect.CodeResourceExhausted, errors.New("too many request ids in use; retry later"))
	}
	e := &sendEntry{done: make(chan struct{})}
	k.entries[key] = e
	k.mu.Unlock()

	seq, err := send()

	k.mu.Lock()
	e.seq, e.err = seq, err
	if err == nil {
		e.expires = time.Now().Add(requestIDWindow)
		if lerr := k.log.Append(keylog.Entry{Key: key, Value: strconv.FormatUint(seq, 10), Expires: e.expires}); lerr != nil {
			log.Printf("chat: persist request id: %v", lerr)
		}
	} else {
		delete(k.entries, key)
	}
	close(e.done)
	k.mu.Unlock()
	return seq, err
}

// chatService is the ChatService handler.
type chatService struct{ h *Hub }

func (c chatService) sleeve(name string) (*chatSleeve, error) {
	if c.h.chat == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("sleeves are not enabled on this hub"))
	}
	return c.h.chat.get(name, false)
}

func actor(ctx context.Context) string {
	if p := auth.PrincipalFrom(ctx); p != nil {
		return p.Actor()
	}
	return "owner"
}

func (c chatService) Watch(ctx context.Context, req *connect.Request[pb.WatchRequest], stream *connect.ServerStream[pb.ChatEvent]) error {
	if err := ownSleeveOnly(ctx, req.Msg.Sleeve); err != nil {
		return err
	}
	s, err := c.sleeve(req.Msg.Sleeve)
	if err != nil {
		return err
	}
	err = s.log.Watch(ctx, req.Msg.AfterSeq, stream.Send)
	switch {
	case errors.Is(err, chatlog.ErrClosed):
		return nil
	case errors.Is(err, chatlog.ErrSlowConsumer):
		return connect.NewError(connect.CodeResourceExhausted, err)
	}
	return err
}

func (c chatService) Send(ctx context.Context, req *connect.Request[pb.SendRequest]) (*connect.Response[pb.SendResponse], error) {
	m := req.Msg
	if strings.TrimSpace(m.Text) == "" {
		return nil, invalid("text is required")
	}
	s, err := c.sleeve(m.Sleeve)
	if err != nil {
		return nil, err
	}
	seq, err := s.send(ctx, actor(ctx), m.Text, m.RequestId)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.SendResponse{Seq: seq}), nil
}

func (c chatService) Interrupt(ctx context.Context, req *connect.Request[pb.InterruptRequest]) (*connect.Response[pb.InterruptResponse], error) {
	s, err := c.sleeve(req.Msg.Sleeve)
	if err != nil {
		return nil, err
	}
	if err := s.interrupt(ctx); err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.InterruptResponse{}), nil
}

func (c chatService) Respond(ctx context.Context, req *connect.Request[pb.RespondRequest]) (*connect.Response[pb.RespondResponse], error) {
	m := req.Msg
	if m.ApprovalId == "" {
		return nil, invalid("approval_id is required")
	}
	s, err := c.sleeve(m.Sleeve)
	if err != nil {
		return nil, err
	}
	if err := s.respond(ctx, actor(ctx), m.ApprovalId, m.Allow, m.Note); err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.RespondResponse{}), nil
}

func (c chatService) Compact(ctx context.Context, req *connect.Request[pb.CompactRequest]) (*connect.Response[pb.CompactResponse], error) {
	s, err := c.sleeve(req.Msg.Sleeve)
	if err != nil {
		return nil, err
	}
	comp, err := s.compact(ctx)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.CompactResponse{Compaction: comp}), nil
}
