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
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// attachSleeves wires SleeveService into h and recovers persisted sleeves.
func attachSleeves(ctx context.Context, h *hub.Hub, toolDirs, refDirs, workspaceRoots []string, goCacheMax uint64) error {
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
	cfg.GoCacheMaxBytes = goCacheMax
	cfg.ReferenceDirs = refDirs
	if workspaceRoots == nil {
		if ownerHome, err := os.UserHomeDir(); err == nil {
			p := filepath.Join(ownerHome, "github")
			if fi, err := os.Stat(p); err == nil && fi.IsDir() {
				workspaceRoots = []string{p}
			}
		}
	}
	for _, root := range workspaceRoots {
		real, err := filepath.EvalSymlinks(root)
		if err != nil {
			return fmt.Errorf("resolve workspace root %q: %w", root, err)
		}
		real, err = filepath.Abs(real)
		if err != nil {
			return fmt.Errorf("absolute workspace root %q: %w", root, err)
		}
		fi, err := os.Stat(real)
		if err != nil {
			return fmt.Errorf("stat workspace root %q: %w", root, err)
		}
		if !fi.IsDir() {
			return fmt.Errorf("workspace root %q is not a directory", root)
		}
		cfg.WorkspaceRoots = append(cfg.WorkspaceRoots, real)
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
	return h.EnableCoordination(home)
}

func sleeveClient(o *opts) (protectoratev1connect.SleeveServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewSleeveServiceClient(hc, o.hubURL), nil
}

func sleeveCmd(o *opts) *command {
	c := &command{Use: "sleeve", Aliases: []string{"sleeves"}, Short: "Run harnesses in isolated workspaces"}
	c.AddCommand(sleeveSpawnCmd(o), sleeveLsCmd(o), sleeveWorkspacesCmd(o), sleeveBrowseCmd(o), sleeveShowCmd(o), sleeveKillCmd(o), sleevePruneCmd(o), sleeveAttachCmd(o), sleeveResleeveCmd(o))
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

var sleeveModes = map[string]pb.SleeveMode{
	"":         pb.SleeveMode_SLEEVE_MODE_UNSPECIFIED,
	"chat":     pb.SleeveMode_SLEEVE_MODE_CHAT,
	"terminal": pb.SleeveMode_SLEEVE_MODE_TERMINAL,
}

func sleeveSpawnCmd(o *opts) *command {
	sel := &pb.ModelSelection{}
	var workspace, harness, mem, mode string
	var branch, newBranch, briefFile string
	var cpu uint32
	var env []string
	var supervised bool
	var nested bool
	var readOnly []string
	var exitAfterIdle time.Duration
	c := &command{
		Use:   "spawn <name> --workspace <dir|git url> --harness <h> [--cpu 200] [--mem 8G] [--supervised] [-- args...]",
		Short: "Start a harness in an isolated sleeve",
		Long: `Start a harness (claude, codex, grok, pi or shell) in a sandbox with its own
process namespace and CPU/memory limits. The workspace is a directory on the
hub or a git URL to clone. --cpu is a percentage of one core (200 = two cores).
For the shell harness, arguments after -- are the command to run. Agent
harnesses start in chat mode (drive them with 'protectorate chat'); use
--mode terminal for the TUI under tmux. A chat harness runs autonomous (the
sandbox is the boundary) unless --supervised: then it asks before acting and
'protectorate chat respond' answers. --ro host-path[:sandbox-path] adds read-only
reference directories (repeatable, default same path). --exit-after-idle 5m
removes a chat sleeve after five minutes idle with no pending mail (default off).`,
		Args: minimumArgs(1),
		RunE: func(cmd *command, args []string) error {
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
			sleeveMode, ok := sleeveModes[mode]
			if !ok {
				return fmt.Errorf("unknown mode %q, want chat or terminal", mode)
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
			var mounts []*pb.ReadOnlyMount
			brief := ""
			if briefFile != "" {
				if mode == "terminal" || h == pb.Harness_HARNESS_SHELL {
					return errors.New("--brief requires chat mode")
				}
				raw, err := os.ReadFile(briefFile)
				if err != nil {
					return fmt.Errorf("read brief: %w", err)
				}
				brief = string(raw)
				if strings.TrimSpace(brief) == "" {
					return errors.New("brief is empty")
				}
			}
			for _, value := range readOnly {
				source, target, _ := strings.Cut(value, ":")
				mounts = append(mounts, &pb.ReadOnlyMount{Source: source, Target: target})
			}
			res, err := cl.SpawnSleeve(cmd.Context(), connect.NewRequest(&pb.SpawnSleeveRequest{
				Branch: branch, NewBranch: newBranch, Brief: brief,
				ReadOnly: mounts, ExitAfterIdle: durationpb.New(exitAfterIdle),
				Name: name, Workspace: workspace, Harness: h, Args: extra, Limits: lim, Env: envMap, Mode: sleeveMode, Selection: optionalSelection(sel),
				Supervised: supervised,
				Nested:     nested,
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				s := res.Msg.Sleeve
				if s.Mode == pb.SleeveMode_SLEEVE_MODE_CHAT {
					fmt.Fprintf(w, "spawned\t%s\nchat\tprotectorate chat send %s \"...\"; protectorate chat watch %s\n", s.Name, s.Name, s.Name)
					return
				}
				fmt.Fprintf(w, "spawned\t%s\nattach\tprotectorate sleeve attach %s\n", s.Name, s.Name)
			})
		},
	}
	c.Flags().StringVar(&workspace, "workspace", "", "directory on the hub, or git URL to clone")
	c.Flags().StringVar(&branch, "branch", "", "branch to clone (local default: current branch; URL default: default branch)")
	c.Flags().StringVar(&newBranch, "new-branch", "", "create this branch in a fresh clone from --branch or the source branch")
	c.Flags().StringVar(&briefFile, "brief", "", "file containing the first chat message (read by this CLI; chat mode only)")
	c.Flags().StringArrayVar(&readOnly, "ro", nil, "read-only host directory[:sandbox directory] (repeatable; default same path)")
	c.Flags().DurationVar(&exitAfterIdle, "exit-after-idle", 0, "kill chat sleeve after idle with no pending mail (0 disables)")
	selectionFlags(c, sel)
	c.Flags().StringVar(&harness, "harness", "", "claude, codex, grok, pi or shell")
	c.Flags().Uint32Var(&cpu, "cpu", 0, "CPU limit in percent of one core (0 = unlimited)")
	c.Flags().StringVar(&mem, "mem", "", "memory limit such as 512M or 8G (empty = unlimited)")
	c.Flags().StringArrayVar(&env, "env", nil, "environment variable K=V (repeatable)")
	c.Flags().StringVar(&mode, "mode", "", "chat or terminal (default: chat for agent harnesses, terminal for shell)")
	c.Flags().BoolVar(&supervised, "supervised", false, "chat mode: the harness asks before acting (answer with chat respond)")
	c.Flags().BoolVar(&nested, "nested", false, "allow sandboxed child sleeves within these CPU and memory limits (requires --cpu and --mem)")
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
	if s.Killed {
		return "killed"
	}
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

func sleeveLsCmd(o *opts) *command {
	return &command{
		Use:   "ls",
		Short: "List sleeves with live usage",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.ListSleeves(cmd.Context(), connect.NewRequest(&pb.ListSleevesRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "NAME\tHARNESS\tMODEL\tEFFORT\tSTATE\tCPU\tMEM\tPEAK\tHOME\tLIMITS\tWORKSPACE\tPARENT\tBRANCH")
				for _, s := range res.Msg.Sleeves {
					u := s.Usage
					model, effort := "-", "-"
					if s.Chat != nil {
						model, effort = s.Chat.Model, s.Chat.ReasoningEffort
						if model == "" {
							model = "unknown"
						}
						if effort == "" {
							effort = "unknown"
						}
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%.0f%%\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", s.Name, harnessName(s.Harness), model, effort, sleeveRowState(s),
						u.CpuPercent, fmtBytes(u.MemoryCurrentBytes), fmtBytes(u.MemoryPeakBytes), fmtBytes(u.HomeDiskBytes), limitLabel(s.Limits), s.Workspace, s.Parent, s.GitBranch)
				}
			})
		},
	}
}

func sleeveWorkspacesCmd(o *opts) *command {
	return &command{
		Use:     "workspaces",
		Aliases: []string{"ws"},
		Short:   "List workspaces on the hub that a sleeve can start in",
		Long: `List the existing workspaces on the hub: checkouts and directories under its
workspaces directory and the workspaces of known sleeves. Pass a PATH to
'sleeve spawn --workspace'. Read-only reference entries are for 'spawn --ro'.
A workspace marked with a required name can only be
used by a sleeve of that name. Any other absolute directory, or a git URL to
clone, can also be given to spawn.`,
		Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.ListWorkspaces(cmd.Context(), connect.NewRequest(&pb.ListWorkspacesRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "NAME\tBRANCH\tREMOTE\tSLEEVE\tNAME REQUIRED\tPATH\tREAD ONLY")
				for _, ws := range res.Msg.Workspaces {
					use := ws.Sleeve
					if ws.Busy {
						use += " (running)"
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\t%t\n", ws.Name, ws.GitBranch, ws.GitRemote, use, ws.RequiredName, ws.Path, ws.ReadOnly)
				}
			})
		},
	}
}

func sleeveBrowseCmd(o *opts) *command {
	return &command{
		Use: "browse [PATH]", Short: "Browse workspace directories on the hub",
		Long: "List the hub's configured workspace roots, or the subdirectories of PATH.\nPaths refer to the hub. Select an acceptable path for 'sleeve spawn --workspace'.\nHidden directories are skipped; large listings report truncation.",
		Args: maximumArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			req := &pb.BrowseWorkspacesRequest{}
			if len(args) > 0 {
				req.Path = args[0]
			}
			res, err := cl.BrowseWorkspaces(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "NAME\tGIT\tBRANCH\tREMOTE\tACCEPTABLE\tREASON\tPATH")
				for _, d := range res.Msg.Directories {
					fmt.Fprintf(w, "%s\t%t\t%s\t%s\t%t\t%s\t%s\n", d.Name, d.IsGit, d.GitBranch, d.GitRemote, d.Acceptable, d.Reason, d.Path)
				}
				if res.Msg.Truncated {
					fmt.Fprintln(w, "Listing truncated")
				}
			})
		},
	}
}

func sleeveShowCmd(o *opts) *command {
	return &command{
		Use:   "show <name>",
		Short: "Show one sleeve",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
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
				fmt.Fprintf(w, "name\t%s\nharness\t%s\nstate\t%s\nworkspace\t%s\nsession\t%s\n", s.Name, harnessName(s.Harness), sleeveRowState(s), s.Workspace, s.Session)
				fmt.Fprintf(w, "branch\t%s\n", s.GitBranch)
				if s.BaseRepo != "" {
					fmt.Fprintf(w, "base repo\t%s\nbase branch\t%s\nbase tip\t%s\n", s.BaseRepo, s.BaseBranch, s.BaseTip)
				}
				fmt.Fprintf(w, "parent\t%s\n", s.Parent)
				fmt.Fprintf(w, "mode\t%s\n", strings.ToLower(strings.TrimPrefix(s.Mode.String(), "SLEEVE_MODE_")))
				if s.Result != nil {
					fmt.Fprintf(w, "report\t%s (chat seq %d, %s)\n%s\n", reportStatusLabel(s.Result.Status), s.Result.ChatSeq, s.Result.Time.AsTime().Format(time.RFC3339), s.Result.Summary)
				}
				fmt.Fprintf(w, "nested\t%t\n", s.Nested)
				fmt.Fprintf(w, "created\t%s\n", s.CreatedAt.AsTime().Local().Format(time.RFC3339))
				if s.ExitedAt != nil {
					fmt.Fprintf(w, "exited\t%s\n", s.ExitedAt.AsTime().Local().Format(time.RFC3339))
				}
				if s.State == pb.SleeveState_SLEEVE_STATE_EXITED {
					fmt.Fprintf(w, "exit\t%s\n", exitLabel(s))
					fmt.Fprintf(w, "exit signal\t%d\noom killed\t%t\n", s.ExitSignal, s.OomKilled)
				}
				fmt.Fprintf(w, "args\t%s\nlimits\t%s\n", strings.Join(s.Args, " "), limitLabel(s.Limits))
				fmt.Fprintf(w, "exit after idle\t%s\n", s.ExitAfterIdle.AsDuration())
				for _, mount := range s.ReadOnly {
					fmt.Fprintf(w, "read-only mount\t%s -> %s\n", mount.Source, mount.Target)
				}
				if s.Supervised {
					fmt.Fprintln(w, "supervised\tyes (the harness asks; answer with chat respond)")
				} else {
					fmt.Fprintln(w, "supervised\tno")
				}
				if s.Chat != nil {
					printSelection(w, s.Chat)
					fmt.Fprintf(w, "chat state\t%s\nharness session\t%s\nprovider\t%s\nlocal model\t%t\ncontext tokens\t%d/%d\nlast sequence\t%d\n", s.Chat.State, s.Chat.HarnessSessionId, s.Chat.Provider, s.Chat.LocalModel, s.Chat.ContextTokens, s.Chat.ContextWindow, s.Chat.LastSeq)
					if s.Chat.CacheExpiresAt != nil {
						fmt.Fprintf(w, "cache expires\t%s\n", s.Chat.CacheExpiresAt.AsTime().Local().Format(time.RFC3339))
					}
					if s.Chat.LastActivityAt != nil {
						fmt.Fprintf(w, "last activity\t%s\n", s.Chat.LastActivityAt.AsTime().Local().Format(time.RFC3339))
					}
				}
				if s.Handoff != nil {
					printHandoff(w, s.Handoff)
				}
				fmt.Fprintf(w, "cpu\t%.1f%% (%.1fs total)\n", u.CpuPercent, u.CpuSeconds)
				fmt.Fprintf(w, "memory\t%s (peak %s)\n", fmtBytes(u.MemoryCurrentBytes), fmtBytes(u.MemoryPeakBytes))
				fmt.Fprintf(w, "pids\t%d\noom kills\t%d\nworkspace disk\t%s\n", u.Pids, u.OomKills, fmtBytes(u.WorkspaceDiskBytes))
				fmt.Fprintf(w, "home disk\t%s (cache %s, rest %s)\ngo build cache\t%s\n", fmtBytes(u.HomeDiskBytes), fmtBytes(u.CacheDiskBytes), fmtBytes(u.HomeDiskBytes-u.CacheDiskBytes), fmtBytes(u.GoBuildCacheDiskBytes))
				if u.SampledAt != nil {
					fmt.Fprintf(w, "usage sampled\t%s\n", u.SampledAt.AsTime().Local().Format(time.RFC3339))
				}
				if u.DiskSampledAt != nil {
					fmt.Fprintf(w, "disk sampled\t%s\n", u.DiskSampledAt.AsTime().Local().Format(time.RFC3339))
				}
				if tail != "" {
					fmt.Fprintf(w, "\nlast %d lines of output:\n%s\n", showTailLines, tail)
				} else if s.State == pb.SleeveState_SLEEVE_STATE_EXITED {
					fmt.Fprintln(w, "\n(no output kept: the session is gone)")
				}
			})
		},
	}
}

func sleeveKillCmd(o *opts) *command {
	var dropCaches bool
	c := &command{
		Use:   "kill <name>",
		Short: "Stop a sleeve and its processes; the workspace is kept",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.KillSleeve(cmd.Context(), connect.NewRequest(&pb.KillSleeveRequest{Name: args[0], DropCaches: dropCaches}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "killed\t%s\n", args[0]) })
		},
	}
	c.Flags().BoolVar(&dropCaches, "drop-caches", false, "owner only: drop regenerable home caches after stopping")
	return c
}

func sleevePruneCmd(o *opts) *command {
	r := &pb.PruneSleevesRequest{}
	c := &command{
		Use: "prune", Short: "Inventory unused sleeve disk; remove only with --yes (owner only)", Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := cl.PruneSleeves(cmd.Context(), connect.NewRequest(r))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "SLEEVE\tKIND\tSIZE\tACTION\tPATH\tREASON")
				for _, e := range res.Msg.Entries {
					action := "keep"
					if e.Removed {
						action = "removed"
					} else if e.Eligible {
						action = "would remove"
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\n", e.Sleeve, e.Kind, fmtBytes(e.Bytes), action, e.Path, e.Reason)
				}
				if !r.Yes {
					fmt.Fprintln(w, "Dry run; pass --yes to remove eligible entries.")
				}
			})
		},
	}
	c.Flags().BoolVar(&r.Yes, "yes", false, "remove eligible entries (default: dry run)")
	c.Flags().BoolVar(&r.Caches, "caches", false, "drop regenerable caches (default when no category selected)")
	c.Flags().BoolVar(&r.Homes, "homes", false, "remove whole non-running homes, including sessions and config")
	c.Flags().BoolVar(&r.Workspaces, "workspaces", false, "remove clean hub clones with all local work verified outside the clone")
	c.Flags().BoolVar(&r.Running, "running", false, "include running sleeves in inventory; their data is always kept")
	return c
}

func sleeveAttachCmd(o *opts) *command {
	var readOnly bool
	var escape string
	c := &command{
		Use:   "attach <name>",
		Short: "Attach this terminal to a sleeve's session (detach with Ctrl-])",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
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

func sleeveResleeveCmd(o *opts) *command {
	sel := &pb.ModelSelection{}
	var defaults bool
	var to string
	var interrupt bool
	var wait time.Duration
	c := &command{
		Use:   "resleeve <name> [--to auto|claude|codex|grok|pi] [--interrupt] [--wait 5m]",
		Short: "Move a chat sleeve to another harness with a handoff",
		Long: `Move a chat sleeve to another harness on the same workspace, under the same
name. The outgoing harness is never asked: the hub waits for its turn to end
(interrupting it after --wait, or at once with --interrupt), composes facts
(git status, diff, recent commits, backlog, mail, the end of the outgoing
harness's own transcript), has a separate model write a summary (the
subscription with the most spare quota, else the local LLM through pi),
overwrites <workspace>/.protectorate/HANDOFF.md (kept out of git), starts the
new harness and tells it to read the file. --to auto takes the top of
'quota suggest' other than the current harness.`,
		Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			h := pb.Harness_HARNESS_UNSPECIFIED
			if to != "auto" {
				var ok bool
				if h, ok = harnessNames[to]; !ok || h == pb.Harness_HARNESS_SHELL {
					return fmt.Errorf("unknown harness %q, want auto, claude, codex, grok or pi", to)
				}
			}
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			if defaults && optionalSelection(sel) != nil {
				return errors.New("--native-defaults cannot be combined with --model or --effort")
			}
			req := &pb.ResleeveSleeveRequest{Name: args[0], To: h, Interrupt: interrupt, Selection: optionalSelection(sel), NativeDefaults: defaults}
			if wait > 0 {
				req.Wait = durationpb.New(wait)
			}
			res, err := cl.ResleeveSleeve(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "resleeved\t%s\n", res.Msg.Sleeve.Name)
				printHandoff(w, res.Msg.Sleeve.Handoff)
			})
		},
	}
	c.Flags().StringVar(&to, "to", "auto", "auto, claude, codex, grok or pi")
	selectionFlags(c, sel)
	c.Flags().BoolVar(&defaults, "native-defaults", false, "explicitly use target harness defaults instead of carrying selection")
	c.Flags().BoolVar(&interrupt, "interrupt", false, "interrupt a running turn instead of waiting for it")
	c.Flags().DurationVar(&wait, "wait", 0, "how long a running turn may continue before it is interrupted (default 5m)")
	return c
}

func printHandoff(w io.Writer, h *pb.Handoff) {
	if h == nil {
		return
	}
	fmt.Fprintf(w, "resleeved from\t%s (session %s) to %s at %s\n", h.FromHarness, h.FromSessionId, h.ToHarness, h.At.AsTime().Local().Format(time.RFC3339))
	by := strings.Join(strings.Fields(h.SummarizerHarness+" "+h.SummarizerProvider+" "+h.SummarizerModel), " ")
	switch {
	case h.SummarizerHarness == "":
		fmt.Fprintf(w, "summary\tnone: %s\n", h.SummaryError)
	case h.SummarizerLocal:
		fmt.Fprintf(w, "summary\tby local model %s (check it against the facts)\n", by)
	default:
		fmt.Fprintf(w, "summary\tby %s\n", by)
	}
	fmt.Fprintf(w, "handoff\t%s\n", h.Path)
	if h.ArchivePath != "" {
		fmt.Fprintf(w, "archive\t%s\n", h.ArchivePath)
	}
	if h.SummarizerHarness != "" && h.SummaryError != "" {
		fmt.Fprintf(w, "summary error\t%s\n", h.SummaryError)
	}
}
