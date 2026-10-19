package main

import (
	"fmt"
	"io"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// deskStatusLabel is the one-word state of a node's desktop for status lines.
func deskStatusLabel(s *pb.DeskState, holder string, hasPolicy bool) string {
	if holder != "" {
		return "in use by " + strings.TrimPrefix(holder, "auto:")
	}
	if s == nil {
		return "-"
	}
	switch s.Reason {
	case pb.DeskReason_DESK_REASON_AVAILABLE:
		if hasPolicy {
			return "away"
		}
		return "available"
	case pb.DeskReason_DESK_REASON_OUTSIDE_WINDOW:
		return "outside window"
	case pb.DeskReason_DESK_REASON_USER_ACTIVE:
		return "user active"
	case pb.DeskReason_DESK_REASON_NOT_INTERACTIVE:
		return "unavailable: not an interactive session"
	case pb.DeskReason_DESK_REASON_LOCKED:
		return "unavailable: screen locked"
	case pb.DeskReason_DESK_REASON_UNSUPPORTED:
		return "-"
	}
	return "unknown"
}

// deskInputsLabel is "42 by sleeve api, 3 by owner" for today's desk inputs.
func deskInputsLabel(u []*pb.DeskUse) string {
	parts := make([]string, 0, len(u))
	for _, e := range u {
		parts = append(parts, fmt.Sprintf("%d by %s", e.Count, e.Caller))
	}
	return strings.Join(parts, ", ")
}

func deskInputsTotal(u []*pb.DeskUse) int32 {
	var n int32
	for _, e := range u {
		n += e.Count
	}
	return n
}

func writeDeskPolicy(w io.Writer, name string, r *pb.DeskPolicyStatus) {
	fmt.Fprintf(w, "node\t%s\n", name)
	if p := r.Policy; p != nil {
		tz := p.Timezone
		if tz == "" {
			tz = "the node's local time"
		}
		fmt.Fprintf(w, "policy\twindow %s in %s, idle %s, lock after %t\n", p.Window, tz, p.Idle.AsDuration(), p.LockAfter)
	} else {
		fmt.Fprintf(w, "policy\tnone: the desktop is always available\n")
	}
	fmt.Fprintf(w, "status\t%s\n", deskStatusLabel(r.State, r.Holder, r.Policy != nil))
	if r.State != nil && r.State.Detail != "" {
		fmt.Fprintf(w, "detail\t%s\n", r.State.Detail)
	}
	if len(r.Inputs) > 0 {
		fmt.Fprintf(w, "desk inputs today\t%s\n", deskInputsLabel(r.Inputs))
	}
	if t := r.State.GetLastInput(); t != nil {
		fmt.Fprintf(w, "last input\t%s ago\n", time.Since(t.AsTime()).Round(time.Second))
	}
}

func deskPolicyCmd(o *opts) *cobra.Command {
	var window, tz string
	var idle time.Duration
	var lockAfter, off, allowFamily bool
	c := &cobra.Command{
		Use:   "policy <node> [--window HH:MM-HH:MM --idle DUR [--tz ZONE] [--lock-after] | --off]",
		Short: "Set or show when agents may drive a node's desktop",
		Long: `Without flags, shows the node's desk policy and whether its desktop is
available now. With --window and --idle, sets the policy (owner token only):
agents get the desktop only inside the window and after the user has had no
physical keyboard or mouse input for the idle period. Input injected by desk
commands does not count as the user. --off removes the policy: the desktop is
always available again, which is also the default.

  protectorate desk policy office-pc --window 19:00-07:00 --idle 15m --lock-after

--tz is an IANA zone such as Europe/Berlin; the default is the node's own
local time. --lock-after locks the workstation when the window ends and when
an agent's lease ends inside the window with none following within a minute,
and never while the user is active.`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			fl := cmd.Flags()
			setting := fl.Changed("window") || fl.Changed("idle") || fl.Changed("tz") || fl.Changed("lock-after")
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			var res *pb.DeskPolicyStatus
			switch {
			case off && setting:
				return fmt.Errorf("--off cannot be combined with policy flags")
			case off:
				r, err := cl.SetDeskPolicy(cmd.Context(), connect.NewRequest(&pb.SetDeskPolicyRequest{Name: args[0], AllowFamily: allowFamily}))
				if err != nil {
					return err
				}
				res = r.Msg
			case setting:
				if window == "" || idle <= 0 {
					return fmt.Errorf("setting a policy needs both --window HH:MM-HH:MM and --idle DURATION (for example 15m)")
				}
				p := &pb.DeskPolicy{Window: window, Idle: durationpb.New(idle), Timezone: tz, LockAfter: lockAfter}
				r, err := cl.SetDeskPolicy(cmd.Context(), connect.NewRequest(&pb.SetDeskPolicyRequest{Name: args[0], AllowFamily: allowFamily, Policy: p}))
				if err != nil {
					return err
				}
				res = r.Msg
			default:
				r, err := cl.GetDeskPolicy(cmd.Context(), connect.NewRequest(&pb.GetDeskPolicyRequest{Name: args[0], AllowFamily: allowFamily}))
				if err != nil {
					return err
				}
				res = r.Msg
			}
			return o.out(cmd).print(res, func(w io.Writer) { writeDeskPolicy(w, args[0], res) })
		},
	}
	f := c.Flags()
	f.StringVar(&window, "window", "", "hours agents may drive the desktop, HH:MM-HH:MM; start after end spans midnight")
	f.DurationVar(&idle, "idle", 0, "required time without physical input, e.g. 15m")
	f.StringVar(&tz, "tz", "", "IANA time zone of the window (default: the node's local time)")
	f.BoolVar(&lockAfter, "lock-after", false, "lock the workstation when the window or an agent lease ends")
	f.BoolVar(&off, "off", false, "remove the policy: the desktop is always available")
	f.BoolVar(&allowFamily, "allow-family", false, "allow nodes of tier family")
	return c
}
