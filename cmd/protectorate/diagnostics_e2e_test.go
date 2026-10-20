package main

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestDiagnosticsEndToEnd(t *testing.T) {
	binary := os.Getenv("PROT_DIAGNOSTICS_BINARY")
	if binary == "" || runtime.GOOS != "linux" {
		t.Skip("set PROT_DIAGNOSTICS_BINARY to a built Linux protectorate binary")
	}
	binary, err := filepath.Abs(binary)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()
	client := &http.Client{Timeout: 10 * time.Second}
	start := func(role string) (string, string) {
		t.Helper()
		home := t.TempDir()
		ln, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			t.Fatal(err)
		}
		addr := ln.Addr().String()
		ln.Close()
		args := []string{role, "serve", "--listen", addr}
		if role == "hub" {
			args = append(args, "--tmux-socket", "footprint-"+filepath.Base(home))
		}
		cmd := exec.CommandContext(ctx, binary, args...)
		cmd.Env = append(os.Environ(), "PROTECTORATE_HOME="+home, "PROTECTORATE_NODE_TOKEN=diagnostics-node",
			"CLAUDE_CONFIG_DIR="+filepath.Join(home, "claude"), "CODEX_HOME="+filepath.Join(home, "codex"), "GROK_HOME="+filepath.Join(home, "grok"))
		log, err := os.Create(filepath.Join(home, "process.log"))
		if err != nil {
			t.Fatal(err)
		}
		cmd.Stdout, cmd.Stderr = log, log
		if err := cmd.Start(); err != nil {
			log.Close()
			t.Fatal(err)
		}
		done := make(chan error, 1)
		go func() { done <- cmd.Wait() }()
		t.Cleanup(func() {
			_ = cmd.Process.Signal(os.Interrupt)
			select {
			case <-done:
			case <-time.After(5 * time.Second):
				_ = cmd.Process.Kill()
				<-done
			}
			log.Close()
		})
		url := "http://" + addr
		for {
			req, err := http.NewRequestWithContext(ctx, http.MethodGet, url+"/debug/metrics", nil)
			if err != nil {
				t.Fatal(err)
			}
			res, err := client.Do(req)
			if err == nil {
				res.Body.Close()
				return url, home
			}
			select {
			case <-ctx.Done():
				t.Fatalf("start %s: %v", role, ctx.Err())
			case <-time.After(50 * time.Millisecond):
			}
		}
	}
	hubURL, home := start("hub")
	token, err := os.ReadFile(filepath.Join(home, "token"))
	if err != nil {
		t.Fatal(err)
	}
	owner := strings.TrimSpace(string(token))
	check := func(url, path, secret string, status int) []byte {
		t.Helper()
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, url+path, nil)
		if err != nil {
			t.Fatal(err)
		}
		if secret != "" {
			req.Header.Set("Authorization", "Bearer "+secret)
		}
		res, err := client.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer res.Body.Close()
		body, err := io.ReadAll(res.Body)
		if err != nil || res.StatusCode != status {
			t.Fatalf("%s: status=%d want=%d err=%v body=%q", path, res.StatusCode, status, err, body)
		}
		if status == http.StatusOK && res.Header.Get("Cache-Control") != "no-store" {
			t.Fatalf("%s: cache policy %q", path, res.Header.Get("Cache-Control"))
		}
		return body
	}
	tokens := protectoratev1connect.NewTokenServiceClient(client, hubURL)
	create := connect.NewRequest(&pb.CreateTokenRequest{Name: "diagnostics-scoped", Scopes: []string{"*"}})
	create.Header().Set("Authorization", "Bearer "+owner)
	scoped, err := tokens.CreateToken(ctx, create)
	if err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{"/debug/metrics", "/debug/pprof/heap", "/debug/pprof/goroutine", "/debug/pprof/profile?seconds=1"} {
		for _, secret := range []string{"", "invalid", scoped.Msg.Secret} {
			check(hubURL, path, secret, http.StatusForbidden)
		}
		body := check(hubURL, path, owner, http.StatusOK)
		if len(body) == 0 {
			t.Fatalf("%s: empty diagnostics", path)
		}
		if path == "/debug/metrics" {
			var values map[string]uint64
			if err := json.Unmarshal(body, &values); err != nil || values["/sched/goroutines:goroutines"] == 0 {
				t.Fatalf("metrics: %s, %v", body, err)
			}
		}
	}
	nodeURL, _ := start("node")
	check(nodeURL, "/debug/metrics", "diagnostics-node", http.StatusForbidden)
	nodes := protectoratev1connect.NewNodeServiceClient(client, hubURL)
	add := connect.NewRequest(&pb.AddNodeRequest{Node: &pb.NodeInfo{Name: "diagnostics-node", Address: nodeURL, Tier: "shared"}})
	add.Header().Set("Authorization", "Bearer "+owner)
	if _, err := nodes.AddNode(ctx, add); err != nil {
		t.Fatal(err)
	}
	check(hubURL, "/debug/nodes/diagnostics-node/metrics", scoped.Msg.Secret, http.StatusForbidden)
	check(hubURL, "/debug/nodes/missing/metrics", owner, http.StatusNotFound)
	check(hubURL, "/debug/nodes/diagnostics-node/metrics", owner, http.StatusUnauthorized)
	agent := protectoratev1connect.NewNodeAgentServiceClient(client, nodeURL)
	want := make([]byte, (1<<20)+137)
	for i := range want {
		want[i] = byte((i >> 13) ^ i)
	}
	input := filepath.Join(t.TempDir(), "output.bin")
	if err := os.WriteFile(input, want, 0o600); err != nil {
		t.Fatal(err)
	}
	run := connect.NewRequest(&pb.RunJobRequest{JobId: "diagnostics-output", Mode: pb.JobMode_JOB_MODE_PLAIN,
		Command: []string{"cat", input}})
	run.Header().Set("Authorization", "Bearer diagnostics-node")
	stream, err := agent.RunJob(ctx, run)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	var output bytes.Buffer
	var finished *pb.JobFinished
	for stream.Receive() {
		if out := stream.Msg().GetOutput(); out != nil {
			output.Write(out.Data)
		}
		if fin := stream.Msg().GetFinished(); fin != nil {
			finished = fin
		}
	}
	if err := stream.Err(); err != nil || !bytes.Equal(output.Bytes(), want) || finished == nil || finished.End != pb.JobEnd_JOB_END_EXITED || finished.ExitCode != 0 {
		t.Fatalf("stream: bytes=%d finished=%v err=%v", output.Len(), finished, err)
	}
}
