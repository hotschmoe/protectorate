package main

import (
	"strings"
	"testing"
	"time"

	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func TestWriteMail(t *testing.T) {
	at := timestamppb.New(time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC))
	req := &pb.MailMessage{Id: "m42", ThreadId: "t7", From: "api", To: "project:lib", Kind: pb.MessageKind_MESSAGE_KIND_REQUEST,
		AckRequired: true, Body: "please export Foo\nthanks", CreatedAt: at,
		Deliveries: []*pb.MailDelivery{{Recipient: "libdev", DeliveredAt: at}, {Recipient: "libdev2", AckedAt: at}}, EscalatedAt: at}

	tests := []struct {
		name string
		msgs []*pb.MailMessage
		want []string
	}{
		{"empty", nil, []string{"no messages"}},
		{"request", []*pb.MailMessage{req}, []string{
			"m42\tt7\tapi -> project:lib\trequest ack-required delivered:libdev acked:libdev2 escalated\t",
			"\t  please export Foo", "\t  thanks"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var sb strings.Builder
			writeMail(&sb, tt.msgs)
			for _, want := range tt.want {
				if !strings.Contains(sb.String(), want) {
					t.Errorf("output lacks %q:\n%s", want, sb.String())
				}
			}
		})
	}
}
