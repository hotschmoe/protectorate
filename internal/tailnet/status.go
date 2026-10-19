package tailnet

import (
	"context"
	"fmt"
	"slices"
	"strings"
	"time"

	"google.golang.org/protobuf/types/known/timestamppb"
	"tailscale.com/ipn/ipnstate"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// Status is the hub's current view of the tailnet from its tsnet member.
func (n *Net) Status(ctx context.Context) (*pb.TailnetServiceGetStatusResponse, error) {
	st, err := n.lc.Status(ctx)
	if err != nil {
		return nil, fmt.Errorf("tailnet status: %w", err)
	}
	return StatusProto(st), nil
}

// StatusProto maps a tsnet status to the API message, peers sorted by name.
func StatusProto(st *ipnstate.Status) *pb.TailnetServiceGetStatusResponse {
	res := &pb.TailnetServiceGetStatusResponse{
		BackendState: st.BackendState,
		Health:       st.Health,
	}
	if st.Self != nil {
		res.Self = peerProto(st.Self)
	}
	for _, p := range st.Peer {
		res.Peers = append(res.Peers, peerProto(p))
	}
	slices.SortFunc(res.Peers, func(a, b *pb.TailnetPeer) int { return strings.Compare(a.Name, b.Name) })
	return res
}

func peerProto(p *ipnstate.PeerStatus) *pb.TailnetPeer {
	name := strings.TrimSuffix(p.DNSName, ".")
	if name == "" {
		name = p.HostName
	}
	out := &pb.TailnetPeer{
		Name:       name,
		Online:     p.Online,
		DirectAddr: p.CurAddr,
		Relay:      p.Relay,
		RxBytes:    p.RxBytes,
		TxBytes:    p.TxBytes,
	}
	if p.Tags != nil {
		out.Tags = p.Tags.AsSlice()
	}
	for _, ip := range p.TailscaleIPs {
		out.Ips = append(out.Ips, ip.String())
	}
	out.LastHandshake = stamp(p.LastHandshake)
	out.LastSeen = stamp(p.LastSeen)
	return out
}

func stamp(t time.Time) *timestamppb.Timestamp {
	if t.IsZero() || t.Unix() <= 0 {
		return nil
	}
	return timestamppb.New(t)
}
