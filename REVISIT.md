# REVISIT

Shortcuts taken to make something work, and what "right" (then "fast") would be. Newest last.
Each entry: where, what was done, what right looks like.

- **Release signing** (SPEC 6): no signing key; updates trust SHA256SUMS over HTTPS. Right: an
  offline-held key and a signature checked against a key embedded in the binary.
- **Outbound HTTPS through curl** (hub, nodes): no Rust TLS stack. Right: decide between
  curl forever, or a forked and trimmed rustls when curl's absence or output parsing bites.
- **Sandbox through bwrap**: right is our own small namespaces helper (SPEC 3.4).
- **Control-key pin is checked by a separate fetch** (`go/cmd/member`): member fetches `/key` and
  compares it with the pin, then tsnet fetches `/key` again itself, so an active man in the middle
  (stronghold) can answer the two fetches differently. It catches a re-keyed or replaced headscale,
  not an attacker. Right: point tsnet at a loopback front inside member that answers `/key` with the
  pinned key and forwards the rest, so Noise IK fails against any other server, or an upstream
  pinned-server-key option.
- **dnsfallback can ask Tailscale's public DERPs** (`go/cmd/member`): if the control host name does
  not resolve, controlclient queries the bootstrap DNS of the DERP map baked into tailscale.com;
  `dnsfallback.SetCachePath` only merges into that map and there is no knob. The e2e uses IP
  literals, so it never hits this. Right: split-horizon DNS that cannot fail, or a control URL that
  needs no lookup.
- **Reverse proxy proven with a stand-in, not Traefik** (`scripts/net-e2e.sh`): ts2021 (`POST
  /ts2021`, `Upgrade: tailscale-control-protocol`) and DERP (`GET /derp`, `Upgrade: DERP`) both pass
  Go's `httputil.ReverseProxy` behind TLS with a path allowlist, and DERP-only traffic works through
  it. Open for stronghold: Traefik v3's entrypoint `respondingTimeouts.readTimeout` (60 s default)
  on hijacked upgrade connections, which would cut the long-lived control and DERP streams; HTTP/1.1
  to the backend for upgrades; `trusted_proxies` with stronghold's IP. Right: the same e2e phases
  against the real router.
- **A member starts only while control answers `/key`** (`go/cmd/member`): the pin check needs it,
  so a member restarted during a headscale or stronghold outage stays down until control is back.
  Right: decide whether a start with an unverifiable key may run on the cached netmap.
- **Windows UDP socket and the firewall prompt**: officewin (the owner's desktop) ran member
  DERP-only (`TS_DEBUG_ALWAYS_USE_DERP=1`), which binds no UDP socket, so no Defender Firewall prompt
  could appear. Whether a normal member's UDP bind prompts on a desktop is unverified. Right: try it
  on a spare Windows box; the installer adds a rule for the fixed `--udp-port`, or Windows nodes
  build with `ts_omit_udptransport` and relay through DERP.
- **tsnet close can hang** (`go/cmd/member`): with `TS_DEBUG_ALWAYS_USE_DERP`, wireguard-go's bind
  close waited forever, on stop and after `--print-registration`; member now gives Close 5 s and
  exits anyway. Right: an upstream fix.
- **`headscale version` reports our module version**, since headscale reads it from the main
  module's build info. Right: stamp it, or have the hub report the go.mod pin instead.
- **Android member is unverified at runtime**: `GOOS=android GOARCH=arm64 CGO_ENABLED=0` builds (a
  PIE using /system/bin/linker64, which picks tailscale's raw-binary netmon and dnsproxyd paths), so
  that is the Termux target rather than a static linux/arm64 build. No device has run it (netmon,
  DNS, SELinux exec). Right: a Termux trial before promising it.
