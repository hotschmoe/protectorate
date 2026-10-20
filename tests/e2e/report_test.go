package e2e

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"google.golang.org/protobuf/types/known/durationpb"
)

func TestBinarySleeveReports(t *testing.T) {
	c := newCluster(t)
	c.stop(c.hub)
	var records []any
	for _, name := range []string{"reporter", "other", "exited", "killed", "kept", "terminal"} {
		state := "running"
		if name == "exited" || name == "kept" || name == "terminal" {
			state = "exited"
		}
		record := map[string]any{"name": name, "harness": "claude", "mode": "chat", "workspace": c.root,
			"state": state, "created": "2026-01-01T00:00:00Z", "chat": map[string]any{"state": "CHAT_STATE_IDLE"}}
		if name == "kept" || name == "terminal" {
			delete(record, "chat")
		}
		if name == "terminal" {
			record["mode"] = "terminal"
		}
		records = append(records, record)
	}
	b, err := json.Marshal(map[string]any{"sleeves": records})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(c.home, "sleeves.json"), b, 0600); err != nil {
		t.Fatal(err)
	}
	c.boot()
	owner := c.token
	identity := c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:reporter", "scopes": []string{"read"}})["secret"].(string)
	other := c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:other", "scopes": []string{"read", "chat_report"}})["secret"].(string)
	ordinary := c.call("TokenService", "CreateToken", map[string]any{"name": "coordinator", "scopes": []string{"read", "chat_*"}})["secret"].(string)
	report := map[string]any{"sleeve": "reporter", "status": "REPORT_STATUS_PARTIAL", "summary": "Core implemented; host validation remains."}
	for _, token := range []string{owner, ordinary, other} {
		c.token = token
		if status := c.rpc("ChatService", "Report", report, nil); status != 403 {
			t.Fatalf("forged report admitted: HTTP %d", status)
		}
	}
	c.token = identity
	if status := c.rpc("ChatService", "Report", map[string]any{"status": "REPORT_STATUS_COMPLETE", "summary": strings.Repeat("x", (32<<10)+1)}, nil); status != 400 {
		t.Fatalf("oversize report admitted: HTTP %d", status)
	}
	if status := c.rpc("ChatService", "Report", map[string]any{"status": "REPORT_STATUS_UNSPECIFIED"}, nil); status != 400 {
		t.Fatalf("unmarked result admitted: HTTP %d", status)
	}
	c.token = owner
	c.rejectedCLI("deadline_exceeded", "chat", "wait", "reporter", "--until", "report", "--timeout", "100ms")
	c.rejectedCLI("exited before", "chat", "wait", "exited", "--until", "report", "--timeout", "1s")
	if got := c.cli("chat", "wait", "reporter", "--until", "idle", "--timeout", "1s"); !strings.Contains(got, "idle") {
		t.Fatalf("idle wait: %s", got)
	}
	eventCtx, stopEvents := context.WithTimeout(c.ctx, 5*time.Second)
	defer stopEvents()
	eventClient := protectoratev1connect.NewEventServiceClient(&http.Client{}, c.url)
	eventReq := connect.NewRequest(&pb.SubscribeRequest{})
	eventReq.Header().Set("Authorization", "Bearer "+owner)
	events, err := eventClient.Subscribe(eventCtx, eventReq)
	if err != nil {
		t.Fatal(err)
	}
	defer events.Close()
	if !events.Receive() {
		t.Fatalf("event stream did not open: %v", events.Err())
	}
	killedWaiter := c.start("wait-killed", c.env, "--hub", c.url, "chat", "wait", "killed", "--until", "report", "--timeout", "5s")
	select {
	case err := <-killedWaiter.done:
		t.Fatalf("wait returned before kill: %v", err)
	case <-time.After(150 * time.Millisecond):
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "killed"})
	select {
	case err := <-killedWaiter.done:
		if err == nil {
			t.Fatal("wait succeeded when the sleeve was killed without a report")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("wait did not notice sleeve kill promptly")
	}
	if output, _ := os.ReadFile(killedWaiter.log.Name()); !strings.Contains(string(output), "wait for killed") || strings.Contains(string(output), "deadline_exceeded") {
		t.Fatalf("unclear kill error: %s", output)
	}
	for events.Receive() {
		if ev := events.Msg(); ev.Kind == "sleeve.killed" && ev.Subject == "killed" {
			break
		}
	}
	if events.Err() != nil {
		t.Fatalf("no kill event: %v", events.Err())
	}
	waiter := c.start("wait-reports", c.env, "--hub", c.url, "chat", "wait", "reporter", "other", "--until", "report", "--timeout", "5s")
	select {
	case err := <-waiter.done:
		t.Fatalf("wait returned before a report: %v", err)
	case <-time.After(150 * time.Millisecond):
	}
	c.token = identity
	first := c.call("ChatService", "Report", report)["result"].(map[string]any)
	if first["chatSeq"] == nil || first["time"] == nil {
		t.Fatalf("report has no position/time: %v", first)
	}
	for events.Receive() {
		if ev := events.Msg(); ev.Kind == "sleeve.report" && ev.Subject == "reporter" {
			if ev.Payload.GetFields()["status"].GetStringValue() != "partial" {
				t.Fatalf("report event status: %v", ev)
			}
			break
		}
	}
	if events.Err() != nil {
		t.Fatalf("no report event: %v", events.Err())
	}
	c.eventually(func() bool {
		b, _ := os.ReadFile(waiter.log.Name())
		return strings.Contains(string(b), "Core implemented")
	})
	select {
	case err := <-waiter.done:
		t.Fatalf("multi-wait returned before other reported: %v", err)
	default:
	}
	c.token = other
	c.call("ChatService", "Report", map[string]any{"status": "REPORT_STATUS_BLOCKED", "summary": "Needs owner input."})
	select {
	case err := <-waiter.done:
		if err != nil {
			t.Fatalf("wait reports: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("wait did not return promptly after all reports landed")
	}
	output, _ := os.ReadFile(waiter.log.Name())
	if !strings.Contains(string(output), "Needs owner input.") {
		t.Fatalf("multi-wait omitted blocked summary: %s", output)
	}
	c.token = identity
	summaryFile := filepath.Join(c.root, "summary.txt")
	if err := os.WriteFile(summaryFile, []byte("Finished all requested work."), 0600); err != nil {
		t.Fatal(err)
	}
	identityFile := filepath.Join(c.root, "reporter-token")
	if err := os.WriteFile(identityFile, []byte(identity), 0600); err != nil {
		t.Fatal(err)
	}
	if got := c.cli("--token-file", identityFile, "report", "--status", "complete", "--summary-file", summaryFile); !strings.Contains(got, "reported complete") {
		t.Fatalf("report CLI: %s", got)
	}
	c.token = owner
	latest := c.call("SleeveService", "GetSleeve", map[string]any{"name": "reporter"})["sleeve"].(map[string]any)["result"].(map[string]any)
	if latest["status"] != "REPORT_STATUS_COMPLETE" || latest["chatSeq"] == first["chatSeq"] {
		t.Fatalf("report did not replace the earlier result: %v", latest)
	}
	c.stop(c.hub)
	c.boot()
	restored := c.call("SleeveService", "GetSleeve", map[string]any{"name": "reporter"})["sleeve"].(map[string]any)
	if restored["chat"].(map[string]any)["lastSeq"] != restored["result"].(map[string]any)["chatSeq"] {
		t.Fatalf("restart exposed a stale chat cursor before the report: %v", restored)
	}
	if got := c.cli("chat", "wait", "reporter", "--until", "report", "--timeout", "1s"); !strings.Contains(got, "Finished all requested work.") {
		t.Fatalf("report lost across restart: %s", got)
	}
	for _, args := range [][]string{{"sleeve", "show", "reporter"}, {"sleeve", "ls", "--json"}} {
		if got := c.cli(args...); !strings.Contains(got, "Finished all requested work.") {
			t.Fatalf("%v missing persisted result: %s", args, got)
		}
	}
	client := protectoratev1connect.NewChatServiceClient(&http.Client{}, c.url)
	for _, name := range []string{"kept", "terminal"} {
		keptIdentity := c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:" + name, "scopes": []string{"read", "chat_report"}})["secret"].(string)
		c.token = keptIdentity
		c.call("ChatService", "Report", map[string]any{"status": "REPORT_STATUS_COMPLETE", "summary": "Retained result."})
		c.token = owner
		if got := c.cli("chat", "wait", name, "--until", "report", "--timeout", "1s"); !strings.Contains(got, "Retained result.") {
			t.Fatalf("exited sleeve %s lost its result: %s", name, got)
		}
		keptReq := connect.NewRequest(&pb.WatchRequest{Sleeve: name})
		keptReq.Header().Set("Authorization", "Bearer "+owner)
		keptCtx, stopKept := context.WithTimeout(c.ctx, 3*time.Second)
		defer stopKept()
		keptHistory, err := client.Watch(keptCtx, keptReq)
		if err != nil {
			t.Fatal(err)
		}
		if !keptHistory.Receive() || keptHistory.Msg().GetReport().GetSummary() != "Retained result." {
			t.Fatalf("exited sleeve %s report history: %v", name, keptHistory.Err())
		}
		keptHistory.Close()
		if kept := c.call("SleeveService", "GetSleeve", map[string]any{"name": name})["sleeve"].(map[string]any); kept["chat"] != nil {
			t.Fatalf("reading retained report history recreated chat state: %v", kept)
		}
	}
	watchReq := connect.NewRequest(&pb.WatchRequest{Sleeve: "reporter"})
	watchReq.Header().Set("Authorization", "Bearer "+owner)
	watchCtx, stopWatch := context.WithTimeout(c.ctx, 3*time.Second)
	defer stopWatch()
	history, err := client.Watch(watchCtx, watchReq)
	if err != nil {
		t.Fatal(err)
	}
	defer history.Close()
	var statuses []pb.ReportStatus
	for history.Receive() {
		ev := history.Msg()
		if r := ev.GetReport(); r != nil {
			if r.ChatSeq != ev.Seq || !r.Time.AsTime().Equal(ev.Time.AsTime()) {
				t.Fatalf("report position differs from its history: %v", ev)
			}
			statuses = append(statuses, r.Status)
			if len(statuses) == 2 {
				break
			}
		}
	}
	if fmt.Sprint(statuses) != "[REPORT_STATUS_PARTIAL REPORT_STATUS_COMPLETE]" {
		t.Fatalf("replacement report erased history: %v (%v)", statuses, history.Err())
	}
	c.token = identity
	if status := c.rpc("ChatService", "Wait", map[string]any{"sleeve": "other", "until": "WAIT_UNTIL_REPORT"}, nil); status != 403 {
		t.Fatalf("cross-sleeve wait leaked report: %d", status)
	}
	c.token = owner
	waitReq := connect.NewRequest(&pb.WaitRequest{Sleeve: "other", Until: pb.WaitUntil_WAIT_UNTIL_REPORT, Timeout: durationpb.New(time.Second)})
	waitReq.Header().Set("Authorization", "Bearer "+ordinary)
	if res, err := client.Wait(c.ctx, waitReq); err != nil || res.Msg.Result.Status != pb.ReportStatus_REPORT_STATUS_BLOCKED {
		t.Fatalf("read-only coordinator wait: %v %v", res, err)
	}
	t.Run("MCP wait-for-report", func(t *testing.T) {
		peer := startCLIMCP(t, c, "--allow", "chat_wait")
		peer.send(1, "initialize", map[string]any{"protocolVersion": "2024-11-05", "clientInfo": map[string]any{"name": "report-e2e", "version": "1"}, "capabilities": map[string]any{}})
		peer.read()
		peer.send(2, "tools/call", map[string]any{"name": "chat_wait", "arguments": map[string]any{"sleeve": "reporter", "until": "WAIT_UNTIL_REPORT", "timeout": "1s"}})
		if res := peer.read(); res.Error != nil || !strings.Contains(string(res.Result), "Finished all requested work.") || strings.Contains(string(res.Result), `"isError":true`) {
			t.Fatalf("MCP wait-for-report: %+v", res)
		}
	})
}
