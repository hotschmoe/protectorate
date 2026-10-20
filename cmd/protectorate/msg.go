package main

import (
	"fmt"
	"io"
	"strings"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func (o *opts) messageClient() (protectoratev1connect.MessageServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewMessageServiceClient(hc, o.hubURL), nil
}

func msgCmd(o *opts) *cobra.Command {
	c := &cobra.Command{
		Use:     "msg",
		Aliases: []string{"message"},
		Short:   "Send messages between sleeves, projects, the orchestrator and the owner",
		Long: `The hub's mailbox. The sender is whoever the token says (a sleeve's own token
names the sleeve). A message to a sleeve in chat mode appears in its chat as a
user message when the turn in progress ends; a sleeve in terminal mode reads
'msg inbox'. Addresses: a sleeve name, project:<id-or-name>, orchestrator, owner.`,
	}
	c.AddCommand(msgSendCmd(o), msgReplyCmd(o), msgAckCmd(o), msgInboxCmd(o), msgThreadCmd(o))
	return c
}

var messageKinds = map[string]pb.MessageKind{
	"note": pb.MessageKind_MESSAGE_KIND_NOTE, "request": pb.MessageKind_MESSAGE_KIND_REQUEST, "notice": pb.MessageKind_MESSAGE_KIND_NOTICE,
}

func bodyArg(cmd *cobra.Command, args []string) (string, error) {
	text := strings.Join(args, " ")
	if text == "-" {
		b, err := io.ReadAll(cmd.InOrStdin())
		if err != nil {
			return "", err
		}
		text = string(b)
	}
	return text, nil
}

func msgSendCmd(o *opts) *cobra.Command {
	var thread, kind, requestID string
	var ack bool
	c := &cobra.Command{
		Use:   "send <to> <body...>",
		Short: "Send a message (use - as the body to read stdin)",
		Long: `<to> is a sleeve name, project:<id-or-name>, orchestrator or owner. A request
asks for something and asks for an ack by default; a project with no live
sleeve keeps the message and files a backlog note. An ack-required message
not acked within the hub's timeout is re-delivered once, then escalated.`,
		Args: cobra.MinimumNArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			k, ok := messageKinds[kind]
			if !ok {
				return fmt.Errorf("unknown kind %q, want note, request or notice", kind)
			}
			var askAck *bool
			if cmd.Flags().Changed("ack") {
				askAck = &ack
			}
			body, err := bodyArg(cmd, args[1:])
			if err != nil {
				return err
			}
			cl, err := o.messageClient()
			if err != nil {
				return err
			}
			res, err := cl.Send(cmd.Context(), connect.NewRequest(&pb.MessageServiceSendRequest{
				To: args[0], Body: body, ThreadId: thread, Kind: k, AckRequired: askAck, RequestId: requestID,
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				m := res.Msg.Message
				fmt.Fprintf(w, "message\t%s\nthread\t%s\n", m.Id, m.ThreadId)
				if res.Msg.Duplicate {
					fmt.Fprintln(w, "note\tsame request id as an earlier send; nothing was sent again")
				}
				if res.Msg.Waiting {
					fmt.Fprintln(w, "waiting\tno live recipient yet; it is delivered when one appears")
				}
				if res.Msg.NoteId != "" {
					fmt.Fprintf(w, "note\t%s\n", res.Msg.NoteId)
				}
			})
		},
	}
	c.Flags().StringVar(&thread, "thread", "", "continue this thread (default: start a new one)")
	c.Flags().StringVar(&kind, "kind", "note", "note, request or notice")
	c.Flags().BoolVar(&ack, "ack", false, "require an ack (default for requests)")
	c.Flags().StringVar(&requestID, "request-id", "", "8-128 chars; a retry with the same id does not send twice")
	return c
}

func msgReplyCmd(o *opts) *cobra.Command {
	var requestID string
	c := &cobra.Command{
		Use:   "reply <id> <body...>",
		Short: "Answer a message in its thread; this also acks it",
		Args:  cobra.MinimumNArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			body, err := bodyArg(cmd, args[1:])
			if err != nil {
				return err
			}
			cl, err := o.messageClient()
			if err != nil {
				return err
			}
			res, err := cl.Reply(cmd.Context(), connect.NewRequest(&pb.ReplyRequest{Id: args[0], Body: body, RequestId: requestID}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "message\t%s\nthread\t%s\nto\t%s\n", res.Msg.Message.Id, res.Msg.Message.ThreadId, res.Msg.Message.To)
			})
		},
	}
	c.Flags().StringVar(&requestID, "request-id", "", "8-128 chars; a retry with the same id does not send twice")
	return c
}

func msgAckCmd(o *opts) *cobra.Command {
	var as string
	c := &cobra.Command{
		Use:   "ack <id>",
		Short: "Confirm a message",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.messageClient()
			if err != nil {
				return err
			}
			res, err := cl.Ack(cmd.Context(), connect.NewRequest(&pb.AckRequest{Id: args[0], As: as}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "acked\t%s\n", res.Msg.Message.Id) })
		},
	}
	c.Flags().StringVar(&as, "as", "", "owner only: ack on behalf of this recipient")
	return c
}

func msgInboxCmd(o *opts) *cobra.Command {
	var unread, peek bool
	var as string
	c := &cobra.Command{
		Use:   "inbox",
		Short: "List messages addressed to you, oldest first, and mark them read",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			cl, err := o.messageClient()
			if err != nil {
				return err
			}
			res, err := cl.Inbox(cmd.Context(), connect.NewRequest(&pb.InboxRequest{UnreadOnly: unread, Peek: peek, As: as}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeMail(w, res.Msg.Messages) })
		},
	}
	c.Flags().BoolVar(&unread, "unread", false, "only messages not yet delivered, read or acked")
	c.Flags().BoolVar(&peek, "peek", false, "do not mark the messages read")
	c.Flags().StringVar(&as, "as", "", "owner only: read this recipient's mailbox")
	return c
}

func msgThreadCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "thread <thread-id>",
		Short: "Show every message of a thread",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.messageClient()
			if err != nil {
				return err
			}
			res, err := cl.Thread(cmd.Context(), connect.NewRequest(&pb.ThreadRequest{ThreadId: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeMail(w, res.Msg.Messages) })
		},
	}
}

// writeMail prints one block per message: a header line with its state, then
// the body indented.
func writeMail(w io.Writer, msgs []*pb.MailMessage) {
	if len(msgs) == 0 {
		fmt.Fprintln(w, "no messages")
		return
	}
	for _, m := range msgs {
		kind := strings.ToLower(strings.TrimPrefix(m.Kind.String(), "MESSAGE_KIND_"))
		flags := ""
		if m.AckRequired {
			flags += " ack-required"
		}
		for _, d := range m.Deliveries {
			switch {
			case d.AckedAt != nil:
				flags += " acked:" + d.Recipient
			case d.DeliveredAt != nil:
				flags += " delivered:" + d.Recipient
			}
		}
		if m.EscalatedAt != nil {
			flags += " escalated"
		}
		fmt.Fprintf(w, "%s\t%s\t%s -> %s\t%s%s\t%s\n", m.Id, m.ThreadId, m.From, m.To, kind, flags, fmtStamp(m.CreatedAt, "-"))
		for _, line := range strings.Split(strings.TrimRight(m.Body, "\n"), "\n") {
			fmt.Fprintf(w, "\t  %s\n", line)
		}
	}
}
