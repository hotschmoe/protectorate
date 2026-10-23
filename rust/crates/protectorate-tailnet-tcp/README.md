# protectorate-tailnet-tcp

A small userspace TCP listener for plaintext IP packets carried inside an authenticated tunnel. It has no TUN device and only one dependency (`getrandom` for per-connection sequence numbers). `Endpoint::receive` checks IPv4/IPv6 and TCP checksums, rejects fragmented IPv4 packets and IPv6 extension headers, and returns packet, accepted-connection, data, peer half-close, and terminal-close events. `send`, `close`, and `tick` provide outbound data, FIN, retransmission, and idle cleanup. The connection table, pending data, advertised window, segment size, and retry period are bounded.

The caller must enforce source address and destination policy against the authenticated WireGuard peer **before** passing plaintext packets here. The caller must deliver every `Packet` event and call `tick` regularly. It must also handle `Data` without unbounded buffering.

This is an intentionally small TCP implementation. It drops out-of-order data and relies on the peer's retransmission; it does not support SACK, ECN, TCP Fast Open, IPv6 extension headers, IP fragmentation/reassembly, path MTU discovery, congestion control beyond the peer's receive window, or zero-window probes. It is not yet wired to the tailnet route, WireGuard session, or Protectorate HTTP server, so it does not establish end-to-end connectivity by itself.
