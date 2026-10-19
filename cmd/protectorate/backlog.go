package main

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/hub"
)

var stateNames = map[string]pb.ItemState{
	"todo":    pb.ItemState_ITEM_STATE_TODO,
	"claimed": pb.ItemState_ITEM_STATE_CLAIMED,
	"done":    pb.ItemState_ITEM_STATE_DONE,
}

func stateLabel(s pb.ItemState) string {
	return strings.ToLower(strings.TrimPrefix(s.String(), "ITEM_STATE_"))
}

func backlogClient(o *opts) (protectoratev1connect.BacklogServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewBacklogServiceClient(hc, o.hubURL), nil
}

func backlogCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "backlog", Short: "Low-priority work for spare capacity"}
	c.AddCommand(backlogLs(o), backlogAdd(o), backlogClaim(o), backlogDone(o), backlogEdit(o))
	return c
}

func writeItem(w io.Writer, it *pb.Item) {
	title := it.Title
	if title == "" {
		title = it.IssueUrl
	}
	prio, claim := "-", ""
	if it.Priority > 0 {
		prio = fmt.Sprintf("p%d", it.Priority)
	}
	if it.Claim != nil {
		claim = it.Claim.By
		if it.Claim.At != nil {
			claim += " " + it.Claim.At.AsTime().Local().Format("2006-01-02 15:04")
		}
	}
	harness := it.Harness
	if harness == "" {
		harness = "any"
	}
	fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\t%s\n", it.Id, stateLabel(it.State), prio, harness, it.Project, title, claim)
}

func backlogLs(o *opts) *cobra.Command {
	var project, state, harness string
	c := &cobra.Command{
		Use:   "ls",
		Short: "List backlog items",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			req := &pb.ListItemsRequest{Project: project, Harness: harness}
			if state != "" {
				st, ok := stateNames[state]
				if !ok {
					return fmt.Errorf("state must be todo, claimed or done")
				}
				req.State = st
			}
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			res, err := cl.ListItems(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			p := o.out(cmd)
			if o.json {
				for _, it := range res.Msg.Items {
					if err := p.print(it, nil); err != nil {
						return err
					}
				}
				return nil
			}
			return p.print(res.Msg, func(w io.Writer) {
				for _, it := range res.Msg.Items {
					writeItem(w, it)
				}
			})
		},
	}
	c.Flags().StringVar(&project, "project", "", "only this project heading")
	c.Flags().StringVar(&state, "state", "", "todo, claimed or done")
	c.Flags().StringVar(&harness, "harness", "", "items this harness may run (its own plus any)")
	return c
}

func backlogAdd(o *opts) *cobra.Command {
	var issue, harness string
	var prio int
	c := &cobra.Command{
		Use:   "add <project> <title>",
		Short: "Add an item; prefix the project with 'donate: ' for public repos",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			res, err := cl.AddItem(cmd.Context(), connect.NewRequest(&pb.AddItemRequest{
				Project: args[0], Title: args[1], IssueUrl: issue,
				Priority: int32(prio), Harness: harness,
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg.Item, func(w io.Writer) { writeItem(w, res.Msg.Item) })
		},
	}
	c.Flags().StringVar(&issue, "issue", "", "GitHub issue or PR URL")
	c.Flags().IntVar(&prio, "prio", 0, "priority 1 (highest) to 9")
	c.Flags().StringVar(&harness, "harness", "", "preferred harness (claude, codex, grok, pi, any)")
	return c
}

func updateItem(o *opts, cmd *cobra.Command, id string, st pb.ItemState, by string) error {
	cl, err := backlogClient(o)
	if err != nil {
		return err
	}
	req := &pb.UpdateItemRequest{Id: id, State: st}
	if by != "" {
		req.Claim = &pb.Claim{By: by}
	}
	res, err := cl.UpdateItem(cmd.Context(), connect.NewRequest(req))
	if err != nil {
		return err
	}
	return o.out(cmd).print(res.Msg.Item, func(w io.Writer) { writeItem(w, res.Msg.Item) })
}

func backlogClaim(o *opts) *cobra.Command {
	var by string
	c := &cobra.Command{
		Use:   "claim <id> --by <sleeve[/harness]>",
		Short: "Mark an item as taken; fails if someone else holds it",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return updateItem(o, cmd, args[0], pb.ItemState_ITEM_STATE_CLAIMED, by)
		},
	}
	c.Flags().StringVar(&by, "by", "", "claimant, e.g. alice/codex")
	_ = c.MarkFlagRequired("by")
	return c
}

func backlogDone(o *opts) *cobra.Command {
	var by string
	c := &cobra.Command{
		Use:   "done <id>",
		Short: "Check an item off",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return updateItem(o, cmd, args[0], pb.ItemState_ITEM_STATE_DONE, by)
		},
	}
	c.Flags().StringVar(&by, "by", "", "record who did it when the item was never claimed")
	return c
}

func backlogEdit(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "edit",
		Short: "Open the backlog file in $EDITOR (run on the hub machine)",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			path := filepath.Join(home, "backlog.md")
			if o.json {
				_, err := fmt.Fprintf(cmd.OutOrStdout(), `{"path":%q}`+"\n", path)
				return err
			}
			argv := strings.Fields(os.Getenv("EDITOR"))
			if len(argv) == 0 {
				argv = []string{"vi"}
				if runtime.GOOS == "windows" {
					argv = []string{"notepad"}
				}
			}
			if err := os.MkdirAll(home, 0o700); err != nil {
				return err
			}
			ed := exec.CommandContext(cmd.Context(), argv[0], append(argv[1:], path)...)
			ed.Stdin, ed.Stdout, ed.Stderr = os.Stdin, os.Stdout, os.Stderr
			if err := ed.Run(); err != nil {
				return fmt.Errorf("run editor %q: %w", argv[0], err)
			}
			return nil
		},
	}
}
