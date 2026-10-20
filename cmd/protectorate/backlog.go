package main

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"runtime"
	"strings"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func backlogClient(o *opts) (protectoratev1connect.BacklogServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewBacklogServiceClient(hc, o.hubURL), nil
}

func backlogCmd(o *opts) *command {
	c := &command{
		Use:   "backlog",
		Short: "Notes kept on the hub, shared with the app and the orchestrator",
		Long: `Notes the owner writes in the app or here and the orchestrator can read and
edit. A note has a title, a markdown body, an optional project tag, a pin and
the name of whoever created it. See docs/backlog.md.`,
	}
	c.AddCommand(backlogLs(o), backlogShow(o), backlogAdd(o), backlogEdit(o), backlogRm(o), backlogPin(o))
	return c
}

func noteTitle(n *pb.Note) string {
	if n.Title != "" {
		return n.Title
	}
	return firstLine(n.Body, 80)
}

func writeNote(w io.Writer, n *pb.Note) {
	pin := "-"
	if n.Pinned {
		pin = "pin"
	}
	project := n.Project
	if project == "" {
		project = "-"
	}
	fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\n", n.Id, pin, n.UpdatedAt.AsTime().Local().Format("2006-01-02 15:04"), project, n.Source, noteTitle(n))
}

func printNote(o *opts, cmd *command, n *pb.Note) error {
	return o.out(cmd).print(n, func(w io.Writer) { writeNote(w, n) })
}

func backlogLs(o *opts) *command {
	var project, query string
	c := &command{
		Use:   "ls",
		Short: "List notes, pinned first and then most recently updated",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			res, err := cl.ListNotes(cmd.Context(), connect.NewRequest(&pb.ListNotesRequest{Project: project, Query: query}))
			if err != nil {
				return err
			}
			p := o.out(cmd)
			if o.json {
				for _, n := range res.Msg.Notes {
					if err := p.print(n, nil); err != nil {
						return err
					}
				}
				return nil
			}
			return p.print(res.Msg, func(w io.Writer) {
				for _, n := range res.Msg.Notes {
					writeNote(w, n)
				}
			})
		},
	}
	c.Flags().StringVar(&project, "project", "", "only notes with this project tag")
	c.Flags().StringVar(&query, "query", "", "only notes whose title or body contains this text")
	return c
}

func backlogShow(o *opts) *command {
	return &command{
		Use:   "show <id>",
		Short: "Print a note with its body",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			res, err := cl.GetNote(cmd.Context(), connect.NewRequest(&pb.GetNoteRequest{Id: args[0]}))
			if err != nil {
				return err
			}
			n := res.Msg.Note
			return o.out(cmd).print(n, func(w io.Writer) {
				fmt.Fprintf(w, "id\t%s\n", n.Id)
				fmt.Fprintf(w, "title\t%s\n", n.Title)
				if n.Project != "" {
					fmt.Fprintf(w, "project\t%s\n", n.Project)
				}
				fmt.Fprintf(w, "source\t%s\n", n.Source)
				fmt.Fprintf(w, "pinned\t%t\n", n.Pinned)
				fmt.Fprintf(w, "created\t%s\n", n.CreatedAt.AsTime().Local().Format("2006-01-02 15:04"))
				fmt.Fprintf(w, "updated\t%s\n", n.UpdatedAt.AsTime().Local().Format("2006-01-02 15:04"))
				if n.Body != "" {
					fmt.Fprintf(w, "\n%s\n", strings.ReplaceAll(strings.TrimRight(n.Body, "\n"), "\t", "    "))
				}
			})
		},
	}
}

// bodyFlags are the ways to give a note its body without an editor.
type bodyFlags struct {
	body, file string
}

func (b *bodyFlags) register(c *command) {
	c.Flags().StringVar(&b.body, "body", "", "markdown body")
	c.Flags().StringVar(&b.file, "body-file", "", "read the body from this file, or - for stdin")
}

func (b *bodyFlags) read(cmd *command) (string, bool, error) {
	switch {
	case b.body != "" && b.file != "":
		return "", false, fmt.Errorf("use --body or --body-file, not both")
	case b.file == "-":
		raw, err := io.ReadAll(cmd.InOrStdin())
		return string(raw), true, err
	case b.file != "":
		raw, err := os.ReadFile(b.file)
		if err != nil {
			return "", false, fmt.Errorf("read body: %w", err)
		}
		return string(raw), true, nil
	}
	return b.body, cmd.Flags().Changed("body"), nil
}

func backlogAdd(o *opts) *command {
	var project string
	var pin bool
	var bf bodyFlags
	c := &command{
		Use:   "add [title]",
		Short: "Add a note",
		Args:  maximumArgs(1),
		RunE: func(cmd *command, args []string) error {
			req := &pb.CreateNoteRequest{Project: project, Pinned: pin}
			if len(args) == 1 {
				req.Title = args[0]
			}
			body, _, err := bf.read(cmd)
			if err != nil {
				return err
			}
			req.Body = body
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			res, err := cl.CreateNote(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			return printNote(o, cmd, res.Msg.Note)
		},
	}
	c.Flags().StringVar(&project, "project", "", "project tag, e.g. hotschmoe/protectorate")
	c.Flags().BoolVar(&pin, "pin", false, "pin the note")
	bf.register(c)
	return c
}

func backlogEdit(o *opts) *command {
	var title, project, appendText string
	var bf bodyFlags
	c := &command{
		Use:   "edit <id>",
		Short: "Change a note: with flags directly, otherwise in $EDITOR",
		Long: `With --title, --body, --body-file, --append or --project the note is changed
directly. With none of them the note opens in $EDITOR as the title, a blank
line and the body. --project "" clears the tag; --append adds text after the
current body.`,
		Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			req := &pb.UpdateNoteRequest{Id: args[0]}
			body, hasBody, err := bf.read(cmd)
			if err != nil {
				return err
			}
			f := cmd.Flags()
			hasAppend := f.Changed("append")
			if hasBody && hasAppend {
				return fmt.Errorf("use --body or --append, not both")
			}
			direct := hasBody || hasAppend || f.Changed("title") || f.Changed("project")
			switch {
			case direct:
				if f.Changed("title") {
					req.Title = &title
				}
				if f.Changed("project") {
					req.Project = &project
				}
				if hasBody {
					req.Body = &body
				}
				if hasAppend {
					cur, err := cl.GetNote(cmd.Context(), connect.NewRequest(&pb.GetNoteRequest{Id: args[0]}))
					if err != nil {
						return err
					}
					joined := strings.TrimRight(cur.Msg.Note.Body, "\n")
					if joined != "" {
						joined += "\n"
					}
					joined += appendText + "\n"
					req.Body = &joined
				}
			case o.json:
				return fmt.Errorf("edit --json needs --title, --body, --body-file, --append or --project")
			default:
				cur, err := cl.GetNote(cmd.Context(), connect.NewRequest(&pb.GetNoteRequest{Id: args[0]}))
				if err != nil {
					return err
				}
				t, b, err := editInEditor(cmd, cur.Msg.Note)
				if err != nil {
					return err
				}
				req.Title, req.Body = &t, &b
			}
			res, err := cl.UpdateNote(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			return printNote(o, cmd, res.Msg.Note)
		},
	}
	c.Flags().StringVar(&title, "title", "", "new title")
	c.Flags().StringVar(&project, "project", "", "new project tag; empty clears it")
	c.Flags().StringVar(&appendText, "append", "", "text to add after the current body")
	bf.register(c)
	return c
}

func editInEditor(cmd *command, n *pb.Note) (string, string, error) {
	argv := strings.Fields(os.Getenv("EDITOR"))
	if len(argv) == 0 {
		argv = []string{"vi"}
		if runtime.GOOS == "windows" {
			argv = []string{"notepad"}
		}
	}
	f, err := os.CreateTemp("", "protectorate-note-*.md")
	if err != nil {
		return "", "", fmt.Errorf("create temp file: %w", err)
	}
	defer os.Remove(f.Name())
	if _, err := f.WriteString(n.Title + "\n\n" + n.Body); err != nil {
		_ = f.Close()
		return "", "", fmt.Errorf("write temp file: %w", err)
	}
	if err := f.Close(); err != nil {
		return "", "", fmt.Errorf("write temp file: %w", err)
	}
	ed := exec.CommandContext(cmd.Context(), argv[0], append(argv[1:], f.Name())...)
	ed.Stdin, ed.Stdout, ed.Stderr = os.Stdin, os.Stdout, os.Stderr
	if err := ed.Run(); err != nil {
		return "", "", fmt.Errorf("run editor %q: %w", argv[0], err)
	}
	raw, err := os.ReadFile(f.Name())
	if err != nil {
		return "", "", fmt.Errorf("read edited note: %w", err)
	}
	title, body, _ := strings.Cut(strings.ReplaceAll(string(raw), "\r\n", "\n"), "\n")
	return strings.TrimSpace(title), strings.TrimPrefix(body, "\n"), nil
}

func backlogRm(o *opts) *command {
	return &command{
		Use:   "rm <id>",
		Short: "Delete a note for good",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			res, err := cl.DeleteNote(cmd.Context(), connect.NewRequest(&pb.DeleteNoteRequest{Id: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "deleted\t%s\n", args[0]) })
		},
	}
}

func backlogPin(o *opts) *command {
	var off bool
	c := &command{
		Use:   "pin <id>",
		Short: "Pin a note to the top of the list (--off to unpin)",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := backlogClient(o)
			if err != nil {
				return err
			}
			pinned := !off
			res, err := cl.UpdateNote(cmd.Context(), connect.NewRequest(&pb.UpdateNoteRequest{Id: args[0], Pinned: &pinned}))
			if err != nil {
				return err
			}
			return printNote(o, cmd, res.Msg.Note)
		},
	}
	c.Flags().BoolVar(&off, "off", false, "unpin instead")
	return c
}

func firstLine(s string, n int) string {
	s = strings.Join(strings.Fields(strings.SplitN(strings.TrimSpace(s), "\n", 2)[0]), " ")
	if r := []rune(s); len(r) > n {
		s = string(r[:n-1]) + "..."
	}
	return s
}
