# protectorate-tailnet-packets

Relay-only WireGuard packet sessions for a current authenticated tailnet map.
The caller must replace the map on every accepted control update and call
`clear_map` immediately when map polling disconnects. Map replacement retains
sessions only for peers with the same key and DERP region and a currently usable
exact route; removed, changed, revoked, expired, or unroutable peers lose their
sessions. The new map filter checks every inbound packet. A selected private
DERP server must match the map's literal IP and port when an IP is present. If
the map publishes only `HostName`, the dial IP must resolve from that name at
the advertised port. TLS validates `HostName`, and the leaf certificate must
match `CertName`'s `sha256-raw:` pin. A CA certificate may also be supplied.

The crate deliberately has no direct UDP path. It rejects IP fragments,
extensions, unsupported protocols, address spoofing, and inbound packets not
permitted by the current map filter. It is a packet engine, not yet a TUN
device or node service.
