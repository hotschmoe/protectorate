# protectorate-tailnet-map

Parses Tailscale/Headscale `/machine/map` HTTP response bodies and applies
incremental netmap updates. The caller supplies an authenticated HTTP/2 response
body over the pinned TS2021 Noise channel. The request advertises no compression,
so response frames are uncompressed JSON prefixed with little-endian `u32` sizes.

`MapRequest` builds the request JSON. `MapStream<R: Read>` consumes body bytes,
and `Netmap` stores the local node, peers, DERP regions, user profiles, and
packet filter rules. Frame size and peer counts are bounded. A reconnect should
call `Netmap::reset_poll()` before applying a new initial response.

This crate does not implement the HTTP/2 body reader, packet engine, or WhoIs.
The packet filter data must be enforced before packets can be accepted.
