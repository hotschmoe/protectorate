//go:build linux

package hub

import (
	"context"
	"testing"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/driver"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func TestSelectionRequiresIdle(t *testing.T) {
	for _, state := range []string{"working", "compacting", "approval", "resleeve"} {
		t.Run(state, func(t *testing.T) {
			s := &chatSleeve{sess: &stuckSession{}, turn: state == "working", compacting: state == "compacting", hubOwned: state == "resleeve"}
			if state == "approval" {
				s.approvals = map[string]bool{"pending": true}
			}
			_, err := s.setSelection(context.Background(), &pb.ModelSelection{Effort: "high"})
			if connect.CodeOf(err) != connect.CodeFailedPrecondition {
				t.Fatalf("%v", err)
			}
		})
	}
	s := &chatSleeve{sess: &stuckSession{}}
	if _, err := s.setSelection(context.Background(), nil); connect.CodeOf(err) != connect.CodeUnimplemented {
		t.Fatalf("unsupported update: %v", err)
	}
}

func TestSelectionStatusReportsRequestedAndEffective(t *testing.T) {
	c := &sleeve.Chat{Model: "actual", Effort: "low", Requested: driver.Selection{Model: "chosen", Effort: "xhigh"}, Defaults: driver.Selection{Model: "native", Effort: "medium"}}
	st := chatToProto(c)
	if st.Model != "actual" || st.ReasoningEffort != "low" || st.Settings.Requested.Model != "chosen" || st.Settings.Requested.Effort != "xhigh" || st.Settings.NativeDefaults.Model != "native" {
		t.Fatalf("conflated requested/effective/defaults: %+v", st)
	}
}
