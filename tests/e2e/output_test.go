package e2e

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func (c *cluster) outputScenario(t *testing.T) {
	want := make([]byte, (1<<20)+137)
	for i := range want {
		want[i] = byte((i >> 13) ^ i)
	}
	path := filepath.Join(c.root, "stream-input")
	if err := os.WriteFile(path, want, 0600); err != nil {
		t.Fatal(err)
	}
	req := connect.NewRequest(&pb.RunJobRequest{JobId: "large-output", Mode: pb.JobMode_JOB_MODE_PLAIN, Command: []string{"cat", path}})
	req.Header().Set("Authorization", "Bearer e2e-node-secret")
	client := protectoratev1connect.NewNodeAgentServiceClient(c.client, c.nodeURL, connect.WithProtoJSON())
	stream, err := client.RunJob(c.ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	var got bytes.Buffer
	var finished *pb.JobFinished
	for stream.Receive() {
		if out := stream.Msg().GetOutput(); out != nil {
			got.Write(out.Data)
		}
		if fin := stream.Msg().GetFinished(); fin != nil {
			finished = fin
		}
	}
	if err := stream.Err(); err != nil || !bytes.Equal(got.Bytes(), want) || finished == nil || finished.End != pb.JobEnd_JOB_END_EXITED || finished.ExitCode != 0 {
		t.Fatalf("job output: bytes=%d finished=%v err=%v", got.Len(), finished, err)
	}
}
