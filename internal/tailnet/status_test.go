package tailnet

import (
	"net/netip"
	"reflect"
	"testing"
	"time"

	"tailscale.com/ipn/ipnstate"
	"tailscale.com/types/key"
	"tailscale.com/types/views"
)

func TestStatusProto(t *testing.T) {
	hs := time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC)
	tags := views.SliceOf([]string{"tag:node", "tag:tier-shared"})
	pk := func() key.NodePublic { return key.NewNode().Public() }
	tests := []struct {
		name string
		st   *ipnstate.Status
		want []peerWant
		self string
	}{
		{
			name: "direct peer",
			st: &ipnstate.Status{BackendState: "Running", Self: &ipnstate.PeerStatus{DNSName: "hub.ts.net."}, Peer: map[key.NodePublic]*ipnstate.PeerStatus{
				pk(): {DNSName: "alpha.ts.net.", HostName: "alpha", Tags: &tags, TailscaleIPs: []netip.Addr{netip.MustParseAddr("100.64.0.2")},
					Online: true, CurAddr: "10.0.0.2:41641", Relay: "protectorate", LastHandshake: hs, RxBytes: 10, TxBytes: 20},
			}},
			self: "hub.ts.net",
			want: []peerWant{{name: "alpha.ts.net", tags: []string{"tag:node", "tag:tier-shared"}, ips: []string{"100.64.0.2"}, online: true,
				direct: "10.0.0.2:41641", relay: "protectorate", handshake: hs, rx: 10, tx: 20}},
		},
		{
			name: "relayed offline peer sorted after, zero times unset, hostname fallback",
			st: &ipnstate.Status{Peer: map[key.NodePublic]*ipnstate.PeerStatus{
				pk(): {HostName: "zeta", Relay: "protectorate", LastSeen: hs},
				pk(): {DNSName: "beta.ts.net.", LastHandshake: time.Unix(0, 0)},
			}},
			want: []peerWant{
				{name: "beta.ts.net"},
				{name: "zeta", relay: "protectorate", lastSeen: hs},
			},
		},
		{name: "no peers", st: &ipnstate.Status{BackendState: "NeedsLogin"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := StatusProto(tt.st)
			if got.BackendState != tt.st.BackendState {
				t.Errorf("state %q", got.BackendState)
			}
			if tt.self != "" && got.Self.GetName() != tt.self {
				t.Errorf("self %q, want %q", got.Self.GetName(), tt.self)
			}
			var peers []peerWant
			for _, p := range got.Peers {
				w := peerWant{name: p.Name, tags: p.Tags, ips: p.Ips, online: p.Online, direct: p.DirectAddr, relay: p.Relay, rx: p.RxBytes, tx: p.TxBytes}
				if p.LastHandshake != nil {
					w.handshake = p.LastHandshake.AsTime()
				}
				if p.LastSeen != nil {
					w.lastSeen = p.LastSeen.AsTime()
				}
				peers = append(peers, w)
			}
			if !reflect.DeepEqual(peers, tt.want) {
				t.Errorf("peers\n got %+v\nwant %+v", peers, tt.want)
			}
		})
	}
}

type peerWant struct {
	name, direct, relay string
	tags, ips           []string
	online              bool
	handshake, lastSeen time.Time
	rx, tx              int64
}
