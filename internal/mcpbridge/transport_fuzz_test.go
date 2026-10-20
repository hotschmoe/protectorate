package mcpbridge

import (
	"encoding/json"
	"testing"

	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func FuzzMCPRequest(f *testing.F) {
	for _, raw := range []string{`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`, `{"jsonrpc":"2.0","id":"1","method":"tools/call","params":{"name":"job_submit_job","arguments":{}}}`, `{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}`, `{"jsonrpc":"2.0","id":[],"method":"tools/list"}`, `[]`, `null`, `{`} {
		f.Add([]byte(raw))
	}
	f.Fuzz(func(t *testing.T, raw []byte) {
		if len(raw) > 64<<10 {
			return
		}
		req, rejected := parseRequest(raw)
		if rejected != nil {
			if rejected.Error == nil || rejected.ID == nil || !json.Valid(rejected.ID) {
				t.Fatalf("invalid error response: %+v", rejected)
			}
			return
		}
		if req.JSONRPC != "2.0" || req.Method == "" {
			t.Fatalf("accepted malformed envelope: %+v", req)
		}
		if len(req.ID) > 0 {
			var id any
			if err := json.Unmarshal(req.ID, &id); err != nil {
				t.Fatal(err)
			}
			switch id.(type) {
			case nil, string, float64:
			default:
				t.Fatalf("accepted compound request id: %s", req.ID)
			}
			encoded, err := json.Marshal(req)
			if err != nil {
				t.Fatal(err)
			}
			again, rejected := parseRequest(encoded)
			if rejected != nil || requestKey(again.ID) != requestKey(req.ID) {
				t.Fatalf("request identity changed on roundtrip: %s -> %s", req.ID, again.ID)
			}
		}
	})
}

func FuzzMCPArguments(f *testing.F) {
	for _, raw := range []string{`{}`, `null`, `{"target":{"node":"peer"},"command":["true"],"timeout":"1s"}`, `{"offset":"18446744073709551615","chunk":{"data":"AA==","rawSize":1}}`, `{"unknown":true}`, `{"resource":"peer","ttl":"-1s"}`} {
		f.Add([]byte(raw), byte(0))
		f.Add([]byte(raw), byte(1))
		f.Add([]byte(raw), byte(2))
	}
	f.Fuzz(func(t *testing.T, raw []byte, kind byte) {
		if len(raw) > 64<<10 {
			return
		}
		messages := []proto.Message{&pb.SubmitJobRequest{}, &pb.PutFileRequest{}, &pb.AcquireLeaseRequest{}}
		msg := messages[int(kind)%len(messages)]
		decoded, err := decodeArgs(msg.ProtoReflect().Descriptor(), raw)
		if err != nil {
			return
		}
		encoded, err := protojson.Marshal(decoded)
		if err != nil {
			t.Fatal(err)
		}
		if err := protojson.Unmarshal(encoded, msg); err != nil || !proto.Equal(decoded, msg) {
			t.Fatalf("MCP arguments changed across the hub wire format: %s: %v", encoded, err)
		}
	})
}
