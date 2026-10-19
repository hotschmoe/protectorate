// Package hub implements the Connect services served by `protectorate hub serve`.
package hub

import (
	"context"
	"crypto/subtle"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"runtime"
	"strconv"
	"strings"
	"sync/atomic"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

type Hub struct {
	Version   string
	Heartbeat time.Duration

	token   string
	started time.Time
	seq     atomic.Uint64
}

func New(version, token string) *Hub {
	return &Hub{Version: version, Heartbeat: 5 * time.Second, token: token, started: time.Now()}
}

// Handler serves every service behind the bearer-token interceptor.
func (h *Hub) Handler() http.Handler {
	opts := connect.WithInterceptors(authInterceptor{token: h.token})
	mux := http.NewServeMux()
	mux.Handle(protectoratev1connect.NewHubServiceHandler(h, opts))
	mux.Handle(protectoratev1connect.NewEventServiceHandler(h, opts))
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
	for {
		if err := stream.Send(h.heartbeat()); err != nil {
			return err
		}
		select {
		case <-ctx.Done():
			return nil
		case <-tick.C:
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

func (a authInterceptor) check(header http.Header) error {
	got, ok := strings.CutPrefix(header.Get("Authorization"), "Bearer ")
	if !ok || subtle.ConstantTimeCompare([]byte(got), []byte(a.token)) != 1 {
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
