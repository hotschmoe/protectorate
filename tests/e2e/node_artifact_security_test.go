package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestBinaryJobArtifactsStayInWorkspace(t *testing.T) {
	c := newCluster(t)
	secretDir := filepath.Join(c.root, "private")
	if err := os.Mkdir(secretDir, 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(secretDir, "secret"), []byte("node credential"), 0600); err != nil {
		t.Fatal(err)
	}
	client := protectoratev1connect.NewNodeAgentServiceClient(c.client, c.nodeURL)
	for i, pattern := range []string{"escape/secret", "escape/*", "escape"} {
		t.Run(pattern, func(t *testing.T) {
			req := connect.NewRequest(&pb.RunJobRequest{
				JobId: fmt.Sprintf("artifacts-%d", i), Mode: pb.JobMode_JOB_MODE_PLAIN,
				Command:       []string{"sh", "-c", `ln -s "$1" escape; mkdir output; printf result > output/result; ln -s "$1/secret" output/link`, "sh", secretDir},
				ArtifactGlobs: []string{pattern, "output"},
			})
			req.Header().Set("Authorization", "Bearer e2e-node-secret")
			stream, err := client.RunJob(c.ctx, req)
			if err != nil {
				t.Fatal(err)
			}
			defer stream.Close()
			var result string
			var finished *pb.JobFinished
			for stream.Receive() {
				if artifact := stream.Msg().GetArtifact(); artifact != nil {
					if artifact.Path != "output/result" {
						t.Fatalf("artifact escaped the workspace: %q", artifact.Path)
					}
					result += string(artifact.Data)
				}
				if fin := stream.Msg().GetFinished(); fin != nil {
					finished = fin
				}
			}
			if err := stream.Err(); err != nil || finished == nil || finished.ExitCode != 0 || finished.Error != "" || result != "result" {
				t.Fatalf("artifact collection: result=%q finished=%v err=%v", result, finished, err)
			}
		})
	}
}
