# Network, trust and language

Design investigation, 2026-11-01. Covers: embedded headscale, how nodes join, one-way trust,
enrollment, TLS/Traefik/DERP, mainplane, the language choice, and how networking limits the
number of binaries.

Legend: **[V]** = read the code or docs, or ran it here. **[I]** = inference. **[E]** = external
fact, not checked here.

Versions read: headscale v0.29.7 (`c9cf210`, checked out in the new clone `references/headscale`);
tailscale.com v1.104.4, which that tag pins (`go.mod:76`), read from
`~/go/pkg/mod/tailscale.com@v1.104.4` (`references/tailscale` is main, `49aa9c8`); new clones
`references/libtailscale` (`ac7ed43`) and `references/tailscale-rs` (`5dfd5a0`);
`references/mainplane` (`560356d`). Old Go tree: `git show bc9b87a^:<path>`.

---

## 0. Recommendation

1. **Headscale stays official and unmodified.** CI builds it from upstream source at a pinned tag.
   The hub binary embeds it as bytes and runs it as a supervised, sandboxed **child process**,
   never linked into the Rust process: "inside the hub binary", not "inside the hub process"
   (section 2).
2. **Go is one module, `go/`, with two mains:** `cmd/headscale` (upstream's own `main`, calling
   `cli.Execute()`) and `cmd/member` (our tsnet glue, about 500 lines [I]). All `CGO_ENABLED=0`.
   No Go anywhere else.
3. **Nodes join with tsnet**: userspace, no TUN, no root, no tailscaled. The same `member` blob runs
   as a child of the Rust node agent. A node listens on one virtual port (`:7475`) and dials
   nothing (section 3).
4. **One-way trust is layered:** a static policy with one rule (`tag:hub -> tag:node:7475`), a
   dial-only hub member with peerAPI compiled out, a hub API that never listens on the tailnet, no
   hub credential on any node, and all node output parsed as hostile. One surface cannot be
   removed: nodes must talk to headscale and DERP. So headscale runs in its own sandbox, with none
   of the hub's secrets (section 4).
5. **Enrollment is "scan to join".** The owner runs the README one-liner on the new machine. The
   machine shows a QR of its headscale registration. The owner scans it in the app, names the
   node, picks a tier and approves. The hub then registers and tags the node through headscale's
   local CLI. No secret ever leaves the hub (section 5).
6. **Traefik terminates TLS for `protectorate.hotschmoe.com`.** One router sends only headscale's
   paths to headscale; the existing router sends everything else to the hub's trusted-proxy
   listener. Headscale's embedded DERP sits behind Traefik, with an IP-pinned DERP map and STUN
   off. No Tailscale public DERP (section 6).
7. **mainplane: adopt nothing structural.** It forks tailscale, trims headscale, uses a kernel TUN,
   has workers dial the harness, and tunnels to the WAN. Two small ideas are worth taking
   (section 7).
8. **Language: Rust + Go.** Zig stays only a C toolchain behind cargo-zigbuild (section 8).
9. **Deliverables: a hub binary and a node binary** (per `nodes-ci-deliverables-verus.md`), each
   embedding its Go blob(s). Networking adds no file (section 9).

---

## 1. Why the old tailnet was dropped, and what is different now

Headscale and tsnet themselves worked in the Go deployment [V]: relay-only traffic through the hub
DERP was proven (`53d7a14`), and node restarts recovered (`a26e576`). The overlay went because the
owner made the branch Rust-only (memory `stack-preferences`), the replacement was a hand-written
Rust tsnet, and the routers then took over connectivity (rust-direction.md:88, :333) [V].

| Failure | Evidence | This design |
|---|---|---|
| Rust rewrite of tsnet: 14 crates, about 15k lines (WireGuard, userspace TCP, DERP, STUN). No cookie defence, NAT traversal or congestion control; unaudited | rust-direction.md:88 (decision 5), :1204 | Only upstream tsnet and headscale code carries packets |
| tsnet and headscale had to be upgraded together; headscale rejects old clients | rust-direction.md:715; old `install.go:23` | One go.mod: the member gets headscale's own `tailscale.com` through MVS, and one release ships both blobs. Headscale accepts the last 10 minor client versions (`capver_generated.go:84-89`) [V] |
| Separate headscale download step (`hub tailnet install`) | old `supervisor.go:108-121`, `install.go:42` | Embedded; extracted into a sealed memfd |
| Embedded DERP "never worked" on an http control URL; fixed with a self-signed pinned cert | `61c457e`, old `derp.go` | Traefik's Let's Encrypt cert, with headscale's own DERP behind it |
| Non-root dials bound to the default-route interface, so a VPN-only hub was never reached | `3132e04`, old `net.go:60-68` | Keep `netns.SetEnabled(false)` in `member` |
| A restarted member got a new disco key; the hub was unreachable for minutes | `a26e576`, old `net.go:141-158` | Keep the node's disco ping on start. It runs at the magicsock layer, so it is part of the irreducible surface (4.4) |
| Backups had to stop headscale (SQLite) | `d33af21`, old `headscaledb/gate.go` | Same stop/copy/resume in the supervisor, plus a snapshot before each update trial |
| Without the overlay, the hub dialled node:7475 over the LAN. That needed an inbound firewall rule per node, a hub CA and mTLS; this upkeep "dominated the cutover" | memory `production-cutover-2026-10-26` (kidsbox ufw, officepc prompt), `one-machine-direction` | Nodes only dial out at the IP layer (control and DERP on TCP 443, UDP hole punching). No CA, no inbound rule. WireGuard keys and control-assigned tags replace mTLS |
| Go hub used 164 MB of memory after two days, plus 38 MB for headscale | rust-direction.md:316 | A Rust hub plus two Go children. Each child should stay near headscale's old 38 MB, and the member less, with features compiled out [I] |

---

## 2. Headscale inside the hub

### 2.1 What headscale gives an embedder [V]

- `hscontrol` is importable: `NewHeadscale(cfg)` (`hscontrol/app.go:125`) and `Serve()`
  (`app.go:617`). It is not a library API, though:
  - `Serve` calls `signal.Notify(SIGHUP, SIGINT, SIGTERM, SIGQUIT)` (`app.go:922`).
  - It shuts down only on a signal: no context, no Stop (`app.go:913-1083`).
  - `log.Fatal` exits the process at `app.go:626, 901, 908, 1091`.
  - Local gRPC on the unix socket runs "without TLS and without authentication" (`app.go:767`).
- `cmd/headscale/cli` is importable. Its `Execute()` (`cli/root.go:144`) is all of upstream's main
  apart from logger setup (`cmd/headscale/headscale.go`).
- headscale's go.mod has no `replace` directives. A module importing `cli` plus tsnet at v0.29.7
  links the same dependencies as upstream `cmd/headscale`, by `go version -m`; the only
  differences are upstream main's `termcolor` and the main module itself. Both builds are
  49,152,160 bytes (arm64, stripped), because headscale already links tsnet for tailsql
  (`hscontrol/tailsql.go:11,36`).
- With no CLI address set, the CLI uses the unix socket (`cli/utils.go:104-112`). `-o json` gives
  JSON output (`root.go:28`).

### 2.2 Options

| Option | Verdict |
|---|---|
| A. `-buildmode=c-archive` hscontrol, linked into the Rust hub | **Rejected.** In c-archive mode, `Notify` installs Go handlers for those signals (Go `os/signal/doc.go:195-202`), so headscale takes SIGTERM and SIGHUP from tokio. Shutdown would need a signal. Any `log.Fatal` or unrecovered goroutine panic kills the hub and every sleeve supervisor. Two GCs share one RSS. It cannot be sandboxed away from the hub's secrets. It needs cgo plus a C toolchain per target |
| B. Upstream release binary, SHA-pinned, embedded, exec'd | Fallback. Literally upstream's bytes, but a second pin to keep in step with the member's `tailscale.com` |
| **C. CI builds upstream `cmd/headscale` at a pinned tag; embed and exec** | **Recommended.** Upstream source and dependencies, checked by go.sum and the sumdb, plus a CI diff against the release binary (2.4). One `go get` moves headscale and tailscale.com together |
| D. Fork or trim hscontrol (mainplane) | Rejected: not official code, and it loses the policy engine (section 7) |
| E. Separate systemd unit | Rejected: a second artifact and install step, which was the old pain |

### 2.3 Mechanics (option C)

- **Build:** `CGO_ENABLED=0 GOOS=linux GOARCH={amd64,arm64} go build -trimpath -ldflags='-s -w
  -X <version vars>' ./cmd/headscale`, then gzip with flate2, already a dependency
  (`rust/Cargo.toml:18`), and embed via `build.rs` `include_bytes!`. The Makefile builds `go/`
  before cargo. A build took 55 s here, including module downloads [V].
- **Start:** `memfd_create(MFD_CLOEXEC|MFD_ALLOW_SEALING)`, decompress, check SHA-256 against the
  compiled-in value, seal (WRITE, SHRINK, GROW), then exec `/proc/self/fd/N headscale serve -c
  <home>/tailnet/headscale.yaml`. Nothing on disk to tamper with [I].
- **Supervision:** port the old backoff supervisor (`supervisor.go:52-106`). `PR_SET_PDEATHSIG`
  ties the child to the hub. Rotate logs as in `87ca746`.
- **Sandbox:** our namespace sandbox: user, mount and PID namespaces, the host network namespace,
  and only `tailnet/` writable. The child has no view of credentials, `identity.json` or harness
  logins.
- **Config:** generated each start from a typed struct, as the old `config.go:175-231` was:

  ```yaml
  server_url: https://protectorate.hotschmoe.com
  listen_addr: 192.168.10.44:7480            # Traefik and the hub member reach it here
  metrics_listen_addr: ""
  grpc_listen_addr: 127.0.0.1:50443          # remote gRPC stays off: no TLS, not insecure (app.go:805)
  grpc_allow_insecure: false
  unix_socket: <home>/tailnet/headscale.sock # 0700 dir; never mounted into sleeves
  trusted_proxies: [192.168.10.50/32]        # real client IPs (app.go:149-153)
  tls_cert_path: ""                          # Traefik terminates
  policy: {mode: file, path: <home>/tailnet/policy.hujson}  # SetPolicy needs DB mode (grpcv1.go:731)
  derp: {server: {enabled: true, verify_clients: true, stun_listen_addr: 127.0.0.1:3478,
         automatically_add_embedded_derp_region: false},
         urls: [], paths: [<home>/tailnet/derp.yaml], auto_update_enabled: false}
  logtail: {enabled: false}                  # clients get Debug.DisableLogTail (types/config.go:1557-1566)
  dns: {magic_dns: false, override_local_dns: false}
  database: {type: sqlite, sqlite: {path: <home>/tailnet/db.sqlite, write_ahead_log: true}}
  disable_check_updates: true
  ```
- **Admin:** exec the same blob as `headscale -c ... <preauthkeys|nodes|auth> ... -o json` over the
  unix socket. **No API key is ever created**, so `/api/v1` answers 401 to everyone
  (`app.go:464-501`) [V], and Rust needs no gRPC or REST client.
- **The hub member** is a second child (`member hub`). At first start it gets a single-use,
  10-minute `tag:hub` pre-auth key minted through the CLI (the old `UpHub`, `net.go:316`). Its
  control URL is `http://192.168.10.44:7480`, direct rather than through Traefik, so the hub's
  netmap never depends on stronghold; mainplane does the same for its own node (`mesh.go:91-92`).
  [I] that one server works through two URLs; check it in e2e.
- **Backup:** stop the child, copy `db.sqlite`, its `-wal` and the keys, then resume (as in
  `d33af21`).
- **Upgrade:** bump `go/go.mod`, which makes a new hub release. The update trial snapshots
  `tailnet/` before the new headscale migrates (gormigrate, `db/db.go:66`), and a rollback
  restores it. Old headscale validates the schema with squibble (`cli/serve.go:22-23`); [I] that it
  refuses a newer one.

### 2.4 Pinning and verification

`go/go.mod` requires `github.com/juanfont/headscale vX` and nothing else of ours; `go.sum` pins
everything. CI downloads the upstream release binary for that tag and diffs its `go version -m`
dependency lines against our blob; any difference fails the build (diff run here [V]). Headscale
code is never edited. Fixes arrive only through a tag bump.

### 2.5 Sizes, measured here [V]

| Blob (stripped) | Raw | gzip -9 | xz |
|---|---|---|---|
| headscale linux/arm64; linux/amd64 | 49.2 MB; 52.4 MB | 18.0 MB | 12.8 MB |
| tsnet member, default features, linux/arm64 | 20.6 MB | 7.9 MB | 5.6 MB |
| member with `ts_omit_*` (3.2): linux/arm64, windows/amd64, android/arm64 | 18.7 / 20.3 / 19.9 MB | 7.0 / 7.8 / 7.0 MB | - |

Hub binary estimate: 16 MB of stripped Rust (rust-direction.md:315) + 18 MB + 7 MB, about 41 MB
[I]. The old Go hub was one 40.8 MB binary (same line) [V].

---

## 3. Nodes on the tailnet

### 3.1 Options

| Option | Verdict |
|---|---|
| **tsnet `member` as a child of the Rust node agent** | **Recommended.** Pure Go with `CGO_ENABLED=0` for linux/amd64, linux/arm64, windows/amd64, windows/arm64 and android/arm64 (all five built) [V]. No root, TUN or route, so jobs on the node cannot use the tailnet. Coexists with the user's Tailscale. Crash-isolated |
| libtailscale (official tsnet C library: c-archive, socketpair fds; `tailscale.h`, `Makefile:32`) in-process | Rejected for now. tsnet calls no `signal.Notify` [V], but this needs cgo per target. Windows would force Rust onto `*-windows-gnu`, since cgo uses mingw or clang [E] and we use MSVC today. Termux needs the NDK. A Go runtime would live in the agent process |
| tailscaled with a kernel TUN | Rejected. Needs admin, a service and a driver; conflicts with the user's Tailscale; exposes the tailnet to every process; impossible on Termux without root |
| tailscale-rs (official Rust, v0.6.1) | Rejected for now. README: "work-in-progress", "no compatibility guarantees", NAT traversal WIP, no Android, no private DERP relays. About 218 external crates including aws-lc-rs (`cargo tree -e normal`) [V], against our 191-entry lock. Crypto audited in 0.6.0 (CHANGELOG). **Watch it:** at 1.0 with Android it could take Go off the nodes |
| Our own stack | The old failure |

### 3.2 `member` design

- **Build tags:** `ts_omit_ssh, taildrop, webclient, peerapiserver, logtail, clientupdate, drive,
  capture, netlog, tailnetlock, relayserver, portmapper, advertiseexitnode, advertiseroutes`. This
  compiles [V]; `ts_omit_serve` breaks tsnet [V]. `peerapiserver` removes the peerAPI listener that
  tsnet opens on every tailnet address (`ipnlocal/local.go:6434`); `portmapper` stops UPnP and
  NAT-PMP traffic to the routers. Runtime behaviour with these omits needs e2e [I].
- **Process:** on Linux, pdeathsig and a sealed memfd. On Windows, a kill-on-close Job Object; the
  old guardian leaked its child without one (rust-direction section 7) [V]. Windows and Termux
  extract to `<data>/bin/member-<sha256[:16]>` and verify before each exec, since Android SELinux
  may refuse memfd exec [I].
- **IPC:** a unix socket in a 0700 dir, or on Windows an owner-only named pipe via
  `tailscale/go-winio`, which is already linked [V `go version -m`]. Rust sends one header line of
  at most 1 KiB, `ACCEPT` (node) or `DIAL <ip>:7475` (hub); `member` answers `OK <stable-node-id>
  <tags>` and then splices bytes. Status (auth URL, online, errors) arrives as JSON lines on
  stdout. No new Rust crates: tokio has unix sockets and named pipes.
- **Node mode:** `Listen(":7475")` only, and `Dial` is never called. Every accept must `WhoIs` as
  `tag:hub` (the old `identify`, `net.go:225-240`), and Rust re-checks the header.
- **Hub mode:** no `Listen`; `DIAL` refuses any target that is not a `tag:node` peer.
- **LAN-only hygiene:**
  - `ts_omit_logtail` plus `TS_NO_LOGS_NO_SUPPORT=1`. Otherwise tsnet uploads logs to
    log.tailscale.com (`tsnet.go:241, 1100-1135`), and headscale only stops that after the first
    map (`direct.go:1403`).
  - Call `dnsfallback.SetCachePath` with a file seeded from our DERP map; only tailscaled calls it
    (`tailscaled.go:782`). Without it, a failed DNS lookup of the control host asks Tailscale's
    built-in public DERPs (`dnsfallback.go:66-110`). [V] for the code path; [I] for the mitigation.
- **State:** `Dir/tailscaled.state`, mode 0600. Node keys are generated on the node and never leave
  it.

---

## 4. One-way trust

### 4.1 Principals

| Principal | Credential | Reaches | Never |
|---|---|---|---|
| Hub member | WireGuard node key, `tag:hub` | `tag:node:7475` | listens on the tailnet |
| Node member and agent | node key, `tag:node` + `tag:tier-{shared,care}` | headscale control and DERP | starts a tailnet flow; holds a hub credential |
| Human client (Flutter desktop, Android, web) | `pd1.` device token (docs/auth.md) | hub API via Traefik | the tailnet. Android allows one VPN at a time [E], and the phone's is WireGuard |
| Vendor general-agent CLI or MCP on a user machine (GrokBot, Claude Duet, ChatGPT Dot) | **agent token**: a device token with a scope ceiling, no approvals or device admin (owner decision) | hub API via Traefik | the tailnet, approvals |
| Sleeves and Envoy | `pc1.` capability | loopback API via the egress proxy | the tailnet, member and headscale sockets |

**A machine that is both node and client** holds two unrelated identities. The node key sits in the
service's state dir, answers only hub-opened connections and is worth nothing at the API. The
client token sits in the user's 0600 CLI config and is used only over HTTPS through Traefik. The
node agent never reads the CLI config, and job sandboxes do not mount it. Where possible the agent
runs as its own OS user, as the Windows runner account already does (memory `node-tiers`);
otherwise a job that escapes its sandbox can read the client token [I].

### 4.2 Layers

1. **Policy:** one accept rule, in a static file the API cannot change (4.3).
2. **Destination enforcement:** the receiver filters inbound packets
   (`wgengine/filter/filter.go:502-560`). The hub's own filter drops node-initiated SYNs and UDP,
   and a node cannot change that.
3. **The hub member has no tailnet listener**, and peerAPI is compiled out (3.2).
4. **The hub API binds loopback and the trusted-proxy listener only**
   (`protectorate-server/src/transport.rs:17-62`). Today `check_listen`
   (`protectorate-wire/src/lib.rs:477`) admits `100.64.0.0/10` through `private_ip` (`:461-467`).
   Refuse that range in the listener check so the API can never bind a tailnet address, even with
   tailscaled on the hub. Leave `private_ip` itself alone, since
   `protectorate-hub/src/local_model.rs:27` also uses it.
5. **Nodes hold nothing the hub accepts.** On the LAN, a node sees the same front door as any host:
   static web files and 401s.
6. **The control plane is a separate sandboxed process**, without the hub's secrets.
7. **Node output is hostile input** (4.4, items 6 to 9).

### 4.3 Policy (HuJSON, file mode, embedded)

```json
{
  "tagOwners": {"tag:hub": [], "tag:node": [], "tag:tier-shared": [], "tag:tier-care": []},
  "acls": [{"action": "accept", "src": ["tag:hub"], "dst": ["tag:node:7475"]}],
  "ssh": []
}
```

- **Empty `tagOwners`:** no interactive login can claim a tag. [I] that empty lists are accepted;
  check with `headscale policy check`.
- **Pre-auth registrations cannot request tags** (`state/state.go:2128-2130`), and tags on re-auth
  need an authenticating tag owner (`:2240-2262`). So a node cannot become `tag:hub`.
- **Only the admin path assigns tags.** `CreatePreAuthKey` and `SetTags` skip `tagOwners`
  (`grpcv1.go:147-186, 316-361`), and tagging is one-way (`:338-340`).
- **No node-to-node rule.** The old pool lateral rule is gone (old `config.go:254`;
  rust-direction.md:137-160). **Tiers are labels**; the scheduler enforces "care is never
  scheduled".
- **Policy `tests` go in the e2e fixture only**, because they fail while a tag has no nodes
  (`policy/v2/test.go`, `errTestDestinationNoIP`) [V]. Assert that node to hub and node to node
  are denied on every port, and hub to node is allowed only on 7475.

### 4.4 What a hostile node can still send

1. **To headscale:** ts2021 Noise, registration, map polls, Hostinfo strings and endpoint lists.
   All of it lands in headscale's parsers, in its sandbox. A headscale RCE can mint `tag:hub` and
   so **control every node**, family machines included. That is why headscale is isolated from the
   hub's secrets.
2. **To DERP:** relay frames, accepted only from known node keys (`verify_clients`).
3. **To the hub member, WireGuard and magicsock:** handshakes, disco pings, CallMeMaybe and pongs.
   This is unmodified upstream code.
4. **Through the hub's filter:** it **accepts every non-SYN TCP segment, ICMP echo replies and
   errors, and TSMP**, with no connection tracking (`filter.go:532` "tcp non-syn", `:518`, `:552`)
   [V]. These packets reach gVisor's netstack, where nothing listens, and get a RST or are ignored.
   On a hub-opened flow, the node's data is simply the reply.
5. **Netmap metadata:** Hostinfo (hostname, OS, services) reaches the hub through headscale.
   Identity uses only the stable node ID and the tags assigned by control, joined to our own
   inventory, never names the node asserts.
6. **Reply bytes on hub RPCs:** HTTP/1.1 (hyper), Connect envelopes (5-byte header, u32 length),
   JSON, file chunks with offsets, archives, compressed streams, metrics, logs. All need size,
   depth and decompression caps, and writes confined to a capability root (the existing `cap-std`
   pattern). Build with `overflow-checks = true` in release and add fuzz targets. Verus targets
   (owner rule 1): the envelope reader (`protectorate-wire/src/streaming.rs`), transfer offset
   accounting, and archive extraction bounds.
7. **Terminal escape sequences** from node shells land in the Flutter xterm widget, not the hub.
8. **Prompt injection:** logs that reach a sleeve's context are an instruction channel. Label them
   as untrusted data in anything handed to a sleeve; approvals on destructive RPCs stay the
   backstop.
9. **Lies:** a node can return false results or artifacts. Nothing built on a node becomes a
   release artifact without verification. That is an owner decision for the CI design.

### 4.5 Where the hub API listens

**LAN only:** loopback plus the trusted-proxy listener behind Traefik, never the tailnet. The phone
cannot join the tailnet (one VPN slot), clients use tokens anyway, the web app needs a
browser-trusted cert (which Traefik has), and with no hub listener on the tailnet a policy mistake
finds nothing to talk to. If WAN access ever comes, revisit with `tag:client -> tag:hub:7470` (old
`config.go:262-265`).

**Network-level separation is an extra (owner decision).** Put nodes on an agent VLAN, as
rust-direction.md:187 proposed, and exclude it from Traefik's API router with an `ipAllowList`
while admitting it on the control router. Without the VLAN, "nodes send nothing to the hub API"
holds by credentials, not by routing.

---

## 5. Enrollment: scan to join

**Install:** a static line in the README. The version is in the URL; signatures are as in the
deliverables doc.

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/<tag>/install-node.sh | sh -s -- https://protectorate.hotschmoe.com
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/hotschmoe/protectorate/<tag>/install-node.ps1))) https://protectorate.hotschmoe.com
```

1. **Script:** verifies `SHA256SUMS`, installs the node binary and a user service, and runs
   `protectorate-node join <url>`.
2. **Join:** the agent starts `member` with no key. tsnet generates keys locally, fetches `/key`
   over TLS and opens ts2021. Headscale returns
   `https://protectorate.hotschmoe.com/register/<auth_id>`: 38 characters, 24 of them random
   (`types/common.go:169-181`), exposed as `Status().AuthURL` (`tsnet.go:1165-1191`) [V].
3. **Node screen:** prints hostname, OS/arch, a fingerprint of the control key it fetched, and a
   terminal QR of `protectorate://enroll?a=<auth_id>&k=<ctrl-fp>&h=<hostname>`, plus the plain URL
   for pasting.
4. **Owner:** ADD NODE in the app. Scan the QR (phone) or paste the URL (desktop), name the node,
   pick `shared` or `care`, and approve. The app calls `NodeService.ApproveEnrollment`, which is
   device-token and owner-class only, never a capability or agent token.
5. **Hub:** refuse unless `ctrl-fp` matches its headscale noise key, which catches a man in the
   middle at Traefik or on the LAN. Run `headscale auth register --auth-id <id> --user enroll -o
   json`, then `nodes tag -i <id> -t tag:node,tag:tier-<tier>`, then `nodes rename`. Record node
   ID, node key, name, tier and approver in the inventory and the audit log.
6. **Node:** its pending control poll completes. The netmap now has the hub, and `member` listens
   on `:7475`.
7. **First contact:** the hub dials `hello` (version, inventory, hardware), and the app shows `ok`.
   Until then the node shows as `joining` with the reason. A stale QR expires with headscale's
   registration cache, and the node then shows a fresh one.

**Removal:** a hub RPC tells the node to uninstall, then the hub runs `nodes expire` and `nodes
delete`.

| Secret | Lives | Leaves |
|---|---|---|
| Node machine and WireGuard keys | node `member` Dir, 0600 | never |
| Hub member keys; headscale noise key, DERP key, DB | hub `tailnet/` (sandboxed) | only in hub backups |
| Headscale unix socket (authority over every node) | hub `tailnet/`, 0700 | never; not mounted into sleeves |
| `tag:hub` pre-auth key | minted and spent within seconds at first start | never |
| `auth_id` | node screen, then the owner's app, then the hub | single use; useless without an approver's token |
| Device and agent tokens | clients; the hub keeps SHA-256 hashes | as today |

**Why not the alternatives.** A code shown on the phone and typed on the node (the sibling doc's
section 5 sketch) needs the node to redeem it at the hub: an unauthenticated node-to-hub endpoint.
A pre-auth key in the one-liner is a bearer secret in clipboards and shell history, and painful to
type from a phone. SSH bootstrap means the hub holds SSH credentials for every machine and needs
sshd everywhere; it could be an optional extra later. Scan to join keeps the device-code shape the
owner liked, where the new device shows something and the phone approves, as in the flow removed in
`8de13f7` (`deviceCode`/`userCode`).

---

## 6. TLS, Traefik and DERP

Two routers on stronghold share the existing cert and the `lan-only` middleware:

```yaml
http:
  routers:
    protectorate-control:
      rule: "Host(`protectorate.hotschmoe.com`) && (Path(`/ts2021`) || Path(`/key`) || PathPrefix(`/derp`) || Path(`/machine/ping-response`) || Path(`/bootstrap-dns`) || Path(`/health`))"
      middlewares: [lan-only@file]
      service: headscale            # http://192.168.10.44:7480
      tls: {certResolver: myresolver}
    protectorate:                   # existing: everything else -> http://192.168.10.44:7470
      rule: "Host(`protectorate.hotschmoe.com`)"
```

- **Not routed:** `/api`, `/register`, `/auth`, `/swagger`, `/verify`, `/apple` and `/windows`
  (`app.go:544-610`). The app takes the `auth_id` from the QR. The longer rule wins [E].
- **Upgrades:** ts2021 is `POST` + `Upgrade: tailscale-control-protocol`; DERP is `GET` +
  `Upgrade: DERP` (headscale `reverse-proxy.md`). Behind a TLS-terminating proxy, DERP uses the
  plain 101 upgrade, since fast start needs a TLS 1.3 meta cert (`derphttp_client.go:497-560`) [V].
  Traefik is not among headscale's documented proxies, so this is the first e2e test. DERP
  fallback: upstream `TS_DEBUG_DERP_WS_CLIENT=1` (`derphttp_client.go:334`). Control has no
  upstream WebSocket switch; mainplane's `TS_CONTROL_WEBSOCKET` exists only in its fork [V].

**DERP map** (`<home>/tailnet/derp.yaml`, generated):

```yaml
regions:
  900:
    regionid: 900
    regioncode: protectorate
    regionname: Protectorate
    nodes:
      - {name: 900a, regionid: 900, hostname: protectorate.hotschmoe.com, ipv4: 192.168.10.50, ipv6: none, derpport: 443, stunport: -1}
```

- `ipv4` forces the dial address with no DNS, and `stunport: -1` disables STUN
  (`tailcfg/derpmap.go:216-233`) [V].
- Headscale refuses an empty DERP map (`app.go:670`), and with its DERP on it needs a STUN address
  (`app.go:653`) [V]. So the DERP is headscale's own, and STUN binds loopback, unused.
- **No public DERP** (`urls: []`): it would relay LAN traffic over the WAN.
- **Direct paths:** on the LAN and across the site-to-site WireGuard, peers should find each other
  through local endpoints without STUN [I]. E2e must assert a direct, non-DERP path.

**DNS (owner decision).** DERP is pinned by IP, but nodes still resolve the control URL. The name
resolves to the public IP and hairpins (memory `production-cutover-2026-10-26`), so a **home-site**
node would leave through the home WAN and be refused by `lan-only` [I]. Fix: a split-horizon record
`protectorate.hotschmoe.com -> 192.168.10.50` on both UDMs and in the phone's WireGuard DNS. It
also restores real client IPs at Traefik.

**Trust boundary.** Stronghold sees the control upgrade in clear text but not the Noise inside it.
It could still serve a forged `/key` and impersonate control to *nodes*, which would let it drive
care machines. It cannot do this to the hub, whose member uses headscale directly. Step 5's
fingerprint check closes this at enrollment. `member` also re-checks `/key` against the pin on
every start; that raises the bar but is not airtight, because tsnet fetches the key itself [I].

---

## 7. mainplane, compared [V]

| Aspect | mainplane | Us | Better |
|---|---|---|---|
| Control | hscontrol "copied ... and trimmed ... no database, CLI, OIDC, policy language" (`coordinator/coordinator.go:1-11`) | official headscale, file policy | us |
| Client | fork: `replace tailscale.com => github.com/mainplane-ai/tailscale` (`go.mod:66`) | upstream tsnet | us |
| Data plane | kernel TUN plus an OS router per worker (`mesh/mesh.go:114`, `router_*.go`) | userspace, no TUN | us |
| Direction | worker dials harness (`worker/dial.go` `Dial`) | hub dials node | us (required) |
| Filter | `tailcfg.FilterAllowAll` (`coordinator.go:549`) | one accept rule | us |
| Reach | cloudflared quick tunnel to the WAN (`tunnel/tunnel.go`) | LAN only | us |
| Relay | upstream derpserver at `/derp`, WebSocket too (`relay/relay.go`) | headscale embedded DERP | equal |

**Borrow:** (1) the harness's own node uses control on loopback, not the front door
(`mesh.go:91-92`), adopted for the hub member; (2) a fixed WireGuard UDP port beside the user's
Tailscale (`mesh.go:47-49`, 41642), useful only if the owner wants firewall rules; tsnet's `Port`
field supports it. **Do not borrow** its control-key pin (`controlclient.VerifyServerKey`,
`mesh.go:97`): it exists only in its fork [V].

---

## 8. Language and build

| | Rust + Go | Zig + Go | Rust + Zig |
|---|---|---|---|
| Reuse | 29 crates, about 56k lines of src (74k with tests) [V]: sandbox, drivers, wire, auth, quota, chatlog | none: full rewrite | the Rust, but Go is still needed for headscale, so three compiled languages plus Dart |
| Verus (narrow scope) | yes | no comparable verifier [E] | Rust half only |
| HTTP, TLS, Connect, proto | axum, rustls/ring, prost already in use | thin std; we would write Connect and proto ourselves [E] | as Rust |
| Toolchain | stable | Zig 0.16 is pre-1.0 and breaks between releases | Zig adds no source value |
| Cross-compiling | cargo-zigbuild for glibc 2.31 (in use) plus `GOOS/GOARCH` with no cgo [V] | native | - |

**Decision: Rust + Go.** The hub embeds Go either way. With cgo out, Zig's cross-compile advantage
buys nothing that cargo-zigbuild does not already give us, and Zig would cost the reuse, Verus and
a stable toolchain.

Rules: Go only in `go/`; no cgo; no Go in the hub API's request path; no Go dependency beyond what
headscale's own go.mod selects; Flutter is the only UI. Verus scope for networking: 4.4 item 6,
using the sibling doc's section 6 method.

---

## 9. Deliverables

Networking needs separate processes, not separate files. The **hub** (linux/amd64 and arm64) embeds
headscale (about 18 MB gzipped) and the member (about 7 MB gzipped). The **node** (five targets)
embeds the member for its own target. Both extract and verify their blobs at start. So the release
stays as the sibling doc's two binaries plus the Flutter apps. A universal binary was rejected:
Linux nodes would carry headscale (+18 MB), and Windows needs its own build anyway.

---

## 10. Rejected alternatives not covered above

| Alternative | Reason |
|---|---|
| Hub API on the tailnet, or human clients on the tailnet (Go design's `tag:client`) | phones cannot join; more surface |
| Today's router-only connectivity with mTLS | an inbound firewall rule per node, a CA and fleet upkeep |
| Nodes pushing metrics or results to the hub | breaks one-way trust; the hub pulls and nodes stream on hub-opened connections |
| Headscale REST API with an API key | a long-lived secret plus an HTTP client; the CLI over the unix socket needs neither |
| Public DERP; no DERP | WAN relay; headscale refuses an empty map (`app.go:670`), and DERP is the fallback when UDP is blocked |
| Headscale terminating TLS itself | the cert would have to be copied onto the hub |

Also rejected, with reasons in place: in-process or forked headscale (2.2), tailscaled,
tailscale-rs and our own stack on nodes (3.1), typed codes and pre-auth keys in the one-liner (5).

---

## 11. Risks

1. **Traefik and the ts2021 and DERP upgrades** are untested. Test them first.
2. **Direct LAN paths with STUN off** [I]. If they fail, everything relays through DERP and
   Traefik: it works, but slower.
3. **gVisor netstack throughput** for large artifacts [I]. Measure a multi-GB transfer early.
4. **Headscale migrations against update rollback.** Snapshot before each trial (2.3).
5. **WAN leaks before the first netmap** (logtail, dnsfallback, tsnet's built-in DERP map).
   Mitigated in 3.2. E2e packet capture must fail on any non-private destination.
6. **Version skew:** a node more than 10 minors behind is rejected (`capver_generated.go:84-89`).
   The hub pushes updates; with monthly minors that is about 10 months of slack [E].
7. **Windows:** AV scanning of the extracted member, a possible firewall prompt for its UDP socket
   [I] (DERP on 443 covers a deny), and Job Object hygiene. Trial on officepc and snapdragon.
8. **Termux** builds [V], but netmon, DNS and SELinux exec behaviour at runtime are unverified.
   Trial before promising Android nodes.
9. **Headscale on 7480 is reachable by every LAN host.** Its CVEs expose the hub host, and an RCE
   controls every node (4.4 item 1). Mitigations: the sandbox, an optional "7480 from stronghold
   only" host rule, and prompt tag bumps.
10. **Stronghold is a single point.** If it is down there is no DERP and no new control sessions.
    Existing direct sessions should continue on the cached netmap [I].
11. **Disco recovery** (`a26e576`) needs a node-initiated disco ping. It may be fixed upstream
    [I]; keep the ping until e2e shows otherwise.
12. **Empty `tagOwners` and the `ts_omit_*` set** are unverified at runtime [I].

---

## 12. Owner decisions

1. Accept **inside the hub binary, outside the hub process**: headscale as an embedded, sandboxed
   child (2.2 A).
2. **Build headscale from the pinned upstream module in CI** (recommended), or embed the release
   binary's bytes (B).
3. Add a **split-horizon DNS record** `protectorate.hotschmoe.com -> 192.168.10.50` at both sites
   and in the phone's WireGuard DNS. Home-site nodes need it (section 6).
4. Put **nodes on an agent VLAN**, excluded from Traefik's API router, now or later (4.5).
5. Adopt **scan to join**. A camera QR scanner in the Flutter app adds one plugin; paste works
   without it. This replaces the sibling doc's phone-shows-code sketch.
6. Give vendor general agents **agent tokens with a scope ceiling**, or full device tokens (4.1).
7. Accept **stronghold in the trusted base for node control integrity**, with the enrollment
   fingerprint check (section 6).
8. **Tiers as tags only:** shared versus care is enforced by the scheduler.
9. A **fixed member UDP port**, or tsnet's random one (section 7).
10. **Refuse `100.64.0.0/10`** in the hub listener check (4.2).
11. **Installer source:** GitHub, as in section 5, or hub-served, as in the sibling doc.
    Enrollment works either way. A hub-served script means a node-to-hub GET before enrollment,
    on the same surface as the web app.
