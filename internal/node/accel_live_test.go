package node

import (
	"context"
	"testing"
	"time"
)

// TestAccelLive runs this machine's probe; with -v it shows what the node
// would report, which is how the fixtures were checked against real nodes.
func TestAccelLive(t *testing.T) {
	p := newAccelProbe()
	if p == nil {
		t.Skip("no accelerator probe on this OS")
	}
	ctx := context.Background()
	devs := p.inventory(ctx)
	p.sample(ctx, devs)
	time.Sleep(300 * time.Millisecond)
	gpus, npus := p.sample(ctx, devs)
	for _, g := range gpus {
		t.Logf("gpu %+v", g)
		if g.Name == "" || g.Vendor == "" {
			t.Errorf("gpu without name or vendor: %v", g)
		}
	}
	for _, n := range npus {
		t.Logf("npu %+v", n)
		if n.Name == "" || n.Vendor == "" || n.Kind != "npu" {
			t.Errorf("npu without name, vendor or kind: %v", n)
		}
	}
}
