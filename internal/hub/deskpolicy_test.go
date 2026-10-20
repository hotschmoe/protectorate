package hub

import (
	"context"
	"net/http"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestDeskPolicyFlow(t *testing.T) {
	e := newJobEnv(t, 1)
	hc := &http.Client{Transport: bearer{testToken}}
	nc := protectoratev1connect.NewNodeServiceClient(hc, e.srv.URL)
	dc := protectoratev1connect.NewDeskServiceClient(hc, e.srv.URL)
	ctx := context.Background()
	click := func() error {
		_, err := dc.Click(ctx, connect.NewRequest(&pb.ClickRequest{Target: &pb.DeskTarget{Name: "peer"}, X: 1, Y: 1}))
		return err
	}
	holder := func() *pb.Lease { return e.h.leases.Blocked("desk:peer", nil) }
	status := func() *pb.DeskPolicyStatus {
		res, err := nc.GetDeskPolicy(ctx, connect.NewRequest(&pb.GetDeskPolicyRequest{Name: "peer"}))
		if err != nil {
			t.Fatal(err)
		}
		return res.Msg
	}
	set := func(p *pb.DeskPolicy) (*pb.DeskPolicyStatus, error) {
		res, err := nc.SetDeskPolicy(ctx, connect.NewRequest(&pb.SetDeskPolicyRequest{Name: "peer", Policy: p}))
		if err != nil {
			return nil, err
		}
		return res.Msg, nil
	}
	policy := &pb.DeskPolicy{Window: "00:00-23:59", Idle: durationpb.New(10 * time.Minute), Timezone: "UTC", LockAfter: true}

	if st := status(); st.Policy != nil || !st.State.Available {
		t.Fatalf("default status = %v, want no policy and available", st)
	}
	if _, err := set(&pb.DeskPolicy{Window: "bogus", Idle: durationpb.New(time.Minute)}); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("bad policy: %v", err)
	}
	if _, err := nc.SetDeskPolicy(ctx, connect.NewRequest(&pb.SetDeskPolicyRequest{Name: "hubhost", Policy: policy})); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("policy on the hub's own node: %v", err)
	}

	st, err := set(policy)
	if err != nil || st.Policy.Window != "00:00-23:59" || !st.State.Available {
		t.Fatalf("set: %v %v", st, err)
	}
	if err := click(); err != nil {
		t.Fatalf("click while away: %v", err)
	}
	if holder() == nil {
		t.Fatal("no auto lease after click")
	}

	e.peerGuard.Input.Event(false)
	e.peerClock.advance(time.Minute)
	if err := click(); connect.CodeOf(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), "user active") {
		t.Fatalf("click after the user returned: %v", err)
	}
	if holder() != nil {
		t.Error("the lease survived the user's return")
	}
	if got := e.peerDesk.got(); len(got) != 1 || got[0] != "click" {
		t.Errorf("node ran %v, want only the first click", got)
	}
	if st := status(); st.State.Available || st.State.Reason != pb.DeskReason_DESK_REASON_USER_ACTIVE {
		t.Errorf("status after return = %v", st.State)
	}

	if _, err := e.lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "desk:peer"}, Holder: "t", Ttl: durationpb.New(time.Hour)})); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("explicit lease while the user is active: %v", err)
	}
	if err := click(); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("auto lease while the user is active: %v", err)
	}

	e.peerClock.advance(11 * time.Minute)
	lease, err := e.lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "desk:peer"}, Holder: "t", Ttl: durationpb.New(time.Hour)}))
	if err != nil {
		t.Fatalf("explicit lease after the idle period: %v", err)
	}

	// The watcher ends a lease the node reports unavailable, without a command.
	e.peerGuard.Input.Event(false)
	e.peerClock.advance(time.Second)
	e.h.deskPass(ctx, false)
	if holder() != nil {
		t.Errorf("watcher left lease %s after the user returned", lease.Msg.Id)
	}

	ls, err := nc.ListNodes(ctx, connect.NewRequest(&pb.ListNodesRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	for _, s := range ls.Msg.Nodes {
		if s.Node.Name == "peer" && (s.Desk == nil || s.Desk.Reason != pb.DeskReason_DESK_REASON_USER_ACTIVE) {
			t.Errorf("node ls desk = %v", s.Desk)
		}
	}

	if st, err := set(nil); err != nil || st.Policy != nil || !st.State.Available {
		t.Fatalf("clear: %v %v", st, err)
	}
	if err := click(); err != nil {
		t.Fatalf("click after clearing the policy: %v", err)
	}
}

func TestDeskPolicyPersistence(t *testing.T) {
	path := t.TempDir() + "/desk-policies.json"
	p, err := openDeskPolicies(path)
	if err != nil {
		t.Fatal(err)
	}
	want := &pb.DeskPolicy{Window: "19:00-07:00", Idle: durationpb.New(15 * time.Minute), Timezone: "Europe/Berlin", LockAfter: true}
	if err := p.set("office", want); err != nil {
		t.Fatal(err)
	}
	q, err := openDeskPolicies(path)
	if err != nil {
		t.Fatal(err)
	}
	if got := q.get("office"); got == nil || got.Window != want.Window || got.Idle.AsDuration() != 15*time.Minute || got.Timezone != want.Timezone || !got.LockAfter {
		t.Errorf("reloaded = %v", got)
	}
	if err := q.set("office", nil); err != nil {
		t.Fatal(err)
	}
	r, err := openDeskPolicies(path)
	if err != nil || r.get("office") != nil {
		t.Errorf("cleared policy came back: %v %v", r.get("office"), err)
	}
}
