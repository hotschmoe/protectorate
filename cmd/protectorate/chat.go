package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"os/signal"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func chatClient(o *opts) (protectoratev1connect.ChatServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewChatServiceClient(hc, o.hubURL), nil
}

func chatCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "chat", Short: "Talk to a sleeve in chat mode"}
	c.AddCommand(chatWatchCmd(o), chatSendCmd(o), chatInterruptCmd(o), chatRespondCmd(o), chatCompactCmd(o))
	return c
}

func chatWatchCmd(o *opts) *cobra.Command {
	var after uint64
	c := &cobra.Command{
		Use:   "watch <sleeve>",
		Short: "Replay a chat from --after, then follow it live",
		Long: `Replays the sleeve's chat events after --after (default: all), then streams new
ones until interrupted. With --json each event is one JSON line. To resume
after a disconnect, pass the last seq you saw; if the last event was assistant
text still being streamed, pass its seq minus one.`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			stream, err := cl.Watch(ctx, connect.NewRequest(&pb.WatchRequest{Sleeve: args[0], AfterSeq: after}))
			if err != nil {
				return err
			}
			defer stream.Close()
			p := o.out(cmd)
			var r chatRenderer
			for stream.Receive() {
				ev := stream.Msg()
				if o.json {
					if err := p.print(ev, nil); err != nil {
						return err
					}
					continue
				}
				r.render(cmd.OutOrStdout(), ev)
			}
			r.flush(cmd.OutOrStdout())
			if err := stream.Err(); err != nil && ctx.Err() == nil {
				return err
			}
			return nil
		},
	}
	c.Flags().Uint64Var(&after, "after", 0, "replay events with seq greater than this")
	return c
}

func chatSendCmd(o *opts) *cobra.Command {
	var requestID string
	c := &cobra.Command{
		Use:   "send <sleeve> <text...>",
		Short: "Send a message (use - as the text to read stdin)",
		Args:  cobra.MinimumNArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			text := strings.Join(args[1:], " ")
			if text == "-" {
				b, err := io.ReadAll(cmd.InOrStdin())
				if err != nil {
					return err
				}
				text = string(b)
			}
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			res, err := cl.Send(cmd.Context(), connect.NewRequest(&pb.SendRequest{Sleeve: args[0], Text: text, RequestId: requestID}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "seq\t%d\n", res.Msg.Seq) })
		},
	}
	c.Flags().StringVar(&requestID, "request-id", "", "8-128 chars; a retry with the same id does not send twice")
	return c
}

func chatInterruptCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "interrupt <sleeve>",
		Short: "Cancel the running turn",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			res, err := cl.Interrupt(cmd.Context(), connect.NewRequest(&pb.InterruptRequest{Sleeve: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintln(w, "interrupted") })
		},
	}
}

func chatRespondCmd(o *opts) *cobra.Command {
	var allow, deny bool
	var note string
	c := &cobra.Command{
		Use:   "respond <sleeve> <approval-id> --allow|--deny",
		Short: "Answer a pending approval",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			if allow == deny {
				return errors.New("pass exactly one of --allow and --deny")
			}
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			res, err := cl.Respond(cmd.Context(), connect.NewRequest(&pb.RespondRequest{
				Sleeve: args[0], ApprovalId: args[1], Allow: allow, Note: note,
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintln(w, "answered") })
		},
	}
	c.Flags().BoolVar(&allow, "allow", false, "allow the action")
	c.Flags().BoolVar(&deny, "deny", false, "deny the action")
	c.Flags().StringVar(&note, "note", "", "reason passed to harnesses that can carry one")
	return c
}

func chatCompactCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "compact <sleeve>",
		Short: "Run the harness's own compaction (only while idle)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			res, err := cl.Compact(cmd.Context(), connect.NewRequest(&pb.CompactRequest{Sleeve: args[0]}))
			if err != nil {
				return err
			}
			c := res.Msg.Compaction
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "compacted\t%d -> %d tokens\n", c.TokensBefore, c.TokensAfter)
			})
		},
	}
}

// chatRenderer prints events as a transcript, joining the deltas of one
// assistant message onto one line.
type chatRenderer struct {
	openText     bool
	openID       string
	openThinking bool
}

func (r *chatRenderer) flush(w io.Writer) {
	if r.openText {
		fmt.Fprintln(w)
		r.openText = false
	}
}

func (r *chatRenderer) render(w io.Writer, ev *pb.ChatEvent) {
	if t := ev.GetText(); t != nil {
		if !r.openText || r.openID != t.MessageId || r.openThinking != t.Thinking {
			r.flush(w)
			label := "assistant"
			if t.Thinking {
				label = "thinking"
			}
			fmt.Fprintf(w, "%s %-9s ", ev.Time.AsTime().Local().Format("15:04:05"), label)
			r.openText, r.openID, r.openThinking = true, t.MessageId, t.Thinking
		}
		fmt.Fprint(w, t.Delta)
		return
	}
	r.flush(w)
	var kind, detail string
	switch b := ev.Body.(type) {
	case *pb.ChatEvent_User:
		kind, detail = "user", fmt.Sprintf("%s: %s", b.User.From, b.User.Text)
	case *pb.ChatEvent_ToolCall:
		kind, detail = "tool", b.ToolCall.Name+" "+oneLine(b.ToolCall.InputJson, 160)
	case *pb.ChatEvent_ToolResult:
		kind, detail = "result", oneLine(b.ToolResult.Output, 160)
		if b.ToolResult.IsError {
			detail = "error: " + detail
		}
	case *pb.ChatEvent_Approval:
		kind, detail = "approval", fmt.Sprintf("%s [%s] %s", b.Approval.Id, b.Approval.Kind, b.Approval.Summary)
	case *pb.ChatEvent_ApprovalResolved:
		kind, detail = "resolved", fmt.Sprintf("%s allow=%v by %s", b.ApprovalResolved.Id, b.ApprovalResolved.Allow, b.ApprovalResolved.By)
	case *pb.ChatEvent_Usage:
		u := b.Usage
		kind, detail = "usage", fmt.Sprintf("context %d/%d, out %d, cache read %d write %d", u.ContextTokens, u.ContextWindow, u.OutputTokens, u.CacheReadTokens, u.CacheWriteTokens)
	case *pb.ChatEvent_Settled:
		kind, detail = "settled", b.Settled.StopReason
	case *pb.ChatEvent_Compaction:
		kind, detail = "compaction", fmt.Sprintf("%d -> %d tokens, native=%v", b.Compaction.TokensBefore, b.Compaction.TokensAfter, b.Compaction.Native)
	case *pb.ChatEvent_Error:
		kind, detail = "error", b.Error.Text
	case *pb.ChatEvent_State:
		kind, detail = "state", strings.ToLower(strings.TrimPrefix(b.State.State.String(), "CHAT_STATE_"))
	default:
		return
	}
	fmt.Fprintf(w, "%s %-9s %s\n", ev.Time.AsTime().Local().Format(time.TimeOnly), kind, detail)
}

func oneLine(s string, n int) string {
	s = strings.Join(strings.Fields(s), " ")
	if len(s) > n {
		s = s[:n] + "..."
	}
	return s
}
