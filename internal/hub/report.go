package hub

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"
	"unicode/utf8"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func resultToProto(r *sleeve.Result) *pb.SleeveResult {
	if r == nil {
		return nil
	}
	return &pb.SleeveResult{Status: pb.ReportStatus(pb.ReportStatus_value[r.Status]), Summary: r.Summary,
		Time: timestamppb.New(r.Time), ChatSeq: r.ChatSeq, BriefSeq: r.BriefSeq}
}

func (c chatService) Report(ctx context.Context, req *connect.Request[pb.ReportRequest]) (*connect.Response[pb.ReportResponse], error) {
	self, ok := callerSleeve(ctx)
	if p := auth.PrincipalFrom(ctx); !ok && p != nil && !p.Owner && p.Name == sleeve.OrchestratorTokenName {
		self, ok = sleeve.OrchestratorName, true
	}
	if !ok || (req.Msg.Sleeve != "" && req.Msg.Sleeve != self) {
		return nil, connect.NewError(connect.CodePermissionDenied, errors.New("only a sleeve identity token may report for its own sleeve; the owner cannot report on its behalf"))
	}
	auth.Annotate(ctx, self, "")
	m := req.Msg
	if m.Status < pb.ReportStatus_REPORT_STATUS_COMPLETE || m.Status > pb.ReportStatus_REPORT_STATUS_BLOCKED {
		return nil, invalid("status must be complete, partial or blocked")
	}
	if !utf8.ValidString(m.Summary) || len(m.Summary) > maxBodyBytes {
		return nil, invalid("summary must be UTF-8 and at most %d bytes", maxBodyBytes)
	}
	if c.h.chat == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("sleeves are not enabled on this hub"))
	}
	s, err := c.h.chat.get(self, false, true)
	if err != nil {
		return nil, err
	}
	s.pmu.Lock()
	defer s.pmu.Unlock()
	s.mu.Lock()
	defer s.mu.Unlock()
	r := &pb.SleeveResult{Status: m.Status, Summary: m.Summary, BriefSeq: s.briefSeq}
	if _, err := s.log.Append(&pb.ChatEvent{Body: &pb.ChatEvent_Report{Report: r}}); err != nil {
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("record sleeve report: %w", err))
	}
	s.activity = r.Time.AsTime()
	if err := c.h.sleeves.SetResult(self, &sleeve.Result{Status: r.Status.String(), Summary: r.Summary,
		Time: r.Time.AsTime(), ChatSeq: r.ChatSeq, BriefSeq: r.BriefSeq}); err != nil {
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("save sleeve report: %w", err))
	}
	payload, _ := structpb.NewStruct(map[string]any{"status": strings.ToLower(strings.TrimPrefix(r.Status.String(), "REPORT_STATUS_")),
		"summary": r.Summary, "time": r.Time.AsTime().Format(time.RFC3339Nano), "chat_seq": fmt.Sprint(r.ChatSeq), "brief_seq": fmt.Sprint(r.BriefSeq)})
	c.h.emit("sleeve.report", self, payload)
	return connect.NewResponse(&pb.ReportResponse{Result: r}), nil
}

// Caller holds s.mu, as does the idle reaper.
func (h *Hub) chatIdleLocked(s *chatSleeve) bool {
	return s.state == pb.ChatState_CHAT_STATE_IDLE && !s.hubOwned &&
		(h.coord == nil || len(h.coord.mail.Pending()[s.name]) == 0)
}

func (c chatService) Wait(ctx context.Context, req *connect.Request[pb.WaitRequest]) (*connect.Response[pb.WaitResponse], error) {
	m := req.Msg
	if err := ownSleeveOnly(ctx, m.Sleeve); err != nil {
		return nil, err
	}
	if m.Until == pb.WaitUntil_WAIT_UNTIL_UNSPECIFIED {
		m.Until = pb.WaitUntil_WAIT_UNTIL_IDLE
	}
	if m.Until != pb.WaitUntil_WAIT_UNTIL_IDLE && m.Until != pb.WaitUntil_WAIT_UNTIL_REPORT {
		return nil, invalid("until must be idle or report")
	}
	timeout := 10 * time.Minute
	if m.Timeout != nil {
		if err := m.Timeout.CheckValid(); err != nil || m.Timeout.AsDuration() < 0 {
			return nil, invalid("timeout must be a nonnegative duration")
		}
		timeout = m.Timeout.AsDuration()
	}
	if timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}
	if c.h.chat == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("sleeves are not enabled on this hub"))
	}
	events, unsub := c.h.events.subscribe()
	defer unsub()
	for {
		i, err := c.h.sleeves.Get(m.Sleeve)
		if err != nil {
			return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("wait for sleeve %s: sleeve is absent or was removed: %w", m.Sleeve, err))
		}
		if m.Until == pb.WaitUntil_WAIT_UNTIL_REPORT && i.Result != nil {
			return connect.NewResponse(&pb.WaitResponse{Sleeve: m.Sleeve, Result: resultToProto(i.Result)}), nil
		}
		if i.State == sleeve.Exited {
			return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s exited before reaching %s", m.Sleeve, m.Until))
		}
		if i.Mode == sleeve.ModeChat {
			s, err := c.sleeve(m.Sleeve)
			if err != nil {
				return nil, err
			}
			s.mu.Lock()
			idle, state := c.h.chatIdleLocked(s), s.state
			s.mu.Unlock()
			if m.Until == pb.WaitUntil_WAIT_UNTIL_IDLE && idle {
				return connect.NewResponse(&pb.WaitResponse{Sleeve: m.Sleeve}), nil
			}
			if state == pb.ChatState_CHAT_STATE_EXITED || state == pb.ChatState_CHAT_STATE_FAILED {
				if m.Until == pb.WaitUntil_WAIT_UNTIL_REPORT {
					if latest, err := c.h.sleeves.Get(m.Sleeve); err == nil && latest.Result != nil {
						return connect.NewResponse(&pb.WaitResponse{Sleeve: m.Sleeve, Result: resultToProto(latest.Result)}), nil
					}
				}
				return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s chat %s before reaching %s", m.Sleeve, state, m.Until))
			}
		} else if m.Until == pb.WaitUntil_WAIT_UNTIL_IDLE {
			return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeve %s has no chat; idle requires chat mode", m.Sleeve))
		}
		select {
		case <-ctx.Done():
			code := connect.CodeCanceled
			if errors.Is(ctx.Err(), context.DeadlineExceeded) {
				code = connect.CodeDeadlineExceeded
			}
			return nil, connect.NewError(code, fmt.Errorf("wait for sleeve %s %s: %w", m.Sleeve, m.Until, ctx.Err()))
		case <-events:
		}
	}
}
