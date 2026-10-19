package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"path/filepath"
	"strings"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/driver"
	"github.com/hotschmoe/protectorate/internal/handoff"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	defaultResleeveWait = 5 * time.Minute
	// stopWait is how long an interrupted turn gets to settle before the
	// driver is closed under it.
	stopWait       = 30 * time.Second
	summaryTimeout = 4 * time.Minute
	statePoll      = 200 * time.Millisecond
	summaryPrefix  = "sum-"
)

// resleeveTiming can be shortened by tests.
var resleeveTiming = struct{ stop, summary, poll time.Duration }{stopWait, summaryTimeout, statePoll}

func (h *Hub) ResleeveSleeve(ctx context.Context, req *connect.Request[pb.ResleeveSleeveRequest]) (*connect.Response[pb.ResleeveSleeveResponse], error) {
	m := req.Msg
	info, err := h.sleeves.Get(m.Name)
	if err != nil {
		return nil, sleeveErr(err)
	}
	switch {
	case info.Name == sleeve.OrchestratorName:
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("the orchestrator moves harness with `protectorate orchestrator spawn --harness`"))
	case info.Helper != "":
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s is the hub's own (%s)", info.Name, info.Helper))
	case info.Mode != sleeve.ModeChat:
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s runs in terminal mode; resleeve needs a chat sleeve", info.Name))
	case info.State != sleeve.Running:
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s is not running", info.Name))
	}
	to, err := h.resleeveTarget(info.Harness, m.To)
	if err != nil {
		return nil, err
	}
	wait := defaultResleeveWait
	if m.Wait != nil && m.Wait.AsDuration() > 0 {
		wait = m.Wait.AsDuration()
	}
	out, err := h.resleeve(ctx, info, to, m.Interrupt, wait)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.ResleeveSleeveResponse{Sleeve: h.sleeveProto(out)}), nil
}

// resleeveTarget resolves the requested harness; auto takes the best
// recommendation of quota suggest other than the current harness.
func (h *Hub) resleeveTarget(from sleeve.Harness, to pb.Harness) (sleeve.Harness, error) {
	if to != pb.Harness_HARNESS_UNSPECIFIED {
		t := harnessFromProto(to)
		if t == "" || t == sleeve.Shell {
			return "", invalid("resleeve onto claude, codex, grok or pi")
		}
		return t, nil
	}
	for _, s := range RankHarnesses(h.quota.Snapshot(), pb.TaskSize_TASK_SIZE_MEDIUM, h.quota.opts.Now()) {
		if t := harnessFromProto(s.Harness); s.Recommended && t != from {
			return t, nil
		}
	}
	return "", connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("quota suggest recommends no harness other than %s; pass --to", from))
}

// summarizers lists who may write the summary, in order: the best
// subscription with spare quota other than the outgoing harness, then pi
// (the local LLM fallback). A new session, never the outgoing one.
func (h *Hub) summarizers(from sleeve.Harness) []sleeve.Harness {
	var out []sleeve.Harness
	for _, s := range RankHarnesses(h.quota.Snapshot(), pb.TaskSize_TASK_SIZE_SMALL, h.quota.opts.Now()) {
		if t := harnessFromProto(s.Harness); s.Recommended && s.Provider != "local" && t != from {
			out = append(out, t)
			break
		}
	}
	return append(out, sleeve.Pi)
}

func (h *Hub) resleeve(ctx context.Context, info sleeve.Info, to sleeve.Harness, interrupt bool, wait time.Duration) (sleeve.Info, error) {
	cs, err := h.chat.get(info.Name, false)
	if err != nil {
		return sleeve.Info{}, err
	}
	// 1. the hub owns the chat from here: nobody else starts a turn
	if !cs.takeOver() {
		return sleeve.Info{}, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s is already being resleeved", info.Name))
	}
	committed := false
	defer func() {
		if !committed {
			cs.giveBack()
		}
	}()
	h.emitMap("resleeve.started", info.Name, map[string]any{"from": string(info.Harness), "to": string(to)})

	// 3 runs while the outgoing turn winds down, on the facts known now
	sctx, cancelSummary := context.WithCancel(context.WithoutCancel(ctx))
	defer cancelSummary()
	idle := cs.status().State == pb.ChatState_CHAT_STATE_IDLE
	facts := h.handoffFacts(ctx, info, cs, to)
	sumc := make(chan handoff.Summary, 1)
	go func(f handoff.Facts) {
		sumc <- h.summarize(sctx, info, handoff.SummaryPrompt(f, f.Markdown(handoff.DefaultBudget)))
	}(facts)

	if err := cs.quiesce(ctx, interrupt, wait); err != nil {
		return sleeve.Info{}, connect.NewError(connect.CodeCanceled, fmt.Errorf("resleeve %s: %w", info.Name, err))
	}
	// 2. the facts in the file are read after the turn ended
	if !idle {
		facts = h.handoffFacts(ctx, info, cs, to)
	}
	var sum handoff.Summary
	select {
	case sum = <-sumc:
	case <-ctx.Done():
		return sleeve.Info{}, connect.NewError(connect.CodeCanceled, ctx.Err())
	}

	// 4. the handoff; nothing of ours may reach a commit, so exclude first
	if _, err := handoff.Exclude(info.Workspace); err != nil {
		return sleeve.Info{}, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("keep %s out of git: %w", handoff.Dir, err))
	}
	doc := []byte(handoff.Document(facts, sum, handoff.DefaultBudget))
	path, err := handoff.Write(info.Workspace, doc)
	if err != nil {
		return sleeve.Info{}, connect.NewError(connect.CodeFailedPrecondition, err)
	}
	archive, err := handoff.Archive(filepath.Join(h.sleeves.StateDir(), "handoffs"), info.Name, string(info.Harness), string(to), facts.At, doc)
	if err != nil {
		log.Printf("resleeve %s: %v", info.Name, err)
	}

	// 5. replace the harness; from here the request may go away
	committed = true
	bg := context.WithoutCancel(ctx)
	env, err := h.sleeves.Env(info.Name)
	if err != nil {
		log.Printf("resleeve %s: read env: %v", info.Name, err)
	}
	out, err := h.replaceHarness(bg, info, to, env, spawnOpts{keepLog: true, hold: true})
	if err != nil {
		if _, rerr := h.replaceHarness(bg, info, info.Harness, env, spawnOpts{keepLog: true, resume: facts.SessionID}); rerr != nil {
			return sleeve.Info{}, connect.NewError(connect.CodeInternal, fmt.Errorf("start %s: %v; restoring %s also failed: %w", to, err, info.Harness, rerr))
		}
		return sleeve.Info{}, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("start %s: %w; %s is back on its own session", to, err, info.Harness))
	}

	rec := sleeve.Handoff{
		From: string(info.Harness), FromSession: facts.SessionID, To: string(to),
		Summarizer: sum.Harness, SummarizerModel: sum.Model, SummarizerProvider: sum.Provider,
		SummarizerLocal: sum.Local, SummaryError: sum.Err, Path: path, Archive: archive, At: facts.At,
	}
	if err := h.sleeves.SetHandoff(info.Name, rec); err != nil {
		log.Printf("resleeve %s: record handoff: %v", info.Name, err)
	}
	ns, err := h.chat.get(info.Name, false)
	if err == nil {
		ns.note(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Handoff{Handoff: handoffToProto(&rec)} })
	}
	h.announceResleeve(info.Name, rec, sum)

	// 6. the new harness is told like any user would tell it, before the
	// mailbox or anyone else gets a turn in
	if err == nil {
		if _, err := ns.sendAsOwner(bg, handoffPrompt(info.Harness, to)); err != nil {
			log.Printf("resleeve %s: first prompt: %v", info.Name, err)
		}
		ns.giveBack()
	}
	if fresh, err := h.sleeves.Get(info.Name); err == nil {
		out = fresh
	}
	return out, nil
}

func handoffPrompt(from, to sleeve.Harness) string {
	return fmt.Sprintf("Continue the task in this workspace. Read %s first: the hub wrote it when this sleeve moved from %s to %s. "+
		"It has the goal, the state, what is unfinished, what to do next, and facts (git status, recent commits, the end of the previous conversation).",
		handoff.File, from, to)
}

func (h *Hub) announceResleeve(name string, rec sleeve.Handoff, sum handoff.Summary) {
	h.emitMap("sleeve.resleeved", name, map[string]any{
		"from": rec.From, "to": rec.To, "summarizer": rec.Summarizer, "summarizer_model": rec.SummarizerModel,
		"summarizer_local": rec.SummarizerLocal, "summary_error": rec.SummaryError, "handoff": rec.Path, "archive": rec.Archive,
	})
	var warn string
	switch {
	case sum.Text == "":
		warn = "no summary was written (" + sum.Err + "); the handoff holds the facts only"
	case rec.SummarizerLocal:
		warn = fmt.Sprintf("summarized by local model %s %s: no subscription had spare quota", rec.SummarizerProvider, rec.SummarizerModel)
	case rec.Summarizer == string(sleeve.Pi):
		warn = fmt.Sprintf("summarized by the fallback pi (%s %s)", rec.SummarizerProvider, rec.SummarizerModel)
	}
	if warn != "" {
		h.emitMap("resleeve.warning", name, map[string]any{"text": warn, "summarizer_local": rec.SummarizerLocal})
	}
}

func (h *Hub) emitMap(kind, subject string, payload map[string]any) {
	p, err := structpb.NewStruct(payload)
	if err != nil {
		log.Printf("event %s: %v", kind, err)
		return
	}
	h.emit(kind, subject, p)
}

// replaceHarness stops a chat sleeve and starts it again under the same
// name, workspace and limits on harness to, continuing its chat log.
func (h *Hub) replaceHarness(ctx context.Context, info sleeve.Info, to sleeve.Harness, env map[string]string, o spawnOpts) (sleeve.Info, error) {
	h.chat.remove(info.Name)
	if err := h.sleeves.Kill(ctx, info.Name); err != nil && !errors.Is(err, sleeve.ErrNotFound) {
		return sleeve.Info{}, fmt.Errorf("stop %s: %w", info.Harness, err)
	}
	h.dropIdentity(info.Name, "")
	return h.spawn(ctx, sleeve.SpawnRequest{
		Name: info.Name, Workspace: info.Workspace, Harness: to, Mode: sleeve.ModeChat,
		Limits: info.Limits(), Env: env, Supervised: info.Supervised,
	}, o)
}

// quiesce returns once the chat is idle. A turn that runs past wait, or any
// turn with interrupt, is interrupted; one that does not settle within
// stopWait after that is left to the driver's close.
func (s *chatSleeve) quiesce(ctx context.Context, interrupt bool, wait time.Duration) error {
	deadline := time.Now().Add(wait)
	interrupted := false
	for {
		switch s.status().State {
		case pb.ChatState_CHAT_STATE_WORKING, pb.ChatState_CHAT_STATE_AWAITING_APPROVAL, pb.ChatState_CHAT_STATE_COMPACTING:
		default:
			return nil
		}
		now := time.Now()
		switch {
		case !interrupted && (interrupt || now.After(deadline)):
			if err := s.interrupt(ctx); err != nil {
				log.Printf("resleeve %s: interrupt: %v", s.name, err)
			}
			interrupted, deadline = true, now.Add(resleeveTiming.stop)
		case interrupted && now.After(deadline):
			return nil
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(resleeveTiming.poll):
		}
	}
}

// note appends a hub event to the chat log.
func (s *chatSleeve) note(body func(*pb.ChatEvent)) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.appendLocked(body)
}

// handoffFacts reads what the hub knows about the outgoing sleeve.
func (h *Hub) handoffFacts(ctx context.Context, info sleeve.Info, cs *chatSleeve, to sleeve.Harness) handoff.Facts {
	st := cs.status()
	f := handoff.Facts{
		Sleeve: info.Name, Harness: string(info.Harness), SessionID: st.HarnessSessionId, Model: st.Model,
		Provider: st.Provider, Workspace: info.Workspace, To: string(to), At: time.Now(),
	}
	if f.SessionID == "" {
		f.TranscriptErr = "the outgoing harness has no session yet"
	} else if tr, err := handoff.ReadTranscript(h.sleeves.HomeDir(info.Name), f.Harness, f.SessionID); err != nil {
		f.TranscriptErr = err.Error()
	} else {
		f.Transcript = tr
	}
	f.ReadGit(ctx, func(ctx context.Context, dir string, args ...string) (string, error) {
		return h.sleeves.Git(ctx, info.Name, dir, args...)
	})
	f.Notes = handoff.HasNotes(info.Workspace)
	f.Backlog = h.backlogLines(info.Name)
	if h.coord != nil {
		for _, m := range h.coord.mail.Involving(info.Name) {
			f.Mail = append(f.Mail, fmt.Sprintf("%s (%s, thread %s) from %s to %s: %s", m.ID, m.Kind, m.Thread, m.From, m.To, firstLine(m.Body, 200)))
		}
	}
	return f
}

// backlogLines lists the open items the sleeve claimed, then the open items
// of the projects it works on.
func (h *Hub) backlogLines(name string) []string {
	if h.backlog == nil {
		return nil
	}
	items, err := h.backlog.List(backlog.Filter{})
	if err != nil {
		return []string{"backlog unreadable: " + err.Error()}
	}
	projects := map[string]bool{}
	if h.coord != nil {
		for _, p := range h.coord.dir.ProjectsOf(name) {
			projects[p] = true
		}
	}
	var claimed, open []string
	for _, it := range items {
		line := fmt.Sprintf("%s [%s] %s", it.ID, it.State, it.Title)
		switch {
		case it.State == backlog.StateDone:
		case it.Claim != nil && (it.Claim.By == name || it.Claim.By == sleeveTokenName(name)):
			claimed = append(claimed, line+" (claimed by this sleeve)")
		case it.State == backlog.StateTodo && projects[it.Project]:
			open = append(open, line)
		}
	}
	return append(claimed, open...)
}

// summarize has a separate model write the narrative part, trying the
// summarizers in order; the outgoing harness is never asked.
func (h *Hub) summarize(ctx context.Context, owner sleeve.Info, prompt string) handoff.Summary {
	var errs []string
	for _, hs := range h.summarizers(owner.Harness) {
		s, err := h.runSummary(ctx, owner, hs, prompt)
		if err == nil {
			return s
		}
		errs = append(errs, fmt.Sprintf("%s: %v", hs, err))
		if ctx.Err() != nil {
			break
		}
	}
	return handoff.Summary{Err: strings.Join(errs, "; ")}
}

func summaryName(owner string) string {
	n := summaryPrefix + owner
	if len(n) > 40 {
		n = strings.TrimRight(n[:40], "-")
	}
	return n
}

// runSummary asks one harness for the summary in a throwaway sleeve on an
// empty workspace: the harness's own headless protocol in its asking mode,
// with every approval denied, so it answers from the prompt alone.
func (h *Hub) runSummary(ctx context.Context, owner sleeve.Info, hs sleeve.Harness, prompt string) (handoff.Summary, error) {
	name := summaryName(owner.Name)
	ws, err := h.sleeves.ScratchWorkspace(name)
	if err != nil {
		return handoff.Summary{}, err
	}
	if _, err := h.sleeves.Spawn(ctx, sleeve.SpawnRequest{
		Name: name, Workspace: ws, Harness: hs, Mode: sleeve.ModeChat, Limits: owner.Limits(),
		Helper: "handoff summary for " + owner.Name,
	}); err != nil {
		return handoff.Summary{}, err
	}
	defer func() {
		if err := h.sleeves.Kill(context.WithoutCancel(ctx), name); err != nil {
			log.Printf("resleeve %s: remove summary sleeve: %v", owner.Name, err)
		}
	}()
	cmd, err := h.sleeves.Command(name)
	if err != nil {
		return handoff.Summary{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, resleeveTiming.summary)
	defer cancel()
	sess, err := h.chat.open(ctx, driver.Harness(hs), ws, "", "summary:"+owner.Name, driver.Options{Command: cmd})
	if err != nil {
		return handoff.Summary{}, err
	}
	defer sess.Close()
	if err := sess.Send(ctx, prompt); err != nil {
		return handoff.Summary{}, err
	}
	out := handoff.Summary{Harness: string(hs)}
	var baseURL, lastErr, msgID string
	var text strings.Builder
	for {
		select {
		case <-ctx.Done():
			return handoff.Summary{}, fmt.Errorf("no answer within %s", resleeveTiming.summary)
		case ev, ok := <-sess.Events():
			if !ok {
				return handoff.Summary{}, errors.New("the harness exited")
			}
			switch ev.Kind {
			case driver.KindText:
				if ev.Thinking {
					continue
				}
				if ev.MessageID != msgID {
					msgID = ev.MessageID
					text.Reset()
				}
				text.WriteString(ev.Text)
			case driver.KindApproval:
				if err := sess.Respond(ctx, ev.Approval.ID, false, "Do not use tools: answer from the facts in the prompt."); err != nil {
					log.Printf("resleeve %s: deny summarizer tool: %v", owner.Name, err)
				}
			case driver.KindUsage:
				if ev.Usage.Model != "" {
					out.Model = ev.Usage.Model
				}
				if ev.Usage.Provider != "" {
					out.Provider, baseURL = ev.Usage.Provider, ev.Usage.BaseURL
				}
			case driver.KindError:
				lastErr = ev.Text
			case driver.KindSettled:
				out.Text = strings.TrimSpace(text.String())
				if out.Text == "" {
					return handoff.Summary{}, fmt.Errorf("empty answer (%s)", orStr(lastErr, ev.StopReason))
				}
				out.Local = hs == sleeve.Pi && servedLocally(baseURL, h.nodeHostnames())
				return out, nil
			case driver.KindExited:
				return handoff.Summary{}, fmt.Errorf("the harness exited: %s", orStr(lastErr, ev.Text))
			}
		}
	}
}

func orStr(s, d string) string {
	if s == "" {
		return d
	}
	return s
}

// dropHelpers removes hub-internal sleeves a crash left behind.
func (h *Hub) dropHelpers(ctx context.Context) {
	for _, i := range h.sleeves.List() {
		if i.Helper != "" {
			if err := h.sleeves.Kill(ctx, i.Name); err != nil {
				log.Printf("sleeve %s: remove leftover %s: %v", i.Name, i.Helper, err)
			}
		}
	}
}
