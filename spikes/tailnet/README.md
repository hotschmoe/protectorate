# Spike: tsnet + Headscale vs plain wireguard-go hub-and-spoke

Question: does embedding `tailscale.com/tsnet` with a self-hosted Headscale work
for the hub/node model across our platforms, and what does it cost versus a
hand-rolled wireguard-go (netstack) hub-and-spoke?

Verdict up front: **it works on every platform I could run, and the cost is
~15 MB of binary and ~20 MB of RAM per node. Recommend tsnet + Headscale as the
reachability layer**, with SSH (see `spikes/ssh-agentless`) kept as an optional
bootstrap/transport, not as a replacement. Details and caveats below.

## What was built (all under `spikes/tailnet/`, its own go.mod)

- `main.go`: one program, `-mode node|call`. Embeds tsnet (state dir per
  instance, `ControlURL` = Headscale). `node` serves `GET /metrics` (hostname,
  os, arch, numcpu, load, caller) on its tailnet listener `:8081` and uses
  `LocalClient().WhoIs()` to reject any caller whose node does not carry
  `tag:hub`. `call` (the hub) joins and GETs `http://<name>:8081/metrics` by
  MagicDNS name.
- `wg/main.go`: wireguard-go + `tun/netstack` equivalent, static keys,
  `-mode genkey|hub|spoke`.
- `baseline/main.go`: stdlib-only HTTP JSON server for size comparison.
- `headscale.sh`, `local-test.sh`, `win-test.sh`, `android-test.sh`,
  `wg-test.sh`, `sizes.sh`: reproduction scripts.
- Headscale v0.29.5 (docker image `headscale/headscale:v0.29.5`, container
  `spike-headscale`, host network, bound to `192.168.10.44:8080`). tsnet
  `tailscale.com v1.104.1`. Go 1.27.2.

## Reproduce

```
export PATH="$HOME/.local/go/bin:$PATH"
cd spikes/tailnet
mkdir -p .state/hs/etc .state/hs/lib .state/hs/run
curl -sL https://raw.githubusercontent.com/juanfont/headscale/v0.29.5/config-example.yaml -o .state/hs/etc/config.yaml
# edit config.yaml: server_url http://192.168.10.44:8080, listen_addr 192.168.10.44:8080,
#   dns.base_domain tn.protectorate.test, policy.path /etc/headscale/policy.hujson
# policy.hujson: tagOwners tag:hub, tag:node -> "spike@"; acls:
#   tag:hub -> tag:node:* ; tag:node -> tag:hub:7470 and tag:node:* (the last
#   only so the intruder test reaches the node and is stopped by WhoIs)
./headscale.sh start
./headscale.sh cli users create spike
./headscale.sh cli preauthkeys create -u 1 --reusable -e 24h --tags tag:hub     # -> HUB_KEY
./headscale.sh cli preauthkeys create -u 1 --reusable -e 24h --tags tag:node    # -> NODE_KEY
./sizes.sh                                   # builds bin/* for all 5 targets x 3 variants
export HUB_KEY=... NODE_KEY=...
./local-test.sh                              # join, call, intruder reject, restart w/o key
./win-test.sh                                # needs scp of bin/tsnet-windows-amd64.exe to officewin first
./android-test.sh                            # docker termux/termux-docker:aarch64
./wg-test.sh                                 # wireguard-go variant (build: go build -o bin/wg-test ./wg)
./headscale.sh stop
```

## Results

### Functional (tsnet + Headscale)

| check | result |
|-------|--------|
| node joins with tagged preauth key (no TUN, no root) | pass, ~2 s cold |
| hub calls node by MagicDNS short name, HTTP 200 | pass |
| non-hub caller (`tag:node` "intruder") reaches the port | gets 403, node logs `REJECT intruder...`; hub gets `ACCEPT hub...` |
| restart node with NO authkey, same state dir | pass: same IP (100.64.0.1), same tag, call works |
| linux/arm64 (this host) | pass |
| Windows AMD64 (`officewin`, run from %TEMP%\protectorate-spike, 70 s bounded, no install) | pass: joined as 100.64.0.4, hub got 200 with `os=windows numcpu=24`; no process left running |
| Android arm64 build in termux-docker (aarch64) | pass in that environment: joined, answered the hub, RSS 33 MB. Caveat: this is a regular Linux kernel with an Android-style rootfs; it does NOT reproduce Android 11+ app SELinux limits (see risks) |
| Windows ARM64 | built only, no device available |

Gotcha found: `srv.Up()` returns before the netmap has all peers, so the first
lookup of a peer short name immediately after join can fail with
`no such host`. Retry for a few seconds (the hub's call loop does).

### Size and memory (CGO_ENABLED=0, `-trimpath -ldflags "-s -w"`, bytes)

| target | stdlib baseline | wireguard-go netstack | tsnet |
|--------|-----------------|-----------------------|-------|
| linux/arm64 | 6.1 MB | 9.2 MB (+3.1) | 20.6 MB (+14.5) |
| linux/amd64 | 6.6 MB | 9.8 MB (+3.2) | 22.2 MB (+15.6) |
| windows/amd64 | 6.8 MB | 10.1 MB (+3.3) | 22.5 MB (+15.7) |
| windows/arm64 | 6.1 MB | 9.3 MB (+3.2) | 20.5 MB (+14.4) |
| android/arm64 | 6.8 MB | 10.4 MB (+3.6) | 21.6 MB (+14.8) |

All five targets build with CGO off, including tsnet on android.

Dependencies (this module's go.sum is 239 lines total): tsnet pulls **360
non-stdlib packages from 37 modules**; the wireguard-go variant pulls **85
packages from 9 modules**. (Baseline is stdlib only.) Tailscale tracks its
own fork of several deps, so the tsnet tree moves quickly; pin and update
deliberately.

Idle RSS: tsnet node linux/arm64 **~43 MB**; tsnet in termux-docker (android
build) **~33 MB**; wireguard-go netstack spoke **~21 MB**. Windows RSS not
captured (my probe reads /proc). Not measured: throughput, CPU under load,
Headscale's own RSS.

### wireguard-go + netstack variant

Works locally (spoke initiates to hub UDP endpoint, hub dials spoke's tunnel
IP through its own netstack, HTTP 200). Authorization is implicit: with
`allowed_ip=<hub>/32` on the spoke's only peer, no one but the holder of the
hub key can deliver packets to the spoke. That is strong and trivial, but note
it only distinguishes "the hub" from "everyone", with no per-node roles.

What we would have to write to match what Headscale provides:

| capability | Headscale/tsnet | wg-go DIY |
|------------|-----------------|-----------|
| enrollment / key distribution | preauth keys, tags, expiry, revoke (CLI/API) | registration API, one-time tokens, key storage, revoke = hub reconfigure |
| IP allocation | automatic, stable per node | allocator + persistence |
| name resolution | MagicDNS | hub-maintained name table |
| roaming (laptop changes network/IP) | handled (netmap, endpoint updates) | spoke->hub keepalive covers spoke->hub, but hub->spoke only works while the hub has seen a recent handshake; fine in star topology |
| NAT traversal | STUN + DERP relay fallback + hole punching | none needed in a star if the hub has a public endpoint; spoke<->spoke impossible |
| ACLs | policy file, per-tag, per-port | write our own (or just hub-only allowed_ip) |
| caller identity in app | `WhoIs` (name, tags, user) | none beyond "came through the tunnel" |
| non-hub clients (phone, laptop apps) | stock Tailscale app with custom control URL | stock WireGuard app + hand-issued configs |
| key rotation / node expiry | built in | build it |

### DERP

I used **Tailscale's public DERP map** (Headscale default
`derp.urls: https://controlplane.tailscale.com/derpmap/default`,
embedded DERP disabled). All test traffic was on one LAN, so relays were
not exercised. Fully self-hosted requires: enable `derp.server` (embedded DERP
+ STUN on UDP 3478, needs a public IPv4 and, for browsers/clients, TLS on the
server_url), set `derp.urls: []` and `auto_update_enabled: false`, and set
`verify_clients: true`. That works for an on-prem hub with a public address;
without public reachability of the hub, nothing off-LAN works under any option.
Note: Headscale here ran plain HTTP (LAN-only test). Production needs TLS on
the control URL (Let's Encrypt or own CA); clients of non-LAN networks should not
run Noise-over-HTTP across the internet unprotected by TLS (Noise itself still
encrypts, but use TLS anyway for the stock apps).

## Comparison with the SSH spike (`spikes/ssh-agentless`)

They answer different questions.

| | SSH stdio | tsnet + Headscale |
|---|-----------|-------------------|
| solves reachability (roaming, NAT, off-LAN) | no | yes |
| node resident process | none | yes (~20-40 MB RSS, 15 MB binary) |
| cold call | 318 ms | ~2 s join, then HTTP over the tunnel (per-request latency not measured here) |
| warm call | 45 ms mux / ~0.2 ms streamed | not measured; expect LAN-direct sub-ms to low ms |
| gating | sshd forced command + our binary | tag ACLs at network layer + WhoIs in app |
| enrollment | key on node | preauth key on node |
| extra infra | none | Headscale (+ DERP for off-LAN) |

They compose: tsnet/Headscale gives every machine a stable address and a
network-level gate; SSH-over-that-address (or our own Connect endpoint) is the
transport. SSH does not remove the need for a reachability layer once laptops
roam or sit behind NAT.

## Risks and unverified items

- Android 11+ restricts netlink for app UIDs; Go's interface enumeration can
  fail in plain binaries run from Termux on a real device. Tailscale's own
  Android app avoids it via VpnService. Not reproducible here; needs a real
  device test before we commit Android node support. Fallback for Android:
  the stock Tailscale app on the phone (as a client of the hub), not an
  embedded node.
- Windows ARM64 and macOS untested at runtime.
- Tailscale's tsnet API is public but its tree is large and moves monthly;
  Headscale supports only recent client versions (it rejected clients below
  v1.80 in its log), so the two must be upgraded in step.
- Headscale is a third-party single-tenant control server (sqlite); it is one
  more service to run, back up and keep up to date.
- Tagged nodes via preauth keys never expire by default; set key and node
  expiry policy deliberately.

## Recommendation

Use **embedded tsnet + Headscale** as the reachability and network-gating
layer. Given that orchestration code may be as complex as needed to be correct,
robust and fast, the 15 MB / ~20 MB-RSS per node is a good price for: automatic
roaming and NAT traversal (including DERP fallback), stable names and IPs,
per-tag ACLs enforced before our code runs, `WhoIs` identity for app-level
checks, rotation and revocation, and zero TUN/admin requirements on all five
targets that I could test (three run end to end). Stock Tailscale apps also
cover phones and laptops without extra code.

Choose plain wireguard-go only if (a) the fleet stays a pure star with a
publicly reachable hub, (b) we explicitly want ~11 MB less binary and ~20 MB
less RAM, and (c) we accept writing enrollment, IP allocation, naming, expiry
and ACL ourselves. That is a few hundred lines to demo and a long tail to make
robust; it duplicates what Headscale already does well.

Pros of tsnet+Headscale: works on every OS tried with no privileges;
identity-aware; NAT-proof; mature. Cons: big dependency tree, version
coupling, extra service, Android real-device behavior unproven, HTTPS/DERP
setup needed for production.

Suggested layering from both spikes: tsnet+Headscale for reachability and
gating, our own Connect-style API on the node's tailnet listener as the
transport (a persistent connection amortizes the join cost), SSH retained as
the zero-install bootstrap and break-glass path for machines not yet enrolled.

## Cleanup state

`spike-headscale` container stopped (not removed, so keys/state in `.state/hs`
remain usable); `spike-termux` stopped. Images pulled and left:
`headscale/headscale:v0.29.5`,
`termux/termux-docker@sha256:3a0a340fca9947f7139a0d695f1234edf0172daed6a2f1e0b1413ef91453c9ce`
(tag `aarch64`). officewin: `%TEMP%\protectorate-spike\spike.exe` and `state\`
left in place (no deletion permitted); no processes running. Five test nodes
registered in the (stopped) Headscale. `.state/` and `bin/` are gitignored.
