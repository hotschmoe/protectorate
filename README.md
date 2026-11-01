# Protectorate

A self-hosted software factory on one hub machine. You talk to Envoy, our orchestrator agent, and
Envoy talks to sleeves. A sleeve is a vendor coding CLI (Claude Code, Codex, Grok Build) or Pi,
running untouched in a light sandbox and bound to one directory. The hub also reaches a fleet of
untrusted machines (nodes) to inventory them and run build, test and maintenance jobs across
architectures. It is a software factory only: no email, calendar or errand agent.

This branch, `next`, is a ground-up rewrite. [SPEC.md](SPEC.md) is the design;
[REVISIT.md](REVISIT.md) lists every shortcut taken so far and what "right" would be;
[DEPENDENCIES.md](DEPENDENCIES.md) lists every third-party package and why it is there.

## Architecture

```
  app (web, Linux, Windows, Android)          owner CLI, Envoy, sleeves, vendor agents
          |  HTTPS, pd1. device token                  |  unix socket on the hub
          v                                            v
   TLS reverse proxy  ------------------------>  protectorate serve (the hub)
          |  control and DERP paths              journal, core thread, sleeves, approvals,
          v                                      history, quota, doctor, updates
   headscale (child of the hub, sandboxed)            |
          ^                                      member (child of the hub, tsnet, dials only)
          |  control, DERP                            |
          |                                           |  hub-initiated RPCs, tailnet port 7475
   protectorate-node + member on each node  <---------+
```

- **One writer.** One core thread owns all state. Each event goes through a pure
  `apply(state, event)`, is journaled with group commit, then executed. Every write to a harness
  or a node is recorded before it is sent.
- **Headless first.** Sleeves run through each harness's official headless protocol, and the
  transcript is the chat the app shows.
- **One-way trust.** The hub may reach and execute on nodes; a node cannot open a flow to the hub
  API. Nodes reach only headscale's control and DERP endpoints, and answer connections the hub
  opened. Official headscale and our small tsnet glue (`member`) are embedded in the binaries and
  run as child processes.
- **Two binaries.** `protectorate` (hub, CLI, stdio MCP) and `protectorate-node`. No async
  runtime, no TLS stack (outbound HTTPS goes through the system `curl`), one shipped third-party
  Rust crate (`libc`).

## Status

Development started 2026-11-01. Against the build order in SPEC section 14:

| Stage | State |
|---|---|
| 1. Core | done: journal, `pt-core` state machines, `sleeve_core` fuzz target with a reference model |
| 2. Chat and one harness | done: headless drivers for Claude Code, Codex, Grok and Pi; Envoy as a root sleeve; `envoy send/wait/read`; the Flutter shell. Terminal takeover is a stub |
| 3. Resleeve | done: guardrails, history index, continuation brief, `pi_local`, forced resleeve end to end. The brief has no LLM summary yet |
| 4. Network | mostly done: embedded headscale and member, join with a terminal QR code and approval from the CLI, inventory and presence. Headscale behind a real TLS proxy is untested |
| 5. Jobs | mostly done: node jobs with artifacts, hub `doctor`, harness updates, hub and node self-update with probation. No cross-arch CI rule, no node `doctor` |
| 6. Hardening | started: the stdio `mcp --envoy`. Not started: Verus proofs, terminal takeover, Antigravity, the Pi Durable experiment |

**Covered by e2e tests** (`make test`, real binaries, real sockets, real sandboxes): sleeve
lifecycle, crash mid-turn and recovery, the Envoy round trip, root stop stopping its children,
scoped sleeve capabilities, forced resleeve with a brief, the API surface and its scopes, the D9
approval rule, device pairing, the MCP bridge, history search and rebuild, a local fleet with node
jobs, node job timeouts, cancels, restarts, presence refusal and artifact fencing, and hub and
node updates with rollback against a stand-in release server. The web app is driven against a
real hub in headless Chromium by `scripts/app-e2e.sh` (pairing, Envoy chat, sleeves, a D9
approval). The fleet and harness tests skip when Go, Pi or a login is missing.

**Run on real machines** (from the development log): the hub on a Linux arm64 machine as a user
service behind a TLS reverse proxy (the web app loads, the API refuses requests without a token);
Envoy on Claude Code answering and resuming natively after a redeploy; two Linux nodes (amd64 and
arm64) as system services, joined LAN-direct, running jobs and returning 2 MB artifacts whose
sha256 matched; the local quota reader; history search. On Windows, the node service, presence
through the idle probe and a job through the service were checked by hand, not yet through a hub.

**Not built or never run:**
- no GitHub release from `next` exists, so the installers have run only as dry runs or against
  local release assets (`--from`), and `hub update` and `node update` have run only against a
  stand-in release server;
- terminal takeover, Antigravity, Pi Durable, Verus proofs, `protectorate git push`, hub export
  and import, sleeve-to-sleeve mail through the API, GPU leases, the backlog and spare-quota
  trigger (the function exists, nothing calls it), node `doctor`;
- a hub update restarts the hub instead of re-executing it, so sleeves resume as after a crash;
- Windows nodes cannot take the `Update` RPC; macOS and Android nodes do not compile;
- the app has no QR scanner and cannot approve nodes; desktop and Android builds forget their
  pairing on restart and are not release assets;
- pending approvals and hub events live in memory and are lost on restart;
- releases are unsigned (sha256 against `SHA256SUMS` only).

## Build and test

Linux is the hub platform. You need a Rust toolchain; Go (for headscale and member), Flutter (for
the app and the embedded web build) and Dart (for `make check-gen`) are optional for a plain build.

```
make build          # debug build of the workspace
make release        # release binaries for this machine
make web release    # the same, with the Flutter web build embedded
make test           # e2e tests (cargo test --workspace)
make test-paid      # also the Claude, Codex and Grok e2e tests, which spend a little quota
make check          # fmt --check, clippy -D warnings, test, check-deps, check-gen, app-check
make release-assets # every release asset into dist/<version>/ with SHA256SUMS
```

The hub e2e tests need `bwrap`, `systemd-run --user`, `git` and `pi` pointed at an
OpenAI-compatible endpoint (`PT_E2E_PI_PROVIDER`, `PT_E2E_PI_MODEL`). A binary built without Go
has no fleet; `protectorate version` says so. Fuzz targets live in `crates/<crate>/fuzz/` and run
with `cargo +nightly fuzz`.

To install a hub or a node, see [docs/install.md](docs/install.md). Day-to-day commands are in
[docs/operate.md](docs/operate.md).

## Crates

| Crate | Purpose |
|---|---|
| `protectorate` | the hub binary: `serve`, the CLI (one API method per command) and `mcp --envoy` |
| `protectorate-node` | the node binary: `join`, `serve`, `install-service`, inventory, presence, jobs |
| `pt-api` | the hub API defined once: types, method table, scopes, event-stream messages |
| `pt-apigen` | dev tool: writes the Dart client (`app/lib/api/api.g.dart`) from `pt-api` |
| `pt-core` | the pure state machines and `apply`, with no I/O |
| `pt-journal` | the append-only journal: CRC frames, group-commit fsync, torn-tail recovery |
| `pt-hub` | the hub runtime: listeners, core thread, executors, fleet, approvals, updates |
| `pt-drivers` | headless harness drivers (Claude stream-json, Codex app-server, Grok ACP, Pi rpc) |
| `pt-sandbox` | the `systemd-run --user --scope` plus `bwrap` prefix for one incarnation |
| `pt-history` | our keyword index over native session logs and hub transcripts, with BM25 |
| `pt-quota` | read-only quota readers per subscription, `select_target` and the spare trigger |
| `pt-doctor` | checks of this machine as fixed JSON, cached in `doctor.json` |
| `pt-update` | updates from GitHub releases: staging against `SHA256SUMS`, probation, rollback |
| `pt-fleet` | the hub's side of the tailnet: headscale and member as children, enrollment, node RPC |
| `pt-frame` | the hub-node frame codec, blocking frame I/O and the RPC messages |
| `pt-node` | the node side: inventory, samples, presence, job priority and the job runner |
| `pt-embed` | binaries a binary carries (headscale, member), gzipped, unpacked by SHA-256 |
| `pt-http` | HTTP/1.1 server and small client on std sockets, with WebSockets |
| `pt-json` | JSON value, strict parser, deterministic writer and the API type macros |
| `pt-hash` | SHA-1, SHA-256 and hex |
| `pt-qr` | QR codes for a terminal, a trimmed port of qrcodegen |
| `pt-sys` | the one place for OS FFI |
| `pt-version` | the release stamp every binary prints |

Outside `crates/`: `go/` holds the only Go (headscale's upstream main and `member`), `app/` the
Flutter client, and `scripts/` the installers, release and network test scripts.
