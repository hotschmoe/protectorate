package hub

import (
	"errors"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// busyChoose picks node, failing with errLeaseBusy when something leases it.
func busyChoose(node string) chooseFunc {
	return func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
		if l := blocked(node, nil); l != nil {
			return "", nil, busyErr("%s is leased by %s", node, l.Holder)
		}
		return node, nil, nil
	}
}

func queueTable(t *testing.T) (*LeaseTable, *clock, *[]ticketRecord) {
	t.Helper()
	tb, clk, _ := newTable(t, filepath.Join(t.TempDir(), "leases.yaml"))
	var granted []ticketRecord
	tb.onGrant = func(q ticketRecord, _ *pb.Lease) { granted = append(granted, q) }
	return tb, clk, &granted
}

func ticketFor(holder string, prio int, ttl time.Duration) Wait {
	return Wait{Holder: holder, Notify: holder, Resource: "win", TTL: ttl, Priority: prio}
}

func TestLeaseQueueOrderAndClaim(t *testing.T) {
	tb, clk, granted := queueTable(t)
	a, err := tb.Acquire("a", time.Hour, busyChoose("win"))
	if err != nil {
		t.Fatal(err)
	}

	tickets := map[string]*pb.Lease{}
	for _, w := range []Wait{ticketFor("b", 0, 30*time.Minute), ticketFor("c", 0, time.Hour), ticketFor("d", 5, 10*time.Minute), ticketFor("e", 0, time.Hour)} {
		l, err := tb.AcquireWait(w, busyChoose("win"))
		if err != nil {
			t.Fatal(err)
		}
		if l.State != pb.LeaseState_LEASE_STATE_QUEUED || l.Id[:7] != "ticket-" {
			t.Fatalf("%s got %v, want a queued ticket", w.Holder, l)
		}
		tickets[w.Holder] = l
	}
	order := func() (out []string) {
		for _, l := range tb.List() {
			if l.State == pb.LeaseState_LEASE_STATE_QUEUED {
				out = append(out, l.Holder)
			}
		}
		return
	}
	if got := order(); !reflect.DeepEqual(got, []string{"d", "b", "c", "e"}) {
		t.Fatalf("queue order = %v, want priority first, then first come first served", got)
	}
	if l := tb.List()[1]; l.Holder != "d" || l.QueuePosition != 1 {
		t.Fatalf("list = %v", tb.List())
	}

	if err := tb.Release(a.Id); err != nil {
		t.Fatal(err)
	}
	if len(*granted) != 1 || (*granted)[0].Holder != "d" {
		t.Fatalf("granted = %v, want d at once on release", *granted)
	}
	leases := tb.List()
	if len(leases) != 4 || leases[0].Holder != "d" || leases[0].Ttl.AsDuration() != defaultClaimWindow {
		t.Fatalf("after grant: %v", leases)
	}

	// d never claims: after the window the lease passes to b
	clk.advance(defaultClaimWindow + time.Second)
	tb.mu.Lock()
	tb.sweepLocked(tb.now())
	tb.mu.Unlock()
	if len(*granted) != 2 || (*granted)[1].Holder != "b" {
		t.Fatalf("granted = %v, want b after d's window lapsed", *granted)
	}

	// b claims with a plain renew: it keeps the ttl it asked for, and keeps it past the window
	live := tb.List()[0]
	claimed, err := tb.Renew(live.Id, 0)
	if err != nil {
		t.Fatal(err)
	}
	if claimed.Ttl.AsDuration() != 30*time.Minute {
		t.Fatalf("claimed ttl = %v, want the requested 30m", claimed.Ttl.AsDuration())
	}
	clk.advance(10 * time.Minute)
	if _, err := tb.Renew(live.Id, 0); err != nil {
		t.Fatal(err)
	}
	if again, _ := tb.Renew(live.Id, 0); again.Ttl.AsDuration() != 30*time.Minute {
		t.Fatalf("a later renew must keep the claimed ttl, got %v", again.Ttl.AsDuration())
	}
	if len(*granted) != 2 {
		t.Fatalf("granted early: %v", *granted)
	}
	if err := tb.Release(live.Id); err != nil {
		t.Fatal(err)
	}
	if len(*granted) != 3 || (*granted)[2].Holder != "c" {
		t.Fatalf("granted = %v, want c next", *granted)
	}
}

func TestLeaseQueueOnlyForOptIn(t *testing.T) {
	tb, _, granted := queueTable(t)
	if _, err := tb.Acquire("a", time.Hour, busyChoose("win")); err != nil {
		t.Fatal(err)
	}
	if _, err := tb.Acquire("b", time.Hour, busyChoose("win")); connect.CodeOf(err) != connect.CodeFailedPrecondition || !errors.Is(err, errLeaseBusy) {
		t.Fatalf("a caller that did not opt in must still fail: %v", err)
	}
	if len(tb.List()) != 1 {
		t.Fatalf("a failed acquire must not queue: %v", tb.List())
	}

	notBusy := func(func(string, *int) *pb.Lease) (string, *int, error) {
		return "", nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("node is unreachable"))
	}
	if _, err := tb.AcquireWait(ticketFor("c", 0, time.Hour), notBusy); connect.CodeOf(err) != connect.CodeFailedPrecondition || len(tb.List()) != 1 {
		t.Fatalf("a failure that is not a held lease must not queue: %v %v", err, tb.List())
	}
	free, err := tb.AcquireWait(ticketFor("d", 0, time.Hour), busyChoose("other"))
	if err != nil || free.State != pb.LeaseState_LEASE_STATE_UNSPECIFIED || free.Holder != "d" {
		t.Fatalf("a free resource is granted at once, not queued: %v %v", free, err)
	}
	if len(*granted) != 0 {
		t.Fatalf("granted = %v", *granted)
	}
	if _, err := tb.AcquireWait(ticketFor("", 0, time.Hour), busyChoose("win")); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("no holder: %v", err)
	}
	if _, err := tb.AcquireWait(ticketFor("x", 0, 0), busyChoose("win")); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("no ttl: %v", err)
	}
}

func TestLeaseQueueResourcesAreIndependent(t *testing.T) {
	tb, _, granted := queueTable(t)
	if _, err := tb.Acquire("a", time.Hour, busyChoose("win")); err != nil {
		t.Fatal(err)
	}
	x, _ := tb.Acquire("x", time.Hour, busyChoose("gpubox"))
	if _, err := tb.AcquireWait(ticketFor("b", 0, time.Hour), busyChoose("win")); err != nil {
		t.Fatal(err)
	}
	if _, err := tb.AcquireWait(ticketFor("c", 0, time.Hour), busyChoose("gpubox")); err != nil {
		t.Fatal(err)
	}
	if err := tb.Release(x.Id); err != nil {
		t.Fatal(err)
	}
	if len(*granted) != 1 || (*granted)[0].Holder != "c" {
		t.Fatalf("a waiter on a free resource must not wait behind one on a held resource: %v", *granted)
	}
}

func TestLeaseQueueCancelPersistAndExpire(t *testing.T) {
	path := filepath.Join(t.TempDir(), "leases.yaml")
	tb, clk, evs := newTable(t, path)
	a, _ := tb.Acquire("a", 48*time.Hour/2, busyChoose("win"))
	b, _ := tb.AcquireWait(ticketFor("b", 0, time.Hour), busyChoose("win"))
	c, _ := tb.AcquireWait(ticketFor("c", 0, time.Hour), busyChoose("win"))

	re, err := OpenLeases(path, nil)
	if err != nil {
		t.Fatal(err)
	}
	re.now = clk.now
	var granted []string
	re.onGrant = func(q ticketRecord, _ *pb.Lease) { granted = append(granted, q.Holder) }
	if got := re.List(); len(got) != 3 || got[1].Id != b.Id || got[2].Id != c.Id {
		t.Fatalf("queue after reload = %v", got)
	}
	re.chooser = func(string) (chooseFunc, error) { return busyChoose("win"), nil }

	if err := re.Release(b.Id); err != nil {
		t.Fatal(err)
	}
	if err := re.Release(b.Id); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("cancelling a ticket twice: %v", err)
	}
	if err := re.Release(a.Id); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(granted, []string{"c"}) {
		t.Fatalf("granted after reload = %v, want c (b cancelled)", granted)
	}

	// a ticket that can never be granted is dropped after a day
	tb2, clk2, evs2 := newTable(t, filepath.Join(t.TempDir(), "leases.yaml"))
	if _, err := tb2.Acquire("a", maxLeaseTTL, busyChoose("win")); err != nil {
		t.Fatal(err)
	}
	if _, err := tb2.AcquireWait(ticketFor("b", 0, time.Hour), busyChoose("win")); err != nil {
		t.Fatal(err)
	}
	clk2.advance(maxQueueWait + time.Minute)
	if got := tb2.List(); len(got) != 0 {
		t.Fatalf("list = %v, want the expired lease and stale ticket gone", got)
	}
	kinds := map[string]int{}
	for _, e := range *evs2 {
		kinds[e.kind]++
	}
	if kinds["lease.queued"] != 1 || kinds["lease.dropped"] != 1 {
		t.Fatalf("events = %v", kinds)
	}
	kinds = map[string]int{}
	for _, e := range *evs {
		kinds[e.kind]++
	}
	if kinds["lease.queued"] != 2 {
		t.Fatalf("events = %v", kinds)
	}
}

func TestLeaseQueueOverRPC(t *testing.T) {
	e := newJobEnv(t, 1)
	acquire := func(holder string, wait bool) (*pb.Lease, error) {
		res, err := e.lc.AcquireLease(t.Context(), connect.NewRequest(&pb.AcquireLeaseRequest{
			Target: &pb.AcquireLeaseRequest_Resource{Resource: "peer"}, Holder: holder, Ttl: durationpb.New(20 * time.Minute), Wait: wait,
		}))
		if err != nil {
			return nil, err
		}
		return res.Msg, nil
	}
	a, err := acquire("alice", false)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := acquire("bob", false); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("held resource without wait: %v", err)
	}
	ticket, err := acquire("bob", true)
	if err != nil || ticket.State != pb.LeaseState_LEASE_STATE_QUEUED || ticket.QueuePosition != 1 || ticket.Resource != "peer" {
		t.Fatalf("ticket = %v %v", ticket, err)
	}
	if _, err := e.lc.ReleaseLease(t.Context(), connect.NewRequest(&pb.ReleaseLeaseRequest{Id: a.Id})); err != nil {
		t.Fatal(err)
	}
	list, _ := e.lc.ListLeases(t.Context(), connect.NewRequest(&pb.ListLeasesRequest{}))
	if len(list.Msg.Leases) != 1 || list.Msg.Leases[0].Holder != "bob" || list.Msg.Leases[0].State != pb.LeaseState_LEASE_STATE_UNSPECIFIED {
		t.Fatalf("leases after release = %v", list.Msg.Leases)
	}
	claimed, err := e.lc.RenewLease(t.Context(), connect.NewRequest(&pb.RenewLeaseRequest{Id: list.Msg.Leases[0].Id}))
	if err != nil || claimed.Msg.Ttl.AsDuration() != 20*time.Minute {
		t.Fatalf("claim = %v %v", claimed, err)
	}
}
