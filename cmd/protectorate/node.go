package main

import (
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/signal"
	"regexp"
	"runtime"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
)

const nodeTokenEnv = "PROTECTORATE_NODE_TOKEN"

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
func attachNodes(h *hub.Hub, interval time.Duration, token string, watch []string) error {
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
	col := node.NewCollector(node.Options{Version: version, WatchPaths: watch})
	h.SetNodes(hub.NewNodeMonitor(inv, self, col, hub.NodeOptions{Interval: interval, Token: token}))
	return nil
}

func nodeCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "node", Short: "Inventory and live metrics of machines"}
	c.AddCommand(nodeLsCmd(o), nodeShowCmd(o), nodeAddCmd(o), nodeRmCmd(o), nodeServeCmd(), nodeMetricsCmd(o))
	return c
}

func (o *opts) nodeClient() (protectoratev1connect.NodeServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewNodeServiceClient(hc, o.hubURL), nil
}

func nodeLsCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "ls",
		Short: "List nodes with live metrics",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
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

func nodeShowCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "show <name>",
		Short: "Show one node in full",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
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

func nodeAddCmd(o *opts) *cobra.Command {
	var n pb.NodeInfo
	c := &cobra.Command{
		Use:   "add <name> --addr <host[:port]>",
		Short: "Add a node to the inventory",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
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
	f.StringVar(&n.Tier, "tier", hub.TierServer, "policy tier: server or family")
	f.StringSliceVar(&n.Tags, "tag", nil, "tag (repeatable)")
	f.StringVar(&n.Notes, "notes", "", "free-form notes")
	_ = c.MarkFlagRequired("addr")
	return c
}

func nodeRmCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "rm <name>",
		Short: "Remove a node from the inventory",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			res, err := cl.RemoveNode(cmd.Context(), connect.NewRequest(&pb.RemoveNodeRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "removed %s\n", args[0]) })
		},
	}
}

func nodeServeCmd() *cobra.Command {
	var listen, token string
	var watch []string
	var slow time.Duration
	c := &cobra.Command{
		Use:   "serve",
		Short: "Run the node agent",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			if token == "" {
				token = os.Getenv(nodeTokenEnv)
			}
			if token == "" {
				return fmt.Errorf("a token is required: pass --token or set %s", nodeTokenEnv)
			}
			l, err := net.Listen("tcp", listen)
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			fmt.Fprintf(cmd.ErrOrStderr(), "node agent %s listening on %s\n", version, l.Addr())
			a := &node.Agent{
				Collector: node.NewCollector(node.Options{Version: version, WatchPaths: watch, SlowInterval: slow}),
				Authorize: node.TokenAuthorizer(token),
			}
			return a.Serve(ctx, l)
		},
	}
	f := c.Flags()
	f.StringVar(&listen, "listen", fmt.Sprintf("127.0.0.1:%d", node.DefaultPort), "address to listen on")
	f.StringVar(&token, "token", "", "bearer token the hub must present (env "+nodeTokenEnv+")")
	f.StringSliceVar(&watch, "watch", nil, "directory whose size is tracked, e.g. a build cache (repeatable)")
	f.DurationVar(&slow, "watch-interval", 15*time.Minute, "how often to measure watched directories and engine disk usage")
	return c
}

func nodeMetricsCmd(o *opts) *cobra.Command {
	var local bool
	var watch []string
	c := &cobra.Command{
		Use:   "metrics --local",
		Short: "Collect metrics on this machine once, without a hub",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
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
