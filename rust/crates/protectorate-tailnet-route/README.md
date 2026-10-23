# Tailnet peer routing and packet authorization

`RouteTable::compile` turns one authenticated, current `Netmap` into a bounded
read-only snapshot. `route_outbound` selects a peer only for an exact node
address. It does not route advertised subnets. `authorize_inbound` binds a
WireGuard peer ID to an exact source address, an exact local destination
address, the authenticated node key, expiry, and a matching packet-filter
rule. Unknown, ambiguous, or expired peers are denied. `Online=false` only
means that the peer is disconnected from coordination; it can still receive
direct or DERP packets and is not an authorization gate.

The packet engine must discard this table when its map poll disconnects and
must authenticate the WireGuard public key against the returned peer key.
This crate does not parse packets, encrypt traffic, open sockets, or implement
connection tracking. `cap:` source rules and application capability grants
are not supported; they grant no packets here. It rejects legacy `SrcBits` and
`Bits` filter fields. The map's filter is an inbound filter for this node;
outbound routing does not apply it, and the receiving node must enforce its
own inbound filter.

The current `Netmap::Node` type does not expose Tailcfg's
`UnsignedPeerAPIOnly` or `MachineAuthorized` fields. This crate cannot make
decisions from those flags until the map parser retains them. The packet
engine must also reject packets before calling this API if the control poll
has disconnected or if the local registration identity no longer matches.
