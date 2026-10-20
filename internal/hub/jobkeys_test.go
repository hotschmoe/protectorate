package hub

import (
	"context"
	"errors"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/jobtest"
)

const keyA, keyB = "request-aaaa-0001", "request-bbbb-0002"

type fakeSubmits struct {
	created atomic.Int32
	gate    chan struct{}
	entered chan struct{}
	fail    error
}

func (f *fakeSubmits) create() (*pb.Job, error) {
	n := f.created.Add(1)
	if f.entered != nil {
		f.entered <- struct{}{}
	}
	if f.gate != nil {
		<-f.gate
	}
	if f.fail != nil {
		return nil, f.fail
	}
	return &pb.Job{Id: "job-" + string(rune('0'+n))}, nil
}

func getJob(id string) (*pb.Job, error) {
	return &pb.Job{Id: id, State: pb.JobState_JOB_STATE_RUNNING}, nil
}

func TestSubmitKeyDedupe(t *testing.T) {
	tests := []struct {
		name     string
		keys     []string
		advance  time.Duration
		fail     error
		wantJobs []string
		wantMade int32
	}{
		{"miss creates", []string{keyA}, 0, nil, []string{"job-1"}, 1},
		{"hit returns the same job", []string{keyA, keyA}, 0, nil, []string{"job-1", "job-1"}, 1},
		{"other key misses", []string{keyA, keyB}, 0, nil, []string{"job-1", "job-2"}, 2},
		{"hit inside the window", []string{keyA, keyA}, requestIDWindow - time.Second, nil, []string{"job-1", "job-1"}, 1},
		{"window expiry creates again", []string{keyA, keyA}, requestIDWindow, nil, []string{"job-1", "job-2"}, 2},
		{"failed submit is not remembered", []string{keyA, keyA}, 0, errors.New("no node"), nil, 2},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			clk := &clock{t: t0}
			k := newSubmitKeys(clk.now)
			f := &fakeSubmits{fail: tt.fail}
			var got []string
			for i, key := range tt.keys {
				if i == 1 {
					clk.advance(tt.advance)
				}
				j, err := k.submit(context.Background(), key, f.create, getJob)
				if tt.fail != nil {
					if err == nil {
						t.Fatal("want an error")
					}
					continue
				}
				if err != nil {
					t.Fatal(err)
				}
				got = append(got, j.Id)
			}
			if f.created.Load() != tt.wantMade {
				t.Fatalf("created %d jobs, want %d", f.created.Load(), tt.wantMade)
			}
			if len(got) != len(tt.wantJobs) {
				t.Fatalf("jobs = %v, want %v", got, tt.wantJobs)
			}
			for i := range got {
				if got[i] != tt.wantJobs[i] {
					t.Fatalf("jobs = %v, want %v", got, tt.wantJobs)
				}
			}
		})
	}
}

func TestSubmitKeyValidation(t *testing.T) {
	k := newSubmitKeys(time.Now)
	for _, key := range []string{"short", "has space in it", "bad/slash/key1", string(make([]byte, 129))} {
		if _, err := k.submit(context.Background(), key, (&fakeSubmits{}).create, getJob); connect.CodeOf(err) != connect.CodeInvalidArgument {
			t.Errorf("submit %q: %v", key, err)
		}
		if _, err := k.cancel(context.Background(), key, getJob); connect.CodeOf(err) != connect.CodeInvalidArgument {
			t.Errorf("cancel %q: %v", key, err)
		}
	}
}

func TestSubmitKeyConcurrentDuplicatesShareOneJob(t *testing.T) {
	k := newSubmitKeys(time.Now)
	f := &fakeSubmits{gate: make(chan struct{}), entered: make(chan struct{}, 4)}
	results := make(chan string, 2)
	submit := func() {
		j, err := k.submit(context.Background(), keyA, f.create, getJob)
		if err != nil {
			results <- err.Error()
			return
		}
		results <- j.Id
	}
	go submit()
	<-f.entered
	go submit()
	time.Sleep(50 * time.Millisecond)
	close(f.gate)
	for range 2 {
		if id := <-results; id != "job-1" {
			t.Fatalf("result = %q", id)
		}
	}
	if f.created.Load() != 1 {
		t.Fatalf("created %d jobs", f.created.Load())
	}
}

func TestCancelByKeyRacesSubmit(t *testing.T) {
	tests := []struct {
		name string
		// order is the sequence of events: "submit", "cancel", "release".
		cancelBefore bool
		inflight     bool
		failSubmit   error
		wantCanceled string
		wantSubmit   connect.Code
		wantCancel   connect.Code
	}{
		{name: "cancel after submit finished", wantCanceled: "job-1"},
		{name: "cancel while submit in flight", inflight: true, wantCanceled: "job-1"},
		{name: "cancel before submit arrives", cancelBefore: true, wantSubmit: connect.CodeAborted, wantCancel: connect.CodeNotFound},
		{name: "in-flight submit fails after cancel", inflight: true, failSubmit: errors.New("no node"), wantSubmit: connect.CodeUnknown, wantCancel: connect.CodeNotFound},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx := context.Background()
			k := newSubmitKeys(time.Now)
			f := &fakeSubmits{fail: tt.failSubmit}
			var canceled atomic.Value
			cancelJob := func(id string) (*pb.Job, error) {
				canceled.Store(id)
				return &pb.Job{Id: id, State: pb.JobState_JOB_STATE_CANCELED}, nil
			}

			var submitErr, cancelErr error
			switch {
			case tt.cancelBefore:
				_, cancelErr = k.cancel(ctx, keyA, cancelJob)
				_, submitErr = k.submit(ctx, keyA, f.create, getJob)
			case tt.inflight:
				f.gate, f.entered = make(chan struct{}), make(chan struct{}, 1)
				submitDone := make(chan error, 1)
				go func() {
					_, err := k.submit(ctx, keyA, f.create, getJob)
					submitDone <- err
				}()
				<-f.entered
				cancelDone := make(chan error, 1)
				go func() {
					_, err := k.cancel(ctx, keyA, cancelJob)
					cancelDone <- err
				}()
				select {
				case <-cancelDone:
					t.Fatal("cancel returned before the in-flight submit finished")
				case <-time.After(50 * time.Millisecond):
				}
				close(f.gate)
				submitErr, cancelErr = <-submitDone, <-cancelDone
			default:
				if _, submitErr = k.submit(ctx, keyA, f.create, getJob); submitErr != nil {
					t.Fatal(submitErr)
				}
				_, cancelErr = k.cancel(ctx, keyA, cancelJob)
			}
			if connect.CodeOf(submitErr) != tt.wantSubmit && !(submitErr == nil && tt.wantSubmit == 0) {
				t.Errorf("submit err = %v, want code %v", submitErr, tt.wantSubmit)
			}
			if tt.wantCancel == 0 && cancelErr != nil || tt.wantCancel != 0 && connect.CodeOf(cancelErr) != tt.wantCancel {
				t.Errorf("cancel err = %v, want code %v", cancelErr, tt.wantCancel)
			}
			got, _ := canceled.Load().(string)
			if got != tt.wantCanceled {
				t.Errorf("canceled job %q, want %q", got, tt.wantCanceled)
			}
			if tt.wantCanceled == "" {
				if _, err := k.submit(ctx, keyA, f.create, getJob); err == nil {
					t.Error("a later submit with a canceled key must be refused")
				}
			}
		})
	}
}

func TestCancelKeyTombstoneExpires(t *testing.T) {
	clk := &clock{t: t0}
	k := newSubmitKeys(clk.now)
	if _, err := k.cancel(context.Background(), keyA, getJob); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatal(err)
	}
	f := &fakeSubmits{}
	if _, err := k.submit(context.Background(), keyA, f.create, getJob); connect.CodeOf(err) != connect.CodeAborted {
		t.Fatalf("inside the window: %v", err)
	}
	clk.advance(requestIDWindow)
	if _, err := k.submit(context.Background(), keyA, f.create, getJob); err != nil {
		t.Fatalf("after the window: %v", err)
	}
}

func TestSubmitKeyWaitHonorsContext(t *testing.T) {
	k := newSubmitKeys(time.Now)
	f := &fakeSubmits{gate: make(chan struct{}), entered: make(chan struct{}, 1)}
	go k.submit(context.Background(), keyA, f.create, getJob)
	<-f.entered
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := k.submit(ctx, keyA, f.create, getJob); connect.CodeOf(err) != connect.CodeCanceled {
		t.Fatalf("err = %v", err)
	}
	close(f.gate)
}

func TestExpiredLeaseIsReselectable(t *testing.T) {
	statuses := []*pb.NodeStatus{status("win1", TierShared, pb.NodeFreshness_NODE_FRESHNESS_FRESH, "windows", "amd64", 10, 10, 8, 8<<30)}
	sel := &pb.NodeSelector{Os: "windows"}
	tests := []struct {
		name    string
		advance time.Duration
		renew   bool
		wantErr bool
	}{
		{"held lease blocks the selector", 59 * time.Second, false, true},
		{"expired lease frees the node at once", 60 * time.Second, false, false},
		{"renewed lease still blocks", 90 * time.Second, true, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tb, clk, _ := newTable(t, filepath.Join(t.TempDir(), "leases.yaml"))
			choose := func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
				return pickNode(statuses, sel, func(string) int { return 0 }, blocked, nil)
			}
			l, err := tb.Acquire("a", time.Minute, choose)
			if err != nil {
				t.Fatal(err)
			}
			if tt.renew {
				clk.advance(45 * time.Second)
				if _, err := tb.Renew(l.Id, 0); err != nil {
					t.Fatal(err)
				}
				tt.advance -= 45 * time.Second
			}
			clk.advance(tt.advance)
			_, err = tb.Acquire("b", time.Minute, choose)
			if (err != nil) != tt.wantErr {
				t.Fatalf("second selector acquire: %v, want error %v", err, tt.wantErr)
			}
			wantHolder := "b"
			if tt.wantErr {
				wantHolder = "a"
			}
			if held := tb.NodeLease("win1"); held == nil || held.Holder != wantHolder {
				t.Fatalf("NodeLease = %v, want holder %s", held, wantHolder)
			}
		})
	}
}

func TestSubmitRequestIDEndToEnd(t *testing.T) {
	e := newJobEnv(t, 1)
	ctx := context.Background()
	req := func() *pb.SubmitJobRequest {
		return &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:a", "sleep:30"), RequestId: keyA}
	}
	first := e.submit(t, req())
	again := e.submit(t, req())
	if first.Id != again.Id {
		t.Fatalf("retry created %s, want %s", again.Id, first.Id)
	}
	if n := len(e.h.jobs.List(pb.JobState_JOB_STATE_UNSPECIFIED, 0)); n != 1 {
		t.Fatalf("%d jobs", n)
	}
	for _, bad := range []*pb.CancelJobRequest{{}, {Id: first.Id, RequestId: keyA}} {
		if _, err := e.jc.CancelJob(ctx, connect.NewRequest(bad)); connect.CodeOf(err) != connect.CodeInvalidArgument {
			t.Errorf("cancel %v: %v", bad, err)
		}
	}
	res, err := e.jc.CancelJob(ctx, connect.NewRequest(&pb.CancelJobRequest{RequestId: keyA}))
	if err != nil {
		t.Fatal(err)
	}
	if res.Msg.Id != first.Id {
		t.Fatalf("canceled %s", res.Msg.Id)
	}
	if j := e.wait(t, first.Id); j.State != pb.JobState_JOB_STATE_CANCELED {
		t.Fatalf("state = %v", j.State)
	}
}
