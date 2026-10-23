# WireGuard packet primitives

Implements WireGuard v1 IKpsk2 initiation and response, MAC1 validation,
ChaCha20-Poly1305 transport packets, and a 2048-packet replay window. The
handshake format follows the cached upstream `wireguard-go/device` implementation
(`Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s`).

This crate does not implement cookie replies or MAC2, rate limiting, routing,
IP source validation, endpoint roaming, retransmission, or TUN. Callers must
enforce those parts before exposing it to untrusted public traffic. Sessions
expire after 180 seconds and refuse transport after the WireGuard nonce limit.
