package main

import (
	"bytes"
	"strings"
	"testing"

	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func TestChatRenderer(t *testing.T) {
	text := func(id, d string) *pb.ChatEvent {
		return &pb.ChatEvent{Time: timestamppb.Now(), Body: &pb.ChatEvent_Text{Text: &pb.AssistantText{MessageId: id, Delta: d}}}
	}
	evs := []*pb.ChatEvent{
		{Time: timestamppb.Now(), Body: &pb.ChatEvent_User{User: &pb.UserMessage{From: "owner", Text: "hi"}}},
		text("m1", "hel"), text("m1", "lo"), text("m2", "second"),
		{Time: timestamppb.Now(), Body: &pb.ChatEvent_ToolCall{ToolCall: &pb.ToolCall{Name: "Bash", InputJson: "{\n \"command\": \"ls\"}"}}},
		{Time: timestamppb.Now(), Body: &pb.ChatEvent_Settled{Settled: &pb.TurnSettled{StopReason: "end_turn"}}},
	}
	var buf bytes.Buffer
	var r chatRenderer
	for _, e := range evs {
		r.render(&buf, e)
	}
	r.flush(&buf)
	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	if len(lines) != 5 {
		t.Fatalf("got %d lines:\n%s", len(lines), buf.String())
	}
	for i, want := range []string{"user      owner: hi", "assistant hello", "assistant second", `tool      Bash { "command": "ls"}`, "settled   end_turn"} {
		if !strings.HasSuffix(lines[i], want) {
			t.Errorf("line %d = %q, want suffix %q", i, lines[i], want)
		}
	}
}
