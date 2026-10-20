package e2e

import "testing"

func (c *cluster) leaseQueueScenario(t *testing.T) {
	previousT := c.t
	c.t = t
	defer func() { c.t = previousT }()
	held := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "queue-a", "ttl": "20s"})
	for _, entry := range []struct {
		holder   string
		priority int
	}{{"queue-b", 0}, {"queue-c", 0}, {"queue-high", 5}} {
		ticket := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": entry.holder, "priority": entry.priority, "wait": true, "ttl": "20s"})
		if ticket["state"] != "LEASE_STATE_QUEUED" {
			t.Fatalf("expected queue ticket: %v", ticket)
		}
	}
	c.stop(c.hub)
	c.boot()
	c.call("LeaseService", "ReleaseLease", map[string]any{"id": held["id"]})
	for _, holder := range []string{"queue-high", "queue-b", "queue-c"} {
		list := c.call("LeaseService", "ListLeases", map[string]any{})
		leases, ok := list["leases"].([]any)
		if !ok || len(leases) == 0 {
			t.Fatalf("queue lost after restart: %v", list)
		}
		granted := leases[0].(map[string]any)
		if granted["holder"] != holder || granted["state"] == "LEASE_STATE_QUEUED" {
			t.Fatalf("FIFO/priority grant=%v want holder=%s", granted, holder)
		}
		claimed := c.call("LeaseService", "RenewLease", map[string]any{"id": granted["id"]})
		if claimed["ttl"] != "20s" {
			t.Fatalf("claim did not restore requested TTL: %v", claimed)
		}
		c.call("LeaseService", "ReleaseLease", map[string]any{"id": granted["id"]})
	}
}
