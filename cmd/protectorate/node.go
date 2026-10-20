package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"regexp"
	"runtime"
	"runtime/debug"
	"strings"
	"syscall"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
	"github.com/hotschmoe/protectorate/internal/tailnet"
)

const nodeTokenEnv = "PROTECTORATE_NODE_TOKEN"

// nodeToken resolves the shared node-agent bearer: the file if given, else
// env PROTECTORATE_NODE_TOKEN, else empty. Secrets are never flags, which
// would show in the process list.
func nodeToken(file string) (string, error) {
	if file == "" {
		return strings.TrimSpace(os.Getenv(nodeTokenEnv)), nil
	}
	b, err := os.ReadFile(file)
	if err != nil {
		return "", fmt.Errorf("read node token file: %w", err)
	}
	tok := strings.TrimSpace(string(b))
	if tok == "" {
		return "", fmt.Errorf("node token file %s is empty", file)
	}
	return tok, nil
}

var invalidNameChars = regexp.MustCompile(`[^a-z0-9._-]+`)

// localNodeName is the hub's own node name: the hostname made inventory-safe.
func localNodeName() string {
	host, _ := os.Hostname()
	n := strings.Trim(invalidNameChars.ReplaceAllString(strings.ToLower(host), "-"), "-._")
	if n == "" {
		return "hub"
	}
	return n
}

// attachNodes wires NodeService into h with this machine as the hub's node.
func attachNodes(h *hub.Hub, interval time.Duration, token string, watch []string, client *http.Client, peers func(context.Context) ([]hub.Peer, error)) error {
	home, err := hub.Home()
	if err != nil {
		return err
	}
	self := hub.LocalNodeInfo(localNodeName(), runtime.GOOS, runtime.GOARCH)
	inv, err := hub.OpenInventory(home, self)
	if err != nil {
		return err
	}
	if err := inv.WriteHosts(); err != nil {
		return fmt.Errorf("write hosts.md: %w", err)
	}
	local, err := node.NewJobs(filepath.Join(home, "local-jobs"), 1)
	if err != nil {
		return err
	}
	local.Home = home
	col := node.NewCollector(node.Options{Version: version, WatchPaths: watch, Jobs: local})
	h.SetNodes(hub.NewNodeMonitor(inv, self, col, hub.NodeOptions{Interval: interval, Token: token, Client: client, Peers: peers}))
	files, err := node.NewFiles(filepath.Join(home, "node-files"), false)
	if err != nil {
		return err
	}
	h.EnableNodeOps(files, &node.Desk{D: desk.New()})
	return h.EnableJobs(home, local)
}

func nodeCmd(o *opts) *command {
	c := &command{Use: "node", Short: "Inventory and live metrics of machines"}
	c.AddCommand(nodeLsCmd(o), nodeShowCmd(o), nodeAddCmd(o), nodeRmCmd(o), nodeServeCmd(), nodeMetricsCmd(o), nodePutCmd(o), nodeGetCmd(o), nodeInstallCmd(), nodeUninstallCmd(), nodeDoctorCmd(o), nodeUpdateCmd(o), nodeTierCmd(o), nodeMigrateTiersCmd())
	c.AddCommand(nodeEnrollCmds(o)...)
	return c
}

func (o *opts) nodeClient() (protectoratev1connect.NodeServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewNodeServiceClient(hc, o.hubURL), nil
}

func nodeLsCmd(o *opts) *command {
	return &command{
		Use:   "ls",
		Short: "List nodes with live metrics",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			res, err := cl.ListNodes(cmd.Context(), connect.NewRequest(&pb.ListNodesRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeNodeList(w, res.Msg) })
		},
	}
}

func nodeShowCmd(o *opts) *command {
	return &command{
		Use:   "show <name>",
		Short: "Show one node in full",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			res, err := cl.GetNode(cmd.Context(), connect.NewRequest(&pb.GetNodeRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeNodeStatus(w, res.Msg) })
		},
	}
}

func nodeAddCmd(o *opts) *command {
	var n pb.NodeInfo
	c := &command{
		Use:   "add <name> --addr <host[:port]>",
		Short: "Add a node to the inventory",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			n.Name = args[0]
			res, err := cl.AddNode(cmd.Context(), connect.NewRequest(&pb.AddNodeRequest{Node: &n}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "added %s (%s, tier %s)\n", res.Msg.Name, res.Msg.Address, res.Msg.Tier)
			})
		},
	}
	f := c.Flags()
	f.StringVar(&n.Address, "addr", "", "tailnet name or address of the node agent (port 7475 assumed)")
	f.StringVar(&n.SshAlias, "ssh", "", "ssh host alias for break-glass access")
	f.StringVar(&n.Tier, "tier", hub.TierShared, "tier: shared (the compute pool) or care (a machine kept for a person, never scheduled)")
	f.StringSliceVar(&n.Tags, "tag", nil, "tag (repeatable)")
	f.StringVar(&n.Notes, "notes", "", "free-form notes")
	_ = c.MarkFlagRequired("addr")
	return c
}

func nodeTierCmd(o *opts) *command {
	return &command{
		Use:   "tier <name> shared|care",
		Short: "Change a node's tier (owner only)",
		Long: `Moves a node between the tiers: shared (the compute pool: jobs and leases,
sandboxed) and care (a machine kept for a person: never scheduled or leased,
used only under an admin grant for it). The hub retags the machine on the
tailnet (tag:tier-<tier>) and then records the tier in its inventory.`,
		Args: exactArgs(2),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			res, err := cl.SetNodeTier(cmd.Context(), connect.NewRequest(&pb.SetNodeTierRequest{Name: args[0], Tier: args[1]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "%s is now tier %s\n", res.Msg.Name, res.Msg.Tier) })
		},
	}
}

func nodeMigrateTiersCmd() *command {
	return &command{
		Use:   "migrate-tiers",
		Short: "Rename the retired tiers in this hub's nodes.json (server to shared, family to care)",
		Long: `Run once on the hub machine, with the hub stopped, when the hub refuses to
start because nodes.json still names the tiers server or family. It copies
nodes.json to nodes.json.pre-tiers (refusing if that copy exists), rewrites
the tiers atomically and regenerates hosts.md. Then start the hub and retag
each node on the tailnet with the printed node tier commands.`,
		Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			changes, err := hub.MigrateTiers(home, hub.LocalNodeInfo(localNodeName(), runtime.GOOS, runtime.GOARCH))
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			if len(changes) == 0 {
				fmt.Fprintln(w, "nodes.json has no retired tiers; nothing to do")
				return nil
			}
			for _, c := range changes {
				fmt.Fprintf(w, "%s: %s -> %s\n", c.Node, c.From, c.To)
			}
			fmt.Fprintf(w, "\nkept the old file as %s\n", filepath.Join(home, "nodes.json.pre-tiers"))
			fmt.Fprintln(w, "start the hub, then retag each node on the tailnet:")
			for _, c := range changes {
				fmt.Fprintf(w, "  protectorate node tier %s %s\n", c.Node, c.To)
			}
			return nil
		},
	}
}

var removeJobs = map[string]pb.RemoveJobs{
	"":       pb.RemoveJobs_REMOVE_JOBS_UNSPECIFIED,
	"wait":   pb.RemoveJobs_REMOVE_JOBS_WAIT,
	"cancel": pb.RemoveJobs_REMOVE_JOBS_CANCEL,
}

func nodeRmCmd(o *opts) *command {
	var jobs string
	var uninstall bool
	var wait time.Duration
	c := &command{
		Use:   "rm <name>",
		Short: "Drain, revoke and remove a node (or cancel a pending invite)",
		Long: `Takes a node out of service on the hub: no new jobs or leases go to it,
running jobs are waited for (--jobs wait) or cancelled (--jobs cancel; without
--jobs a node with running jobs is refused), then its tailnet enrollment and any
unused invite key are revoked, its leases released and its inventory entry
removed. A revoked machine cannot rejoin without a new invite. --uninstall
first asks a reachable agent to unregister its startup and stop; it deletes
nothing on the machine. For a node that was invited but never joined this
cancels the invite.`,
		Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			mode, ok := removeJobs[jobs]
			if !ok {
				return fmt.Errorf("--jobs must be wait or cancel, not %q", jobs)
			}
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			req := &pb.RemoveNodeRequest{Name: args[0], Jobs: mode, Uninstall: uninstall}
			if wait > 0 {
				req.WaitTimeout = durationpb.New(wait)
			}
			stream, err := cl.RemoveNode(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			defer stream.Close()
			pr := o.out(cmd)
			for stream.Receive() {
				p := stream.Msg()
				if err := pr.print(p, func(w io.Writer) { fmt.Fprintf(w, "%s\t%s\n", removePhaseLabel(p.Phase), p.Message) }); err != nil {
					return err
				}
			}
			return stream.Err()
		},
	}
	c.Flags().StringVar(&jobs, "jobs", "", "running jobs: wait (let them finish) or cancel")
	c.Flags().DurationVar(&wait, "wait-timeout", 0, "with --jobs wait, give up after this long (default 1h)")
	c.Flags().BoolVar(&uninstall, "uninstall", false, "also ask the agent, if reachable, to unregister its startup and stop (deletes nothing)")
	return c
}

func removePhaseLabel(p pb.RemovePhase) string {
	return strings.ToLower(strings.TrimPrefix(p.String(), "REMOVE_PHASE_"))
}

func nodeServeCmd() *command {
	var memoryLimit int64
	var listen, tokenFile, jobDir, fileRoot, homeDir string
	var fileUnconfined bool
	var watch []string
	var slow time.Duration
	var jobSlots int
	c := &command{
		Use:   "serve",
		Short: "Run the node agent",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) (err error) {
			if memoryLimit <= 0 {
				return errors.New("--memory-limit must be positive")
			}
			if cmd.Flags().Changed("memory-limit") || os.Getenv("GOMEMLIMIT") == "" {
				debug.SetMemoryLimit(memoryLimit)
			}
			if homeDir != "" {
				if err := os.Setenv("PROTECTORATE_HOME", homeDir); err != nil {
					return fmt.Errorf("set home: %w", err)
				}
			}
			restartReq := make(chan node.RestartKind, 1)
			exe, err := os.Executable()
			if err != nil {
				return fmt.Errorf("locate this executable: %w", err)
			}
			if r, rerr := filepath.EvalSymlinks(exe); rerr == nil {
				exe = r
			}
			defer func() {
				select {
				case k := <-restartReq:
					err = errors.Join(err, finishRestart(k, exe, os.Args[1:]))
				default:
				}
			}()
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt, syscall.SIGTERM)
			defer stop()
			ctx, cancelRun := context.WithCancel(ctx)
			defer cancelRun()
			var l net.Listener
			home, err := hub.Home()
			if err != nil {
				return err
			}
			a := &node.Agent{}
			if jobSlots > 0 {
				if jobDir == "" {
					jobDir = filepath.Join(home, "node-jobs")
					if d, ok := node.RunnerJobDir(home); ok {
						jobDir = d
					}
				}
				if a.Jobs, err = node.NewJobs(jobDir, jobSlots); err != nil {
					return err
				}
				a.Jobs.Home = home
				watch = append(watch, a.Jobs.Dir())
			}
			if fileRoot == "" {
				fileRoot = filepath.Join(home, "node-files")
			}
			if a.Files, err = node.NewFiles(fileRoot, fileUnconfined); err != nil {
				return err
			}
			dd := desk.New()
			guard, err := node.NewDeskGuard(dd, home)
			if err != nil {
				return err
			}
			a.Desk = &node.Desk{D: dd, Guard: guard}
			go func() {
				if err := guard.Run(ctx); err != nil {
					fmt.Fprintf(cmd.ErrOrStderr(), "node agent: desk input monitor: %v (the desk policy treats the user as active since the agent started)\n", err)
				}
			}()
			a.Updater = &node.Updater{Files: a.Files, Exe: exe, Version: version, Supervision: node.DetectSupervision(runningAsService()),
				Restart: func(k node.RestartKind) {
					select {
					case restartReq <- k:
					default:
					}
					cancelRun()
				}}
			a.Uninstaller = agentUninstaller(cmd.ErrOrStderr(), cancelRun)
			if rolledBack, err := a.Updater.Start(); err != nil {
				return err
			} else if rolledBack {
				fmt.Fprintln(cmd.ErrOrStderr(), "node agent: the updated binary never confirmed; rolled back to the previous one and restarting")
				return nil
			}
			a.Collector = node.NewCollector(node.Options{Version: version, WatchPaths: watch, SlowInterval: slow, Jobs: a.Jobs})
			tsDir := filepath.Join(home, "tsnet")
			if tailnet.Joined(tsDir) {
				n, err := tailnet.OpenNode(ctx, tsDir)
				if err != nil {
					return err
				}
				defer n.Close()
				if l, err = n.Listen(node.DefaultPort); err != nil {
					return err
				}
				a.Authorize = n.AuthorizeHub()
				a.DebugAuthorize = a.Authorize
				go n.KeepHubReachable(ctx, func(f string, args ...any) { fmt.Fprintf(cmd.ErrOrStderr(), "tailnet: "+f+"\n", args...) })
				fmt.Fprintf(cmd.ErrOrStderr(), "node agent %s serving on the tailnet, port %d (hub only)\n", version, node.DefaultPort)
			} else {
				token, err := nodeToken(tokenFile)
				if err != nil {
					return err
				}
				if token == "" {
					return fmt.Errorf("not enrolled in a tailnet and no token: run `protectorate node join`, or pass --node-token-file or set %s", nodeTokenEnv)
				}
				if l, err = net.Listen("tcp", listen); err != nil {
					return err
				}
				a.Authorize = node.TokenAuthorizer(token)
				fmt.Fprintf(cmd.ErrOrStderr(), "node agent %s listening on %s\n", version, l.Addr())
			}
			return a.Serve(ctx, l)
		},
	}
	f := c.Flags()
	f.Int64Var(&memoryLimit, "memory-limit", 128<<20, "soft Go runtime memory limit in bytes; overrides GOMEMLIMIT when explicitly set (excludes child jobs)")
	f.StringVar(&homeDir, "home", "", "PROTECTORATE_HOME for this agent (default: env, then ~/.protectorate)")
	f.StringVar(&listen, "listen", fmt.Sprintf("127.0.0.1:%d", node.DefaultPort), "address to listen on when not enrolled in a tailnet")
	f.StringVar(&tokenFile, "node-token-file", "", "file holding the bearer token the hub must present when not enrolled in a tailnet (or the token itself in env "+nodeTokenEnv+")")
	f.StringSliceVar(&watch, "watch", nil, "directory whose size is tracked, e.g. a build cache (repeatable)")
	f.StringVar(&jobDir, "job-dir", "", "where hub jobs run and git worktrees are cached (default $PROTECTORATE_HOME/node-jobs, or the runner's directory on Windows after node install --runner)")
	f.StringVar(&fileRoot, "file-root", "", "directory file transfers are confined to (default $PROTECTORATE_HOME/node-files)")
	f.BoolVar(&fileUnconfined, "file-unconfined", false, "let file transfers touch any path this user can, like a job can; --file-root then only anchors relative paths")
	f.IntVar(&jobSlots, "job-concurrency", 1, "jobs that may run at once; 0 refuses jobs")
	f.DurationVar(&slow, "watch-interval", 15*time.Minute, "how often to measure watched directories and engine disk usage")
	return c
}

func nodeMetricsCmd(o *opts) *command {
	var local bool
	var watch []string
	c := &command{
		Use:   "metrics --local",
		Short: "Collect metrics on this machine once, without a hub",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			if !local {
				return errors.New("only --local is supported")
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			col := node.NewCollector(node.Options{Version: version, WatchPaths: watch})
			col.Refresh(ctx)
			m, err := col.Collect(ctx)
			if err != nil {
				return err
			}
			return o.out(cmd).print(m, func(w io.Writer) { writeMetrics(w, m) })
		},
	}
	c.Flags().BoolVar(&local, "local", false, "collect on this machine")
	c.Flags().StringSliceVar(&watch, "watch", nil, "directory to size (repeatable)")
	return c
}
