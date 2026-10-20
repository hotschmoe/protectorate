package e2e

import (
	"context"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/dynamicpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestBinaryStreamAuthorizationAndRevocation(t *testing.T) {
	c := newCluster(t)
	issued := c.call("TokenService", "CreateToken", map[string]any{"name": "event-observer", "scopes": []string{"events"}})
	secret := issued["secret"].(string)
	id := issued["token"].(map[string]any)["id"]
	ctx, cancel := context.WithTimeout(c.ctx, 10*time.Second)
	defer cancel()
	client := protectoratev1connect.NewEventServiceClient(c.client, c.url)
	req := connect.NewRequest(&pb.SubscribeRequest{})
	req.Header().Set("Authorization", "Bearer "+secret)
	stream, err := client.Subscribe(ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	lease := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "event-observer", "ttl": "30s"})
	observed := false
	for stream.Receive() {
		if event := stream.Msg(); event.Kind == "lease.acquired" && event.Subject == lease["id"] {
			observed = true
			break
		}
	}
	if err := stream.Err(); err != nil || !observed {
		t.Fatalf("authorized event stream did not observe the real lease: %v", err)
	}
	stream.Close()
	c.call("LeaseService", "ReleaseLease", map[string]any{"id": lease["id"]})
	c.call("TokenService", "UpdateTokenScopes", map[string]any{"id": id, "remove": []string{"events"}})
	checkedStreams, checkedAgent := 0, 0
	protoregistry.GlobalFiles.RangeFiles(func(file protoreflect.FileDescriptor) bool {
		if file.Package() != "protectorate.v1" {
			return true
		}
		for i := 0; i < file.Services().Len(); i++ {
			service := file.Services().Get(i)
			for j := 0; j < service.Methods().Len(); j++ {
				method := service.Methods().Get(j)
				agent := service.Name() == "NodeAgentService"
				if !agent && !method.IsStreamingServer() {
					continue
				}
				base, token, want := c.url, secret, connect.CodePermissionDenied
				if agent {
					base, token, want = c.nodeURL, c.token, connect.CodeUnauthenticated
				}
				path := "/" + string(service.FullName()) + "/" + string(method.Name())
				call := connect.NewClient[dynamicpb.Message, dynamicpb.Message](c.client, base+path, connect.WithProtoJSON())
				req := connect.NewRequest(dynamicpb.NewMessage(method.Input()))
				req.Header().Set("Authorization", "Bearer "+token)
				var err error
				if method.IsStreamingServer() {
					var result *connect.ServerStreamForClient[dynamicpb.Message]
					result, err = call.CallServerStream(ctx, req)
					if err == nil {
						if result.Receive() {
							t.Fatalf("unauthorized stream emitted data: %s", path)
						}
						err = result.Err()
						result.Close()
					}
				} else {
					_, err = call.CallUnary(ctx, req)
				}
				if connect.CodeOf(err) != want {
					t.Fatalf("%s with token from the wrong authority: %v want=%v", path, err, want)
				}
				if agent {
					checkedAgent++
				} else {
					checkedStreams++
				}
			}
		}
		return true
	})
	if checkedStreams < 7 || checkedAgent < 14 {
		t.Fatalf("incomplete stream/agent matrix: hub streams=%d agent methods=%d", checkedStreams, checkedAgent)
	}
	c.call("TokenService", "UpdateTokenScopes", map[string]any{"id": id, "add": []string{"events"}})
	c.call("TokenService", "RevokeToken", map[string]any{"id": id})
	c.stop(c.hub)
	c.boot()
	stream, err = client.Subscribe(c.ctx, req)
	if err == nil {
		stream.Receive()
		err = stream.Err()
		stream.Close()
	}
	if connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("revoked stream token returned after restart: %v", err)
	}
}
