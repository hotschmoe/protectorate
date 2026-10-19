package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"slices"
	"sort"
	"strings"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/directory"
	"github.com/hotschmoe/protectorate/internal/mailbox"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	ownerName        = "owner"
	hubName          = "hub"
	orchestratorAddr = "orchestrator"
	projectPrefix    = "project:"
	maxBodyBytes     = 32 << 10
	maxBatch         = 8
)

// sendArgs is one message to store and route.
type sendArgs struct {
	To          string
	Body        string
	Thread      string
	Kind        mailbox.Kind
	AckRequired bool
	RequestID   string
	ReplyTo     string
	// ReplyVia is the address a recipient of a hub notice answers to with
	// msg send; the notice ends with that command.
	ReplyVia string
	// Direct skips address validation: a reply goes back to a sender that may
	// have gone.
	Direct bool
}

// sendResult tells the caller what happened to a message.
type sendResult struct {
	Msg     mailbox.Message
	Dup     bool
	Waiting bool
	Note    string
}

// chatMode classifies a recipient: "chat" for a sleeve that takes injected
// messages, "inbox" for one that only reads its inbox (terminal sleeves, the
// owner), "" for a name that is not a sleeve.
func (c *coordinator) chatMode(name string) string {
	if name == ownerName {
		return "inbox"
	}
	info, err := c.h.sleeves.Get(name)
	switch {
	case err != nil:
		return ""
	case info.Mode == sleeve.ModeChat:
		return "chat"
	}
	return "inbox"
}

// addr resolves an address to its recipients now. project is the id of the
// project addressed, if any. A project nobody works on yet has no recipients.
func (c *coordinator) addr(to, sender string, direct bool) (rcpts []string, project string, err error) {
	to = strings.TrimSpace(to)
	switch {
	case to == "":
		return nil, "", invalid("to is required")
	case direct:
		return []string{to}, "", nil
	case to == orchestratorAddr:
		return []string{sleeve.OrchestratorName}, "", nil
	case to == ownerName:
		return []string{ownerName}, "", nil
	case strings.HasPrefix(to, projectPrefix):
		v, err := c.dir.Get(strings.TrimPrefix(to, projectPrefix))
		if err != nil {
			return nil, "", directoryErr(err)
		}
		return liveOwners(v, sender), v.ID, nil
	}
	if to == sender {
		return nil, "", invalid("a message to yourself goes nowhere")
	}
	if _, err := c.h.sleeves.Get(to); err != nil {
		return nil, "", connect.NewError(connect.CodeNotFound, fmt.Errorf("no sleeve %q; address a sleeve name, project:<name>, orchestrator or owner", to))
	}
	return []string{to}, "", nil
}

func liveOwners(v directory.View, exclude string) []string {
	var out []string
	for _, o := range v.Owners {
		if o.Sleeve != exclude {
			out = append(out, o.Sleeve)
		}
	}
	return out
}

// send stores a message from sender and routes it. It is the one path for
// callers of the API and for the hub's own notices.
func (c *coordinator) send(sender string, a sendArgs) (sendResult, error) {
	if strings.TrimSpace(a.Body) == "" {
		return sendResult{}, invalid("body is required")
	}
	if len(a.Body) > maxBodyBytes {
		return sendResult{}, invalid("body is %d bytes; the limit is %d", len(a.Body), maxBodyBytes)
	}
	if a.RequestID != "" {
		if err := checkRequestID(a.RequestID); err != nil {
			return sendResult{}, err
		}
	}
	if a.Kind == "" {
		a.Kind = mailbox.Note
	}
	if a.Thread != "" && len(c.mail.Thread(a.Thread)) == 0 {
		return sendResult{}, connect.NewError(connect.CodeNotFound, fmt.Errorf("no thread %q", a.Thread))
	}
	rcpts, project, err := c.addr(a.To, sender, a.Direct)
	if err != nil {
		return sendResult{}, err
	}
	var fromProject string
	if ps := c.dir.ProjectsOf(sender); len(ps) > 0 {
		fromProject = ps[0]
	}
	m, created, err := c.mail.Send(mailbox.NewMessage{
		From: sender, To: strings.TrimSpace(a.To), Kind: a.Kind, AckRequired: a.AckRequired, Body: a.Body,
		RequestID: a.RequestID, Thread: a.Thread, ReplyTo: a.ReplyTo,
		FromProject: fromProject, Project: project, Recipients: rcpts, Footer: replyFooter(a.ReplyVia),
	})
	if err != nil {
		return sendResult{}, connect.NewError(connect.CodeInternal, err)
	}
	res := sendResult{Msg: m, Dup: !created, Waiting: len(m.Recipients) == 0, Note: m.NoteID}
	if !created {
		return res, nil
	}
	c.event("msg.sent", m, map[string]any{"recipients": strings.Join(m.Recipients, ",")})
	c.armInboxOnly(m)
	if m.Kind == mailbox.Request && len(m.Recipients) == 0 && project != "" {
		res.Note = c.fileRequest(m, project)
	}
	c.nudge()
	return res, nil
}

// replyFooter is the last line of a hub notice that tells the reader how to
// answer it. A hub notice cannot be replied to, so it points at msg send on
// the notice's thread.
func replyFooter(via string) func(thread string) string {
	if via == "" {
		return nil
	}
	return func(thread string) string {
		return fmt.Sprintf("Reply: protectorate msg send %s --thread %s \"...\"", via, thread)
	}
}

// armInboxOnly starts the ack timer for recipients that cannot be injected.
func (c *coordinator) armInboxOnly(m mailbox.Message) {
	for _, r := range m.Recipients {
		if c.chatMode(r) == "inbox" {
			if err := c.mail.Arm(m.ID, r); err != nil {
				log.Printf("mailbox: arm %s for %s: %v", m.ID, r, err)
			}
		}
	}
}

// fileRequest turns a request to a project with no live sleeve into a note
// tagged with that project, and returns its id.
func (c *coordinator) fileRequest(m mailbox.Message, project string) string {
	if c.h.backlog == nil {
		return ""
	}
	v, err := c.dir.Get(project)
	if err != nil {
		return ""
	}
	body := fmt.Sprintf("%s\n\nFrom %s, thread %s, message %s.\nReply: protectorate msg reply %s \"...\"",
		strings.TrimRight(m.Body, "\n"), m.From, m.Thread, m.ID, m.ID)
	n, err := c.h.fileNote(backlog.NewNote{
		Title:   fmt.Sprintf("%s asks: %s", m.From, firstLine(m.Body, 80)),
		Body:    body,
		Project: v.Display,
		Source:  m.From,
	})
	if err != nil {
		log.Printf("mailbox: file note for %s: %v", m.ID, err)
		return ""
	}
	if err := c.mail.SetNoteID(m.ID, n.ID); err != nil {
		log.Printf("mailbox: record note for %s: %v", m.ID, err)
	}
	return n.ID
}

func firstLine(s string, n int) string {
	s = strings.Join(strings.Fields(strings.SplitN(strings.TrimSpace(s), "\n", 2)[0]), " ")
	if r := []rune(s); len(r) > n {
		s = string(r[:n-1]) + "..."
	}
	return s
}

func (c *coordinator) event(kind string, m mailbox.Message, extra map[string]any) {
	p := map[string]any{"from": m.From, "to": m.To, "thread": m.Thread, "kind": string(m.Kind)}
	for k, v := range extra {
		p[k] = v
	}
	payload, err := structpb.NewStruct(p)
	if err != nil {
		return
	}
	c.h.emit(kind, m.ID, payload)
}

// pass runs one delivery round: resolve waiting project addresses, inject
// what is pending into idle chats, then handle overdue acks.
func (c *coordinator) pass(ctx context.Context) {
	c.passMu.Lock()
	defer c.passMu.Unlock()
	c.resolveWaiting()
	c.deliver(ctx)
	c.timers(ctx)
}

func (c *coordinator) resolveWaiting() {
	for _, m := range c.mail.Unresolved() {
		if m.Project == "" {
			continue
		}
		v, err := c.dir.Get(m.Project)
		if err != nil {
			continue
		}
		rcpts := liveOwners(v, m.From)
		if len(rcpts) == 0 {
			continue
		}
		var armed []string
		for _, r := range rcpts {
			if c.chatMode(r) == "inbox" {
				armed = append(armed, r)
			}
		}
		if err := c.mail.Resolve(m.ID, rcpts, armed); err != nil {
			log.Printf("mailbox: resolve %s: %v", m.ID, err)
		}
	}
}

// idle reports whether name is a running chat sleeve waiting for input.
func (c *coordinator) idle(name string) (*chatSleeve, bool) {
	info, err := c.h.sleeves.Get(name)
	if err != nil || info.State != sleeve.Running || info.Mode != sleeve.ModeChat {
		return nil, false
	}
	s, err := c.h.chat.get(name, false)
	if err != nil {
		return nil, false
	}
	return s, s.status().State == pb.ChatState_CHAT_STATE_IDLE
}

func (c *coordinator) deliver(ctx context.Context) {
	pending := c.mail.Pending()
	names := make([]string, 0, len(pending))
	for n := range pending {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, rcpt := range names {
		msgs := pending[rcpt]
		switch c.chatMode(rcpt) {
		case "inbox":
			for _, m := range msgs {
				if err := c.mail.Arm(m.ID, rcpt); err != nil {
					log.Printf("mailbox: arm %s for %s: %v", m.ID, rcpt, err)
				}
			}
		case "chat":
			if len(msgs) > maxBatch {
				msgs = msgs[:maxBatch]
			}
			c.inject(ctx, rcpt, msgs, false)
		}
	}
}

// inject sends msgs into rcpt's chat as one ordinary user message, if the
// chat is idle, and records the delivery.
func (c *coordinator) inject(ctx context.Context, rcpt string, msgs []mailbox.Message, reminder bool) bool {
	s, ok := c.idle(rcpt)
	if !ok || len(msgs) == 0 {
		return false
	}
	parts := make([]string, len(msgs))
	ids := make([]string, len(msgs))
	for i, m := range msgs {
		parts[i], ids[i] = renderMessage(m, c.projectLabel(m), reminder), m.ID
	}
	rid := "mailbox-" + strings.Join(ids, "-")
	if reminder {
		rid += fmt.Sprintf("-r%d", msgs[0].Deliveries[rcpt].Reminders+1)
	}
	from := "mailbox"
	if len(msgs) == 1 {
		from = chatFrom(msgs[0].From)
	}
	if _, err := s.send(ctx, from, strings.Join(parts, "\n\n---\n\n"), rid); err != nil {
		if connect.CodeOf(err) != connect.CodeFailedPrecondition {
			log.Printf("mailbox: deliver to %s: %v", rcpt, err)
		}
		return false
	}
	for _, m := range msgs {
		var err error
		kind := "msg.delivered"
		if reminder {
			err, kind = c.mail.Remind(m.ID, rcpt), "msg.reminded"
		} else {
			err = c.mail.Deliver(m.ID, rcpt)
		}
		if err != nil {
			log.Printf("mailbox: record delivery of %s to %s: %v", m.ID, rcpt, err)
			continue
		}
		c.event(kind, m, map[string]any{"recipient": rcpt})
	}
	return true
}

func chatFrom(from string) string {
	if from == ownerName || from == hubName {
		return from
	}
	return "sleeve:" + from
}

func (c *coordinator) projectLabel(m mailbox.Message) string {
	if m.Project == "" {
		return ""
	}
	if v, err := c.dir.Get(m.Project); err == nil {
		return v.Display
	}
	return m.Project
}

// timers re-delivers messages whose ack is overdue, and escalates those that
// stayed unacked after the re-delivery.
func (c *coordinator) timers(ctx context.Context) {
	for _, d := range c.mail.Overdue(c.ackTimeout()) {
		m, ok := c.mail.Get(d.ID)
		if !ok {
			continue
		}
		switch d.Action {
		case mailbox.ActRemind:
			switch c.chatMode(d.Rcpt) {
			case "chat":
				c.inject(ctx, d.Rcpt, []mailbox.Message{m}, true)
			case "inbox":
				if err := c.mail.Remind(d.ID, d.Rcpt); err != nil {
					log.Printf("mailbox: remind %s: %v", d.ID, err)
					continue
				}
				c.event("msg.reminded", m, map[string]any{"recipient": d.Rcpt})
			}
		case mailbox.ActEscalate:
			c.escalate(m, d.Rcpt)
		}
	}
}

// escalate tells the orchestrator that a message stayed unacked, if it is
// running, and always announces it as an event.
func (c *coordinator) escalate(m mailbox.Message, rcpt string) {
	if err := c.mail.Escalate(m.ID); err != nil {
		log.Printf("mailbox: escalate %s: %v", m.ID, err)
		return
	}
	c.event("msg.escalated", m, map[string]any{"recipient": rcpt})
	if info, err := c.h.sleeves.Get(sleeve.OrchestratorName); err != nil || info.State != sleeve.Running || rcpt == sleeve.OrchestratorName {
		return
	}
	body := fmt.Sprintf("Message %s (thread %s) from %s to %s asked for an ack and %s has not given one after a reminder.\n\nIt said: %s",
		m.ID, m.Thread, m.From, m.To, rcpt, firstLine(m.Body, 300))
	if _, err := c.send(hubName, sendArgs{
		To: sleeve.OrchestratorName, Direct: true, Body: body, Thread: m.Thread, Kind: mailbox.Notice, RequestID: "escalate-" + m.ID,
		ReplyVia: rcpt,
	}); err != nil {
		log.Printf("mailbox: notify orchestrator of %s: %v", m.ID, err)
	}
}

// renderMessage is what a recipient sees in its chat: a short header, the
// body, and exactly how to reply and ack.
func renderMessage(m mailbox.Message, project string, reminder bool) string {
	word := "message"
	if m.Kind != mailbox.Note {
		word = string(m.Kind)
	}
	who := "sleeve " + m.From
	if m.From == ownerName || m.From == hubName {
		who = m.From
	}
	head := fmt.Sprintf("%s %s from %s, thread %s", word, m.ID, who, m.Thread)
	if project != "" {
		head += ", to project " + project
	}
	if m.AckRequired {
		head += ", ack required"
	}
	if reminder {
		head = "reminder, still unacked: " + head
	}
	var b strings.Builder
	fmt.Fprintf(&b, "[%s]\n%s\n", head, strings.TrimRight(m.Body, "\n"))
	b.WriteByte('\n')
	if m.NoteID != "" {
		fmt.Fprintf(&b, "This request is already filed as note %s for the project. Work it from the note (protectorate backlog show %s); do not add another note for it.\n", m.NoteID, m.NoteID)
	}
	if m.From != hubName {
		fmt.Fprintf(&b, "Reply: protectorate msg reply %s \"...\"\n", m.ID)
	}
	if m.AckRequired || m.From != hubName {
		fmt.Fprintf(&b, "Ack: protectorate msg ack %s\n", m.ID)
	}
	return strings.TrimRight(b.String(), "\n")
}

// messageService is the MessageService handler.
type messageService struct{ c *coordinator }

func identity(ctx context.Context) string { return senderOf(auth.PrincipalFrom(ctx)) }

// actingAs returns the mailbox a call works on: the caller's own, or, for the
// owner only, the one named by as.
func actingAs(ctx context.Context, as string) (string, error) {
	self := identity(ctx)
	switch {
	case as == "" || as == self:
		return self, nil
	case self == ownerName:
		return as, nil
	}
	return "", connect.NewError(connect.CodePermissionDenied, errors.New("only the owner may act as another mailbox"))
}

func mailErr(err error) error {
	switch {
	case errors.Is(err, mailbox.ErrNotFound):
		return connect.NewError(connect.CodeNotFound, err)
	case errors.Is(err, mailbox.ErrNotRecipient):
		return connect.NewError(connect.CodePermissionDenied, err)
	case errors.As(err, new(*connect.Error)):
		return err
	}
	return connect.NewError(connect.CodeInternal, err)
}

var kindToMailbox = map[pb.MessageKind]mailbox.Kind{
	pb.MessageKind_MESSAGE_KIND_UNSPECIFIED: mailbox.Note,
	pb.MessageKind_MESSAGE_KIND_NOTE:        mailbox.Note,
	pb.MessageKind_MESSAGE_KIND_REQUEST:     mailbox.Request,
	pb.MessageKind_MESSAGE_KIND_NOTICE:      mailbox.Notice,
}

func kindToProto(k mailbox.Kind) pb.MessageKind {
	switch k {
	case mailbox.Request:
		return pb.MessageKind_MESSAGE_KIND_REQUEST
	case mailbox.Notice:
		return pb.MessageKind_MESSAGE_KIND_NOTICE
	}
	return pb.MessageKind_MESSAGE_KIND_NOTE
}

func mailToProto(m mailbox.Message) *pb.MailMessage {
	ts := func(t *time.Time) *timestamppb.Timestamp {
		if t == nil {
			return nil
		}
		return timestamppb.New(*t)
	}
	out := &pb.MailMessage{
		Id: m.ID, ThreadId: m.Thread, From: m.From, To: m.To, Kind: kindToProto(m.Kind), AckRequired: m.AckRequired,
		Body: m.Body, ReplyTo: m.ReplyTo, CreatedAt: timestamppb.New(m.Created), EscalatedAt: ts(m.Escalated),
		NoteId: m.NoteID,
	}
	for _, r := range m.Recipients {
		d := m.Deliveries[r]
		out.Deliveries = append(out.Deliveries, &pb.MailDelivery{
			Recipient: r, DeliveredAt: ts(d.Delivered), ReadAt: ts(d.Read), AckedAt: ts(d.Acked), Reminders: uint32(d.Reminders),
		})
	}
	return out
}

func (s messageService) Send(ctx context.Context, req *connect.Request[pb.MessageServiceSendRequest]) (*connect.Response[pb.MessageServiceSendResponse], error) {
	m := req.Msg
	kind, ok := kindToMailbox[m.Kind]
	if !ok {
		return nil, invalid("unknown message kind")
	}
	ack := kind == mailbox.Request
	if m.AckRequired != nil {
		ack = *m.AckRequired
	}
	res, err := s.c.send(identity(ctx), sendArgs{
		To: m.To, Body: m.Body, Thread: m.ThreadId, Kind: kind, AckRequired: ack, RequestID: m.RequestId,
	})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.MessageServiceSendResponse{
		Message: mailToProto(res.Msg), Duplicate: res.Dup, Waiting: res.Waiting, NoteId: res.Note,
	}), nil
}

func (s messageService) Inbox(ctx context.Context, req *connect.Request[pb.InboxRequest]) (*connect.Response[pb.InboxResponse], error) {
	who, err := actingAs(ctx, req.Msg.As)
	if err != nil {
		return nil, err
	}
	msgs, err := s.c.mail.Inbox(who, req.Msg.ThreadId, req.Msg.UnreadOnly, req.Msg.Peek)
	if err != nil {
		return nil, mailErr(err)
	}
	res := &pb.InboxResponse{}
	for _, m := range msgs {
		res.Messages = append(res.Messages, mailToProto(m))
	}
	return connect.NewResponse(res), nil
}

func (s messageService) ack(id, who string) (mailbox.Message, error) {
	m, changed, err := s.c.mail.Ack(id, who)
	if err != nil {
		return mailbox.Message{}, mailErr(err)
	}
	if changed {
		s.c.event("msg.acked", m, map[string]any{"recipient": who})
	}
	return m, nil
}

func (s messageService) Ack(ctx context.Context, req *connect.Request[pb.AckRequest]) (*connect.Response[pb.AckResponse], error) {
	who, err := actingAs(ctx, req.Msg.As)
	if err != nil {
		return nil, err
	}
	m, err := s.ack(req.Msg.Id, who)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.AckResponse{Message: mailToProto(m)}), nil
}

func (s messageService) Reply(ctx context.Context, req *connect.Request[pb.ReplyRequest]) (*connect.Response[pb.ReplyResponse], error) {
	who := identity(ctx)
	orig, ok := s.c.mail.Get(req.Msg.Id)
	switch {
	case !ok:
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("no message %q", req.Msg.Id))
	case orig.Deliveries[who] == nil && who != ownerName:
		return nil, connect.NewError(connect.CodePermissionDenied, fmt.Errorf("%s is not a recipient of %s", who, orig.ID))
	case orig.From == hubName:
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%s is a notice from the hub; ack it instead", orig.ID))
	}
	res, err := s.c.send(who, sendArgs{
		To: orig.From, Direct: true, Body: req.Msg.Body, Thread: orig.Thread, ReplyTo: orig.ID, RequestID: req.Msg.RequestId,
	})
	if err != nil {
		return nil, err
	}
	if orig.Deliveries[who] != nil && !res.Dup {
		if _, err := s.ack(orig.ID, who); err != nil {
			return nil, err
		}
	}
	return connect.NewResponse(&pb.ReplyResponse{Message: mailToProto(res.Msg)}), nil
}

func (s messageService) Thread(_ context.Context, req *connect.Request[pb.ThreadRequest]) (*connect.Response[pb.ThreadResponse], error) {
	msgs := s.c.mail.Thread(req.Msg.ThreadId)
	if len(msgs) == 0 {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("no thread %q", req.Msg.ThreadId))
	}
	res := &pb.ThreadResponse{}
	for _, m := range msgs {
		res.Messages = append(res.Messages, mailToProto(m))
	}
	return connect.NewResponse(res), nil
}

const (
	defaultThreads = 100
	maxThreads     = 500
)

func (s messageService) ListThreads(ctx context.Context, req *connect.Request[pb.ListThreadsRequest]) (*connect.Response[pb.ListThreadsResponse], error) {
	limit := int(req.Msg.Limit)
	switch {
	case limit < 0:
		return nil, invalid("limit must not be negative")
	case limit == 0:
		limit = defaultThreads
	case limit > maxThreads:
		limit = maxThreads
	}
	var project string
	if p := strings.TrimSpace(req.Msg.Project); p != "" {
		v, err := s.c.dir.Get(strings.TrimPrefix(p, projectPrefix))
		if err != nil {
			return nil, directoryErr(err)
		}
		project = v.ID
	}
	sleeveName := strings.TrimSpace(req.Msg.Sleeve)
	res := &pb.ListThreadsResponse{}
	for _, ti := range s.c.mail.Threads(identity(ctx)) {
		if len(res.Threads) == limit {
			break
		}
		if (sleeveName != "" && !slices.Contains(ti.Participants, sleeveName)) || (project != "" && !slices.Contains(ti.Projects, project)) {
			continue
		}
		res.Threads = append(res.Threads, &pb.ThreadSummary{
			ThreadId: ti.ID, Participants: ti.Participants, MessageCount: uint32(ti.Messages), Last: mailToProto(ti.Last),
			Unread: uint32(ti.Unread), OpenAcks: uint32(ti.OpenAcks), Escalated: uint32(ti.Escalated), Projects: ti.Projects,
		})
	}
	return connect.NewResponse(res), nil
}
