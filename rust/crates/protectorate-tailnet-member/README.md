# protectorate-tailnet-member

Native Rust foundation for hub and node members of a Headscale tailnet. It
creates durable X25519 machine and node identities, fetches and pins the Headscale
Noise public key from `GET /key?v=148`, and opens an authenticated TS2021
Noise IK channel over the HTTP/1.1 `/ts2021` upgrade. The channel implements
Tailscale's 4096-byte encrypted record framing and can carry HTTP/2 bytes.

The first discovery over plain HTTP is trust on first use. Verify the pinned
key through an independent channel before relying on authentication. The
HTTPS discovery and upgrade validate the server name and certificate against
public roots. The `*_with_ca_pem` APIs accept a private PEM CA bundle instead
of public roots for both steps. The origin and Noise key remain durably pinned.

`MemberIdentity::register_http` submits the durable node public key and a
preauth key over encrypted HTTP/2 and requires an authorized registration
response. The `real_headscale_registration` test verifies this against a
disposable Headscale 0.29.5 process and checks the node list. Registration is
**not** tailnet connectivity: map streaming, DERP/disco, WireGuard, userspace
TCP, MagicDNS and peer identity (`WhoIs`) remain.

`Http2Channel::post_stream` delivers bounded HTTP/2 DATA chunks to a caller
and replenishes receive windows, providing the transport for a map poll. The
member crate does not yet maintain or reconnect that poll.

The opt-in `real_headscale_noise_upgrade` and
`real_headscale_https_noise_upgrade` tests start disposable Headscale 0.29.5
processes, authenticate the Noise channel and exchange encrypted HTTP/2
records. The HTTPS test also verifies rejection of an untrusted certificate and
a mismatched server name. Run them with
`PROT_HEADSCALE_BINARY=/path/to/headscale cargo test -p protectorate-tailnet-member --test member real_headscale_ -- --ignored`.

Next steps:

1. Define an explicit control-key rotation policy.
2. Integrate registration into hub/node startup, persist disco identity, and
   maintain the map stream through restarts.
3. Implement direct WireGuard and DERP relay transport, userspace TCP, and a
   netmap-backed peer identity gate.
4. Integrate into hub and node after join, reconnect, dial/listen, and tag
   enforcement pass authenticated end-to-end tests.

Protocol sources: [Tailscale controlbase](https://github.com/tailscale/tailscale/blob/v1.104.1/control/controlbase/handshake.go), [record framing](https://github.com/tailscale/tailscale/blob/v1.104.1/control/controlbase/conn.go), [HTTP upgrade](https://github.com/tailscale/tailscale/blob/v1.104.1/control/controlhttp/client.go), and [HTTP/2 handoff](https://github.com/tailscale/tailscale/blob/v1.104.1/control/ts2021/conn.go).
