package main

import (
	"bytes"
	"context"
	"errors"
	"strings"
	"testing"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

type failingUpdates struct {
	protectoratev1connect.NodeServiceClient
	asked []string
}

func (c *failingUpdates) UpdateNode(_ context.Context, r *connect.Request[pb.UpdateNodeRequest]) (*connect.ServerStreamForClient[pb.UpdateNodeProgress], error) {
	c.asked = append(c.asked, r.Msg.Name)
	return nil, errors.New("boom")
}

func TestUpdateAllSkipsUnreachable(t *testing.T) {
	fresh := func(name string) *pb.NodeStatus {
		return &pb.NodeStatus{Node: &pb.NodeInfo{Name: name}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_FRESH, Metrics: &pb.NodeMetrics{}}
	}
	invited := &pb.NodeStatus{Node: &pb.NodeInfo{Name: "winarm", Invited: true}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_INVITED}
	down := &pb.NodeStatus{Node: &pb.NodeInfo{Name: "gone"}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE}
	hubNode := &pb.NodeStatus{Node: &pb.NodeInfo{Name: "hub", Local: true}, Freshness: pb.NodeFreshness_NODE_FRESHNESS_FRESH, Metrics: &pb.NodeMetrics{}}

	tests := []struct {
		name      string
		nodes     []*pb.NodeStatus
		args      []string
		all       bool
		wantErr   bool
		wantAsked []string
		wantOut   []string
	}{
		{name: "all skips invited and unreachable", nodes: []*pb.NodeStatus{hubNode, invited, down}, all: true,
			wantOut: []string{"winarm\tskipped: node is not reachable (invited, not yet joined)", "gone\tskipped: node is not reachable (UNREACHABLE (never seen))"}},
		{name: "all continues past skips and fails on a failed attempt", nodes: []*pb.NodeStatus{invited, fresh("a"), fresh("b")}, all: true,
			wantErr: true, wantAsked: []string{"a", "b"}, wantOut: []string{"winarm\tskipped"}},
		{name: "named invited node still errors", nodes: []*pb.NodeStatus{invited}, args: []string{"winarm"}, wantErr: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var out bytes.Buffer
			cmd := &cobra.Command{}
			cmd.SetOut(&out)
			cl := &failingUpdates{}
			err := updateNodes(cmd, cl, tt.nodes, tt.args, updateFlags{all: tt.all})
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v, wantErr %v", err, tt.wantErr)
			}
			if strings.Join(cl.asked, ",") != strings.Join(tt.wantAsked, ",") {
				t.Fatalf("asked %v, want %v", cl.asked, tt.wantAsked)
			}
			for _, w := range tt.wantOut {
				if !strings.Contains(out.String(), w) {
					t.Errorf("output %q lacks %q", out.String(), w)
				}
			}
		})
	}
}
