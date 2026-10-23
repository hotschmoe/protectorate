# protectorate-tailnet-discovery

Stores a member's disco X25519 key at `disco.key` and produces the
`discokey:<64 hex digits>` value sent in `tailcfg.MapRequest.DiscoKey`.

```rust
use protectorate_tailnet_discovery::DiscoIdentity;

let disco = DiscoIdentity::open(member_state_dir)?;
let map_disco_key = disco.public_key().to_string();
```

Keep the same `DiscoIdentity` available to the later disco transport, which
must prove possession of its private key. This crate does not send discovery
packets or establish tailnet connectivity. Existing invalid or exposed key
files fail closed instead of silently rotating the identity.

Wire reference: Tailscale v1.104.1 `types/key/disco.go` and
`tailcfg/tailcfg.go` (`MapRequest.DiscoKey`).
