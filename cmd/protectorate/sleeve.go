package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// attachSleeves wires SleeveService into h and recovers persisted sleeves.
func attachSleeves(ctx context.Context, h *hub.Hub, toolDirs []string) error {
	home, err := hub.Home()
	if err != nil {
		return err
	}
	if len(toolDirs) == 0 {
		toolDirs = nil
	}
	cfg, err := sleeve.DefaultConfig(home, toolDirs)
	if err != nil {
		return err
	}
	if exe, err := os.Executable(); err == nil {
		if err := cfg.AddBinaryDir(exe); err != nil {
			log.Printf("sleeves will not have the protectorate CLI: %v", err)
		}
	}
	m, err := sleeve.Open(ctx, h.SleeveOptions(cfg))
	if err != nil {
		return fmt.Errorf("open sleeves: %w", err)
	}
	h.SetSleeves(m)
	return nil
}

func sleeveClient(o *opts) (protectoratev1connect.SleeveServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewSleeveServiceClient(hc, o.hubURL), nil
}

func sleeveCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "sleeve", Aliases: []string{"sleeves"}, Short: "Run harnesses in isolated workspaces"}
	c.AddCommand(sleeveSpawnCmd(o), sleeveLsCmd(o), sleeveShowCmd(o), sleeveKillCmd(o), sleeveAttachCmd(o))
	return c
}

var harnessNames = map[string]pb.Harness{
	"claude": pb.Harness_HARNESS_CLAUDE, "codex": pb.Harness_HARNESS_CODEX, "grok": pb.Harness_HARNESS_GROK,
	"pi": pb.Harness_HARNESS_PI, "shell": pb.Harness_HARNESS_SHELL,
}

func harnessName(h pb.Harness) string {
	for k, v := range harnessNames {
		if v == h {
			return k
		}
	}
	return "?"
}

// parseSize reads 512, 512M, 8G, 8GiB (powers of 1024).
func parseSize(s string) (uint64, error) {
	t := strings.ToUpper(strings.TrimSpace(s))
	t = strings.TrimSuffix(strings.TrimSuffix(t, "B"), "I")
	mult := uint64(1)
	for i, suf := range []string{"K", "M", "G", "T"} {
		if strings.HasSuffix(t, suf) {
			mult, t = 1<<(10*uint(i+1)), strings.TrimSuffix(t, suf)
			break
		}
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(t), 64)
	if err != nil || f < 0 {
		return 0, fmt.Errorf("invalid size %q, want e.g. 512M or 8G", s)
	}
	return uint64(f * float64(mult)), nil
}

func sleeveSpawnCmd(o *opts) *cobra.Command {
	var workspace, harness, mem string
	var cpu uint32
	var env []string
	c := &cobra.Command{
		Use:   "spawn <name> --workspace <dir|git url> --harness <h> [--cpu 200] [--mem 8G] [-- args...]",
		Short: "Start a harness in an isolated sleeve",
		Long: `Start a harness (claude, codex, grok, pi or shell) in a sandbox with its own
process namespace and CPU/memory limits. The workspace is a directory on the
hub or a git URL to clone. --cpu is a percentage of one core (200 = two cores).
For the shell harness, arguments after -- are the command to run.`,
		Args: cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name, extra := args[0], args[1:]
			if dash := cmd.ArgsLenAtDash(); dash >= 0 && dash != 1 {
				return errors.New("usage: sleeve spawn <name> [flags] -- <args...>")
			} else if dash < 0 && len(args) > 1 {
				return errors.New("separate harness arguments from the name with --")
			}
			h, ok := harnessNames[harness]
			if !ok {
				return fmt.Errorf("unknown harness %q, want claude, codex, grok, pi or shell", harness)
			}
			if workspace == "" {
				return errors.New("--workspace is required")
			}
			if !strings.Contains(workspace, "://") && !strings.Contains(workspace, "@") && !filepath.IsAbs(workspace) {
				abs, err := filepath.Abs(workspace)
				if err != nil {
					return err
				}
				workspace = abs
			}
			lim := &pb.SleeveLimits{CpuPercent: cpu}
			if mem != "" {
				n, err := parseSize(mem)
				if err != nil {
					return err
				}
				lim.MemoryBytes = n
			}
			envMap := map[string]string{}
			for _, kv := range env {
				k, v, ok := strings.Cut(kv, "=")
				if !ok || k == "" {
					return fmt.Errorf("invalid --env %q, want K=V", kv)
				}
				envMap[k] = v
			}
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.SpawnSleeve(cmd.Context(), connect.NewRequest(&pb.SpawnSleeveRequest{
				Name: name, Workspace: workspace, Harness: h, Args: extra, Limits: lim, Env: envMap,
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "spawned\t%s\nattach\tprotectorate sleeve attach %s\n", res.Msg.Sleeve.Name, res.Msg.Sleeve.Name)
			})
		},
	}
	c.Flags().StringVar(&workspace, "workspace", "", "directory on the hub, or git URL to clone")
	c.Flags().StringVar(&harness, "harness", "", "claude, codex, grok, pi or shell")
	c.Flags().Uint32Var(&cpu, "cpu", 0, "CPU limit in percent of one core (0 = unlimited)")
	c.Flags().StringVar(&mem, "mem", "", "memory limit such as 512M or 8G (empty = unlimited)")
	c.Flags().StringArrayVar(&env, "env", nil, "environment variable K=V (repeatable)")
	return c
}

func sleeveStateLabel(s pb.SleeveState) string {
	if s == pb.SleeveState_SLEEVE_STATE_RUNNING {
		return "running"
	}
	return "exited"
}

// exitLabel describes how an exited sleeve ended, e.g. "exit 3",
// "killed by signal 11" or "exit 137 (OOM killed)".
func exitLabel(s *pb.Sleeve) string {
	var l string
	switch {
	case s.ExitCode == nil:
		return "unknown"
	case s.ExitSignal > 0:
		l = fmt.Sprintf("killed by signal %d (exit %d)", s.ExitSignal, *s.ExitCode)
	default:
		l = fmt.Sprintf("exit %d", *s.ExitCode)
	}
	if s.OomKilled {
		l += " (OOM killed)"
	}
	return l
}

func sleeveRowState(s *pb.Sleeve) string {
	if s.State == pb.SleeveState_SLEEVE_STATE_RUNNING || s.ExitCode == nil {
		return sleeveStateLabel(s.State)
	}
	if s.OomKilled {
		return fmt.Sprintf("exited(%d,oom)", *s.ExitCode)
	}
	return fmt.Sprintf("exited(%d)", *s.ExitCode)
}

const showTailLines = 20

func limitLabel(l *pb.SleeveLimits) string {
	cpu, mem := "-", "-"
	if l.GetCpuPercent() > 0 {
		cpu = fmt.Sprintf("%d%%", l.CpuPercent)
	}
	if l.GetMemoryBytes() > 0 {
		mem = fmtBytes(l.MemoryBytes)
	}
	return cpu + "/" + mem
}

func sleeveLsCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "ls",
		Short: "List sleeves with live usage",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.ListSleeves(cmd.Context(), connect.NewRequest(&pb.ListSleevesRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "NAME\tHARNESS\tSTATE\tCPU\tMEM\tPEAK\tLIMITS\tWORKSPACE")
				for _, s := range res.Msg.Sleeves {
					u := s.Usage
					fmt.Fprintf(w, "%s\t%s\t%s\t%.0f%%\t%s\t%s\t%s\t%s\n", s.Name, harnessName(s.Harness), sleeveRowState(s),
						u.CpuPercent, fmtBytes(u.MemoryCurrentBytes), fmtBytes(u.MemoryPeakBytes), limitLabel(s.Limits), s.Workspace)
				}
			})
		},
	}
}

func sleeveShowCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "show <name>",
		Short: "Show one sleeve",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.GetSleeve(cmd.Context(), connect.NewRequest(&pb.GetSleeveRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			s, u := res.Msg.Sleeve, res.Msg.Sleeve.Usage
			var tail string
			if s.State == pb.SleeveState_SLEEVE_STATE_EXITED {
				sc, err := sessionClient(o)
				if err != nil {
					return err
				}
				if cp, err := sc.CapturePane(cmd.Context(), connect.NewRequest(&pb.CapturePaneRequest{Name: s.Session, Lines: 100})); err == nil {
					tail = session.Tail(cp.Msg.Text, showTailLines)
				}
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "name\t%s\nharness\t%s\nstate\t%s\nworkspace\t%s\nsession\t%s\n", s.Name, harnessName(s.Harness), sleeveStateLabel(s.State), s.Workspace, s.Session)
				fmt.Fprintf(w, "created\t%s\n", s.CreatedAt.AsTime().Local().Format(time.RFC3339))
				if s.ExitedAt != nil {
					fmt.Fprintf(w, "exited\t%s\n", s.ExitedAt.AsTime().Local().Format(time.RFC3339))
					fmt.Fprintf(w, "exit\t%s\n", exitLabel(s))
				}
				fmt.Fprintf(w, "args\t%s\nlimits\t%s\n", strings.Join(s.Args, " "), limitLabel(s.Limits))
				fmt.Fprintf(w, "cpu\t%.1f%% (%.1fs total)\n", u.CpuPercent, u.CpuSeconds)
				fmt.Fprintf(w, "memory\t%s (peak %s)\n", fmtBytes(u.MemoryCurrentBytes), fmtBytes(u.MemoryPeakBytes))
				fmt.Fprintf(w, "pids\t%d\noom kills\t%d\nworkspace disk\t%s\n", u.Pids, u.OomKills, fmtBytes(u.WorkspaceDiskBytes))
				if tail != "" {
					fmt.Fprintf(w, "\nlast %d lines of output:\n%s\n", showTailLines, tail)
				} else if s.State == pb.SleeveState_SLEEVE_STATE_EXITED {
					fmt.Fprintln(w, "\n(no output kept: the session is gone)")
				}
			})
		},
	}
}

func sleeveKillCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "kill <name>",
		Short: "Stop a sleeve and its processes; the workspace is kept",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.KillSleeve(cmd.Context(), connect.NewRequest(&pb.KillSleeveRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "killed\t%s\n", args[0]) })
		},
	}
}

func sleeveAttachCmd(o *opts) *cobra.Command {
	var readOnly bool
	var escape string
	c := &cobra.Command{
		Use:   "attach <name>",
		Short: "Attach this terminal to a sleeve's session (detach with Ctrl-])",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			esc, err := parseEscape(escape)
			if err != nil {
				return err
			}
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.GetSleeve(cmd.Context(), connect.NewRequest(&pb.GetSleeveRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return attach(cmd.Context(), o, res.Msg.Sleeve.Session, readOnly, esc)
		},
	}
	c.Flags().BoolVar(&readOnly, "readonly", false, "attach without being able to send input")
	c.Flags().StringVar(&escape, "escape", "^]", `detach key as ^X, or "none"`)
	return c
}
