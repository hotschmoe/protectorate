package node

import (
	"context"
	"crypto/subtle"
	"errors"
	"fmt"
	"net"
	"net/http"
	"strings"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/codec"
)

// DefaultPort is the tailnet port of the node agent.
const DefaultPort = 7475

const (
	defaultWatchInterval = 10 * time.Second
	minWatchInterval     = 5 * time.Second
)

// Agent serves NodeAgentService. Authorize is called for every request before
// any handler runs; a nil Authorize rejects everything.
type Agent struct {
	Collector *Collector
	Authorize func(*http.Request) error
	// Jobs executes hub jobs; nil refuses them.
	Jobs *Jobs
	// Files serves file transfers; nil refuses them.
	Files *Files
	// Desk drives the desktop; nil refuses it.
	Desk *Desk
	// Updater swaps the agent's binary; nil refuses updates.
	Updater *Updater
	// Uninstaller plans unregistering the agent's startup; nil refuses. It
	// returns the actions, what stays on disk and run, which the agent calls
	// once it has answered: run does the actions and stops the agent.
	Uninstaller func() (actions, left []string, run func(), err error)
}

// uninstallDelay lets the answer to Uninstall reach the hub before the agent
// starts taking itself down.
const uninstallDelay = time.Second

// TokenAuthorizer accepts requests carrying the given bearer token.
func TokenAuthorizer(token string) func(*http.Request) error {
	return func(r *http.Request) error {
		got, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
		if !ok || token == "" || subtle.ConstantTimeCompare([]byte(got), []byte(token)) != 1 {
			return errors.New("missing or invalid bearer token")
		}
		return nil
	}
}

func (a *Agent) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.Handle(protectoratev1connect.NewNodeAgentServiceHandler(a, connect.WithHandlerOptions(connect.WithCodec(codec.StrictJSON{}))))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if a.Authorize == nil {
			http.Error(w, "no authorizer configured", http.StatusForbidden)
			return
		}
		if err := a.Authorize(r); err != nil {
			http.Error(w, err.Error(), http.StatusUnauthorized)
			return
		}
		mux.ServeHTTP(w, r)
	})
}

// Serve accepts HTTP/1.1 and cleartext HTTP/2 on ln and runs the collector's
// slow-interval measurements until ctx is cancelled.
func (a *Agent) Serve(ctx context.Context, ln net.Listener) error {
	srv := &http.Server{
		Handler:           a.Handler(),
		ReadHeaderTimeout: 10 * time.Second,
		BaseContext:       func(net.Listener) context.Context { return ctx },
		Protocols:         new(http.Protocols),
	}
	srv.Protocols.SetHTTP1(true)
	srv.Protocols.SetUnencryptedHTTP2(true)

	slowDone := make(chan struct{})
	slowCtx, stopSlow := context.WithCancel(ctx)
	defer func() {
		stopSlow()
		<-slowDone
	}()
	go func() {
		defer close(slowDone)
		a.Collector.Run(slowCtx)
	}()

	errc := make(chan error, 1)
	go func() { errc <- srv.Serve(ln) }()
	select {
	case err := <-errc:
		return err
	case <-ctx.Done():
	}
	sctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := srv.Shutdown(sctx); err != nil {
		return err
	}
	if err := <-errc; !errors.Is(err, http.ErrServerClosed) {
		return err
	}
	return nil
}

func (a *Agent) GetMetrics(ctx context.Context, _ *connect.Request[pb.GetMetricsRequest]) (*connect.Response[pb.NodeMetrics], error) {
	if a.Updater != nil {
		a.Updater.Touch()
	}
	m, err := a.Collector.Collect(ctx)
	if err != nil {
		return nil, connect.NewError(connect.CodeUnavailable, fmt.Errorf("collect metrics: %w", err))
	}
	return connect.NewResponse(m), nil
}

func (a *Agent) WatchMetrics(ctx context.Context, req *connect.Request[pb.WatchMetricsRequest], stream *connect.ServerStream[pb.NodeMetrics]) error {
	iv := time.Duration(req.Msg.IntervalSeconds) * time.Second
	if iv == 0 {
		iv = defaultWatchInterval
	}
	iv = max(iv, minWatchInterval)
	tick := time.NewTicker(iv)
	defer tick.Stop()
	for {
		m, err := a.Collector.Collect(ctx)
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return connect.NewError(connect.CodeUnavailable, fmt.Errorf("collect metrics: %w", err))
		}
		if err := stream.Send(m); err != nil {
			return err
		}
		select {
		case <-ctx.Done():
			return nil
		case <-tick.C:
		}
	}
}

var errNoJobs = connect.NewError(connect.CodeUnimplemented, errors.New("this node does not run jobs"))

func (a *Agent) PrepareJob(ctx context.Context, req *connect.Request[pb.PrepareJobRequest]) (*connect.Response[pb.PrepareJobResponse], error) {
	if a.Jobs == nil {
		return nil, errNoJobs
	}
	res, err := a.Jobs.Prepare(ctx, req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

func (a *Agent) UploadJobData(_ context.Context, req *connect.Request[pb.UploadJobDataRequest]) (*connect.Response[pb.UploadJobDataResponse], error) {
	if a.Jobs == nil {
		return nil, errNoJobs
	}
	res, err := a.Jobs.Upload(req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

func (a *Agent) RunJob(ctx context.Context, req *connect.Request[pb.RunJobRequest], stream *connect.ServerStream[pb.JobEvent]) error {
	if a.Jobs == nil {
		return errNoJobs
	}
	return a.Jobs.Run(ctx, req.Msg, stream.Send)
}

func (a *Agent) CancelJob(_ context.Context, req *connect.Request[pb.NodeAgentServiceCancelJobRequest]) (*connect.Response[pb.NodeAgentServiceCancelJobResponse], error) {
	if a.Jobs == nil {
		return nil, errNoJobs
	}
	return connect.NewResponse(&pb.NodeAgentServiceCancelJobResponse{Found: a.Jobs.Cancel(req.Msg.JobId)}), nil
}

var (
	errNoFiles = connect.NewError(connect.CodeUnimplemented, errors.New("this node does not serve file transfers"))
	errNoDesk  = connect.NewError(connect.CodeUnimplemented, errors.New("this node does not serve desktop commands"))
)

func (a *Agent) PutFile(_ context.Context, req *connect.Request[pb.PutFileRequest]) (*connect.Response[pb.PutFileResponse], error) {
	if a.Files == nil {
		return nil, errNoFiles
	}
	res, err := a.Files.Put(req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

func (a *Agent) GetFile(ctx context.Context, req *connect.Request[pb.GetFileRequest], stream *connect.ServerStream[pb.GetFileResponse]) error {
	if a.Files == nil {
		return errNoFiles
	}
	return a.Files.Get(ctx, req.Msg, stream.Send)
}

func (a *Agent) DeskAction(ctx context.Context, req *connect.Request[pb.DeskActionRequest]) (*connect.Response[pb.DeskActionResponse], error) {
	if a.Desk == nil {
		return nil, errNoDesk
	}
	res, err := a.Desk.Do(ctx, req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

func (a *Agent) ApplyUpdate(ctx context.Context, req *connect.Request[pb.ApplyUpdateRequest]) (*connect.Response[pb.ApplyUpdateResponse], error) {
	if a.Updater == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("this node does not accept updates"))
	}
	res, err := a.Updater.Apply(ctx, req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

func (a *Agent) GetDeskState(_ context.Context, req *connect.Request[pb.GetDeskStateRequest]) (*connect.Response[pb.DeskState], error) {
	if a.Desk == nil || a.Desk.Guard == nil {
		return nil, errNoDesk
	}
	g := a.Desk.Guard
	if err := g.SetPolicy(req.Msg.Policy); err != nil {
		return nil, deskErr(err)
	}
	if req.Msg.LeaseEnded {
		g.LeaseEnded()
	}
	return connect.NewResponse(g.State()), nil
}

func (a *Agent) Uninstall(context.Context, *connect.Request[pb.UninstallRequest]) (*connect.Response[pb.UninstallResponse], error) {
	if a.Uninstaller == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("this agent cannot uninstall itself"))
	}
	actions, left, run, err := a.Uninstaller()
	if err != nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, err)
	}
	time.AfterFunc(uninstallDelay, run)
	return connect.NewResponse(&pb.UninstallResponse{Actions: actions, Left: left}), nil
}
