//go:build linux

package hub

import (
	"context"
	"net/http"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/sleeve"
	"google.golang.org/protobuf/types/known/durationpb"
)

func TestMailWakeWaitE2E(t *testing.T) {
	r := newMailRig(t)
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
	r.spawnNamed(sleeve.OrchestratorName, pb.SleeveMode_SLEEVE_MODE_CHAT, "")
	r.waitIdle(r.name)
	r.waitIdle(sleeve.OrchestratorName)
	// spawnNamed stands in for SpawnEnvoy without its token; mint the one SpawnEnvoy would.
	_, secret, err := r.h.authz.Store().Create(sleeve.OrchestratorTokenName, []string{"message_*"}, time.Hour, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	worker := r.as(r.name)
	envoy := protectoratev1connect.NewMessageServiceClient(&http.Client{Transport: bearer{secret}}, r.url)
	ctx, cancel := context.WithTimeout(r.ctx, 20*time.Second)
	defer cancel()
	// Use the production clock and delivery loop, not manually driven passes.
	r.h.coord.now = time.Now
	done := make(chan struct{})
	go func() { defer close(done); r.h.coord.Run(ctx) }()
	defer func() { cancel(); <-done }()

	ask := r.post(envoy, r.name, "Report in 30 words")
	waitResult := make(chan *pb.MessageServiceWaitResponse, 1)
	waitErr := make(chan error, 1)
	go func() {
		res, err := envoy.Wait(ctx, connect.NewRequest(&pb.MessageServiceWaitRequest{Target: ask.Message.Id, Timeout: durationpb.New(15 * time.Second)}))
		if err != nil {
			waitErr <- err
			return
		}
		waitResult <- res.Msg
	}()
	waitFor(t, "worker awakened by request", func() bool { return len(r.users(r.name)) == 1 })
	r.waitIdle(r.name)
	reply, err := worker.Reply(ctx, connect.NewRequest(&pb.ReplyRequest{Id: ask.Message.Id, Body: "Finished with commit abc123"}))
	if err != nil {
		t.Fatal(err)
	}
	r.post(worker, "orchestrator", "Checks passed")
	select {
	case err := <-waitErr:
		t.Fatal(err)
	case result := <-waitResult:
		if result.TimedOut || result.Message == nil || (result.Message.Id != ask.Message.Id && result.Message.ReplyTo != ask.Message.Id) {
			t.Fatalf("wait result: %v", result)
		}
	case <-ctx.Done():
		t.Fatal(ctx.Err())
	}
	waitFor(t, "idle Envoy awakened by coalesced mail", func() bool { return len(r.users(sleeve.OrchestratorName)) == 1 })
	r.waitIdle(sleeve.OrchestratorName)
	users := r.users(sleeve.OrchestratorName)
	if !strings.Contains(users[0].text, reply.Msg.Message.Id) || !strings.Contains(users[0].text, "Checks passed") {
		t.Fatalf("mail was not coalesced: %v", users)
	}

	thread, err := envoy.Wait(ctx, connect.NewRequest(&pb.MessageServiceWaitRequest{Target: ask.Message.ThreadId, Timeout: durationpb.New(time.Second)}))
	if err != nil || thread.Msg.GetMessage() == nil {
		t.Fatalf("thread wait: %v %v", thread, err)
	}

	ackOnly := r.post(worker, "orchestrator", "Acknowledge without reply")
	if _, err := envoy.Ack(ctx, connect.NewRequest(&pb.AckRequest{Id: ackOnly.Message.Id})); err != nil {
		t.Fatal(err)
	}
	ackResult, err := worker.Wait(ctx, connect.NewRequest(&pb.MessageServiceWaitRequest{Target: ackOnly.Message.Id, Timeout: durationpb.New(time.Second)}))
	if err != nil || ackResult.Msg.GetMessage().GetId() != ackOnly.Message.Id {
		t.Fatalf("ack-only wait: %v %v", ackResult, err)
	}

	read := r.post(worker, "orchestrator", "Already read through inbox")
	if _, err := envoy.Inbox(ctx, connect.NewRequest(&pb.InboxRequest{UnreadOnly: true})); err != nil {
		t.Fatal(err)
	}
	answered := r.post(worker, "orchestrator", "Answer before injection")
	if _, err := envoy.Reply(ctx, connect.NewRequest(&pb.ReplyRequest{Id: answered.Message.Id, Body: "Received"})); err != nil {
		t.Fatal(err)
	}
	acked, err := worker.Wait(ctx, connect.NewRequest(&pb.MessageServiceWaitRequest{Target: answered.Message.Id, Timeout: durationpb.New(time.Second)}))
	if err != nil || acked.Msg.GetMessage() == nil {
		t.Fatalf("ack wait: %v %v", acked, err)
	}
	timeout, err := worker.Wait(ctx, connect.NewRequest(&pb.MessageServiceWaitRequest{Target: read.Message.Id, Timeout: durationpb.New(3 * time.Second)}))
	if err != nil || !timeout.Msg.TimedOut {
		t.Fatalf("timeout: %v %v", timeout, err)
	}
	if got := r.users(sleeve.OrchestratorName); len(got) != 1 {
		t.Fatalf("read/answered mail reinjected: %v", got)
	}
}
