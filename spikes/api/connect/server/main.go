package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"

	"connectrpc.com/connect"

	pb "spike/connect/gen/protectorate/v1"
	"spike/connect/gen/protectorate/v1/protectoratev1connect"
)

type hub struct {
	mu      sync.Mutex
	sleeves []*pb.Sleeve
	subs    map[chan *pb.Event]string
	seq     int64
}

func (h *hub) publish(typ, sleeveID, msg string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.seq++
	for ch, filter := range h.subs {
		if filter != "" && filter != sleeveID {
			continue
		}
		select {
		case ch <- &pb.Event{Seq: h.seq, Type: typ, SleeveId: sleeveID, Message: msg}:
		default:
		}
	}
}

func (h *hub) List(_ context.Context, _ *connect.Request[pb.ListRequest]) (*connect.Response[pb.ListResponse], error) {
	h.mu.Lock()
	defer h.mu.Unlock()
	return connect.NewResponse(&pb.ListResponse{Sleeves: h.sleeves}), nil
}

func (h *hub) Spawn(_ context.Context, req *connect.Request[pb.SpawnRequest]) (*connect.Response[pb.SpawnResponse], error) {
	m := req.Msg
	if m.Name == "" {
		return nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("name is required"))
	}
	h.mu.Lock()
	for _, s := range h.sleeves {
		if s.Name == m.Name {
			h.mu.Unlock()
			return nil, connect.NewError(connect.CodeAlreadyExists, fmt.Errorf("sleeve %q exists", m.Name))
		}
	}
	s := &pb.Sleeve{Id: fmt.Sprintf("sl-%d", len(h.sleeves)+1), Name: m.Name, Harness: m.Harness, State: "running"}
	h.sleeves = append(h.sleeves, s)
	h.mu.Unlock()
	h.publish("sleeve.spawned", s.Id, s.Name)
	return connect.NewResponse(&pb.SpawnResponse{Sleeve: s}), nil
}

func (h *hub) Subscribe(ctx context.Context, req *connect.Request[pb.SubscribeRequest], stream *connect.ServerStream[pb.Event]) error {
	ch := make(chan *pb.Event, 16)
	h.mu.Lock()
	h.subs[ch] = req.Msg.SleeveId
	h.mu.Unlock()
	defer func() {
		h.mu.Lock()
		delete(h.subs, ch)
		h.mu.Unlock()
	}()
	tick := time.NewTicker(time.Second)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return nil
		case e := <-ch:
			if err := stream.Send(e); err != nil {
				return err
			}
		case <-tick.C:
			if err := stream.Send(&pb.Event{Type: "heartbeat"}); err != nil {
				return err
			}
		}
	}
}

func main() {
	h := &hub{subs: map[chan *pb.Event]string{}}
	mux := http.NewServeMux()
	p, hd := protectoratev1connect.NewSleeveServiceHandler(h)
	mux.Handle(p, hd)
	p, hd = protectoratev1connect.NewEventServiceHandler(h)
	mux.Handle(p, hd)
	mux.HandleFunc("/mcp/tools", mcpTools)
	log.Fatal(http.ListenAndServe("127.0.0.1:7471", mux))
}
