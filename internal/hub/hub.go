// Package hub implements the Connect services served by `protectorate hub serve`.
package hub

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/codec"
	"github.com/hotschmoe/protectorate/internal/node"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

type Hub struct {
	deskUse   deskUse
	Version   string
	Heartbeat time.Duration
	// SelfURL is how sleeves on this machine reach the hub; it is exported to
	// them as PROTECTORATE_HUB.
	SelfURL string
	// AckTimeout is how long an ack_required message may stay unacked before
	// it is re-delivered, and again before it is escalated; 0 means 30m.
	AckTimeout time.Duration
	// BinaryDir holds the protectorate-<os>-<arch> builds UpdateNode sends to
	// nodes of other platforms; empty means the directory of the hub's own
	// executable. SelfExe overrides the path of that executable.
	BinaryDir string
	SelfExe   string
	// UpdateTimeout is how long UpdateNode waits for a node to report the
	// new version; 0 means 3m.
	UpdateTimeout time.Duration
	updatePoll    time.Duration

	authz   *auth.Authorizer
	started time.Time
	seq     atomic.Uint64
	quota   *QuotaCache
	nodes   *NodeMonitor
	leases  *LeaseTable
	jobs    *JobManager
	local   nodeOps
	events  eventBus

	deskPol   *deskPolicies
	deskMu    sync.Mutex
	deskCache map[string]deskEntry

	tailnetStatus func(context.Context) (*pb.TailnetServiceGetStatusResponse, error)

	Sessions    *session.Manager
	SessionPoll time.Duration
	// DeadRetention is how long an exited session is kept before the hub
	// kills it; 0 keeps them until killed or respawned.
	DeadRetention time.Duration
	PingInterval  time.Duration

	scanMu sync.Mutex
	known  map[string]session.Session
	seeded bool

	backlog *backlog.Store
	sleeves *sleeve.Manager
	chat    *chatManager
	coord   *coordinator

	// WebDir, when set, serves a built web client (app/build/web) from the
	// hub's own origin so browsers need no CORS. The client still authenticates.
	WebDir string

	// LocalModelHosts are model endpoint hostnames the hub treats as the
	// user's own server even when they do not look private.
	LocalModelHosts []string
}

func New(version, token string) *Hub {
	h := &Hub{
		Version:       version,
		Heartbeat:     5 * time.Second,
		Sessions:      session.New(session.DefaultSocket),
		SessionPoll:   time.Second,
		DeadRetention: 24 * time.Hour,
		PingInterval:  20 * time.Second,
		authz:         auth.New(token, nil, nil),
		started:       time.Now(),
	}
	h.SetQuota(NewQuotaCache(nil, QuotaOptions{}))
	if home, err := Home(); err == nil {
		h.SetBacklog(backlog.NewStore(filepath.Join(home, "backlog.md")))
	}
	return h
}

// SetNodes installs the node monitor and enables NodeService. Call before
// Handler or Serve.
func (h *Hub) SetNodes(m *NodeMonitor) {
	m.publish = h.emit
	m.localSleeves = h.sleeveRows
	h.nodes = m
}

// EnableJobs enables LeaseService and JobService, keeping their state under
// home. local runs jobs placed on the hub's own node. Call after SetNodes and
// before Handler or Serve.
func (h *Hub) EnableJobs(home string, local *node.Jobs) error {
	leases, err := OpenLeases(filepath.Join(home, "leases.yaml"), h.leaseEvent)
	if err != nil {
		return err
	}
	if h.deskPol, err = openDeskPolicies(filepath.Join(home, "desk-policies.yaml")); err != nil {
		return err
	}
	jobs, err := OpenJobs(filepath.Join(home, "jobs"), h.nodes, leases, local, h.emit)
	if err != nil {
		return err
	}
	leases.chooser, leases.onGrant = h.leaseChooserFromJSON, h.leaseGranted
	h.leases, h.jobs = leases, jobs
	return nil
}

// SetQuota installs the quota cache and publishes its updates as
// "quota.updated" events. Call before Handler or Serve.
func (h *Hub) SetQuota(c *QuotaCache) {
	c.onUpdate = h.publishQuota
	h.quota = c
}

func (h *Hub) publishQuota(pq *pb.ProviderQuota) {
	raw, err := protojson.Marshal(pq)
	if err != nil {
		return
	}
	var m map[string]any
	if json.Unmarshal(raw, &m) != nil {
		return
	}
	payload, err := structpb.NewStruct(m)
	if err != nil {
		return
	}
	h.emit("quota.updated", pq.Provider, payload)
}

func (h *Hub) emit(kind, subject string, payload *structpb.Struct) {
	h.events.publish(&pb.Event{
		Id:      strconv.FormatUint(h.seq.Add(1), 10),
		Time:    timestamppb.Now(),
		Kind:    kind,
		Subject: subject,
		Payload: payload,
	})
}

// eventBus fans events out to subscribers, dropping for slow ones.
type eventBus struct {
	mu   sync.Mutex
	subs map[chan *pb.Event]struct{}
}

func (b *eventBus) subscribe() (<-chan *pb.Event, func()) {
	ch := make(chan *pb.Event, 64)
	b.mu.Lock()
	if b.subs == nil {
		b.subs = map[chan *pb.Event]struct{}{}
	}
	b.subs[ch] = struct{}{}
	b.mu.Unlock()
	return ch, func() {
		b.mu.Lock()
		delete(b.subs, ch)
		b.mu.Unlock()
	}
}

func (b *eventBus) publish(e *pb.Event) {
	b.mu.Lock()
	defer b.mu.Unlock()
	for ch := range b.subs {
		select {
		case ch <- e:
		default:
		}
	}
}

// SetAuth replaces the default authorizer, which knows only the owner token
// and keeps no audit log. Call before Handler or Serve.
func (h *Hub) SetAuth(a *auth.Authorizer) { h.authz = a }

// Handler serves every service. Every RPC and the terminal go through the
// authorizer; the web client files are static and public.
func (h *Hub) Handler() http.Handler {
	opts := connect.WithHandlerOptions(
		connect.WithInterceptors(authInterceptor{a: h.authz}),
		connect.WithCodec(codec.StrictJSON{}),
	)
	mux := http.NewServeMux()
	mux.Handle(protectoratev1connect.NewHubServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewEventServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewQuotaServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewSessionServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewTokenServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewTailnetServiceHandler(tailnetService{h}, opts))
	mux.HandleFunc("GET /ws/terminal/{name}", h.handleTerminal)
	mux.Handle(protectoratev1connect.NewBacklogServiceHandler(h, opts))
	if h.sleeves != nil {
		mux.Handle(protectoratev1connect.NewSleeveServiceHandler(h, opts))
		mux.Handle(protectoratev1connect.NewChatServiceHandler(chatService{h}, opts))
	}
	if h.coord != nil {
		mux.Handle(protectoratev1connect.NewDirectoryServiceHandler(directoryService{h.coord}, opts))
		mux.Handle(protectoratev1connect.NewMessageServiceHandler(messageService{h.coord}, opts))
	}
	if h.nodes != nil {
		mux.Handle(protectoratev1connect.NewNodeServiceHandler(h, opts))
	}
	if h.jobs != nil {
		mux.Handle(protectoratev1connect.NewLeaseServiceHandler(h, opts))
		mux.Handle(protectoratev1connect.NewJobServiceHandler(h, opts))
		mux.Handle(protectoratev1connect.NewDeskServiceHandler(h, opts))
	}
	if h.WebDir != "" {
		mux.Handle("/", http.FileServer(http.Dir(h.WebDir)))
	}
	return mux
}

// tailnetHandler admits only callers identify accepts (tailnet WhoIs), then
// applies the same bearer-token scope checks as the main listener: a tagged
// device must also present a token. The identity is recorded in the audit log.
func (h *Hub) tailnetHandler(identify func(*http.Request) (string, error)) http.Handler {
	next := h.Handler()
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		who, err := identify(r)
		if err != nil {
			http.Error(w, "forbidden", http.StatusForbidden)
			return
		}
		next.ServeHTTP(w, r.WithContext(auth.WithTailnet(r.Context(), who)))
	})
}

// Extra is an additional listener whose callers are first identified by the
// network. Identify rejects unknown peers and returns the identity of the rest;
// bearer-token scopes still apply afterwards.
type Extra struct {
	Listener net.Listener
	Identify func(*http.Request) (string, error)
}

// Serve accepts HTTP/1.1 and cleartext HTTP/2 on l, plus on each extra
// listener, until ctx is cancelled.
func (h *Hub) Serve(parent context.Context, l net.Listener, extra ...Extra) error {
	ctx, cancelWorkers := context.WithCancel(parent)
	newServer := func(handler http.Handler) *http.Server {
		srv := &http.Server{
			Handler:           handler,
			ReadHeaderTimeout: 10 * time.Second,
			BaseContext:       func(net.Listener) context.Context { return ctx },
			Protocols:         new(http.Protocols),
		}
		srv.Protocols.SetHTTP1(true)
		srv.Protocols.SetUnencryptedHTTP2(true)
		return srv
	}
	servers := map[*http.Server]net.Listener{newServer(h.Handler()): l}
	for _, e := range extra {
		servers[newServer(h.tailnetHandler(e.Identify))] = e.Listener
	}

	pollDone := make(chan struct{})
	pollCtx, stopPoll := context.WithCancel(ctx)
	defer func() {
		stopPoll()
		<-pollDone
	}()
	go func() {
		defer close(pollDone)
		h.quota.Run(pollCtx)
	}()

	watchDone := make(chan struct{})
	go func() { defer close(watchDone); h.WatchSessions(ctx) }()
	defer func() { <-watchDone }()

	if h.chat != nil {
		defer h.chat.shutdown()
	}
	if h.sleeves != nil {
		sleevesDone := make(chan struct{})
		go func() { defer close(sleevesDone); h.sleeves.Run(ctx) }()
		defer func() { <-sleevesDone }()
	}

	if h.coord != nil {
		coordDone := make(chan struct{})
		go func() { defer close(coordDone); h.coord.Run(ctx) }()
		defer func() { <-coordDone; h.coord.close() }()
	}

	if h.nodes != nil {
		nodesDone := make(chan struct{})
		nodesCtx, stopNodes := context.WithCancel(ctx)
		defer func() {
			stopNodes()
			<-nodesDone
		}()
		go func() {
			defer close(nodesDone)
			h.nodes.Run(nodesCtx)
		}()
	}

	if h.jobs != nil {
		leasesDone := make(chan struct{})
		leasesCtx, stopLeases := context.WithCancel(ctx)
		go func() { defer close(leasesDone); h.leases.Run(leasesCtx) }()
		deskDone := make(chan struct{})
		go func() { defer close(deskDone); h.watchDesks(leasesCtx) }()
		defer func() {
			stopLeases()
			<-leasesDone
			<-deskDone
			h.jobs.Shutdown()
		}()
	}

	// Registered last so it runs first: workers must stop before the waits above.
	defer cancelWorkers()

	errc := make(chan error, len(servers))
	for srv, ln := range servers {
		go func() { errc <- srv.Serve(ln) }()
	}
	var first error
	pending := len(servers)
	select {
	case first = <-errc:
		pending--
	case <-ctx.Done():
	}
	sctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	for srv := range servers {
		if err := srv.Shutdown(sctx); err != nil && first == nil {
			first = err
		}
	}
	for ; pending > 0; pending-- {
		if err := <-errc; !errors.Is(err, http.ErrServerClosed) && first == nil {
			first = err
		}
	}
	if errors.Is(first, http.ErrServerClosed) {
		return nil
	}
	return first
}

func (h *Hub) GetStatus(context.Context, *connect.Request[pb.GetStatusRequest]) (*connect.Response[pb.GetStatusResponse], error) {
	host, _ := os.Hostname()
	return connect.NewResponse(&pb.GetStatusResponse{
		Version:       h.Version,
		Hostname:      host,
		Os:            runtime.GOOS,
		Arch:          runtime.GOARCH,
		UptimeSeconds: int64(time.Since(h.started).Seconds()),
		StartedAt:     timestamppb.New(h.started),
	}), nil
}

func (h *Hub) Subscribe(ctx context.Context, _ *connect.Request[pb.SubscribeRequest], stream *connect.ServerStream[pb.Event]) error {
	tick := time.NewTicker(h.Heartbeat)
	defer tick.Stop()
	events, unsub := h.events.subscribe()
	defer unsub()
	for {
		if err := stream.Send(h.heartbeat()); err != nil {
			return err
		}
	wait:
		for {
			select {
			case <-ctx.Done():
				return nil
			case e := <-events:
				if err := stream.Send(e); err != nil {
					return err
				}
			case <-tick.C:
				break wait
			}
		}
	}
}

func (h *Hub) heartbeat() *pb.Event {
	payload, _ := structpb.NewStruct(map[string]any{
		"uptime_seconds": time.Since(h.started).Seconds(),
	})
	return &pb.Event{
		Id:      strconv.FormatUint(h.seq.Add(1), 10),
		Time:    timestamppb.Now(),
		Kind:    "heartbeat",
		Subject: "hub",
		Payload: payload,
	}
}

type authInterceptor struct{ a *auth.Authorizer }

func bearerToken(header http.Header) string {
	v := header.Get("Authorization")
	if len(v) > 7 && strings.EqualFold(v[:7], "Bearer ") {
		return v[7:]
	}
	return ""
}

// admit authenticates and authorizes one call, auditing mutations. finish
// must be called with the call's result.
func (i authInterceptor) admit(ctx context.Context, procedure string, header http.Header, msg any) (context.Context, func(error), error) {
	m, known := i.a.Method(procedure)
	audited := !known || m.Mutating()
	subject := auth.Subject(msg)
	p, err := i.a.Authenticate(bearerToken(header))
	if err == nil {
		err = i.a.AuthorizeProcedure(p, procedure)
	}
	if err != nil {
		if audited || (known && !m.ReadOnly) {
			i.a.Record(ctx, p, procedure, subject, err, deniedCode(err))
		}
		return ctx, nil, deniedConnectError(err)
	}
	quiet := continuesTransfer(msg)
	finish := func(callErr error) {
		if !audited || (quiet && callErr == nil) {
			return
		}
		code := ""
		if callErr != nil {
			code = connect.CodeOf(callErr).String()
		}
		i.a.Record(ctx, p, procedure, subject, callErr, code)
	}
	return auth.WithPrincipal(ctx, p), finish, nil
}

// continuesTransfer marks the middle chunks of a file upload, which are not
// audited unless they fail: the first and last chunk record the transfer.
func continuesTransfer(msg any) bool {
	p, ok := msg.(*pb.PutFileRequest)
	return ok && p.Offset > 0 && !p.Last
}

func deniedCode(err error) string { return deniedConnectError(err).Code().String() }

func deniedConnectError(err error) *connect.Error {
	var d *auth.DeniedError
	if errors.As(err, &d) {
		return connect.NewError(connect.CodePermissionDenied, err)
	}
	return connect.NewError(connect.CodeUnauthenticated, err)
}

func (i authInterceptor) WrapUnary(next connect.UnaryFunc) connect.UnaryFunc {
	return func(ctx context.Context, req connect.AnyRequest) (connect.AnyResponse, error) {
		ctx, finish, err := i.admit(ctx, req.Spec().Procedure, req.Header(), req.Any())
		if err != nil {
			return nil, err
		}
		res, err := next(ctx, req)
		finish(err)
		return res, err
	}
}

func (i authInterceptor) WrapStreamingClient(next connect.StreamingClientFunc) connect.StreamingClientFunc {
	return next
}

func (i authInterceptor) WrapStreamingHandler(next connect.StreamingHandlerFunc) connect.StreamingHandlerFunc {
	return func(ctx context.Context, conn connect.StreamingHandlerConn) error {
		ctx, finish, err := i.admit(ctx, conn.Spec().Procedure, conn.RequestHeader(), nil)
		if err != nil {
			return err
		}
		err = next(ctx, conn)
		finish(err)
		return err
	}
}
