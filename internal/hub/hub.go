// Package hub implements the Connect services served by `protectorate hub serve`.
package hub

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
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
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/session"
)

type Hub struct {
	Version   string
	Heartbeat time.Duration

	token   string
	started time.Time
	seq     atomic.Uint64
	quota   *QuotaCache
	events  eventBus

	Sessions     *session.Manager
	SessionPoll  time.Duration
	PingInterval time.Duration

	scanMu sync.Mutex
	known  map[string]session.Session
	seeded bool

	backlog *backlog.Store

	// WebDir, when set, serves a built web client (app/build/web) from the
	// hub's own origin so browsers need no CORS. The client still authenticates.
	WebDir string
}

func New(version, token string) *Hub {
	h := &Hub{
		Version:      version,
		Heartbeat:    5 * time.Second,
		Sessions:     session.New(session.DefaultSocket),
		SessionPoll:  time.Second,
		PingInterval: 20 * time.Second,
		token:        token,
		started:      time.Now(),
	}
	h.SetQuota(NewQuotaCache(nil, QuotaOptions{}))
	if home, err := Home(); err == nil {
		h.SetBacklog(backlog.NewStore(filepath.Join(home, "backlog.md")))
	}
	return h
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
	h.events.publish(&pb.Event{
		Id:      strconv.FormatUint(h.seq.Add(1), 10),
		Time:    timestamppb.Now(),
		Kind:    "quota.updated",
		Subject: pq.Provider,
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

// strictJSON replaces connect's JSON codec so unknown request fields are an
// error instead of being silently dropped.
type strictJSON struct{}

func (strictJSON) Name() string { return "json" }

func (strictJSON) Marshal(m any) ([]byte, error) {
	pm, ok := m.(proto.Message)
	if !ok {
		return nil, fmt.Errorf("%T is not a proto message", m)
	}
	return protojson.Marshal(pm)
}

func (strictJSON) Unmarshal(data []byte, m any) error {
	pm, ok := m.(proto.Message)
	if !ok {
		return fmt.Errorf("%T is not a proto message", m)
	}
	if len(data) == 0 {
		return errors.New("zero-length payload is not a valid JSON object")
	}
	return protojson.UnmarshalOptions{DiscardUnknown: false}.Unmarshal(data, pm)
}

func (strictJSON) IsBinary() bool { return false }

// Handler serves every service behind the bearer-token interceptor.
func (h *Hub) Handler() http.Handler {
	opts := connect.WithHandlerOptions(
		connect.WithInterceptors(authInterceptor{token: h.token}),
		connect.WithCodec(strictJSON{}),
	)
	mux := http.NewServeMux()
	mux.Handle(protectoratev1connect.NewHubServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewEventServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewQuotaServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewSessionServiceHandler(h, opts))
	mux.HandleFunc("GET /ws/terminal/{name}", h.handleTerminal)
	mux.Handle(protectoratev1connect.NewBacklogServiceHandler(h, opts))
	if h.WebDir != "" {
		mux.Handle("/", http.FileServer(http.Dir(h.WebDir)))
	}
	return mux
}

// Serve accepts HTTP/1.1 and cleartext HTTP/2 on l until ctx is cancelled.
func (h *Hub) Serve(ctx context.Context, l net.Listener) error {
	srv := &http.Server{
		Handler:           h.Handler(),
		ReadHeaderTimeout: 10 * time.Second,
		BaseContext:       func(net.Listener) context.Context { return ctx },
		Protocols:         new(http.Protocols),
	}
	srv.Protocols.SetHTTP1(true)
	srv.Protocols.SetUnencryptedHTTP2(true)

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

	errc := make(chan error, 1)
	go func() { errc <- srv.Serve(l) }()
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

type authInterceptor struct{ token string }

func tokenMatches(want, got string) bool {
	return subtle.ConstantTimeCompare([]byte(got), []byte(want)) == 1
}

func (a authInterceptor) check(header http.Header) error {
	got, ok := strings.CutPrefix(header.Get("Authorization"), "Bearer ")
	if !ok || !tokenMatches(a.token, got) {
		return connect.NewError(connect.CodeUnauthenticated, fmt.Errorf("missing or invalid bearer token"))
	}
	return nil
}

func (a authInterceptor) WrapUnary(next connect.UnaryFunc) connect.UnaryFunc {
	return func(ctx context.Context, req connect.AnyRequest) (connect.AnyResponse, error) {
		if err := a.check(req.Header()); err != nil {
			return nil, err
		}
		return next(ctx, req)
	}
}

func (a authInterceptor) WrapStreamingClient(next connect.StreamingClientFunc) connect.StreamingClientFunc {
	return next
}

func (a authInterceptor) WrapStreamingHandler(next connect.StreamingHandlerFunc) connect.StreamingHandlerFunc {
	return func(ctx context.Context, conn connect.StreamingHandlerConn) error {
		if err := a.check(conn.RequestHeader()); err != nil {
			return err
		}
		return next(ctx, conn)
	}
}
