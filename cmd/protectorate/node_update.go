package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/node"
)

const updateStage = "updates/protectorate-next"

// binaryName is the file `make cross` writes for a platform.
func binaryName(goos, arch string) string {
	n := "protectorate-" + goos + "-" + arch
	if goos == "windows" {
		n += ".exe"
	}
	return n
}

// pickBinary chooses what to push to a node: this executable for this
// platform, otherwise the build in dir.
func pickBinary(dir, goos, arch, selfOS, selfArch, self string) string {
	if goos == selfOS && arch == selfArch && self != "" {
		return self
	}
	return filepath.Join(dir, binaryName(goos, arch))
}

// needsUpdate reports whether a node should be updated and why not if not.
func needsUpdate(nodeVersion, hubVersion string, force bool) (bool, string) {
	switch {
	case nodeVersion == "":
		return true, ""
	case nodeVersion == hubVersion && !force:
		return false, "already at " + hubVersion
	}
	return true, ""
}

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
	dir                             string
	timeout                         time.Duration
}

func nodeUpdateCmd(o *opts) *cobra.Command {
	var f updateFlags
	c := &cobra.Command{
		Use:   "update <node> | --all [--binary-dir bin]",
		Short: "Push this build to a node and restart it onto it",
		Long: `Send the right binary to a node over the file transfer, then have the node
verify it (size, SHA-256, and that it runs), swap it in, and restart itself:
through its systemd unit, Windows logon task or Termux service when it has
one, by re-executing otherwise. Run it on the hub with the hub's own build:
this executable goes to nodes of its own OS and architecture, and the
protectorate-<os>-<arch> files that 'make cross' writes into --binary-dir go
to the rest.

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
			hubVer := hubVersionOf(list.Msg.Nodes)
			if hubVer == "" {
				hubVer = version
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
				if err := updateNode(cmd, cl, s, hubVer, f); err != nil {
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
	fl.StringVar(&f.dir, "binary-dir", "bin", "directory with the protectorate-<os>-<arch> builds from `make cross`")
	fl.BoolVar(&f.force, "force", false, "push even when the node already reports the hub's version")
	fl.BoolVar(&f.noWait, "no-wait", false, "return once the node accepted the update")
	fl.DurationVar(&f.timeout, "timeout", 3*time.Minute, "how long to wait for the node to report the new version")
	fl.BoolVar(&f.allowFamily, "allow-family", false, "allow nodes of tier family")
	return c
}

func fileDigest(path string) (size uint64, sum string, err error) {
	fh, err := os.Open(path)
	if err != nil {
		return 0, "", err
	}
	defer fh.Close()
	h := sha256.New()
	n, err := io.Copy(h, fh)
	if err != nil {
		return 0, "", err
	}
	return uint64(n), hex.EncodeToString(h.Sum(nil)), nil
}

func updateNode(cmd *cobra.Command, cl protectoratev1connect.NodeServiceClient, s *pb.NodeStatus, hubVer string, f updateFlags) error {
	w := cmd.OutOrStdout()
	name := s.Node.Name
	if s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || s.Metrics == nil {
		return errors.New("node is not reachable (" + nodeFreshnessLabel(s) + ")")
	}
	if ok, why := needsUpdate(s.Metrics.AgentVersion, hubVer, f.force); !ok {
		fmt.Fprintf(w, "%s\t%s\n", name, why)
		return nil
	}
	self, _ := os.Executable()
	bin := pickBinary(f.dir, s.Node.Os, s.Node.Arch, runtime.GOOS, runtime.GOARCH, self)
	size, sum, err := fileDigest(bin)
	if err != nil {
		return fmt.Errorf("read %s (run `make cross`, or pass --binary-dir): %w", bin, err)
	}
	ctx := cmd.Context()
	start := time.Now()
	put, wire, _, err := putFile(ctx, cl, name, bin, updateStage, transferFlags{overwrite: true, allowFamily: f.allowFamily})
	if err != nil {
		return err
	}
	fmt.Fprintf(w, "%s\tsent %s (%s on the wire, %s)\n", name, filepath.Base(bin), fmtBytes(uint64(wire)), time.Since(start).Round(time.Millisecond))
	res, err := cl.ApplyUpdate(ctx, connect.NewRequest(&pb.ApplyUpdateRequest{Name: name, AllowFamily: f.allowFamily, Path: put.Path, Size: size, Sha256: sum}))
	if err != nil {
		return fmt.Errorf("apply: %w", err)
	}
	fmt.Fprintf(w, "%s\tswapped %s -> %s, restarting (%s)\n", name, res.Msg.PreviousVersion, res.Msg.Version, res.Msg.Restart)
	if f.noWait {
		return nil
	}
	if res.Msg.Version == res.Msg.PreviousVersion {
		fmt.Fprintf(w, "%s\tsame version as before; not waiting\n", name)
		return nil
	}
	return waitVersion(ctx, cl, name, res.Msg.Version, f.timeout, w)
}

func waitVersion(ctx context.Context, cl protectoratev1connect.NodeServiceClient, name, want string, timeout time.Duration, w io.Writer) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(2 * time.Second):
		}
		st, err := cl.GetNode(ctx, connect.NewRequest(&pb.GetNodeRequest{Name: name}))
		if err == nil && st.Msg.Freshness == pb.NodeFreshness_NODE_FRESHNESS_FRESH && st.Msg.Metrics != nil && st.Msg.Metrics.AgentVersion == want {
			fmt.Fprintf(w, "%s\treports %s\n", name, want)
			return nil
		}
	}
	return fmt.Errorf("did not report %s within %s; the node puts its previous binary back if the hub cannot reach the new one within a few minutes", want, timeout)
}
