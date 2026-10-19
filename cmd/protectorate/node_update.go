package main

import (
	"errors"
	"fmt"
	"os/exec"
	"strings"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/node"
)

// versionCell is the VERSION column of `node ls`: the node's version, marked
// when it differs from the hub's.
func versionCell(nodeVersion, hubVersion string) string {
	switch {
	case nodeVersion == "":
		return "-"
	case hubVersion != "" && nodeVersion != hubVersion:
		return nodeVersion + " (!)"
	}
	return nodeVersion
}

// hubVersionOf is the version of the hub's own node in a listing.
func hubVersionOf(nodes []*pb.NodeStatus) string {
	for _, s := range nodes {
		if s.Node.Local && s.Metrics != nil {
			return s.Metrics.AgentVersion
		}
	}
	return ""
}

// finishRestart brings the agent back after it has shut down for an update.
func finishRestart(k node.RestartKind, exe string, args []string) error {
	switch k {
	case node.RestartSystemd:
		out, err := exec.Command("systemctl", "--user", "restart", "protectorate-node.service").CombinedOutput()
		if err != nil {
			return fmt.Errorf("restart the unit: %w: %s", err, strings.TrimSpace(string(out)))
		}
		return nil
	case node.RestartTermux:
		out, err := exec.Command("sv", "restart", "protectorate-node").CombinedOutput()
		if err != nil {
			return fmt.Errorf("restart the service: %w: %s", err, strings.TrimSpace(string(out)))
		}
		return nil
	case node.RestartTask:
		if err := node.StartDetached("cmd.exe", []string{"/c", "ping -n 4 127.0.0.1 >nul & schtasks /Run /TN protectorate-node"}); err != nil {
			return fmt.Errorf("re-run the logon task: %w", err)
		}
		return &exitError{code: node.ExitRestart, msg: "restarting through the logon task"}
	case node.RestartExit:
		return &exitError{code: node.ExitRestart, msg: "exiting for the supervisor to restart"}
	}
	if err := node.StartDetached(exe, args); err != nil {
		return fmt.Errorf("re-exec %s: %w", exe, err)
	}
	return nil
}

type updateFlags struct {
	all, force, allowFamily, noWait bool
}

func nodeUpdateCmd(o *opts) *cobra.Command {
	var f updateFlags
	c := &cobra.Command{
		Use:   "update <node> | --all",
		Short: "Push the hub's build to a node and restart it onto it",
		Long: `Ask the hub to bring a node to the hub's version. The hub sends the right
binary over the file transfer, the node verifies it (size, SHA-256, and that
it runs), swaps it in and restarts itself: through its systemd unit, Windows
logon task or Termux service when it has one, by re-executing otherwise. The
hub sends its own executable to nodes of its own OS and architecture, and the
protectorate-<os>-<arch> files that 'make cross' writes into its --binary-dir
(default: the directory of the hub's executable) to the rest.

The node keeps exactly one previous binary (protectorate.exe.old, or
protectorate.old) and puts it back if the hub does not reach the new one within
a few minutes. Nodes already at the hub's version are skipped unless --force.
The hub's own node is updated by replacing its binary and restarting the hub.`,
		Args: func(cmd *cobra.Command, args []string) error {
			if f.all == (len(args) == 1) {
				return errors.New("name one node, or pass --all")
			}
			return cobra.MaximumNArgs(1)(cmd, args)
		},
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			list, err := cl.ListNodes(cmd.Context(), connect.NewRequest(&pb.ListNodesRequest{}))
			if err != nil {
				return err
			}
			var errs []error
			matched := false
			for _, s := range list.Msg.Nodes {
				if !f.all && s.Node.Name != strings.ToLower(args[0]) {
					continue
				}
				matched = true
				if s.Node.Local {
					if !f.all {
						return errors.New(s.Node.Name + " is the hub's own node: replace the hub's binary and restart the hub")
					}
					continue
				}
				if err := updateNode(cmd, cl, s, f); err != nil {
					errs = append(errs, fmt.Errorf("%s: %w", s.Node.Name, err))
				}
			}
			if !matched {
				return fmt.Errorf("no node %q in the inventory", args[0])
			}
			return errors.Join(errs...)
		},
	}
	fl := c.Flags()
	fl.BoolVar(&f.all, "all", false, "update every node")
	fl.BoolVar(&f.force, "force", false, "push even when the node already reports the hub's version")
	fl.BoolVar(&f.noWait, "no-wait", false, "return once the node swapped the binary, without waiting for it to report the new version")
	fl.BoolVar(&f.allowFamily, "allow-family", false, "allow nodes of tier family")
	return c
}

// updateLine renders one progress message; sending is reported only when it
// finishes.
func updateLine(p *pb.UpdateNodeProgress) string {
	switch p.Phase {
	case pb.UpdatePhase_UPDATE_PHASE_SKIPPED:
		return p.Message
	case pb.UpdatePhase_UPDATE_PHASE_SENDING:
		if p.TotalBytes == 0 || p.SentBytes < p.TotalBytes {
			return ""
		}
		return "sent " + fmtBytes(p.SentBytes)
	case pb.UpdatePhase_UPDATE_PHASE_VERIFYING:
		return "verifying"
	case pb.UpdatePhase_UPDATE_PHASE_SWAPPING:
		return fmt.Sprintf("swapped %s -> %s, restarting (%s)", p.PreviousVersion, p.Version, p.Restart)
	case pb.UpdatePhase_UPDATE_PHASE_RESTARTING:
		return "waiting for the node to come back"
	case pb.UpdatePhase_UPDATE_PHASE_CONFIRMED:
		return "reports " + p.Version
	case pb.UpdatePhase_UPDATE_PHASE_ROLLED_BACK:
		return "rolled back: " + p.Message
	}
	return ""
}

func updateNode(cmd *cobra.Command, cl protectoratev1connect.NodeServiceClient, s *pb.NodeStatus, f updateFlags) error {
	w := cmd.OutOrStdout()
	name := s.Node.Name
	if s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || s.Metrics == nil {
		return errors.New("node is not reachable (" + nodeFreshnessLabel(s) + ")")
	}
	stream, err := cl.UpdateNode(cmd.Context(), connect.NewRequest(&pb.UpdateNodeRequest{Name: name, AllowFamily: f.allowFamily, Force: f.force}))
	if err != nil {
		return err
	}
	defer stream.Close()
	var last pb.UpdatePhase
	for stream.Receive() {
		p := stream.Msg()
		last = p.Phase
		if line := updateLine(p); line != "" {
			fmt.Fprintf(w, "%s\t%s\n", name, line)
		}
		if f.noWait && p.Phase == pb.UpdatePhase_UPDATE_PHASE_SWAPPING {
			return nil
		}
	}
	if err := stream.Err(); err != nil {
		return err
	}
	if last == pb.UpdatePhase_UPDATE_PHASE_ROLLED_BACK {
		return errors.New("the node came back on its previous version")
	}
	return nil
}
