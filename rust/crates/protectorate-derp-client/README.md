# Protectorate DERP client

Blocking TLS DERP v2 client for the Protectorate relay and compatible DERP
servers. Without a pin, TLS checks the hostname against public roots and any
explicitly supplied CA certificate. With a `sha256-raw:<hex>` pin and no CA,
the exact pinned leaf is trusted while its name, validity, and TLS handshake
signature are still checked. This permits Headscale's private self-signed DERP
relay, whose map supplies a certificate pin but no CA. When a CA is supplied,
the pin is an additional exact leaf check. The DERP server key can also be
pinned independently.

The client authenticates with the Tailscale node key using NaCl `crypto_box`,
checks encrypted server info, and relays bounded packets. `recv_timeout` keeps
partial frame bytes across idle timeouts so a node can alternate reads and
sends on one connection. It has no reconnect loop, network-map policy, STUN
discovery, UDP direct path, or WireGuard packet
engine. Callers must use the registered node private key and authorize source
peers against a current authenticated netmap before accepting packets.

Protocol reference: the locally cached Tailscale `derp/derp.go` and
`derp/derp_client.go` at v1.104.1.
