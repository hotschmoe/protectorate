# Spike: per-sleeve network namespaces without root

Problem: sleeves are bwrap inside a systemd user scope and share the host network namespace, so
two sleeves' dev servers fight over ports and any sleeve can reach every host loopback service.
Goal: a rootless per-sleeve netns where harnesses work untouched (HTTPS egress, the `protectorate`
CLI reaches the hub API, the user reaches a sleeve dev server through a hub-side port forward).

Host: Debian 12 aarch64, kernel 6.6, bwrap 0.8.0, Go 1.27, uid 1001, `/dev/net/tun` mode 0666.
Neither pasta nor slirp4netns is installed (not installed or downloaded for this spike).
Measured 2026-10-20. Nodes: Windows and Android run no sleeves (ARCHITECTURE.md line 303; the
sleeve manager is Linux-only, `manager_test.go` is `//go:build linux`), so nothing here touches them.

## Prototypes (not wired into the product)

- `spikes/netns/` (A): `netns run [-forward 127.0.0.1:18001=8000] [-allow names] [-gw-ports 7470] -- bwrap ... -- cmd`.
  Re-execs itself in a fresh user+net namespace with `CAP_NET_ADMIN` as an ambient cap (uid mapped
  1001 to 1001), creates a TAP, sends the fd to the hub over a unix socketpair (SCM_RIGHTS), clears
  the ambient cap and execs the command. The command is plain bwrap WITHOUT `--unshare-net`, so it
  inherits the namespace. The hub runs a gvisor netstack on the TAP fd (promiscuous + spoofing,
  TCP and UDP forwarders) and dials host sockets. 10.0.2.2 is the gateway (optionally mapped to host
  127.0.0.1, restricted to listed ports), 10.0.2.3 answers DNS by relaying to the host resolver.
  gvisor is already linked via tailscale.com (`gvisor.dev/gvisor` in go.mod), so no new module.
- `spikes/netns/proxyfwd/` (B): `proxy` (HTTP/CONNECT proxy on a unix socket, host allowlist),
  `bridge` (inside the sleeve: 127.0.0.1:3128 to the unix socket, plus `-expose` for dev-server
  access), `forward` (hub side: TCP listener to the sleeve's exposed loopback port).

## Results

| | A. bwrap + netstack NAT in hub | B. `--unshare-net` + unix sockets + CONNECT proxy | C. pasta / slirp4netns (not run) |
|---|---|---|---|
| Added cold start | 5-12 ms in-process (child spawn + TAP + fd pass + netstack init). One-shot process wall incl. Go start/teardown was ~75 ms vs 15 ms plain bwrap; a long-lived hub does not pay the process start. Works under `systemd-run --user --scope` with MemoryMax | ~20 ms (bwrap `--unshare-net` costs nothing, 13 ms vs 15 ms; the in-sleeve bridge to listening measured 32 ms vs 12 ms) | one extra fork/exec per sleeve, typically a few ms to tens of ms (unmeasured) |
| Idle memory | Go heap 0.65 MiB per netstack, process RSS 14 MiB standalone (the hub already carries the runtime); no per-sleeve process | proxy 9.5 MiB RSS (one per hub, shareable); bridge 12 MiB RSS as built (Go with net/http; a dedicated forwarder would be smaller) per sleeve | one process per sleeve, a few MB (pasta C, slirp4netns C) |
| Throughput | 42 MB/s (340 Mbit/s) bulk via gateway vs 1.26 GB/s direct loopback: fdbased/readv single queue, fine for API traffic and package fetches | proxy streaming, not measured, expected line rate | pasta is fast (splice); slirp4netns ~ 1 Gbit |
| LOC (spike) | ~590 (main 115, child 164 mostly ioctl glue, nat 311); product version ~700-900 with tests, lifecycle, DNS/UDP GC, IPv6 | ~150 plus per-sleeve exec of the bridge | ~60 to spawn and supervise; plus a packaged binary |
| Port clash fixed | yes: two sleeves both bound `:8000`, both reachable through separate hub forwards | yes (private loopback) | yes |
| Host loopback isolation | yes: sleeve `127.0.0.1` is private; `10.0.2.2` maps to host loopback only for the ports given (`-gw-ports`), others refused | yes: no IP route at all | only if host-loopback mapping disabled (pasta maps by default via `--map-host-loopback`) |
| HTTPS egress, harness untouched | transparent: curl, git, go, python, npm, cargo not configured at all; claude, codex, grok, pi need no env | needs proxy env; see compatibility below | transparent |
| Egress allowlist | possible and strong: DNS answers for non-allowed names get NXDOMAIN and only IPs the hub resolved for allowed names may be dialed (tested: example.com 200, example.org refused). Raw IPs, DoH to an allowed host and SNI games are policy gaps, not mechanism gaps | possible at the CONNECT host name (tested: 403 for others); cooperative, since a tool ignoring the env simply has no network | not possible beyond blocking ports; would need the host nft (root) |
| Hub API from inside | `protectorate` CLI uses `http://10.0.2.2:7470` (hub listens on 127.0.0.1:7470 today); no code change besides the URL env | hub must also listen on a unix socket (not today) and the CLI must dial it; or tunnel 7470 through the bridge | gateway to host loopback |
| Hub-side port forward to sleeve dev server | netstack `gonet.DialTCP` to 10.0.2.100:port from a hub-side TCP listener (tested, two sleeves on :8000 concurrently). No privileges and no process entry needed | needs an in-sleeve agent: `bridge -expose` unix socket plus `forward` (tested). nsenter is not possible unprivileged | `pasta -t` / slirp4netns `add_hostfwd` API (works) |
| What breaks | no ICMP ping (not NATed), IPv4 only, inbound LAN access to the sleeve impossible by design, `/dev/net/tun` must exist on the host (absent inside bwrap, so the child opens it before bwrap), unprivileged userns required (same as bwrap today), netstack bugs are our bugs. mDNS/raw sockets fail | tools that ignore proxy env (Node `fetch` w/o `NODE_USE_ENV_PROXY=1`: EAI_AGAIN, node-based `pi` risk), non-HTTP protocols (git over ssh, raw TCP, UDP DNS, MCP servers over sockets), loopback `NO_PROXY` handling, every new tool is a compat question | none of ours; external dependency |
| Windows / Termux | none: sleeves are Linux-only on the hub; nodes run no sleeves. Netstack is portable Go but TAP, userns and bwrap are not | none (same) | none |

### B: proxy-env compatibility (inside `bwrap --unshare-net`, only the unix socket reachable)

Measured with real requests to public endpoints through the proxy (log showed CONNECT):
curl OK, git https `ls-remote` OK, `go list -m -versions` OK (proxy.golang.org, sum.golang.org),
`npm view` OK, `cargo search` OK (needs `RUSTUP_HOME`), python urllib OK, Node 22 `fetch` FAILS
(EAI_AGAIN) unless `NODE_USE_ENV_PROXY=1`, then OK.
Harnesses (no credentials, empty HOME, no model calls): `claude` OK (4x CONNECT api.anthropic.com
before reporting not logged in). `grok`, `codex`: binaries contain HTTPS_PROXY/ALL_PROXY/NO_PROXY
handling (static `grep` only; behavior unverified live). `pi` (node): bundle ships
https-proxy-agent, live behavior unverified. So B depends on the proxy env being honored per
harness, which the "hands-off" rule says we must not enforce by patching.

### C: pasta / slirp4netns as an optional dependency (documented only)

- Install: `passt` package on Debian 12 (`pasta`), or the static `slirp4netns` release binary.
  Neither is on this host; not downloaded here.
- Flow: bwrap `--unshare-net` cannot be joined from outside unprivileged, so launch a holder
  process in `unshare -Urn`, run `pasta <holder-pid>` (it enters the netns via /proc/PID/ns), then
  start bwrap inside the same namespaces (as in A). slirp4netns takes the same netns PID and a TAP
  it creates itself. Both give transparent egress and a hostfwd API, no policy hook.
- Costs: a new external binary per host, version skew, packaging for hub installs, and no egress
  allowlist. If A proves insufficient this is the cheapest fallback; it fits "optional dependency"
  behind a feature check like the existing `bwrap` / `systemd-run` probe.

## Findings and gotchas

- Joining a netns that bwrap does not own works because bwrap only needs its own user namespace:
  start bwrap inside an outer `CLONE_NEWUSER|CLONE_NEWNET` child and omit `--unshare-net`.
- The outer child needs `CAP_NET_ADMIN` only until the TAP is configured. Go's `AmbientCaps` gives
  it; clear it with `PR_CAP_AMBIENT_CLEAR_ALL` before exec. No sudo, no setuid helper.
- Interface address, `lo` up and the default route are plain ioctls (`SIOCSIFADDR`, `SIOCADDRT`
  with a hand-laid `rtentry`); no netlink dependency was needed.
- `fdbased.New` needs `PacketDispatchMode: fdbased.Readv` for a TAP fd (the default recvmmsg path is
  for sockets). The NAT is just `tcp.NewForwarder` + `udp.NewForwarder` with promiscuous+spoofing.
- Closing the hub-side TAP fd or killing the holder (`Pdeathsig`) tears the netns down; no named
  namespaces or files exist, so cleanup is process termination only.
- The sleeve resolv.conf points at the LAN resolver (192.168.10.1 here). Port-53 UDP to any
  address is intercepted and relayed, so no resolv.conf rewrite was needed. Hosts whose resolv.conf
  is `127.0.0.53` also work for the same reason.
- Unix socket paths are limited to 108 bytes; the scratch path overflowed and `bind` reported
  EINVAL. Use `/run/user/UID/...`.

## Recommendation

Adopt A for Linux hubs, behind a per-sleeve opt-in (`network: isolated`) with the current shared
host network as the default until it has soaked. It is the only option that keeps harnesses and
every package tool untouched (no proxy env), fixes port clashes and loopback exposure, adds
~10 ms, needs no new dependency or privileges, gives a real DNS-level egress allowlist, and a
port forward falls out of the same stack for free. Cost is real code in the hub (~800 LOC with
lifecycle and tests) and owning a NAT; mitigate by keeping bulk throughput modest and mapping the
gateway to host loopback for the hub API port only. Keep C as a documented fallback if netstack
maintenance proves a burden. Reject B as the primary mechanism: it breaks non-proxy-aware tools
(Node fetch by default) and relies on harness cooperation; its unix-socket forward pattern is
only worth reusing if a no-IP sleeve mode is ever wanted.

Reproduce: `go build -o /tmp/netns ./spikes/netns`, then
`/tmp/netns run -forward 127.0.0.1:18001=8000 -- bwrap --unshare-pid --die-with-parent --ro-bind / / --dev /dev --proc /proc -- python3 -m http.server 8000`.
