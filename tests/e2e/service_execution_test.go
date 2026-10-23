package e2e

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestBinaryGeneratedServiceExecution(t *testing.T) {
	c := newCluster(t)
	binary := filepath.Join(c.root, "binary 'quoted' $literal")
	if err := os.Symlink(c.binary, binary); err != nil {
		t.Fatal(err)
	}
	home := filepath.Join(c.root, "state 'quoted' $(touch injected)")
	fileRoot := filepath.Join(c.root, "files 'quoted' $literal")
	output := filepath.Join(c.root, "services")
	address := loopback(t)
	args := []string{"service", "termux", "--binary", binary, "--home", home, "--write", output, "--", "--listen", address, "--file-root", fileRoot}
	if c.rust {
		args = append(args, "--agent-token-file", filepath.Join(c.root, "agent-token"), "--allow-plain-jobs")
	}
	c.cli(args...)
	script := filepath.Join(output, "protectorate-node", "run")
	before, err := os.ReadFile(script)
	if err != nil {
		t.Fatal(err)
	}
	if info, err := os.Stat(script); err != nil || info.Mode().Perm()&0100 == 0 {
		t.Fatalf("generated service is not executable: %v %v", info, err)
	}
	c.rejectedCLI("exists", args...)
	if after, err := os.ReadFile(script); err != nil || !bytes.Equal(before, after) {
		t.Fatalf("regeneration overwrote the service: %q %v", after, err)
	}
	launcher := *c
	launcher.binary = "/bin/sh"
	p := launcher.start("generated-service", launcher.env, script)
	c.call("NodeService", "AddNode", map[string]any{"node": map[string]any{"name": "generated", "address": address}})
	agent := protectoratev1connect.NewNodeAgentServiceClient(c.client, "http://"+address, connect.WithProtoJSON())
	waitFresh := func() {
		t.Helper()
		c.eventually(func() bool {
			req := connect.NewRequest(&pb.GetMetricsRequest{})
			req.Header().Set("Authorization", "Bearer e2e-node-secret")
			_, err := agent.GetMetrics(c.ctx, req)
			return err == nil
		})
		c.eventually(func() bool {
			return strings.Contains(c.cli("node", "show", "generated", "--json"), "NODE_FRESHNESS_FRESH")
		})
	}
	waitFresh()
	input := filepath.Join(c.root, "service-input")
	if err := os.WriteFile(input, []byte("service survived quoting"), 0600); err != nil {
		t.Fatal(err)
	}
	c.cli("node", "put", "generated", input, "persisted")
	if b, err := os.ReadFile(filepath.Join(fileRoot, "persisted")); err != nil || string(b) != "service survived quoting" {
		t.Fatalf("service did not receive its literal file-root argument: %q %v", b, err)
	}
	if _, err := os.Stat(home); err != nil {
		t.Fatalf("service did not receive its literal state home: %v", err)
	}
	c.stop(p)
	p = launcher.start("generated-service-restart", launcher.env, script)
	waitFresh()
	download := filepath.Join(c.root, "service-download")
	c.cli("node", "get", "generated", "persisted", download)
	if b, err := os.ReadFile(download); err != nil || string(b) != "service survived quoting" {
		t.Fatalf("generated service lost its files on restart: %q %v", b, err)
	}
	c.stop(p)
}
