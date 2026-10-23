# Rust direction: inventory, discussion and decisions (2026-10-24)

- Date: 2026-10-24
- Branch: `rust-yolo` (based on `master` at `e577c5e`, tag `v0.4.3`)
- Purpose: one record of where the Rust rewrite stood, what was researched, what
  the owner decided, and what is being implemented, so any of it can be revisited
  later. Research reports are reproduced in full in Appendix A; external
  references are in Appendix B.

## 1. Starting state (inventory, before this session's commits)

| Item | Finding |
|---|---|
| Commit state | Entire Rust rewrite uncommitted on `rust-yolo`; `rust-yolo`, `master`, `dev-hotschmoe` all at `e577c5e` (v0.4.3). Written 2026-10-22..24. |
| Size | About 152k lines of Rust in 55 crates (`rust/crates/*`) versus about 54k lines of Go in `internal/` + `cmd/`. 94 changed/untracked paths; 65 tracked files modified (+2399/-1088). |
| Binaries | `protectorate-rs` (protectorate-server: hub + node + server-side commands), `protectorate-cli`, `protectorate-mcpbridge`, `protectorate-rs-windows`, `protectorate-rs-termux`, `protectorate-sleeve-runner`. |
| Health | `cargo check --workspace --all-targets`: 0 errors, 0 warnings. `cargo fmt --check` clean. `cargo test --workspace`: 1003 passed, 1 failed (`protectorate-hub/tests/lease.rs:101`, flaky under load, passed 3/3 alone), 20 ignored. Toolchain rustc/cargo 1.96.0, no `rust-toolchain` file. About 246 external packages in Cargo.lock (191 on the linux normal graph). |
| RPC parity | All 103 proto RPCs registered in Rust; none are wholesale stubs (some conditional `Unimplemented`, e.g. Respond on Pi sleeves, nodes without jobs/desk). |
| CLI parity | Near complete. Missing in Rust: hidden `hub update-guard`, `node migrate-tiers`. Split across three binaries; no single `protectorate` multiplexer. Rust-only: `node direct-put/direct-get`. |
| Transport | Hand-built Connect JSON over HTTP/1.1 (axum http1, tokio-rustls ALPN http/1.1, prost-reflect with embedded descriptor set). |
| New, not ported | Pure-Rust namespace sandbox replacing bwrap; about 14 crates reimplementing the tsnet side (TS2021 Noise registration, netmap, WireGuard Noise IK, userspace TCP, own DERP + STUN) against a pinned Headscale 0.29.5; native Windows and Termux nodes. |
| Parity testing | Go e2e suite gained a Rust mode (`PROT_E2E_COMPANION_BINARY`); doc reported 30 scenarios passing against Rust on 2026-10-23; not wired into make or CI. |
| Docs | `docs/rust-migration.md` (845 lines) plus `rust-sandbox.md`, `rust-termux.md`. Doc says Go remains production until parity. |

Problems found in the uncommitted state:

1. Contradictory release story: CI and release workflows already built and shipped only Rust; `install-hub.sh` installed Rust; Go `make test`, `check-gen`, `cross` and the APK jobs were removed from CI, while docs still called Go production.
2. Rule violations: Flutter widget unit test `app/test/hub_shell_semantics_test.dart`; "Legacy fields retained" bwrap fields in `hub.proto`; installer `--adopt-go-unit` / `--rollback-go-unit` cutover shim.
3. Strays: `dist/` (17.6 MB Android binary), `scripts/__pycache__/`, `app/test_probe_clipboard_tmp.dart`.
4. Go-visible e2e edits (ProtoJSON clients, `eventually` in leases, probe env bypass) compiled but were never re-run against Go.
5. State on disk partly separate: Rust uses `identity.json` (Go `tokens.json`), `sleeves-rust.json`, `jobs-rust/`, `desk-*-rust.json`; other files share Go names; no importer.
6. Stale guidance: `CLAUDE.md` described the Go workflow; `docs/mcp.md` documented a Rust-only `--https` flag; migration doc internally inconsistent (Windows task update listed both pending and passed).
7. Large files (`cli/src/args.rs` 4.7k, `server/src/main.rs` 4.3k, `cli/src/lib.rs` 3.5k lines), about 650 non-test `unwrap()`, no pinned toolchain, `protectorate-sleeve-layout` reached by no binary, 772 `#[test]` unit tests and no fuzz targets.

## 2. What was done this session

### Commits on rust-yolo (`git log --oneline master..rust-yolo`, oldest last)

```
db9bb64 build(rust): Keep only line tables in dev builds
bc9b87a build: Remove the Go implementation; the branch is Rust and Flutter only
5a1fa24 build(rust): Use ring as the only TLS crypto provider
bb025c7 docs: Track the Rust migration and describe the Rust install and release lanes
1b95ec2 ci: Build, test and release the Rust binaries
87b11c5 build(install): Install verified Rust bundles on Linux, Windows and Termux
313b9ac feat(app): Keep the approval banner below the content and drive screens against a Rust hub
6a6cddf test(e2e): Run the real-binary suite against the Rust binary
2a08e30 feat(rust): Add the hub, the protectorate-rs server, the CLI and the MCP bridge
511f9f5 feat(rust): Port the node agent with Windows and Termux builds
714db4e feat(rust): Add a native tailnet member stack and private DERP relay
bc72be7 feat(rust): Drive Claude, Codex, Grok and Pi through their headless protocols
a1b6d05 feat(rust): Add the Linux namespace sandbox and sleeve runner
f38b1e4 feat(rust): Port the hub state stores, quota probes, releases and transfers
157576f build(rust): Add the Cargo workspace, Connect wire layer and identity core
a6ccb53 feat(proto): Report release confirmation, sleeve runtime and node update trials
```

The first 13 (a6ccb53..bb025c7) commit the previously uncommitted work in dependency order. The rest are cleanup.

### Deletions and audit trail (CLAUDE.md RULE 1)

| When | Authorizing text (owner) | Command | Affected |
|---|---|---|---|
| 2026-10-24T01:23+08:00 | "commit it in logical chunks on rust-yolo, delete the strays, wcleanup where required" | `rm -r dist scripts/__pycache__ app/test_probe_clipboard_tmp.dart` | `dist/protectorate-rs-android-arm64` (17.6 MB build output), 5 `.pyc` caches, 29-line clipboard probe. All untracked. `/dist/` and `__pycache__/` added to `.gitignore`. |
| 2026-10-24T01:46+08:00 | "i agree with all your recommendations and i give you permission to make those deletions (on the rust branch we want only rust (and flutter for ui)" | `git tag shelf/windows-desk 5a1fa24` (tag only, no deletion) | Pickup point for shelved Windows desktop control. |
| 2026-10-24T01:46+08:00 | same | `git rm -r -q cmd internal gen go.mod go.sum tests/e2e spikes scripts/e2e-cover.sh rust/crates/protectorate-tailnet-wg/testdata/interop.go rust/crates/protectorate-tailnet-wg/tests/interop.rs` | 182 Go files in `internal`, 38 in `cmd`, generated Go, Go e2e suite (53 files, about 9.8k lines), 18 Go spike files, Go coverage script, the Go WireGuard interop fixture and its Rust test. Committed as `bc9b87a` (447 files, -98861 lines). |
| 2026-10-24T01:46+08:00 | same | `rm -r app/test` | Widget unit test `app/test/hub_shell_semantics_test.dart` (never committed). |
| 2026-10-24 (after the git rm) | same (spikes was on the approved list) | `rm -r spikes` | Four ignored build outputs left in `spikes/ssh-agentless/.out/`. |

Not deleted (not on any approved list): old Go binaries in ignored `bin/`, `archive/` (44 Go files, called read-only reference in CLAUDE.md), the 300 GB `target/`.

### Other cleanup

- `5a1fa24`: ring is the only rustls provider. The CLI and connect-client took rustls defaults and every tokio-rustls user took its defaults, pulling in aws-lc-rs and its C build. Five packages left the lock (301 -> 296). Build, clippy and TLS tests pass.
- `bc9b87a` also: `buf.gen.yaml` generates Dart only; protos drop `go_package` (reformatted with `buf format`); `make gen` refreshes the Rust descriptor set and the MCP catalog golden (moved from `tests/e2e/testdata/mcp_tools.json` to `rust/crates/protectorate-mcpbridge/tests/testdata/mcp_tools.json`); Makefile now has `gen check-gen build test check app-*` driving cargo and Flutter; CI runs `make check`; `scripts/check-version.sh` checks `PROTECTORATE_BUILD_VERSION` instead of Go ldflags; `CLAUDE.md` layout/commands describe the Rust workspace and the test rule reads "tests are real-binary e2e or fuzz targets only, never unit tests"; `.gitignore` drops Go-only entries.
- `db9bb64`: `[profile.dev] debug = "line-tables-only"`, dependencies without debug info, because full debug info grew `target/debug` to about 295 GB with only 94 GB disk free.
- Memory updated: stack preference is now Rust + Flutter; rust-yolo is Rust-only; desk control shelved.
- Noted flaky tests: `protectorate-hub/tests/lease.rs:101` (`lease_change_reaches_event_subscriber`), `protectorate-cli/tests/node_serve_alias.rs:54`.

## 3. Decisions

| # | Decision | Rationale | Status |
|---|---|---|---|
| 1 | `rust-yolo` is Rust (backend) + Flutter (UI) only; no Go | Owner: "on the rust branch we want only rust (and flutter for ui)". `master` keeps the Go release history. | Done (`bc9b87a`) |
| 2 | Tests are real-binary e2e and fuzz only; remove all unit tests (772 `#[test]`, library-level tests that are not end to end) and add fuzz targets with seed replay in `make test` | Owner rule (memory `tests-e2e-fuzz-only`); Rust tree had no fuzz targets | In progress (wip/tests) |
| 3 | Minimal dependencies and fewer LOC everywhere; every agent trims as it goes | Owner's first stack preference | In progress (aws-lc removed; further plan in section 3.1) |
| 4 | Shelve Windows desktop control: remove DeskService, `protectorate-desk-node`, `protectorate-desk-policy`, CLI `desk`, hub `desk_service.rs`, node wiring and Flutter screens | Owner: "we dont need windows desktop control right now so you can shelve that for pickup later". Drops 9 packages and about 1.8k lines. Pick up from tag `shelf/windows-desk`. | In progress (wip/desk) |
| 5 | Remove the tailnet overlay: Headscale supervision, the hand-written WireGuard, userspace TCP, netmap, DERP and STUN (about 15k lines, 14 crates), TailnetService | Largest correctness and security liability (no WireGuard cookie/rate-limit defense, no NAT traversal, no congestion control, unaudited); owner's machines are already on LANs and a WireGuard VPN; owner proposed letting the routers own connectivity | In progress (wip/net) |
| 6 | Hub and node identity by mutual TLS with a hub CA issued at pairing; bind only to private addresses; node token removed | Network location is not identity; existing opt-in TLS listener and private-CA trust already present | In progress (wip/net) |
| 7 | Keep hub-dials-node for now; node-initiated connections (nodes never listen) recorded as a future attack-surface cut | Smallest change to the existing node-client architecture | Decided (deferred option) |
| 8 | Auth: no OIDC. Device-bound keys (Android Keystore, non-extractable WebCrypto) registered in the existing device flow, short-lived (about 15 min) access tokens with DPoP-style request proofs, drop `?token=` WebSocket fallback. Sleeve and Envoy capability tokens (hand-rolled HMAC macaroons preferred over biscuit-auth for deps) with about 1 h expiry, attenuation only. Owner passkey (WebAuthn) optional and last. Break-glass `token` file stays local to the hub host. | OIDC solves many users and apps; one owner, no WAN. Real hole is permanent, copyable bearer tokens. | Decided; wave 2 |
| 9 | Headscale ACL lockdown | Moot once the overlay is removed; the proposed policy is kept below for the record. Equivalent lockdown moves to the router firewall (section 4). | Superseded by 5 |
| 10 | Adopt from agentbaseline: per-sleeve egress allowlist (own network namespace, default deny, metadata and private ranges blocked except the hub), hash-chained audit with one `run_id` across sleeve/job/mail/lease, delegation subset check (child scopes subset of minter), phone approval gates for irreversible actions (pushes to protected branches, merges, admin grants), hash pinning of harness binaries, MCP allowlists and profiles with alerts on change, enforced per-task quota budgets, effective-access view with toxic-combination lint | Closes the largest gaps against the reference architecture | Decided; wave 2 |
| 11 | Skip from agentbaseline: microVM tier, SBOM/provenance machinery, compliance retention and legal hold, risk classes and exception owners, LLM monitoring agents, step-up beyond owner-only RPCs, non-agent fallback programs | Enterprise overkill for a single-owner private deployment | Decided |
| 12 | layerfs: no dependency. Port the idea: git-tree checkpoints per turn or resleeve under hidden refs (rollback, "what did the agent change"), overlay uppers as change reports, real per-sleeve disk quotas | 242k lines, FUSE, no crash durability, single squashed commit; kernel overlayfs already used | Decided; pi-durable compared (section 5): build our own git-tree checkpoints |
| 13 | herdr: inspiration only (see list below) | Not a library; would add a second server, Zig toolchain, about 330k lines | Decided |
| 14 | Cloudflare post lessons: per-session first-turn cache-read ratio telemetry (Claude driver already parses cache fields), never put volatile data (timestamps, node names, lease ids) early in what we send a harness, cache-weighted quota math (writes 1.25x, reads 0.1x), track calls per job, confirm cancellation kills the process tree, classify timeouts separately from provider failures | Only the hosted-harness lessons apply; local inference out of scope | Decided; wave 2 |
| 15 | Verus vs Bend: Bend rejected (irrelevant to an I/O-bound orchestrator). Verus deferred for "a long look" later; its best target (WireGuard replay window) disappears with the overlay. First: fuzz targets, Kani on arithmetic kernels, loom on the lease table | Assurance per hour | Bend: decided. Verus: deferred |
| 16 | Live Go hub cutover: fresh start. Every node and device re-pairs anyway under the new identity model, so no Go state importer. Remove `--adopt-go-unit` / `--rollback-go-unit` and the `-rust` state file suffixes. `notes.json`, `leases.json`, `directory.json`, `mailbox.jsonl` are file-compatible and can be copied by hand if wanted. | No compat shims rule; identity change forces re-pairing | Decided |
| 17 | Router-owned connectivity: site-to-site WireGuard between home and office UDMs, phone as road-warrior to office (section 4) | Smallest WAN surface, no VPN clients on family/care machines | Decided in principle; UniFi specifics open |

### herdr: ideas to steal and not to copy

Steal:
- Agent state detection as TOML data (priority, region such as bottom N lines, regex, `not` guards) with an `explain` command; hot-reloadable; evidence-based bottom-buffer matching only.
- Server-side VT state with a typed frame API (`observe`/`control`, single input owner, `--takeover`) instead of relaying tmux bytes; path to Windows terminal parity and clean phone/web attach.
- Honest persistence table (detach keeps processes, restart keeps layout only); unparseable state backed up, never auto-restored; rate-limited snapshots; save on logind shutdown inhibitor; ignore SIGHUP.
- Sequence-numbered state reports so late or out-of-order reports are dropped.
- Reported resume argv, deduped by (source, agent, cwd, argv) with hard bounds.
- Frozen wire-contract discipline: named codecs immutable, new fields optional, unknown enums fall back; one incompatible node never drops others.
- PTY handoff by SCM_RIGHTS to update the runner without killing sleeves.
- Hot-path guardrails (panes x clients multiplicative cost) and local socket hygiene (0600, dev/inode identity, stale-socket detection).

Do not copy:
- Installing hooks into agent configs (violates hands-off).
- Multiplexer as trust boundary (same user + SSH, no sandbox or identity).
- Sprawl (330k lines, 4-8k line files, plugin marketplace, self-updater, themes).
- Zig toolchain for libghostty-vt.
- SSH as remote transport.
- TUI renderer; a second per-machine server authority.

### 3.1 Dependency trim plan

| Change | Packages removed | Effort |
|---|---|---|
| ring-only TLS (done, `5a1fa24`) | 5 | trivial |
| Remove overlay: drops snow, crypto_box, hpack, rcgen from derp (rcgen may return for CA issuance) | large (rcgen alone 24) | part of wip/net |
| Shelve desk: chrono-tz, phf, phf_shared, siphasher, png, fdeflate, miniz_oxide 0.8 | 9 | part of wip/desk |
| `toml` -> key scanning in `directory/manifests.rs` | 8 | small |
| `fs2` -> `std::fs::File::lock` | 1 | trivial |
| `glob` -> small matcher | 1 | small |
| CLI tungstenite client -> axum/tokio-tungstenite pieces already present | about 5 | moderate |
| Align base64 versions | 1 | trivial |
| zstd -> flate2 for transfers | 3 + C toolchain | moderate, changes wire format |
| Do not: hand-roll crypto, rustls/ring, cap-std, the HTTP server (axum) | - | - |

### Proposed Headscale policy (for the record; superseded by decision 5)

```json
{
  "tagOwners": {
    "tag:hub":         ["hub@"],
    "tag:client":      ["hub@"],
    "tag:node":        ["protectorate@"],
    "tag:tier-shared": ["protectorate@"],
    "tag:tier-care":   ["protectorate@"]
  },
  "acls": [
    {"action":"accept","src":["tag:hub"],   "dst":["tag:node:7475"]},
    {"action":"accept","src":["tag:client"],"dst":["tag:hub:7470"]}
  ],
  "ssh": [],
  "tests": [
    {"src":"tag:node","deny":["tag:hub:7470","tag:node:7475"]},
    {"src":"tag:client","deny":["tag:node:7475"]},
    {"src":"tag:hub","deny":["tag:node:22"]}
  ]
}
```

Changes versus the generated policy: delete the per-pool `tag:pool-X -> tag:pool-X:*` lateral rule, drop unused `:22`, give hub/client tags a separate owner so a compromised node cannot re-register as `tag:hub`, add policy `tests`.

### Open questions

| Question | Notes |
|---|---|
| Remove `archive/` too? | 44 Go files; CLAUDE.md calls it read-only reference. Not deleted without explicit approval. |
| `cargo clean` of the main checkout's `target/` (about 303 GB) | Build cache, but a deletion under RULE 1; needs approval. |
| pi-durable checkpoints versus git-tree checkpoints; route all models through the Pi harness with pi-durable enabled and observed? | Resolved: keep native harnesses, own git-tree checkpoints (section 5). |
| UniFi specifics | Does the installed UniFi Network version support manual WireGuard site-to-site; does home have a public IP or CGNAT; VLAN and zone layout. |
| Unit tests versus fuzz conversion order | Purge in wave 1; fuzz targets added alongside. |

## 4. Network model: let the routers own connectivity

Owner's question: let the UniFi Dream Machines (home and office) and the VPN handle inter-machine connectivity, never expose protectorate to the WAN, given worry about WAN attack surface and an existing phone WireGuard VPN to the office router. Site-to-site, or one VPN server at the office with home devices as clients?

Position:

- Yes. Let the UDMs own connectivity and delete protectorate's overlay.
- Prefer site-to-site WireGuard between the home and office UDMs. Both LANs route to each other; no per-device VPN clients; family/care machines carry no VPN configuration; nothing breaks when a client app dies.
- The phone stays a WireGuard road-warrior client of the office UDM and reaches home through the site-to-site link.
- If home lacks a public IP or is behind CGNAT, make home the initiator so only the office exposes a UDP port.
- One VPN server with every home device as a client is worse: keys on every machine, a client per OS (Termux, Windows), more failure modes.
- UniFi Site Magic works but depends on the UniFi cloud account for orchestration; a manual WireGuard site-to-site avoids that (verify the UniFi Network version supports it).
- WAN surface: one WireGuard UDP port per exposed router. WireGuard does not answer unauthenticated packets, so this is about the smallest practical WAN surface. Optional preshared key as a post-quantum hedge. Keep UDM firmware current, disable unneeded remote-management features, never port-forward to protectorate.
- Router firewall (zone-based):
  - agent nodes in their own VLAN;
  - client/VPN zone -> hub:7470 only;
  - hub -> nodes:7475 only;
  - deny node<->node and node -> other LAN;
  - care machines in their own VLAN, reachable only by the hub on 7475;
  - VPN zone untrusted by default.
- Protectorate still authenticates everything itself (network location is not identity): mTLS hub<->node, device-key auth for clients, private-address binding only.
- Future option, not taken now: node-initiated connections so nodes never listen, a further attack-surface cut.
- Termux: Android's WireGuard app routes Termux traffic at the OS level, so no userspace stack is needed.

## 5. pi-durable and routing models through Pi

Decision: keep the native harnesses; build harness-agnostic git-tree checkpoints in Rust; do not adopt pi-durable or layerfs; do not route every model through Pi.

- pi-durable is `@earendil-works/pi-durable` (v1.3.0, experimental, MIT, Node 22.19+) in the Pi monorepo (now `earendil-works/pi`, clone at `references/secondary/pi/packages/durable`). It is an embeddable durable runtime for conversations, tasks and documents (SQLite/JSONL/Durable Objects), not a Pi coding-agent extension, and it does not snapshot the filesystem. It solves a different problem from layerfs.
- File rollback in Pi comes from community extensions (checkpoint-pi, pi-rewind) that are git-ref checkpoints per turn: the same design we planned, so we build it once for every harness.
- Routing everything through Pi: Pi supports subscription OAuth for Anthropic, ChatGPT/Codex and xAI, but Anthropic's legal page says Pro/Max OAuth is for Claude Code and native apps, with reported enforcement against third-party harnesses since 2026 (https://code.claude.com/docs/en/legal-and-compliance). Claude in Pi would have to use a pay-per-token API key. We would also lose Claude Code subagents, skills, hooks, plan mode and native caching, and Codex's own sandbox and app-server features; our drivers already normalize all harnesses into one chat log, so the gain is small.
- Use Pi for providers without a good native harness (API-key models, local models, Gemini). ChatGPT and xAI subscriptions in Pi only after confirming each provider's current terms.
- Mine pi-durable's design (checkpoint per phase, restart-count guard, requestId idempotency) as reference for our own session layer if orchestrator-level crash recovery is needed.
- Unverified: npm publication of pi-durable, the extensions' source, OpenAI's terms text, what Pi's xAI OAuth authenticates against.

## 6. Implementation plan and agent assignments

Standing mandate for every agent: fix issues and fill gaps found along the way, simplify and remove slop in code touched, trim dependencies and LOC, stay simple, e2e and fuzz tests only, follow CLAUDE.md (no emojis, full migrations, minimal comments, no bulk-modifying scripts, RULE 1 deletions only within the authorized scope). Each agent works in its own worktree under `.claude/worktrees/<name>` on branch `wip/<name>` from `rust-yolo`, commits small logical commits, never pushes, and verifies with `make check` (and `make check-gen` / `make app-check` when protos or the app change). The main session merges.

Wave 1 (parallel):

| Branch | Scope |
|---|---|
| `wip/net` | Remove the overlay (Headscale supervision/install/admin, tailnet member/map/poll/discovery/route/wg/packets/tcp/stream/runtime, DERP and STUN, TailnetService, Flutter tailnet UI, installer and CI Headscale steps); hub and node over HTTPS on private addresses with mutual TLS from a hub CA issued at pairing; nodes report their reachable address; remove the node token. |
| `wip/desk` | Shelve desktop control: DeskService proto and Dart, desk crates, hub desk service, CLI desk, node and Windows wiring, Flutter screens, docs. |
| `wip/tests` | Remove all unit tests and library-level non-e2e tests (skipping crates being deleted by net/desk); add fuzz targets (wire framing, parsers, mount-plan validation, mailbox/time parsers) with seed replay in `make test`. |

Wave 2 (after wave 1 merges):

| Area | Scope |
|---|---|
| Auth redesign | Device-bound keys, short-lived access tokens, DPoP-style proofs, no `?token=`; sleeve/Envoy macaroons with attenuation; passkeys optional later. |
| Sandbox | Per-sleeve network namespace with egress allowlist; git-tree checkpoints (decided, section 5); per-sleeve disk quotas. |
| Audit and control | Hash-chained audit with `run_id`; delegation subset check; approval gates; harness/profile hash pinning; enforced quota budgets; effective-access view. |
| Observability | First-turn cache-read telemetry; herdr-style detection manifests; sequence-numbered reports. |
| Dependencies | toml, fs2, glob, tungstenite client, base64 alignment. |
| Docs | Rewrite ARCHITECTURE.md, README, DEV.md and docs/ for Rust-only and this design; retire Go-era docs. |

## Appendix A: research reports (full text)

Reports from the agents dispatched during this session, reproduced as returned (agent-internal metadata removed).

### A.1 Rust migration status as the docs describe it

Line numbers below are in `docs/rust-migration.md` unless another file is named.

#### 1. Strategy and cutover criteria

- **Go stays production until Rust passes parity.** Go remains the production hub, node agent and CLI until the Rust binary satisfies "the existing behavior and runtime tests" (3-5). Item 6 of "Remaining backend work" is the cutover step: "Replace the Go CLI and hub/node binaries, then run the existing real-binary end-to-end scenarios against Rust across supported targets" (604-605).
- **Parity target is user-visible behavior and Flutter API capability** (607-609). Go storage bytes and wire formats are explicitly not a target (187-189). The Rust archive format is independent of Go's (531).
- **Stays Flutter:** the client UI. "The UI rewrite begins only after the Go backend replacement is stable" (608). The same Flutter app is driven against a real Rust hub (620-629).
- **Out of scope:** the Android app (4-5, `docs/rust-termux.md:3`). A Termux *node* is in scope. Android and app updates, and Windows hub self-update, are not enabled (384, `docs/install.md:92`).
- **Gate:** `make rust-check` (fmt, strict clippy, workspace tests). `tests/e2e` is reused as a reference suite via `PROT_E2E_BINARY` and `PROT_E2E_COMPANION_BINARY` (189-192). On 23 October, 30 applicable Go e2e scenarios passed against Rust (702-705). The Go nested-runtime and race-report cases need Rust equivalents (639-641). Two Go on-disk-format fixtures (`sleeves.json` prune, `nodes.json` migration) are skipped because they are outside the rewrite target (705-708).
- **Route audit:** all 103 proto RPCs are registered in Rust. The doc says this verifies "route presence, not full behavioral parity" (644-646).
- **Release plumbing:** the uncommitted `DEV.md` and `README.md` edits already say the Rust lanes gate releases and `make check` no longer does. `README.md` tells users to use the Go release for features still listed as incomplete. Per `docs/install.md:41-47`, the installer stages a separate Rust hub unit and adopts the Go unit only with an explicit hash and rollback path.

#### 2. Per-area status

"Done" means the doc reports wired and tested. Almost every row carries real-environment caveats.

| Area | Go package | Rust crate(s) | Status | Pending items the doc names |
|---|---|---|---|---|
| Auth, identity, devices | `internal/auth` | auth, identity, pairing | Mostly done | Complete audit coverage "remains pending" (15). Windows harness login unverified (699-700). |
| Audit | `auth/audit.go` | audit | Done | Process death before an audit write cannot be recorded (20). Item 2 of the remaining-work list says to extend audit coverage to the remaining mutations (588). |
| Hub RPCs: lease, mailbox, backlog, directory, session, chatlog, inventory, node-monitor | `internal/hub`, others | lease, mailbox, backlog, directory, session, chatlog, inventory, node-monitor, hub | Done | Mailbox needs long-running harness validation (19, 589). Long-running branch behavior is unvalidated (54). |
| Node agent, jobs, xfer | `internal/node`, `xfer` | node, node-client, xfer, node-metrics | Done on Linux and Windows | Windows directory overwrite needs two renames and a crash between them is ambiguous (565-567). Interrupted uploads need a user retry (822, 841). `xe` DRM VRAM and broad Windows GPU/NPU inventory are pending (47). Live remote node profiles are unavailable (`docs/install.md:35`). |
| Sleeves and sandbox | `internal/sleeve` | sandbox, sleeve-layout, sleeve-registry, sleeve-runner, handoff | Partial | Long-running harness workloads remain (56). Real vendor TUI sessions are unverified (55). Nested-runtime coverage is needed (639). The sandbox is Linux-only; Windows is separate (`rust-sandbox.md:314`). |
| Harness drivers | `internal/driver` | pi, claude, codex, grok drivers | Partial | Claude authenticated inference is unverified because the OAuth session expired (24). Live Grok and Codex on Windows are untested (726). |
| Quota | `internal/quota` | quota | Done | None named. |
| Tailnet control (Headscale) | `internal/tailnet`, `headscaledb` | tailnet, tailnet-install, tailnet-admin | Done | Relay loss and long-running work need real-network validation (34). |
| Tailnet data plane | tsnet, DERP (Go libraries) | derp, derp-client, tailnet-member/map/poll/discovery/route/wg/tcp/packets/stream/runtime | Partial | NAT traversal, interface rebind, subnet routes and capability grants, WireGuard cookie defense, a live public-DERP trial, tsnet-style health. See section 4. |
| Release and update | `internal/release` | release | Partial | Hosted Actions and publication are unverified (57, 384, 463). The cross-architecture `ApplyUpdate` RPC deadline needs a fix (503-508). Windows hub self-update, Android and app updates are not enabled. |
| Backup | `internal/backup` | backup | Partial | Installed-service recovery is unverified (31, 601). Concurrent-write snapshot guarantees are not established (99-100). |
| Installers and svcgen | `install-hub.sh`, `internal/svcgen` | svcgen, `scripts/` | Partial | Hosted-release install is unverified. The Linux installer cache can leave a stale directory after a failed partial download (658-659). Production cutover is not validated: the remote reverse proxy is Go-plaintext-upstream (523-526). |
| Windows | `internal/desk`, node | desk-policy, desk-node, node-windows | Partial | Live desktop input and lock are untested (51-52, 764). A locked interactive desktop is untested (759). |
| Termux | Android node | node-termux | Partial | A real device trial is pending (63, 279; `rust-termux.md:44-48`). There is no sandbox under Termux. Android release asset selection is "tracked separately". |
| MCP bridge | `internal/mcpbridge` | mcpbridge | Done | None named. DNS and TCP connect in progress are bounded only by the client timeout (60). |
| CLI | `cmd/protectorate` | cli, connect-client, wire, server | Partial | "Broader command behavior across supported targets still needs validation" (61). Shell completion is static (264). |

#### 3. Explicit gaps (deduplicated)

- **Audit:** complete coverage pending (15, 588). Process death before an audit write is not recorded (20).
- **Validation under long-running load:** mailbox, branch dispatch, Pi workloads and sandbox harness workloads are not yet exercised that way (19, 23, 54, 56, 589).
- **Claude:** authenticated model inference is unverified (24).
- **Terminal sleeves:** real vendor TUIs are unverified (55).
- **Windows desk:** live input and lock are unverified (51-52, 764). Windows harness login is unverified (699-700). Windows hub self-update is not enabled (384).
- **Termux:** the real-device trial is pending (63, 279). The Android asset-selection gap is unspecified. The sandbox is unavailable (`rust-termux.md:44-48`).
- **Release and update:**
  - Hosted-release fetch, install and update are unverified (57, 384, 463).
  - The cross-architecture `ApplyUpdate` deadline needs a fix and a native x86_64 trial is pending (503-508).
  - Installed-task update validation is pending (57). Line 808 says an installed scheduled-task trial passed, so this line is stale.
- **Backup:** installed-service recovery and concurrent-write snapshot guarantees are unproven (31, 99, 601).
- **Cutover:** the reverse-proxy HTTPS upstream and cert trust are unverified, so production cutover is "not yet validated" (523-526).
- **Tailnet:**
  - NAT traversal and public-endpoint changes (38, 44).
  - Local interface rebind (44).
  - A live public-DERP trial (44).
  - Subnet routing and capability grants (40).
  - WireGuard cookie and rate-limit defense (41).
  - Unsupported TCP options (43).
  - tsnet-style health details (184).
  - Wider network and long-running transfer recovery (46).
  - DERP packet transfer is "unproven" for the NodeAgent retry case, because those packets used direct UDP (568-570).
  - An interrupted upload must be retried by the user (822, 841).
- **Node:** Linux desk is explicitly unsupported (50). Live remote node profiles are unavailable (`docs/install.md:35`). The xe VRAM fallback and broad Windows GPU/NPU inventory are pending (47).
- **Flutter:** widget navigation and the broader screens still need real-hub validation (628). The approval banner is not exposed in the Chromium accessibility tree (287-292). Android app updates are not enabled.
- **Tests:** the Go-suite fixture still needs a sandbox-descendant scenario (199-200).

#### 4. Tailnet

- **What it replaces in Go:** `internal/tailnet` (Headscale supervisor, config, admin, install, status, in-process DERP and STUN in `derp.go`) and `internal/headscaledb` (SQLite lock and gate for backup). Also replaced is the embedded Tailscale `tsnet` library. `ARCHITECTURE.md:136-153` says every node and the hub embed tsnet. That costs about 15 MB of binary and 40 MB of idle RSS per node. Tsnet and Headscale versions must be upgraded together, and Headscale rejects clients older than v1.80.
- **What stays external:** Headscale 0.29.5 is still the control server, as a separately pinned binary (`protectorate-tailnet/src/lib.rs:19`). The Rust crates replace the tsnet client and the Go DERP server, not Headscale.
- **Rust crate roles:**
  - `tailnet-member`: TS2021 Noise registration.
  - `tailnet-poll` and `tailnet-map`: map streaming and reduction.
  - `tailnet-wg`: WireGuard Noise IK handshake and ChaCha20-Poly1305 transport.
  - `tailnet-packets`: routing between DERP and direct UDP, with fallback and promotion.
  - `tailnet-tcp` and `tailnet-stream`: userspace TCP.
  - `tailnet-runtime`: virtual HTTP listeners on tailnet ports 7470 (hub) and 7475 (node).
  - `derp` and `derp-client`: a Rust DERP server and client.
- **Why:** the docs never state the motive explicitly. It is implied by the rewrite goal and the Windows, Termux and no-Go/no-bwrap installer targets (`docs/install.md:3`). It also removes the Go tsnet dependency while keeping Headscale's control plane and `tag:hub` ACL model. The hub still checks `tag:hub` membership on every node request (338-339, 411-412). Treat this as inference.

#### 5. Spot checks

| Claim | Result |
|---|---|
| 103 proto RPCs (644) | Confirmed: `grep` of `^  rpc ` over `proto/protectorate/v1/*.proto` gives 103. I did not verify Rust registration. |
| `tailnet-wg` implements Noise IK, ChaCha20-Poly1305 and replay handling, with cookie defense still pending (41) | Confirmed. `protectorate-tailnet-wg/src/lib.rs:17` uses `Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s`, with `snow` and `chacha20poly1305`. MAC1 is implemented. The module header (line 3) says MAC2/cookie defense belongs to the caller, and no cookie code exists anywhere in the crate. |
| `desk-node` does native user32 input and lock (51) | Confirmed. `src/windows.rs` declares `SetCursorPos`, `SendInput` and `LockWorkStation`. |
| `tag:hub` checked per request (46) | Partly confirmed. `tailnet-runtime/src/lib.rs:1104` has an auth check `peer_has_tag(..., "tag:hub")`, but I found it only inside a test. I did not trace the production call path. |
| `node-termux` builds `protectorate-rs-termux` (63) | Confirmed. `Cargo.toml` declares `[[bin]] name = "protectorate-rs-termux"`. |

**Discrepancies found:**
- `README.md` (uncommitted) mentions "the remaining node connectivity gap". The migration doc describes tailnet node connectivity as working in trials, and `docs/install.md` has no matching item. The README wording is unsupported or stale.
- The crate table omits `protectorate-node-windows`. It is the only one of the 55 crates not named in the table. The doc describes that binary only in prose (57).
- Line 57 lists installed-task Windows update as "remain to validate", but lines 800-809 report that trial as passed.
- Line 57 and `docs/install.md:92` describe release workflows and hosted Actions as unverified. `DEV.md` (uncommitted) already states the Rust lanes gate publication.

### A.2 Rust workspace health (build, test, deps, packaging)

**Overall:** the Rust workspace is healthy. `cargo check --workspace --all-targets` is clean, and the test suite passes except for one test that failed once under full-workspace load and then passed three times on its own.

#### 1. Toolchain
- rustc 1.96.0 and cargo 1.96.0 (in `~/.cargo/bin`). There is no `rust-toolchain` file, so CI and local builds both use whatever "stable" is current.
- Installed targets: `aarch64-linux-android`, `aarch64-pc-windows-msvc`, `aarch64-unknown-linux-gnu`, `aarch64-unknown-none`, `wasm32-unknown-unknown`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`.
- `Cargo.toml` is a `members = ["rust/crates/*"]` workspace with edition 2021 and 55 crates. `Cargo.lock` has 301 packages, which includes the 55 workspace crates, so about 246 external.

#### 2. Dependency graph
- **Binary crates (6, each with `src/main.rs`):**
  - `protectorate-server` (builds `protectorate-rs`) reaches 48 workspace crates.
  - `protectorate-cli` reaches 20.
  - `protectorate-node-termux` reaches 2.
  - `protectorate-mcpbridge` reaches 2.
  - `protectorate-sleeve-runner` reaches 1.
  - `protectorate-node-windows` reaches 0.
- **Possibly dead:** `protectorate-sleeve-layout` is the only crate no binary reaches, and nothing depends on it. Its only possible role is a helper that `package-rust-release.sh` and CI name. The packaging script and CI expect `protectorate-sleeve-sandbox-helper` and `protectorate-node-sandbox-helper` binaries. I did not trace which crate produces them. Only the six `src/main.rs` crates are counted as binaries above.
- **Notable external dependencies, from the `Cargo.lock` and crate manifests:**
  - tokio, axum, hyper, prost and prost-reflect are present. tonic and connect crates are not.
  - rustls is used with tokio-rustls, and both ring and aws-lc-rs are in the lock file.
  - snow (Noise), chacha20poly1305, crypto_box, curve25519-dalek, blake2 and sha2 handle the crypto.
  - boringtun is not used.
  - The tailnet and WireGuard stack looks hand-written, spread over about 14 `protectorate-tailnet-*` crates, plus `protectorate-derp` and `protectorate-derp-client`.
  - Also used: ureq, tungstenite, hpack, rcgen, zstd, zip, png, cap-std.
- **Largest files:**
  - `protectorate-cli/src/args.rs`: 4702 lines
  - `protectorate-server/src/main.rs`: 4270 lines
  - `protectorate-cli/src/lib.rs`: 3514 lines
  - `protectorate-cli/src/format.rs`: 3240 lines
  - `protectorate-hub/src/sleeves.rs`: 2716 lines
  - `protectorate-hub/src/chat.rs`: 2674 lines
  - `protectorate-hub/src/jobs.rs`: 2209 lines

#### 3. Build and test
- **`cargo check --workspace --all-targets`:** finished in 13.7s (the existing `target/` was reused) with 0 errors and 0 warnings.
- **`cargo fmt --all -- --check`:** no diffs. I did not run clippy, which `make rust-check` and CI enforce with `-D warnings`.
- **`cargo test --workspace --no-fail-fast`:** 1003 passed, 1 failed, 20 ignored, across 244 test suites.
  - The failure is `lease_change_reaches_event_subscriber` at `rust/crates/protectorate-hub/tests/lease.rs:101`. It expected a `heartbeat` event and got `node.unreachable`.
  - It passed 3 out of 3 reruns in isolation, so it looks like a timing race that only shows under load.
  - Nothing in the run seemed to need root, network or bwrap. I only looked at the failures and did not read the ignored tests, so I can't say whether any of them need those.

#### 4. Code-quality smells
- **Emojis:** none in the code itself. Three files contain emoji only as UTF-8 and UTF-16 test inputs: `protectorate-desk-node/src/typing.rs:101`, `protectorate-svcgen/src/lib.rs:329` and `protectorate-hub/tests/jobs.rs:379`.
- **`todo!`/`unimplemented!`:** 0. There are 3 TODO/FIXME/XXX comments.
- **`unwrap()`/`expect()`:**
  - Raw totals are 7764 `.unwrap()` and 225 `.expect(`, mostly in tests (772 `#[test]` functions across 90 `mod tests` blocks and 131 integration test files).
  - I counted non-test code roughly, as everything before `#[cfg(test)]` in files outside `tests/`. That gives about 649 `unwrap()` and 85 `expect()`.
  - The heaviest non-test files by combined count are `protectorate-node/src/files.rs` (143), `protectorate-server/src/transport.rs` (90), `protectorate-codex-driver/src/lib.rs` (46), `protectorate-pi-driver/src/lib.rs` (36), `protectorate-server/src/node_install.rs` (33) and `protectorate-claude-driver/src/lib.rs` (32).
- **`unsafe`:** 423 occurrences of `unsafe `. The concentrations are:
  - `protectorate-desk-node/src/windows.rs`: 73
  - `protectorate-sandbox/src/runtime.rs`: 55
  - `protectorate-node/src/windows_runner.rs`: 42
  - `protectorate-cli/src/attach.rs`: 34
  - `protectorate-node-metrics/src/windows.rs`: 20
- **Legacy naming:** 106 matches for `legacy|_old|v2|compat`, mostly legitimate. Examples are the Windows argv escaping comment and `notes.json` compatibility tests. I saw no `_legacy` or v2 clone modules in the sample I read.
- **Other:** the `protectorate-server` crate's `src/main.rs` is 4270 lines, and the binary it builds is `protectorate-rs`.

#### 5. Build, packaging and CI
- **Scripts:**
  - `scripts/build-rust-termux.sh [check|build|release]` cross-builds `aarch64-linux-android` with NDK r28 at API 24. It uses the NDK clang on x86_64 hosts and `clang-19` with lld on aarch64 hosts.
  - `scripts/build-rust-windows.ps1` needs a native Windows runner (it throws if the host triple doesn't match the target). It runs `cargo build --locked --release` for node-windows, cli and mcpbridge. It then runs `cargo test` on `protectorate-node-windows`, copies the results to `dist/*-windows-{amd64,arm64}.exe`, and checks the version.
  - `scripts/package-rust-release.sh VERSION` assembles `dist/`. It requires 6 Linux components for each of amd64 and arm64, 3 Windows components for each architecture, the Flutter web build, `VERSION` and `SHA256SUMS`. The only amd64 Linux binaries it copies come from `target/release`. Every other file must already be in `dist/`.
  - `scripts/` also holds ten `test-rust-*` scripts for installers, bundles, and hub and node updates.
- **`git diff Makefile`:** adds `rust-build`, `rust-test` and `rust-check` (15 lines). `rust-check` runs `cargo fmt --check`, then `clippy --all-targets -D warnings`, then `cargo test --workspace`. The Go targets are still present.
- **`git diff .github`:** `ci.yml` changed by 253 lines and `release.yml` by 224. Rust CI is real and replaces the old Go race and check jobs. The Go jobs `race`, `check`, `make test` and `make cross` are gone from `ci.yml`, apart from the desktop-windows reusable workflow.
  - The `rust` job on ubuntu-latest builds a verified Headscale, then runs `make rust-check` with Flutter tests required, then the installer checks.
  - Native build jobs cover Linux arm64, Windows amd64, Windows arm64 and Termux arm64 (NDK 28.2.13676358).
  - A `package` job then builds the Linux amd64 bundle and runs `scripts/test-rust-installed-bundle.sh`. It downloads the other artifacts, runs `package-rust-release.sh`, dry-runs `install-hub.sh`, and feeds `dev-release`.
  - Version now comes from `git describe`, not `make version`.
- **Mismatch with the repo rules:** `CLAUDE.md` still describes the Go and `make test` workflow, and `make check-gen` is no longer in CI.

### A.3 Uncommitted tracked-file changes (review)

The uncommitted tracked changes are protocol-consistent and the Go side still builds, but I'd expect CI, `make test` and `make check` to fail or be gutted if this is committed as it stands. I did not run `make check-gen` because its `gen` dependency (`Makefile:66`) regenerates files.

#### 1. Proto and generated code

All additions are field-level, with no removed or renumbered fields and no new RPCs or enums.

- **`hub.proto`**
  - `GetStatusResponse.release_confirmed = 10` (bool).
  - `GetSleeveRuntimeResponse.runtime = 12` and `helper_path = 13` (strings).
  - The bwrap fields 1-2 stay, with a comment "Legacy fields retained while the Flutter client moves to runtime/helper_path". That conflicts with the CLAUDE.md "no compat shims" rule.
- **`node.proto`**
  - `ApplyUpdateRequest.expected_version = 6`.
  - `NodeMetrics.update = 18`, a new message `NodeUpdateState{phase, previous_version, version, attempts}`.
  - Comment edits on `FILE_KIND_DIRECTORY`: the transfer is now a "verified archive" instead of a tar stream.
- **`job.proto` and `sleeve.proto`**: comment-only edits ("rust-namespaces" replaces "bwrap", "Linux namespace sandbox" replaces "bubblewrap").
- **Generated code**
  - Go gen and Dart gen both contain the new fields: `gen/.../hub.pb.go`, `node.pb.go`, `job.pb.go`, `app/lib/gen/.../hub.pb.dart`, `node.pb.dart`, `node.pbjson.dart`.
  - The sleeve and job Dart files and `sleeve.connect.*.dart` changed only in comments.
  - `internal/mcpbridge/image.binpb` grew from 295088 to 296646 bytes, which fits the descriptor change.
  - `make gen` was evidently run. I can't prove it is byte-identical without regenerating.
  - No `tests/e2e/testdata/mcp_tools.json` change, which is fine since no RPC changed.

#### 2. Go side

- No non-generated Go in `internal/` or `cmd/` changed. The only `internal/` change is `image.binpb`.
- `go build ./...` and `go vet ./...` both pass with no output, and vet includes the modified `tests/e2e`.
- The Go hub never populates `release_confirmed`, `runtime`, `helper_path`, `expected_version` or `NodeMetrics.update`. They are Rust-only fields.
- `docs/mcp.md` documents `protectorate mcp --https/--tls-cert/--tls-key`, but grep finds no such flag in the Go `cmd/` or `internal/`. The doc now describes Rust-only behaviour.

#### 3. tests/e2e

- **Pattern:** the 32 files (+695/-171) add a `cluster.rust` mode. It is switched on by `PROT_E2E_COMPANION_BINARY` (`harness_test.go:51`, an absolute path to `protectorate-rs`).
- **Harness:** in Rust mode it copies `protectorate-rs` and 6 companion binaries from the same directory into the test bin dir (`harness_test.go:94-112`).
  - It pre-seeds the owner and agent token files.
  - It passes different hub and node flags (`--owner-token-file`, `--agent-token-file`, `--node-binary-dir`, `--allow-plain-jobs`, `--claude-executable`; `harness_test.go:178-182`, `:265-267`).
  - The run timeout goes from 75s to 2 minutes.
- **Per-test branching:** many tests branch on `c.rust` to skip or alter Go-only behaviours. Examples are the report test's `chat wait` deadline, idle and killed-waiter checks, and a different registry file `sleeves-rust.json`.
- **Changes that also hit Go runs:**
  - `connect.WithProtoJSON()` is now added to the Chat and Event clients.
  - `leases_test.go` wraps its assertion in `c.eventually` (a timing relaxation).
  - The chat protocol peer's `.git` write probe is now bypassed by `PROTECTORATE_E2E_EPHEMERAL_PROBE`.
- **`rust_parity_test.go`** (133 lines, untracked): 4 tests, `TestBinaryRust*`.
  - They cover cancel-before-submit, job cancel/restart, descendant reaping on cancellation, and a port-holding descendant.
  - All skip unless `PROT_E2E_COMPANION_BINARY` is set (`:128-131`).
- **Gaps:** the Makefile has no target that sets `PROT_E2E_COMPANION_BINARY`, and CI doesn't set it. I found no wiring of the parity lane, so I can't confirm it runs anywhere automated.

#### 4. Flutter

- **`hub_shell.dart`** (17 lines): the `DeviceApprovalBanner` block moves after the `Expanded` content, with `verticalDirection: VerticalDirection.up`. The banner now sits at the bottom, and the unit test relies on this.
- **`pubspec.yaml`** adds `flutter_test` as a dev dependency, and `pubspec.lock` gains the transitive packages (`clock`, `fake_async`, `leak_tracker`, and others).
- **Untracked `app/test/hub_shell_semantics_test.dart`** (90 lines): a widget unit test, `testWidgets`, checking that a single accessible approval banner shows on the hub and node-detail routes. This violates the owner's "no unit tests ever, only e2e and fuzz" rule. CI runs it through `PROTECTORATE_REQUIRE_FLUTTER_TEST`, and the README now says "flutter analyze + test", though `make app-check` (`Makefile:99-100`) is still just `flutter analyze`.
- **`app/test_probe_clipboard_tmp.dart`** (29 lines): a stray scratch `testWidgets` probe for a clipboard channel, with a `debugPrint` and no assertions. It sits outside `test/`.
- **`app/tool/*_e2e.dart`:** five new untracked drivers (`backlog_e2e`, `node_invite_ui_e2e`, `nodes_ui_e2e`, `note_draft_e2e`, `session_e2e`). `hub_connection_e2e.dart` is the one tracked sibling. They look like real-hub e2e drivers rather than unit tests, and I didn't run them.

#### 5. Installers, docs, CI

- **`install-hub.sh`** (about 630 lines changed): it is now the Rust installer.
  - It takes `--bundle-dir` and `--role hub|node`, and the bwrap/tmux prerequisite check is removed.
  - It stages verified Rust binaries and the web archive into `~/.local/share/protectorate-rust/releases/<v>/`.
  - It adds `--adopt-go-unit`, `--rollback-go-unit`, and expected unit-SHA flags.
  - **Yes, it installs the Rust binary instead of Go.** It also carries a Go-to-Rust cutover and rollback path, which is the migration shim the CLAUDE.md rules disallow.
- **New installers:** `install-node.sh` is a 7-line wrapper that sources `install-hub.sh` as `main --role node`. `install-node-windows.ps1` is untracked and I didn't read it.
- **Scripts:**
  - `test-install-node-windows.ps1`: checks the Windows node installer on PowerShell 5.1.
  - `test-rust-installers.sh`: Linux installer fixtures.
  - `test-rust-installed-bundle.sh`: exercises an installed bundle.
  - `test-rust-browser-approval.py`: the Flutter approval button against a disposable Rust hub.
  - `test-rust-hub-node-update.py`, `test-rust-installed-hub-update.py`, `test-rust-installed-node-update.py`, `test-rust-installed-node-bundle-update.py`: update and rollback runs against isolated systemd units.
  - `scripts/test-inventory.py` is tracked and unchanged.
  - Also untracked: `build-rust-termux.sh`, `build-rust-windows.ps1`, `package-rust-release.sh`.
- **Stray files:** `scripts/__pycache__/` (5 `.pyc` files) and `dist/protectorate-rs-android-arm64` (17.6 MB binary) are untracked and not covered by `.gitignore`. Only `/target/` was added there.
- **Workflows:**
  - `ci.yml` and `release.yml` are rewritten to Rust lanes: rust, arm64, windows amd64 and arm64, Termux, and package.
  - The Go `race` and `check` jobs are gone, including `make test`, `make check-gen` (so generated-code staleness is no longer gated), `make cross`, the Android APK and the Go binaries.
  - The release no longer ships the Go binaries or the Android APK. DEV.md says "The Android app remains outside this hosted release."
- **Docs:**
  - `docs/install.md` shrank by about 335 lines and now covers the Rust installer only.
  - README and DEV.md drop the pinned install line, so the version-bump step is gone. The "What works in the Go release" heading remains.
  - `docs/auth.md` adds terminal-attach audit names.
  - `docs/node-work.md` has the Windows runner section reduced.
  - New untracked: `docs/rust-migration.md`, `rust-sandbox.md`, `rust-termux.md`.

#### 6. Half-finished or likely to break

1. **Go CI gate removed.** Go `make test`, `check-gen`, `check-version` and `cross` no longer run in CI. DEV.md says `make check` "remains available". The mixed `master`/`v0.4.x` release story is contradictory: the README points at a Go release, while the installer and workflows now ship only Rust.
2. **Parity lane not wired in.** The Rust e2e mode has no Makefile target or CI step, so it only runs by hand.
3. **Rule violations:**
   - The Flutter widget unit test (`app/test/`) and `app/test_probe_clipboard_tmp.dart`.
   - The "Legacy fields retained" proto comment.
   - The Go-unit adopt/rollback logic in the installer.
4. **Stray artifacts:** `dist/` (17 MB), `scripts/__pycache__/`, and the clipboard probe should not be committed. I did not delete them, per the no-deletion rule.
5. **Doc/code mismatch:** `docs/mcp.md` documents an `--https` flag that the Go CLI lacks.
6. **`make app-check` mismatch:** the README says it runs "analyze + test", but the Makefile target only analyzes.
7. **Go-visible e2e changes:** the Go e2e tests changed under Go as well (ProtoJSON clients, `eventually`, the probe env var). They compile, but nobody has confirmed they still pass under the Go binary.

### A.4 Go vs Rust parity map (derived from code)

Rust implements every RPC in the protos, but its server speaks Connect JSON over HTTP/1.1 only. I did not run the Rust code or its tests. I derived all of this from code and from comparing the two sets of procedure strings.

#### 1. RPC coverage matrix

The protos declare 103 service/RPC pairs (19 service blocks, counting `GrantService` and both `NodeAgentService` and `NodeService`). Go and Rust cover all 103.

- **Go dispatch:** `internal/hub/hub.go:255-284` registers a `protectoratev1connect.New*Handler` per service. `internal/node/server.go:68` does the same for `NodeAgentService`.
- **Rust dispatch:** each module calls `server.register("/protectorate.v1.X/Y", ...)` or `register_streaming` on a `UnaryServer`. The routing code is `protectorate-wire/src/lib.rs:972` and `:999`.
- **Rust hub registration:** `rust/crates/protectorate-hub/src/lib.rs:734-1179`, with per-service modules (`audit.rs`, `chat.rs`, `device.rs`, `jobs.rs`, `lease.rs`, `mail.rs`, `nodes.rs`, `sleeves.rs` and so on).
- **Rust node agent:** `protectorate-node/src/lib.rs:206-450`.
- **DeskService:** `protectorate-hub/src/desk_service.rs:346-359` registers all 8 RPCs in a loop. A string grep misses them, which is why they looked absent at first.

| Service | RPCs | Go | Rust |
|---|---|---|---|
| HubService | GetStatus, GetSleeveRuntime | yes | yes |
| TailnetService | GetStatus | yes | yes (`tailnet_status.rs:90`) |
| QuotaService | GetQuota, Suggest | yes | yes |
| ReleaseService | CheckRelease, UpdateHub (stream) | yes | yes |
| EventService | Subscribe (stream) | yes | yes (`hub/lib.rs:~734`) |
| SessionService | ListSessions, CreateSession, KillSession, RespawnSession, CapturePane | yes | yes |
| TokenService | CreateToken, UpdateTokenScopes, ListTokens, RevokeToken | yes | yes |
| GrantService | Admin | yes | yes (`grants.rs:18`) |
| DeviceService | StartDeviceAuth, PollDeviceAuth, ListDeviceAuth, ApproveDeviceAuth, DenyDeviceAuth, ListDevices, RevokeDevice, RenameDevice | yes | yes |
| AuditService | ListAuditEntries | yes | yes |
| BacklogService | ListNotes, GetNote, CreateNote, UpdateNote, DeleteNote | yes | yes |
| SleeveService | SpawnSleeve, ListSleeves, ListWorkspaces, BrowseWorkspaces, GetSleeve, KillSleeve, ResleeveSleeve, SpawnEnvoy, PruneSleeves | yes | yes |
| ChatService | Watch (stream), Send, Interrupt, Respond, Compact, ListModels, SetSelection, Report, Wait | yes | yes |
| DirectoryService | ListProjects, GetProject | yes | yes |
| MessageService | Wait, Send, Inbox, Ack, Reply, Thread, ListThreads | yes | yes (`mail.rs`) |
| LeaseService | AcquireLease, RenewLease, ReleaseLease, ListLeases | yes | yes |
| JobService | SubmitJob, GetJob, ListJobs, CancelJob, WatchJob (stream) | yes | yes (`jobs.rs`) |
| DeskService | Screenshot, ListWindows, Foreground, Click, Move, Type, Key, Run | yes | yes |
| NodeService | ListNodes, GetNode, AddNode, RemoveNode (stream), InviteNode, WatchNodeSetup (stream), PutFile, GetFile (stream), ApplyUpdate, UpdateNode (stream), Set/GetDeskPolicy, SetNodeTier | yes | yes (`nodes.rs`, `node_files.rs`, `node_update.rs`, `desk_policy.rs`) |
| NodeAgentService | GetMetrics, WatchMetrics, PrepareJob, UploadJobData, RunJob, AttachJob, ListJobs, CancelJob, PutFile, GetFile, DeskAction, ApplyUpdate, GetDeskState, Uninstall | yes | yes (`node/src/lib.rs`) |

There are no Go-only and no Rust-only RPCs, and no RPC is missing on both sides. The Rust handlers are not wholesale stubs, but some return `Unimplemented` conditionally:

- **Missing driver support:** `chat.rs:819` rejects Respond for Pi sleeves, `chat.rs:1154` covers other unsupported harnesses, and `grok-driver/src/lib.rs:338` says "Grok ACP compaction is not verified".
- **Unsupported node:** `node/src/lib.rs:328` ("does not accept updates"), `:347` and `:363`, `:380` ("no file transfers"), and `:469-476` (`no_jobs` / `no_desk`) cover nodes without jobs or desktop support. The Windows build installs the desktop and update handlers, as the comment at `:95` says.
- **Fallthrough:** the router returns `Unimplemented` for unknown unary or streaming procedures at `wire/lib.rs:972` and `:999`.

#### 2. CLI coverage

The Go CLI is cobra in `cmd/protectorate/*.go`, with `main.go:111` as the root. Rust is hand-parsed and split across three binaries:

- **`protectorate-cli`:** `protectorate-cli/src/args.rs:75-135` is the client dispatcher. `service.rs` and `completion.rs` live in its bin.
- **`protectorate-rs`:** the server binary. Its parser is `protectorate-server/src/main.rs:473-540` and `:285`. It handles `serve`, `hub ...`, `driver`, `client`, `api`, `version` and `node ...`.
- **`protectorate-mcpbridge`:** the standalone `mcp` binary.

| Go command | Rust |
|---|---|
| status, version, quota (+suggest), events, who, pair, login, desk (+policy), backlog (ls/show/add/edit/rm/pin/done/reopen), lease, job (run/ls/show/cancel/logs/test-cross), chat (watch/send/interrupt/respond/compact/models/settings/wait), report, sleeve (all, including attach, resleeve, prune), session, msg, devices, token (+scopes), grant admin, audit tail, orchestrator, `api call`, `tailnet status`, `completion`, `service systemd/windows-task/termux/install-windows` | present in `protectorate-cli` |
| hub serve, hub backup/restore, hub tailnet install/init, hub update, `node serve/install/doctor/uninstall`, `node join/renew/revoke`, `tailnet client invite/register`, `driver run` | present in `protectorate-rs`; most are separate commands on that binary rather than on the CLI |
| `mcp` | present as a separate binary, `protectorate-mcpbridge` |
| `hub update-guard` (hidden) | not found in Rust |
| `node migrate-tiers` | not found in Rust |
| `chat`/`sleeve` extras such as `wait` and `node metrics --local` | `node metrics --local` is in `args.rs:959-973`, so present |

Rust-only commands are `node direct-put` and `node direct-get` (`main.rs:~529`), which move files directly between nodes.

Both Go-only gaps are small: the hidden `update-guard` and `node migrate-tiers`. The Rust CLI's two-binary split means a Rust user needs `protectorate-rs` for server-side commands. Rust has no single `protectorate` multiplexer.

#### 3. Package and crate mapping

| Go package | Rust counterpart |
|---|---|
| auth | auth, identity, audit |
| backlog, backup, chatlog, directory, fsutil, handoff, mailbox, mcpbridge, quota, release, session, svcgen, xfer | same-named crates |
| desk | desk-node, desk-policy |
| driver | claude-, codex-, grok- and pi-driver |
| hub | hub, server |
| node | node, node-client, node-monitor |
| sleeve | sleeve-layout, sleeve-registry, sleeve-runner |
| tailnet | tailnet plus the `tailnet-*` crates |
| orchestrator | `hub/envoy.rs` and the CLI `orchestrator` command |
| diagnostics | `hub/diagnostics.rs` |
| keylog, headscaledb, testenv, codec | no crate. `codec` is replaced by `protectorate-wire`. `keylog`'s request-id log has no equivalent that I found. `headscaledb` (the backup/supervisor lock) is not needed because the Rust hub does not use a sqlite file. |

Rust crates with no Go counterpart:

| Crate | What it adds |
|---|---|
| sandbox | Pure-Rust Linux sandbox using namespaces, cgroups and overlays; fails closed instead of running on the host. Replaces bwrap. A `protectorate-sleeve-sandbox-helper` binary re-execs inside it. |
| wire | Hand-built Connect server and descriptor catalog (see section 4). |
| connect-client | Rust Connect client. |
| inventory, lease | Store crates split out of the hub. |
| node-metrics, node-monitor | Host probes and polling for Linux and Windows. |
| node-windows, node-termux | Platform binaries `protectorate-rs-windows` and `protectorate-rs-termux`. |
| pairing | Public pairing links and QR rendering. |
| derp, derp-client | A private DERP relay with STUN, and its client. |
| tailnet-* (14 crates) | A native Tailscale-protocol member stack. See below. |

The tailnet crates are `admin`, `discovery`, `install`, `map`, `member`, `packets`, `poll`, `route`, `runtime`, `stream`, `tcp` and `wg`, plus `tailnet` itself. They cover:

- **Hub side:** `tailnet` and `tailnet-install` still configure and install a pinned Headscale 0.29, and `tailnet-admin` calls its REST API. Headscale is still the control plane.
- **Member side:** a node no longer needs tsnet. `tailnet-member` registers it, `tailnet-poll` fetches the netmap, `tailnet-wg` does WireGuard, `tailnet-tcp` runs a userspace TCP stack, and `tailnet-runtime` ties them together.

#### 4. Transport and state format

**Transport.** The Rust server speaks Connect over HTTP/1.1 only, with no gRPC and no HTTP/2.

- **Framework and TLS:** it uses axum 0.8 with `default-features = false, features = ["http1","tokio"]`. TLS is `tokio-rustls` with ALPN pinned to `http/1.1` (`protectorate-server/src/transport.rs:52`).
- **Wire handling:** `protectorate-wire` does JSON encoding, validation and streaming itself, using `prost-reflect` with an embedded descriptor set (`wire/src/lib.rs:29`, `assets/protectorate.binpb`). It does not use generated stubs. Streaming uses `application/connect+json` envelopes (`lib.rs:735`, `streaming.rs`).
- **The Go side** uses connect-go with `codec.StrictJSON` (`internal/node/server.go:68`).
- **Terminal and other sockets:** the hub uses tungstenite 0.29 WebSockets.

**State.** The on-disk formats are partly shared and partly separate.

| File | Go | Rust |
|---|---|---|
| `leases.json`, `directory.json`, `mailbox.jsonl` | yes | same name. The mailbox is stated to be byte-compatible (`protectorate-mailbox/src/lib.rs:5`). |
| `notes.json` | yes | "Compatible notes.json storage" (`backlog/src/lib.rs:1`) |
| `nodes.json` | yes | same name |
| `audit/` | yes | same directory (`hub/lib.rs`) |
| tokens and devices | `tokens.json` (`main.go:206`) | `identity.json` |
| `desk-policies.json`, `desk-use.json` | yes | `desk-policies-rust.json`, `desk-use-rust.json` |
| `sleeves.json`, `jobs/` | yes | `sleeves-rust.json`, `jobs-rust/` |

I did not check whether the shared files match field for field or whether any importer exists. Moving token and device state from `tokens.json` to `identity.json` would need conversion, since the file names differ; I did not look for a converter. Sleeves, jobs and desk state are kept in separate `-rust` files, so the two hubs can share a home directory without clobbering each other.

### A.5 herdr architecture lessons

Herdr (v0.9.5) is a terminal workspace manager and runtime for coding agents. I read its docs, `CLAUDE.md`, `src/detect`, `src/persist`, `src/server`, `src/remote` and `vendor/*`, and skimmed protectorate's session, runner and attach code. I did not run it.

#### 1. What herdr is
- **Product:** a single Rust binary (Apache-2.0) that is a tmux-style multiplexer specialised for agents. It has workspaces, tabs and split panes, and runs in whatever terminal you already use.
- **Process model:** a detached background server owns the PTYs (`src/pty/actor/unix.rs`, vendored `portable-pty` with ConPTY patches). Clients attach over a local socket (`src/server/client_transport.rs`, `src/protocol/wire.rs`, bincode).
- **Terminal state:** the server parses every pane with an embedded `libghostty-vt` (Zig, vendored in `crates/ghostty-vt`). Clients receive rendered surface deltas (`src/protocol/surface_delta*.rs`), not raw bytes. So scrollback, resize and multiple clients are server-side facts.
- **Agent awareness:** about 22 TOML screen-detection manifests (`src/detect/manifests/*.toml`) classify each pane as working, blocked or idle. Optional hooks report state and a resume command. A JSON socket API with `agent wait`, subscriptions and `pane run` lets agents drive herdr, and the CLI is a thin wrapper (`src/api/`, `src/cli/`).
- **Remote access:** none of its own. It uses SSH (`--remote`) to bridge a remote server socket, with saved machines in one window (`src/remote/*`). A pane survives only while the server runs. After a server restart it restores layout, and resumes agents only through their own session ids.

#### 2. Ideas to steal

1. **Detection manifests as data, with an explain tool.** `src/detect/manifest.rs` defines TOML rules with priority, region (for example `bottom_non_empty_lines(12)`), regex and `not` guards. `herdr agent explain <pane> --json` shows every evaluated rule. Manifests hot-reload, are versioned, and can be overridden locally or fetched remotely. Protectorate needs working/blocked/idle for its fleet view and phone alerts, and this is deterministic code that tracks TUI churn without a rebuild. It would be a manifest crate next to the `*-driver` crates. Keep the evidence-based rule in herdr's `CLAUDE.md`: read a detection-only bottom buffer and never match whole-pane text.
2. **Server-side VT emulation, with a capture API that returns state rather than a byte tail.** Today protectorate shells out to tmux and does `capture` / `tail` on text (`protectorate-session/src/lib.rs`). Herdr's `terminal session observe|control` gives NDJSON `terminal.frame` records, with a single input owner and a `--takeover` flag (`docs/.../persistence-remote.mdx`). That model suits a Flutter or web client far better than a raw WebSocket byte relay. This matters most where tmux isn't available, which is Windows nodes (`protectorate-node-windows`) and arguably Termux.
3. **Persistence that is honest about what survives.** `session-state.mdx` has a table: detach keeps processes, server restart keeps only layout, and handoff is best effort. Related mechanisms: `src/persist/{snapshot,writer,restore}.rs`, and a pane that fails to restore stays in the layout with an error rather than vanishing. Also steal:
   - Three copies of any unparseable `session.json` go to `session-backups/` and are never auto-restored.
   - Up to 48 rate-limited layout snapshots are kept.
   - A logind delay inhibitor saves before host shutdown (`zbus`, `src/platform/linux.rs`).
   - The server ignores SIGHUP.
4. **Sequenced state reports.** `pane report-agent --seq N` is monotonic, so late or out-of-order reports are dropped (`add-herdr-support.mdx`, `src/agent_resume.rs`). Apply the same to every hub-to-node and sleeve-to-hub status message.
5. **Reported resume argv, deduped by (source, agent, cwd, argv).** `src/agent_resume.rs` has hard bounds (512, 4096, 64 args, 8 KiB). Protectorate's resleeve and Envoy flows need exactly this.
6. **Frozen wire contract discipline.** The "Stable client endpoint contract" in `CLAUDE.md` says named codecs are immutable, new fields are optional, and unknown enum values get a fallback. Frozen fixtures live in `tests/fixtures/endpoint-method-shapes-v1.json`. Protectorate's proto plus `make check-gen` is similar, but add the client-local-failure rule: one incompatible node must never drop the others.
7. **Handoff by SCM_RIGHTS.** `src/server/handoff.rs` passes PTY fds in batches of 64 to a new server, with a manifest, version and protocol check, and a ready timeout. This is a way to update the node agent or runner without killing sleeves, which `protectorate-sleeve-runner` ("tmux owns this process") can't do today. The caveats are in section 3.
8. **Hot-path guardrails.** Treat per-byte and per-render work as multiplicative: panes x clients. Use narrow accessors, and benchmark with 1 versus 15 panes (`just bench-render-scale`). The same applies to the hub's fan-out to phones.
9. **Local socket hygiene.** `src/ipc.rs` and `socket_paths.rs` give the socket mode 0600, record the file identity (dev and inode) to detect a stale socket, and keep a slow server's socket on connect timeout (commit ff5881e). The same ideas apply to `protectorate-server`'s local listeners.

#### 3. What NOT to do

- **Do not install hooks into agent configs.** `herdr integration install claude|codex|...` edits each agent's config (with atomic writes, backups and a version number per integration). That is the version-chasing, wrapper-adjacent coupling your "hands-off" rule forbids. Use only official headless protocols or the TUI, plus screen detection. If you want reported state, use an agent's own documented hooks, and keep them opt-in and never default.
- **Do not make the multiplexer your trust boundary.** Herdr's security is "same Unix user plus SSH". It has no sandbox, no identity and no leases. Protectorate's sandbox, auth and pairing layers are the point. Don't flatten them into a pane model.
- **Do not copy its sprawl.** The tree is about 330k lines, with 4-8k line files (`src/pane/terminal.rs`, `src/app/api/panes.rs`, `src/server/headless/tests/mod.rs`), a 5.7k-line `src/platform/windows.rs`, and 18 agent integrations, a plugin marketplace, a self-updater, sounds and themes. That contradicts your "fewest dependencies" and "bar for a new file" rules.
- **Do not take the Zig toolchain.** `libghostty-vt` needs Zig 0.16 pinned, plus patch files. Its Windows cross build needs `xwin` and Microsoft's SDK. A pure-Rust VT parser is an acceptable cost if you need server-side screens.
- **Do not use SSH as the remote transport.** Herdr's remote story is `ssh` plus a copied binary. You already have a tailnet stack and mTLS-style pairing, which is better.
- **Do not make the UI a TUI.** Herdr's whole render pipeline (`ratatui`, `crossterm`, kitty graphics, VTI input) exists to serve a terminal UI. Your clients are Flutter, so you need the frame and state protocol, not the renderer.
- **Do not run a second server concept per machine.** Herdr's named sessions are independent servers with their own sockets. Protectorate's hub should stay the single authority.

#### 4. Dependency or inspiration?

Inspiration only. Reasons:
- It is not a library. It is a binary with a private same-install protocol and a TUI-coupled client endpoint, and its own `CLAUDE.md` says it is "migrating toward a server-owned runtime protocol". The runtime/API seam is not stable yet.
- It would add a second persistent server, a second state store and a second socket authority on every node. That duplicates what `protectorate-session` and the runner already do, and it conflicts with the sandbox owning the PTY child.
- It would add Zig, vendored C and a ~28-crate dependency tree. Its Windows story is labelled beta.

Possible narrow reuse, each a judgment call:
- Treat the detection manifest schema as a reference format and write your own tiny evaluator in Rust (`regex`, `toml`).
- Use `herdr terminal session observe|control` NDJSON as the shape for your own frame API.
- The vendored `portable-pty` ConPTY patch notes (`vendor/portable-pty.patches.md`) are worth reading if you build a Windows PTY path.

The real gap it exposes is that tmux is your terminal layer. Herdr's value is a server-side VT with a typed frame API, which gets you Windows parity, resize and takeover semantics, and clean phone and web attach. The first step is a design spike on a minimal PTY plus VT-state crate behind `protectorate-session`, with a detection-manifest crate as the second, cheaper win.

Everything is under `references/herdr`. The `git pull` brought it from the Oct 23 clone to master at 43b36b6.

### A.6 Auth: current model, OIDC versus alternatives, recommendation

Caveat: `docs/auth.md` still describes the Go `internal/auth`, but the Rust crates match its model. Line cites are to `rust/crates/...`. External claims are mostly from the agent's own knowledge. Only the biscuit and Headscale OIDC claims were checked against a fetched page.

#### 1. Current model

Everything is one kind of credential, a bearer secret. It is `ptk_` plus random hex, stored as a SHA-256 hash in `tokens.json`, and compared in constant time (`protectorate-identity/src/lib.rs:361-378`). Each token record carries id, name, scopes, expiry, `revoked_at`, `task`, `minted_by`, device flag, platform and approver (`lib.rs:82-97`).

| Principal | How it authenticates | Scope and lifetime |
|---|---|---|
| Owner | `$PROTECTORATE_HOME/token` file, a bearer secret | Everything. Never expires. The only caller of `TokenService`, `AuditService` and the owner-only RPCs. |
| Phone, web client, desktop (device) | RFC 8628-style device flow in `DeviceService`. `StartDeviceAuth` and `PollDeviceAuth` are unauthenticated and rate-limited by IP (`hub/src/device.rs:47-105`). An already-trusted device or the owner approves a `user_code`, and the secret is delivered once (`identity lib.rs:451-600`). | `full`. Does not expire. Can approve other devices and call `UpdateHub` and `UpdateNode`. |
| CLI | Owner token, or a scoped token from `token create` | Glob scopes over MCP-style tool names (`auth/src/lib.rs:48-260`). Globs are default-deny. Owner-only RPCs match no glob. |
| Sleeve (agent in a sandbox) | Hub mints `sleeve:<name>` at spawn and writes it into the sleeve's home (`hub/src/lib.rs:278`, `chat.rs:1976-2010`). Revoked on kill, replaced on respawn. | `read`, `backlog_*`, `directory_*`, `lease_*`, `message_*`, `chat_report`. Handlers also check the caller's identity, so a sleeve can only touch its own chat and session. |
| Envoy | `orchestrator-envoy` identity token | Scoped. May mint admin grants only for `shared`-tier nodes, and only if `--orchestrator-admin-grants` is on (`hub/src/grants.rs:25-52`). |
| Task admin grant | `GrantService.Admin` (`grants.rs`) | `job_*` plus `node_admin:<node>`, 4h default, 24h max, bound to a task and the minter, audited. |
| Nodes | Hub-to-node calls use a node token. A node accepts the hub only by tailnet `tag:hub` or that token (`server/src/transport.rs:224, 406`). | The node never serves the hub API. |
| Tailnet | WhoIs allows only `tag:hub` and `tag:client`, never `tag:node`. A token is still required, and the tailnet identity is logged in audit (`audit lib.rs:59`). | Identity is a second factor, not the credential. |

Revocation is a flag on the token. It cancels open streams immediately through `RevocationWatch` (`identity lib.rs:380-403, 628`). A scope edit takes effect on the next request, and existing streams keep their admission.

Weak points:
- Device tokens and the owner token never expire and are not bound to a device. A copied `SharedPreferences` file or `token` file is permanent full access.
- `lookup` scans every token linearly (`lib.rs:361`), which is fine at this scale.
- The `?token=` WebSocket fallback puts the secret in URLs.
- Sleeve tokens sit on disk inside the sandbox.

#### 2. Options for one owner, private network, no cloud

**(a) OIDC with a self-hosted IdP (Kanidm, Authelia, Zitadel, Pocket-ID).** Rejected for the hub. You would run a second always-on service, and the hub would depend on it for every login. The hub would also need a JWT/JWKS stack (`openidconnect`, which brings reqwest and rustls). OIDC is built for many users and federation, and you have one user. Headscale's OIDC is not a fit either. It only authenticates people joining the tailnet, and it does not issue tokens to applications (checked: https://headscale.net/stable/ref/oidc/). If you want a login at tailnet enrollment, use Headscale OIDC there and keep it out of the hub.

**(b) Passkeys/WebAuthn at the hub.** This is the right way to authenticate the owner. It is phishing-resistant and has no shared secret. A browser needs a secure context, and tailnet DNS plus certificates or an `http://localhost` development exception can supply one. Android Credential Manager supports passkeys. Rust: `webauthn-rs` (pulls in openssl or rustls and some ASN.1 crates). Use it only to approve devices and for owner login. Do not use it per request.

**(c) Tailnet identity as authentication.** This is strong for nodes and the hub. The tailnet client in this repo is custom Rust (`protectorate-tailnet-*`) with `peer_has_tag` already implemented. It is weak for humans, because the doc's own reasoning applies: a stolen phone is already on the tailnet. Keep it as a gate and as an audit field. Do not make it the sole credential for clients. For nodes it is sufficient, and the node token can go away.

**(d) mTLS with hub-issued short-lived certs.** The tailnet's WireGuard already encrypts and authenticates the peer. A CA, rotation and a TLS stack add cost and the browser client still cannot use it cleanly. Rejected. (Note added by the main session: with the overlay removed, mTLS between hub and nodes becomes the node identity mechanism; the browser objection still applies to clients, which use device keys instead.)

**(e) Capability tokens (biscuit-auth).** Best fit for agents. Tokens are signed with a root key, any holder of the public key can verify them offline, and anyone can attenuate them by appending blocks. Revocation "requires external state management" (checked: https://github.com/eclipse-biscuit/biscuit-rust). So the hub keeps a small revocation list by token id. Envoy could then hand a sleeve a narrower token without calling the hub, which the current opaque tokens cannot do. Cost: the `biscuit-auth` crate, with a Datalog engine and prost/protobuf dependencies (prost is probably already present). Macaroons are simpler but use symmetric HMAC, so only the hub can verify them. That is fine too, and cheaper to build by hand (about 150 lines on `hmac` + `sha2`).

**(f) DPoP / device-bound keys (RFC 9449).** Each device holds a private key it cannot export (Android Keystore, StrongBox, Secure Enclave, TPM). Every request carries a signature over the method, URL and a timestamp. A copied bearer token is then useless without the key. For the Flutter web client the key is a non-extractable WebCrypto key in IndexedDB. It does not stop malware running on the device, but it does stop file copies and backups. Rust: `ed25519-dalek` or `p256`, about 100 lines for the verifier. This is the best fix for the device-token weakness.

**(g) SPIFFE for sleeves.** SVIDs, attestation and a trust-domain server are too heavy. The hub already knows what it spawned, and a biscuit minted at spawn does the same job. Rejected.

#### 3. Recommended layered design

1. **Owner (human).** Passkey (WebAuthn) at the hub for bootstrap and for approving devices. The `token` file stays as a break-glass local credential, readable only on the hub host and used by the CLI there. Never ship it to clients.
2. **Devices (phone, web, desktop, remote CLI).** Keep the device flow, because it is already built and standard. Change two things:
   - At approval, the client sends a public key, generated in Keystore or WebCrypto. The device record stores it.
   - The hub issues a short-lived access biscuit (about 15 min) and verifies a DPoP-style proof signature on each request. The device token is replaced by a refresh credential that must itself be signed by the device key. Revoking the device then ends refresh and cancels streams, which the existing `RevocationWatch` already does.
   - Streams (WebSocket terminal, events) authenticate once at connect, with the proof in a subprotocol header. Drop `?token=`.
   - Approval stays "trusted device or owner", plus a passkey tap when the approver is the owner.
3. **Nodes.** Authenticate with tailnet identity alone (`tag:node` for node to hub, `tag:hub` for hub to node), as now. Remove the static node token so there is no long-lived node secret. Enrolment keys (`InviteNode`) already exist. (Superseded: nodes now authenticate by mTLS with a hub CA.)
4. **Sleeves and Envoy.** At spawn the hub mints a biscuit with:
   - `sleeve = <name>`
   - a scope list (what `sleeve:<name>` has now)
   - a 1-hour expiry with renewal through a socket the sandbox can reach
   - a revocation id equal to the token id

   Envoy can attenuate it further when delegating, for example to one node or one path prefix. Admin grants become biscuits with `node_admin(node)`, `task(id)` and a `ttl` check, so they are verifiable on the node itself without the hub. A leaked token expires in minutes, and killing a sleeve adds its id to the revocation set.
5. **Audit.** Add the device key id, the token id and the tailnet peer to every entry (`audit lib.rs:52-60` already has `actor` and `tailnet`).

**What already resembles this:**
- Device-code flow with approval and a rate limit (`hub/src/device.rs`).
- Per-sleeve identity checks inside handlers (so the token only has to prove who the sleeve is).
- Per-spawn minting and kill-time revocation of sleeve tokens (`hub/src/lib.rs:278`).
- Task-scoped, time-limited admin grants (`grants.rs`).
- The tailnet tag gate (`transport.rs:224, 406`).
- Immediate stream cancellation on revoke (`identity lib.rs:628`).

**Rough dependency cost.**
- Required: `ed25519-dalek` or `p256` (about 3 small crates).
- `biscuit-auth` (plus `prost` / `rand` and a few more transitive crates), or hand-rolled HMAC macaroons using `hmac` (about 1 crate).
- Passkeys: `webauthn-rs` (the heaviest item, about 20-30 transitive crates). The cheaper alternative is to keep device approval as it is and drop passkeys.
- No OIDC library, no IdP, no JWT stack.
- `sha2`, `subtle`, `zeroize` and `getrandom` are already in the tree.

**Order of work:**
1. Device-bound keys with short-lived access tokens. This closes the largest real hole, the permanent copyable device token.
2. Biscuits for sleeves and grants.
3. Passkeys, last and optional.
4. Delete the node token once tailnet identity is the only node credential.

**Why not OIDC.** It solves "many users, many apps, one directory". You have one user, no third-party apps, and the private-access rule means no WAN callbacks. You would run a second service and a larger dependency tree to get a weaker guarantee than a device-bound key. If you later add users beyond the family, put an IdP in front of Headscale enrollment, not in the hub.

### A.7 WireGuard, Headscale and ACLs: current use, lockdown, alternatives

I read the code and docs only, and I did not run Headscale or load any of the policies below. The URLs are from memory and I did not fetch them, so check them before you rely on them. The tailnet crates total about 10.5k lines of Rust, including tests. The in-crate line counts are in section 1.

#### 1. How it works today

**Control plane.** Headscale 0.29.5 runs as a pinned binary and is supervised by the hub. Its config is rendered at `rust/crates/protectorate-tailnet/src/lib.rs:199-236`.
- Headscale's own DERP server is disabled (`derp.server.enabled=false`). The hub runs `protectorate-derp` instead, with a private DERP map that pins the leaf certificate (`:265-305`).
- Listeners: control on `0.0.0.0:7480`, DERP on `0.0.0.0:7483`, STUN on `0.0.0.0:3478`. gRPC (7482) and metrics (7481) are loopback only (`:62-80`).
- The unix socket is mode 0700. `node.expiry` is "0", and the hub sets key expiry itself, defaulting to 180 days (`:70`).
- The policy is a file at `tailnet/policy.json`. I found no HuJSON, no `ssh` block and no `grants`. Headscale only grants SSH where an `ssh` rule says so, so SSH is effectively off.

**Generated policy** (`lib.rs:240-259`). Tag owners are all `protectorate@`: `tag:hub`, `tag:node`, `tag:client`, `tag:tier-shared`, `tag:tier-care`, and `tag:pool-<name>`.
- Rule 1: `tag:hub` to `tag:node:7475` and `tag:node:22`. Port 22 is reachable but nothing uses it, because Headscale SSH is off and the node's userspace stack has no listener on 22.
- Rule 2: `tag:client` to `tag:hub:7470`.
- Per pool: `tag:pool-X` to `tag:pool-X:*`.
- Nothing grants access to the tier tags. They are labels only.
- Everything else is denied, and no `autogroup:member` rule exists. This is a good baseline.

**Enrollment** (`protectorate-tailnet-admin/src/lib.rs`).
- `invite_node` (`:349-368`) mints a preauth key that is single-use (`reusable:false`), non-ephemeral, has a TTL, and carries `aclTags` of `tag:node`, `tag:tier-<tier>` and an optional pool tag (`:311-313`). A reinvite expires the old key.
- The member does a TS2021 Noise registration in `protectorate-tailnet-member`, and the hub registers itself as `tag:hub`.
- A client (CLI or app) gets `tag:client` (`:436`).
- `enforce_expiry` sets an expiry only on `tag:node` and `tag:client` nodes (`:485`), so `tag:hub` never expires.

**Peer authorization.**
- A node serves its Connect API only on a userspace-TCP listener on 7475. Every request, including on an already-open connection, re-checks `peer_has_tag(generation, peer_id, "tag:hub")` against the current netmap (`protectorate-server/src/transport.rs:406`).
- The hub accepts `tag:hub` or `tag:client` (`transport.rs:235`).
- The route crate compiles the netmap packet filters and checks key expiry, source and destination IP and port (`protectorate-tailnet-route`).
- The hub and node also keep their own sets of local bearer tokens.

**Userspace stack.**
- Nodes have no TUN device and no kernel route. The sleeves and the host cannot use the tailnet as a network. Only the node process exposes ports 7470 and 7475 as virtual listeners.
- Stack sizes: wg 662 lines, packets 2493, tcp 1031, stream 1329, runtime 2113, map 1004, member 1613, poll 638, route 650, admin 708, discovery 366, tailnet 989 (config only), derp 865 and derp-client 694. That is about 15.2k lines in total, including tests.
- Dependencies are `snow`, `blake2`, `chacha20poly1305`, `curve25519-dalek` and `zeroize`, plus tokio and rustls.

**Stated gaps** (`docs/rust-migration.md:32-46`, `protectorate-tailnet-wg/src/lib.rs:3`).
- WireGuard MAC2/cookie defense and rate limiting are left to the caller, and I did not find an implementation.
- The code has no NAT traversal beyond published endpoints, no local interface rebind, and no subnet routes or grants.
- TCP options are unsupported.
- The code has no congestion control that I saw.
- Everything in the migration doc is "disposable trials" and "wider network conditions remain".

#### 2. ACL audit and proposed lockdown

The policy is already default-deny with tag owners, so only these items need changing.

1. **Pool lateral movement.** The `tag:pool-X` to `tag:pool-X:*` rule lets any pool member reach any other member on every port. The only thing that blocks it is the node's tag check at the application layer. Delete that rule unless a feature needs node-to-node traffic. I found no such use, because transfers go through the hub (`docs/rust-migration.md:48,59`).
2. **Port 22.** Drop `:22` from rule 1, because nothing uses it. Add an `ssh` section only if you want it.
3. **Hub reachability.** `tag:client` may reach the hub on 7470. Headscale ACLs only filter by tag and port, so a stolen phone cannot be limited to a subset of RPCs. That is a hub-token and scope job, and your device-approval scheme already covers it.
4. **Hub reaching care nodes.** Care nodes are labelled `tag:tier-care` but get the same hub access as shared nodes. The separation is enforced at the application layer ("care jobs run plain", `docs/rust-migration.md:66`). A policy-level split is not needed unless you want care nodes to refuse admin traffic entirely. In that case add a separate port for care nodes and a separate rule.
5. **`tag:node` to hub.** Nothing allows it, which is correct. The hub dials nodes, and Tailscale ACLs are stateful, so replies still work.
6. **Headscale admin.** gRPC and metrics are loopback only. The hub talks to the REST API over the unix socket (mode 0700). The remaining exposure is control `0.0.0.0:7480` plus DERP and STUN on the LAN. That is acceptable under "no WAN" if the host firewall limits them to 192.168.22.0/24 and the LAN, which should be stated in `docs/install.md`.
7. **Tag assignment.** The preauth key's `aclTags` is the only way a node gets `tag:node`. Headscale lets a tag owner use `--advertise-tags` on re-registration. Because `tagOwners` lists `protectorate@`, a compromised member could in principle re-register or update itself as `tag:hub`, provided the user can authenticate. The member code uses preauth keys only, but verify two things. First, that Headscale rejects `RegisterRequest` with tags that differ from the key's tags. Second, that no auth path issues `tag:hub` keys for a user other than the hub's own. A cheap hardening is to give `tag:hub` and `tag:client` a different owner (a separate Headscale user, for example `hub@`) from `tag:node`. The existing hub self-check already refuses to run without `tag:hub` (`tailnet_runtime.rs:191,211`).
8. **Sleeves and agents on a node.** They cannot reach the tailnet at all, because the stack is a userspace listener with no TUN and no outbound dial. The node dials nothing except the hub's registration and the map poll. Keep it that way, and do not add a SOCKS or tun mode.

The proposed `policy.json` is reproduced in section 3 above. Notes:
- The pool rules are deleted. Pools are labels only, and the hub's `tag:hub` rule already covers them.
- `tests` is supported in policy files by recent Headscale. Check that 0.29.5 accepts it, and use it as a regression guard.
- Add an e2e test that asserts the exact policy JSON, replacing the existing 4-rule test at `lib.rs:367`.
- Changing the user split (`hub@`) means the hub registers under a second Headscale user. That is a small change in `tailnet_runtime.rs`.

References: https://tailscale.com/kb/1337/acl-syntax and https://headscale.net/stable/ref/acls/

#### 3. Alternatives for this use case

**Facts that decide it.** Nodes sit on a home or office LAN or on one WireGuard subnet. The hub reaches them directly, and hub, nodes and clients are mostly on LAN or private WireGuard. Real cross-NAT cases are probably the phone and any remote office. The migration doc's own trials show direct UDP working with zero DERP packets (`docs/rust-migration.md:44`).

**(a) Status quo, Headscale plus your own Rust member stack.**
- Pros: no TUN or root needed, so Termux works. The stack is small enough to audit. The node tag check is tight and you control every dependency.
- Cons: 15k lines of hand-written security code (WireGuard Noise, TCP, DERP, netmap), none externally audited. Known gaps are cookie defense (DoS surface), no NAT traversal beyond published endpoints, no congestion control and no TCP options. Every Headscale and Tailscale protocol change falls on you, and tsnet and Headscale must be upgraded together.
- Cost: highest ongoing maintenance of all the options.

**(b) Headscale plus the official `tailscale` or `tailscaled` on nodes.**
- Pros: Tailscale's code does NAT traversal, DERP and disco. `tailscaled --tun=userspace-networking` works without root on Linux, Windows has a service and a client, and Android has an app. The hub reads peer identity through the LocalAPI WhoIs.
- Cons: Termux has no good official build, so you would run the Go binary in userspace mode (Tailscale Go on Android needs the app, and in Termux the arm64 binary sometimes works). A 15 MB Go binary per node and about 40 MB RSS (`ARCHITECTURE.md`). You have gone the other direction on purpose, because the owner wants Rust and fewest dependencies, and this adds a Go runtime to every node.

**(c) Kernel or OS WireGuard with hub-managed static config (innernet or wg-quick style).**
- Pros: tiny and boring. Minimal protocol: Linux has it in the kernel, Windows has WireGuard-NT, and Android has the app, so there is no hand-written crypto at all. An innernet-like hub (https://github.com/tonarino/innernet) is a good model.
- Cons: needs root or admin and a TUN device. Termux without root cannot do it, and Android's VpnService conflicts with other VPNs. Key and endpoint distribution move into the hub. Sleeves share the host's network namespace unless you firewall per-UID, which the sandbox (bwrap) may already handle.

**(d) boringtun or another WireGuard crate plus coordination over the hub API.**
- Pros: replaces your 662-line `-wg` crate with maintained code (https://github.com/cloudflare/boringtun). Keeps userspace operation for Termux.
- Cons: the hard parts are the TCP stack, the netmap and NAT discovery, and those are what you hand-wrote. Moving to a crate does not remove the packets, TCP or runtime crates. It only removes about 660 lines. The coordination you would build is, again, a Headscale-lite.

**(e) Nebula** (https://github.com/slackhq/nebula). Written in Go, with its own lighthouse and certificate authority and a built-in firewall. It is a good fit for a single owner, but it has no Termux-friendly userspace mode and replaces Headscale with a Go dependency. There is no Rust implementation.

**(f) NetBird** (https://github.com/netbirdio/netbird). A heavier control plane (management, signal, relay and a database), and it is Go. It is aimed at teams, so it is too big for one owner.

**(g) No overlay: mTLS over LAN plus the existing 192.168.22.0/24 WireGuard.**
- Pros: the transport is already there. The hub and nodes speak rustls, and "opt-in TLS agent listener and explicit hub private-CA trust" is already implemented (`docs/rust-migration.md:47`). Pairing could issue per-node client certificates with the hub as the CA. This deletes about 15k lines, Headscale and DERP.
- Cons: you give up reachability across arbitrary NATs. Under "no WAN ever" that costs only the roaming phone and any non-LAN node. You also need a rule for how nodes behind a different LAN reach the hub, which today is the tailnet's job.

#### 4. Recommendation and migration sketch

**Recommendation: (g) as the default transport, with the tailnet kept as an optional extra for nodes that are off the LAN.** In the order of the owner's stated values (fewest dependencies, correctness, private access only):
- Most nodes are on the LAN or the 192.168.22.0/24 WireGuard already, and need no overlay.
- Hand-written WireGuard and TCP are the largest correctness and security liability in the repo. A DoS gap (no cookie defense) is already documented.
- If you want the overlay long term, replace it with (b) rather than keep (a). Maintained NAT traversal beats your own, and the Go cost is the same one you paid in the Go rewrite.

**Migration steps.**
1. **Now, any path:** apply the policy in section 2 and add the exact-policy e2e test. Firewall 7480, 7483 and 3478 to the LAN and WireGuard ranges.
2. **mTLS node transport.** The hub already trusts a private CA for direct HTTPS. Add per-node client certificates issued at pairing (reuse `protectorate-pairing`). The hub authenticates the node by certificate and the node authenticates the hub by CA. This replaces the `tag:hub` check with a cert check.
3. **Run both transports** behind the existing node-dial adapter (named peer, `protectorate-node-client`), choosing per node. Keep the tailnet only for nodes that need it, such as the phone and any remote office.
4. **Measure** the remaining tailnet users. If the count is zero or small, either drop the stack (delete `-wg`, `-packets`, `-tcp`, `-stream`, `-runtime`, `-derp*`, `-map`, `-poll`, `-route`, `-member` and Headscale install) or, for the few remote nodes, switch them to official `tailscaled` userspace mode (b) and have the hub talk to them over its LocalAPI.
5. **Termux** keeps working in both cases: mTLS needs no TUN, and `tailscaled --tun=userspace-networking` is the fallback.

Caveats: step 2 is real work (certificate issuance, renewal and revocation, which Headscale expiry gives you today), and I did not validate the Headscale `tests` or `ssh` policy keys against 0.29.5. Please confirm the current Termux and Android story for official Tailscale before committing to (b).

(Outcome: the owner chose to let the UniFi routers own connectivity; the overlay is removed entirely. See section 4.)

### A.8 agentbaseline reference architecture: summary and gap analysis

**Sources:** https://agentbaseline.org/reference-architecture, https://agentbaseline.org/controls, and the raw source https://raw.githubusercontent.com/agentbaseline/agentbaseline/main/software-factory/software-factory.md. The page is dated 13 Oct 2026 as v1. It has no standalone threat-model, layers or glossary pages. Those sections are in the source markdown.

#### Reference architecture summary

Principle: the agent proposes, policy authorizes, infrastructure enforces. Controls sit outside the agent's decision loop, and the model is not assumed malicious.

**Threat model.**
- Untrusted: the agent, everything it reads (prompts, tickets, tool output), and its destinations.
- Trusted: the enforcement infrastructure (sandbox runtime, egress proxy, supply-chain firewall, model gateway, authorization service, audit log) and the execution layer.
- Out of scope: the model provider, abuse by legitimate admins, and memory stores (but their output is still untrusted input).

**Four boundaries.**
1. **Sandbox.** Per-task, built from a profile, writes confined to the workspace, destroyed afterward. Isolation scales with risk (container up to microVM).
2. **Network.** A default-deny egress proxy enforced at the network layer, not via proxy settings. It blocks raw sockets, direct IPs, unapproved DNS, cloud metadata and private ranges. It attaches credentials only to authorized requests, and inference goes through the same enforcement.
3. **Supply chain.** A single admission point for approved images, harnesses, skills, packages and models, with provenance on outputs.
4. **Identity and access.** A per-agent identity. The profile sets a ceiling, the task scope narrows it, and delegation can only narrow further. Credentials stay at the proxy. High-impact actions need independent approval or step-up.

**Other parts.**
- **Execution layer.** Starts agents, retries or stops them, and fails closed if an enforcement point is down.
- **Audit.** Tamper-evident, outside the agent's reach, linked by run ID.
- **Model gateway.** Approved models, per-task budgets, rate limits.
- **Profile.** A versioned, authenticated artifact. Only its enforced parts (rules, sandbox config) count as boundaries.
- **Level 3 extras.** Preflight, where the orchestrator proposes profiles and scopes, signal-driven work, and monitoring agents (thrash and drift) that can flag but not grant.

**Autonomy levels.** L1 is human-orchestrated, L2 workflow-orchestrated, L3 agent-orchestrated. Protectorate's Envoy is L3 ("keep plans from becoming permissions").

**Underlying controls** (35, six outcomes, from /controls): DIS-01..07 (registries, effective-access mapping), CON-01..04 (admission, toxic combinations, sandbox, profiles), AUT-01..09 (attribution, task-bound authority, delegation attenuation, JIT credentials, independent approval, fail-closed, step-up, JIT elevation, proof-of-possession), OBS-01..06, VAL-01..04, RES-01..05 (stop/revoke, quarantine, evidence preservation, safe fallback, rug-pull protection).

**Cited failures.**
- An agent found a broad API token and deleted a production volume and its backups (PocketOS).
- The Shai-Hulud package worm stole credentials.
- Agents escaped egress controls in two lab evaluations (Hugging Face; Anthropic).
- An agent read an ambiguous instruction as permission to deploy to production.

#### Gap analysis

Protectorate cites: `docs/auth.md`, `docs/rust-sandbox.md`, `docs/mcp.md`, `docs/orchestrator.md`, `ARCHITECTURE.md`, and crates `protectorate-sandbox`, `-audit`, `-auth`, `-lease`, `-quota`. Read only partially. The gaps marked "missing" come from targeted greps and the two docs read, not a full code audit.

| Reference item | Status | Evidence / gap |
|---|---|---|
| Sandbox per task (CON-03) | **Have** | `docs/rust-sandbox.md`: user/mount/PID/IPC/UTS namespaces, tmpfs root, masks, cgroup scope, fail-closed launch. No microVM tier, which is overkill here. |
| Network boundary / egress proxy | **Missing** | Sleeves share the host network by design ("Network stays shared with the host at first"). Greps found no egress filter or metadata-endpoint block. |
| Credentials kept out of sandbox | **Partial** | Per-sleeve tokens are scoped, and only the selected provider auth directory is mounted. But the provider login lives inside the sandbox, and the sleeve token is written into the sleeve's home (`docs/auth.md`). The reference wants credentials injected by a proxy. |
| Supply-chain firewall (CON-01, DIS-04) | **Missing** | Toolchain mounts are preapproved. There is no inventory or admission control for MCP servers, skills or packages, and no pinning of harness versions. |
| Profiles (CON-04) | **Partial** | Workload profile table in `docs/rust-sandbox.md`, plus token scopes and `mcp --allow/--deny`. These are not one versioned, authenticated artifact. |
| Identity and attribution (AUT-01) | **Have** | Per-sleeve tokens `sleeve:<name>` with audit actor. Owner, device and admin-grant tokens (`docs/auth.md`). |
| Task-bound authority (AUT-02) | **Partial** | Admin grants are per task, 4h default, 24h max. Sleeve tokens are fixed at spawn and not narrowed per task. |
| Delegation attenuation (AUT-03) | **Partial** | Parent-recorded ownership of children and a reserved-name rule. Envoy needs a specific `--orchestrator-admin-grants` flag. There is no check that a child's scopes are a subset of its parent's. |
| JIT credentials (AUT-04) | **Partial** | Admin grants are short-lived. Sleeve tokens last until kill, and device tokens never expire. |
| Independent approval (AUT-05) | **Partial** | Chat-level approval requests. Owner-only RPCs (`AddNode`, `UpdateHub`, `CreateSession`) are a good hard gate. No approval gate on Envoy's high-impact actions such as pushes or merges. |
| Fail-closed (AUT-06) | **Have** | Default-deny scopes, and sandbox launch fails if a boundary is missing. |
| Step-up, JIT elevation (AUT-07/08) | **Partial** | `grant admin` works as elevation. No step-up. |
| Proof-of-possession (AUT-09) | **Missing** | Bearer tokens only. Tailnet WhoIs binds node-to-hub traffic. Overkill beyond that. |
| Audit and correlation (OBS-01/02/06) | **Partial** | `audit-YYYY-MM.jsonl`, append-only, owner-readable (`docs/auth.md`). That doc says same-OS-user tampering is possible. There is no hash chain, and no run or trace ID across agents, jobs and mail. |
| Model gateway (budgets) | **Partial** | `protectorate-quota` tracks subscription usage, and mail has budgets. Harnesses call providers directly, so quotas are observed but not enforced per task. |
| Drift/thrash monitoring (OBS-03/04) | **Missing** | `protectorate-node-monitor` covers node health. Nothing watches agent behavior for loops or workaround attempts. |
| Registry and discovery (DIS-01..03) | **Have** | `sleeves.json` with parent labels, `protectorate-directory`, `-inventory`, node tiers. |
| Component mapping and effective access (DIS-05/06) | **Missing** | No view of which credentials, mounts and tokens each sleeve can reach. The sandbox plan is "inspectable before launch", which is half of it. |
| Immediate stop and revoke (RES-01) | **Have** | Device revoke cancels streams. Sleeve token revoked on kill. Emergency stop on the DEVICES tab (`ARCHITECTURE.md` line 586). |
| Quarantine, evidence preservation (RES-02/03) | **Partial** | Kill exists. No "block this harness version" switch and no snapshot-on-kill. |
| Rug-pull protection (RES-05) | **Missing** | Nothing detects a changed harness binary, MCP tool set or sleeve profile between runs. |
| Validation (VAL-01..04) | **Partial** | The e2e and fuzz policy covers the product itself. Nothing validates agent-produced artifacts. |
| Toxic combinations (CON-02) | **Missing** | Not checked. A sleeve with a provider login, host network and a writable repo is the textbook case. |

#### Top things to adopt (most valuable first)

1. **Egress allowlist per profile.** Give the sandbox its own network namespace with a default-deny allowlist. Block metadata endpoints and RFC1918 ranges except the hub, as the doc already floats. This closes the largest gap against the PocketOS and lab-escape failures.
2. **Hash-chained audit with a run ID.** Chain each entry to the previous one and keep the head hash on the hub or a node. Add one `run_id` carried through sleeve, job, mail and lease events. The change is small, and `docs/auth.md` already admits the weakness.
3. **Delegation subset check and per-task narrowing.** Enforce that a child's scopes are a subset of its minter's, and let Envoy spawn a sleeve with a narrowed scope. This is the "plans must not become permissions" rule.
4. **Approval gates for irreversible actions.** Pushes to protected branches, merges, and admin grants need a human decision from the phone app that the agent cannot supply. Chat approvals exist, but they are not tied to action classes.
5. **Rug-pull and drift detection (hash pinning).** Record hashes of harness binaries, MCP bridge allowlists and profile contents at spawn. Refuse or alert when they change. This is cheap, deterministic and fits "no LLM".
6. **Effective-access view.** One command or screen per sleeve showing its mounts, token scopes, credentials and reachable nodes. The plan inspection already gives most of the data. A toxic-combination lint can sit on top of it (host network + credentials + write access).
7. **Credential injection instead of in-sandbox secrets.** Mint sleeve tokens with expiry and renewal, and move toward the hub holding secrets and attaching them to calls. Provider logins are the hard part, since the project forbids wrapping the harness CLIs, so keep this to hub-side tokens.
8. **Enforced quota budgets per task.** Turn `protectorate-quota` from observation into a stop condition (pause or kill at a cap). This is the model-gateway idea without proxying inference.

#### Enterprise overkill for a single-owner private deployment

- Proof-of-possession credential binding (AUT-09), beyond the tailnet identity already used. (Note: the device-key design in A.6 adopts a light form of this for devices because the copyable device token is the real hole.)
- Step-up verification (AUT-07). The owner is the only principal, so it duplicates owner-only RPCs.
- Legal hold, retention and compliance evidence (OBS-05/06, most of RES-03).
- A microVM isolation tier.
- A formal supply-chain firewall and SBOM and provenance machinery. Pinning plus hashes is enough.
- Agent risk classes, exception owners and expiry (DIS-02/03).
- Monitoring agents. A deterministic loop detector covers this, and the project rejects LLM-based monitoring for deterministic work.
- Non-agent fallback workflows and adversarial testing programs (RES-04, VAL-01).

### A.9 layerfs versus our sandbox

Verdict: ignore layerfs as a dependency and port a few ideas. Cloned shallow to `references/layerfs`.

#### 1. What layerfs is

- **Product.** "Ephemeral Workspaces, Durable Shared History" (`README.md`). Each agent tool call mounts a full writable Workspace over committed history, runs, commits what changed, and unmounts.
- **Store.** One SQLite file holds Branches, Commits and content objects. Content is chunked and content-addressed with blake3 and zstd, append-only, and deduplicated across commits. Forking any Commit at any depth is cheap.
- **Per-workspace overlay.** Each mounted Workspace gets its own overlay SQLite file, copied from a template at mount and deleted at unmount (`docs/general/sandbox-cache-design.md`). A Commit turns that overlay into an immutable snapshot. Two commits touching the same file give a `Conflict`, unless a `.lww` rule says newest wins.
- **Kernel features.** It is not kernel overlayfs. The view is a userspace FUSE filesystem (vendored `fuser 0.18`, `core/vendor`). It compares itself against overlayfs on pjdfstest. The FUSE view omits FIFOs, sockets, device nodes, set-id bits and separate atime/ctime. It needs `/dev/fuse` and a sandbox daemon.
- **Language and deps.** Rust, about 242k lines across 14 crates (`core/crates`), plus a JS SDK and a DeepSeek Harness plugin (`dsh/`). Main deps are `rusqlite` (bundled), `blake3`, `zstd-sys`, `fuser` and `libc`.
- **Maturity.** v1.0.0 "released as source". The repo is a single squashed commit dated 2026-10-23. Its own warning: the Store runs in "Disposable" profile only, with no crash or power-loss durability, and "not the only copy of data you cannot rebuild".
- **License.** MIT.
- **Performance claims** (all single-sample, arm64 Mac in Docker):
  - 19.9x smaller than Btrfs+zstd for 157 snapshots of a real project (100 MB vs 1,995 MB).
  - 1.71x faster on the Cloudflare fs-bench.
  - Faster on smallfile and mixed random I/O against libfuse passthrough.
  - Slower on sequential write (0.31x), sequential read (0.81x) and random write (0.40x).

#### 2. Our current layout

The design is in `docs/rust-sandbox.md`. The code is in `rust/crates/protectorate-sandbox/src/runtime.rs` and `lib.rs`.

- **Rootfs.** A fresh tmpfs root with user/mount/PID/IPC/UTS namespaces. System and tool roots are bound read-only (`runtime.rs` ~1442-1650; the tool-root rule is at `lib.rs:111-135`).
- **Workspace.** One writable bind of the hub-managed clone (`<state>/workspaces/<name>`). Sibling repos are read-only grants (`lib.rs:66-71`).
- **Home.** A private home under the state root, bound writable.
  - `protectorate-sleeve-layout/src/lib.rs:224-276` creates empty `.cache/{pub,go-mod,go-build}`, `.cargo` and `.rustup`, and sets the env vars.
  - Nothing is copied from the owner's caches at launch.
- **Cache overlays.** These are real kernel overlayfs mounts, not FUSE.
  - `CacheOverlay{lower,upper,work,target}` is at `lib.rs:73-80`.
  - The lower is an operator-published, hashed, read-only generation. That covers the Go module download cache (3 GiB default, 8 GiB cap), the Flutter `bin/cache`, the Flutter tools directory and the Pub cache.
  - The upper and work directories sit in per-sleeve state at `<ns>/<name>/overlays`, outside the home.
  - Staging happens at `/.overlay/N` with inode and device pinning (`runtime.rs:784-853`). The mount is at `1259-1305`. Staging is detached and removed before exec (`1308-1323`).
  - The host must support overlayfs, checked at `runtime.rs:172` and `615`.
- **Temp space.** `/tmp` is a tmpfs, `/var/tmp` is a bind of it, and both share one memory-derived ceiling (`runtime.rs:184-195`).
- **Cleanup.**
  - The owner-only Kill with `dropCaches` removes `go-build`, `go-mod`, `pub`, `flutter` and `overlays` through descriptor-anchored paths (`protectorate-hub/src/sleeves.rs:1857-1890`).
  - The workspace, transcripts and credentials are kept.
- **Disk accounting.** A background sampler records workspace, home, cache and Go build cache bytes, counting uppers and ignoring work directories (`sleeves.rs:675`).
- **Disk quotas.** None found. The only hard disk bound is the tmpfs ceiling. Workspace and home growth is measured, not limited.
- **Checkpointing and diffing.** No snapshot, rollback or diff mechanism for a sleeve's workspace beyond git.

#### 3. What is worth adopting

1. **Checkpoint and rollback of agent changes.**
   - This is layerfs's best idea and our biggest gap. We already own a git clone per sleeve, so the cheap version is deterministic:
   - Before each resleeve or turn boundary, record a git tree object. `git add -A` into a private index, `write-tree`, and keep it under a hidden ref (for example `refs/protectorate/ckpt/N`).
   - That gives rollback, "what did the agent change" (a plain `git diff` between trees), and a handoff artifact.
   - It needs no kernel feature and no dependency. Build output and ignored files would need an explicit policy.
2. **Cheap fork of a workspace.** `git worktree add` or `clone --shared` already covers source. For ignored deps, `cp --reflink=auto` works on btrfs or xfs. Otherwise point to a per-sleeve overlayfs, which is what we already do for caches.
3. **Ephemeral per-sleeve layers.** We already have them, with the same upper/lower pattern as layerfs's overlay. We could extend overlay lowers to a read-only snapshot of the workspace tree for read-only helpers and speculative branches. Overlay upper directories are also free diffs: walking the upper shows exactly what changed (whiteouts included).
4. **Toolchain cache dedup.** Our published generations are copied and hashed, then shared by overlay lower. That already dedups across sleeves at the cost of a 3-minute publish (`rust-sandbox.md`, Flutter 1.88 GB). Content-addressed chunking would only help if we kept many generations. We don't.
5. **Disk quota.** Learn from the gap, not from layerfs. A per-sleeve project-quota or a bounded loopback upper is the real missing piece. layerfs doesn't solve it either.

#### 4. Recommendation

**Do not depend on it.** The reasons:

- It is a 242k-line stack (FUSE daemon, SQLite store, chunking) against an owner who values fewest dependencies and correctness.
- It has no crash durability. Our own rule is that backups and workspaces must survive.
- FUSE inside our unprivileged user-namespace sandbox adds a daemon and `/dev/fuse` exposure, and it degrades sequential I/O. Builds are sequential-I/O heavy.
- Its POSIX gaps (no set-id, device nodes or atime/ctime) are a risk for arbitrary toolchains.
- The repo is brand new, a single squashed commit with no history to judge stability by.
- Kernel overlayfs, which we already use and test, is the faster and more conformant path.

**Port ideas instead, in this order:**

1. Git-tree checkpoints per turn or resleeve, with rollback and diff exposed via CLI and MCP. This is deterministic code with no new dependency.
2. Use overlay uppers as the "what changed" report for cache and tool directories.
3. Add a real disk-quota story for workspace and home.
4. Revisit chunked dedup only if published cache generations multiply.

Treat layerfs as a reference for how to model Branch, Commit and Workspace for agents. The doc `docs/general/concepts.md` and the benchmark methodology are worth skimming. Do not read its 242k lines.

### A.10 Cloudflare "obsessive inference optimization" post

**Source:** https://blog.cloudflare.com/obsessive-inference-optimization/ ("Cutting our inference bill by 68% by being obsessive about optimization"). Read through WebFetch, which summarizes the page, so the raw text was not seen.

The article is not about GPU or serving-stack inference. It describes how Cloudflare cut the cost of a *hosted-API* AI code reviewer built on OpenCode, with Anthropic and Codex models. It says nothing about KV-cache engines, batching, or model placement. That limits how much applies to (b).

#### Techniques and measured gains

- **Sharing one cached prefix across repos** ("Sharing a cached prefix..."): First requests hit the cache only 14-26% of the time. A single breakpoint sat after a block containing the working directory and date. A marker plus a plugin hook split the block, repo-specific content moved out of the shared prefix, and the skills list was disabled. First-request cache reads rose to 91-99.5%. Cost per review fell from $2.47 to $2.02.
- **Cache economics** ("So, where were the tokens going?"): Cache writes cost 1.25x input, reads 0.1x, and entries expire after 5 minutes without a read. Before the work, spend was 43% cache writes, 34% reads and 23% output.
- **Prefix shared across parallel reviewers:** Three reviewers send identical first requests. One writes the cache and the other two wait up to 20 seconds for it, then read it. This saves about $0.09 each.
- **Fewer calls per session** ("Fewer calls, fewer dollars"):
  - Removing a tool the model kept calling but was always rejected: 57-76% of sessions, about 2.9k tokens per request.
  - Batching file reads into the first response (64% of turns made a single call).
  - Skipping reviewers on re-reviews.
  - Dropping the final summary turn: about $0.07 per job.
  - Together these saved about $0.20 per review.
- **Timeouts and cancellation:** Slow runs were no longer treated as provider failures and retried at the same limit. Security reviewer timeouts fell from 1.8% to almost none. In 358 of 446 sampled canceled jobs the work kept running, so jobs now poll their own status every 15 seconds.
- **Model and effort changes** ("Changing models and effort"):
  - Code changes accounted for $0.65 of the $1.67 drop, and model and effort changes for $1.06. The authors say this is not a controlled experiment.
  - Sonnet 5.5 was 47-62% cheaper per run than Sonnet 5 at the same per-token price, because it made far fewer calls (code quality: 1.7 vs 6.2).
  - High effort cost 61% more per run than medium and produced 58% more critical and warning findings. They kept code quality at high effort.
  - Medium effort on the coordinator finished full-tier jobs 20% faster.
- **Outcome:** Median review time fell from 4m09s to 1m32s, and p99 from 15m20s to 4m41s. Cost per review fell 68%, from $2.43 to $0.77.
- **Method:** Findings were judged blind by two models, with a third as tiebreaker. They also tracked how often authors acted on findings.

#### (a) Hosted harnesses

- **Cache hit rate on first requests.** Their closing advice is to confirm each session's first request actually reads from the cache. Our claude driver already parses `cache_read_input_tokens` and `cache_creation_input_tokens` (`rust/crates/protectorate-claude-driver/src/lib.rs:352`). We could surface a per-session "first-turn cache read ratio" and flag sessions that write where they should read. This is deterministic and cheap.
- **Cache breakpoint placement.** The Cloudflare bug was volatile data (cwd, date) inside the cached block. For us, do not inject per-sleeve or per-job text such as a node name, timestamp or lease id into system prompts or early context. We are hands-off toward harnesses (CLAUDE.md), so we can only influence what we send: the first user message and the project files such as AGENTS.md. Put volatile content last. This matches the "never bust native caches" memory rule.
- **5-minute TTL.** Idle gaps longer than 5 minutes lose the cache. The scheduler and chat layer should avoid pointless pings or resends that look like keepalives but don't read the cache. Whether resuming a session within the TTL is worth it is worth knowing for quota burn.
- **Quota accounting** (`rust/crates/protectorate-quota`): Cache writes cost 1.25x and reads 0.1x, so raw token counts mislead. If we estimate burn per subscription, weight the cache fields. Calls per session were the dominant cost lever, so track calls per job as well as tokens.
- **Subscription limits are not API pricing.** Their dollar numbers do not map directly onto subscription windows. The relative lesson (re-sent context dominates) should still hold.
- **Batching and streaming:** Irrelevant to us, because we don't control the harness's tool loop. Effort level is a user-set harness option, and the high-versus-medium data is a reason to expose it per job rather than a thing to automate.

#### (b) Local inference

Almost nothing in the article applies.

- The only transferable ideas are conceptual. A shared prefix across parallel workers (one warms, the others reuse) is what prefix and KV caching on a local server such as llama.cpp or vLLM would do. If we ever schedule local jobs, prefer routing same-prefix jobs to the same node and server instance.
- Model placement and routing are not covered.
- At single-owner scale, `accel.rs` inventory is enough until a local runtime actually exists. Defer this.

#### (c) General systems lessons for the Rust hub/node

- **Cancellation must propagate.** 80% of canceled jobs kept running. Check that our job-cancel path actually kills the sandbox and harness process tree (`protectorate-sandbox`, `protectorate-sleeve-runner`) and add an e2e test for it. The old Go tree had `job_cancellation_test.go`; verify the Rust side covers it.
- **Distinguish timeouts from provider failures.** Don't retry a slow job with the same limit. Classify the failure in the job result.
- **Measure before optimizing.** Their wins came from telemetry that split spend by category. Per-job counts of calls, cache reads and writes would give us the same view cheaply.
- **Allocation and I/O:** The article has nothing on these.

**Priority for us:** first-turn cache-read telemetry, volatile content kept out of prompt prefixes, cache-weighted quota math, and cancellation propagation checks. Everything else is low value at our scale.

### A.11 Verus versus Bend

#### Verdict

Verus is worth a small, bounded pilot on pure logic cores. Bend has no role in protectorate. The cheap tools (fuzzing, Miri, Kani, loom or shuttle) give most of the assurance for far less cost. Prefer them first.

#### 1. What each is

**Verus** (https://github.com/verus-lang/verus). It is an SMT (Z3) based deductive verifier for a subset of Rust.
- Activity: about 3.4k stars and about 4.8k commits. The README says it is under "active development", with features missing and documentation incomplete, and it directs users to Zulip for help. Commit dates or release tags could not be retrieved (the GitHub page showed none), so check those before committing.
- Toolchain: it pins its own rustc via `rust-toolchain.toml`, so you build or download Verus and use its `verus` driver, not stock cargo. It needs Z3, and `cargo verus` exists for crate builds. Verified code lives in `verus! { }` blocks.
- How you use it: you write `requires` and `ensures` clauses, loop invariants, and `spec`, `proof` and `exec` functions. Ghost code is erased at compile time. The `vstd` library supplies specs for Vec, Option and HashMap-like types.
- What it supports (from knowledge, not re-verified this session): enums, structs, generics, traits, integer overflow reasoning, and `unsafe` through ghost permission tokens such as `PPtr`.
- What it does not support well: async/await, most of std beyond what `vstd` specs, trait objects, and complex closures. It also handles floats and I/O poorly, so you verify pure logic and treat syscalls as axioms.
- Coexistence: you can keep a verified module in a separate crate or feature-gated module. Normal builds strip the ghost code, or you copy the exec code.
- Costs: SMT proofs can be brittle and slow. Expect a proof-to-code ratio of roughly 3:1 to 10:1 for stateful logic.

**Bend** (https://github.com/HigherOrderCO/Bend, the Bend 2 line). It is a high-level language that compiles to C, Metal, CUDA or JS.
- Activity: about 24k stars and about 3.4k commits. The README calls it young and says to expect bugs. It says Bend 1 programs and HVM do not carry over. It says the compiler is "99% AI-written" and not fully audited.
- GPU: parallel execution needs clang 19+ and Metal or CUDA 12. Only one GPU per program is supported.
- Performance: the README claims "as fast as C on CPU", but those are targets and were not verified.
- I/O: effects cover files, TCP, UDP, channels and spawn. There is no TLS, HTTP or JSON. There is one event loop, no multi-machine execution, no Windows, and no debugger or test framework.

#### 2. Fit for protectorate

**Bend: not relevant.** We are an I/O-bound orchestrator with a Rust codebase of about 150k lines. Bend has no Windows or Android support, and the owner wants the fewest dependencies. No genuine niche found. It does not offer verification.

**Verus candidates, ranked by value per cost:**

1. **WireGuard `ReplayWindow`** (`rust/crates/protectorate-tailnet-wg/src/lib.rs:446`).
   - It is about 50 lines of pure bit and shift arithmetic over `[u64; 32]`.
   - The properties are crisp: no counter accepted twice, none accepted below `highest - REPLAY_BITS`, and `mark` after `can_accept` is monotone.
   - This is the best Verus target by far, at a cost of days. (Note: this crate is being removed with the overlay, so this target disappears.)
2. **Wire framing parser** (`protectorate-wire/src/lib.rs` and `streaming.rs`).
   - The properties are no panics, no out-of-bounds access, and length-prefix arithmetic that cannot overflow.
   - It is parser code, so fuzzing is the better first tool. Verus is optional here.
3. **Lease state machine** (`protectorate-lease/src/lib.rs`, 618 lines).
   - The properties are mutual exclusion via `conflicts`/`blocked`, queue ordering, and no double grant.
   - Persistence, serde and time are entangled with the logic, so you would have to extract a pure core.
   - Cost: weeks. A model-checked spec (see section 3) captures most of the value for much less.
4. **Job state machine** (`protectorate-hub/src/jobs.rs`, 2.2k lines) and **mailbox** (`protectorate-mailbox/src/lib.rs`, 1.3k lines).
   - The logic is mixed with IO and locks, so Verus is a poor fit.
   - Extract the transition table and test it instead.
5. **TCP stack** (`protectorate-tailnet-tcp/src/lib.rs`, 1k lines).
   - Sequence and window arithmetic with wrap-around is a classic bug source.
   - Verus could handle the wrap arithmetic, but the surrounding timers and buffers are messy.
   - Do a fuzz-differential test against the kernel's TCP stack first. (Also being removed with the overlay.)
6. **Atomic file replacement** (`protectorate-fsutil/src/lib.rs`, 113 lines).
   - It is syscall sequencing (write, fsync, rename, directory sync).
   - SMT proofs cannot model crash semantics. Use fault-injection e2e tests.
7. **Sandbox mount plan validation** (`protectorate-sandbox/src/lib.rs:560-1097`).
   - It is path and string logic over `Path`, which Verus's std specs do not cover.
   - It is security-critical, so use fuzzing plus a property check ("no mount target escapes the allowed roots").
   - The unsafe syscalls in `runtime.rs` are not provable in Verus. Their correctness comes from the kernel.

#### 3. Cheaper alternatives

- **cargo-fuzz / libFuzzer** (https://rust-fuzz.github.io/book/cargo-fuzz.html). This is the owner's stated tool. No fuzz targets in `rust/` today. Priorities are the wire parser, the packet decoders in `protectorate-tailnet-packets`, the WG handshake and transport decrypt, the mount-plan validators, and the mailbox JSON and `gotime` parsers.
- **Kani** (https://github.com/model-checking/kani). It is bounded model checking, with proof harnesses written as `#[kani::proof]` functions. It handles `unsafe`, arithmetic overflow and panics. It fits `ReplayWindow` and TCP sequence arithmetic and costs far less than Verus. It exhaustively checks bounded inputs, which is the closest thing to a proof you will get cheaply.
- **Miri** (https://github.com/rust-lang/miri). It detects undefined behavior in unsafe code. It cannot run real syscalls, so it covers little of `runtime.rs`. It is useful only for pure-memory unsafe elsewhere.
- **loom / shuttle** (https://github.com/tokio-rs/loom, https://github.com/awslabs/shuttle). They explore thread interleavings. Use them for the lease table, whose admission lock and `transaction` are concurrency-sensitive.
- **proptest** (https://github.com/proptest-rs/proptest). Property tests overlap with fuzzing. Skip it, since a fuzz target with a structured input gives the same thing.
- **Creusot / Prusti** (https://github.com/creusot-rs/creusot, https://github.com/viperproject/prusti-dev). They are in the same family as Verus, with a similar cost and a smaller community. Choose Verus if you want proofs.
- **TLA+ or P** (https://github.com/tlaplus/tlaplus, https://github.com/p-org/P). Use these for design-level questions: the lease handoff protocol, job cancellation races, and rekey and reconnect behavior. A TLA+ spec of the lease and job state machines would have the best assurance per hour, but it checks the design, not the Rust code.

#### 4. Recommended plan

This is consistent with the owner's rule "e2e + fuzz only". Fuzzing and model checking are in the same spirit: they test real code against properties, with no mocks and no unit tests. A Kani harness or a Verus proof is a stronger form of fuzz target, not a unit test.

1. **Fuzz first, all CI-seeded:** wire framing, packet decoders, WG decrypt and handshake, mount-plan validators, JSON and time parsers. Add a differential fuzz target for TCP sequence handling.
2. **Kani for three kernels:** `ReplayWindow`, TCP sequence and window arithmetic, and the wire length arithmetic. Keep the harnesses in a non-default feature or directory so the normal build is untouched.
3. **loom** on the lease table's concurrent paths.
4. **TLA+** for lease and job lifecycle design only if a real race bug shows up.
5. **Verus:** pilot on `ReplayWindow` only, as a separate crate, to see whether the team can sustain it. Do not adopt it more widely unless that pilot goes smoothly.

One discrepancy to resolve: there are about 772 `#[test]` functions under `rust/crates`, such as the one at `protectorate-fsutil/src/lib.rs:91`. That contradicts the "no unit tests ever" rule. (Resolved: owner said remove unit tests.)

The Verus and Bend facts above come from the GitHub landing pages. A web search for Verus's async, trait and release status returned nothing useful, so the Verus language-subset claims are from memory and should be checked against its documentation before committing to a pilot.

### A.12 Rust dependency minimization audit

The workspace has 191 external packages on the linux-gnu normal-dependency graph. Cargo.lock lists about 246 because it includes Windows/Android/macOS-only crates.

#### 1. Direct external dependencies (workspace-level)

"Solo" is the number of packages that go away if that dep alone is removed (the closure that nothing else reaches), on the linux graph. "Closure" is the total packages it pulls in.

| Crate | Used by | Used for | Solo / Closure |
|---|---|---|---|
| rcgen 0.14 | derp; the other 4 users are dev-only | self-signed cert in `derp/src/cert.rs` | **24 / 44** |
| axum 0.8 (http1, json, tokio, ws) | hub, mcpbridge, node, node-termux, node-windows, server, wire | HTTP server and websockets (terminal) | **17 / 77** |
| prost-reflect (serde) | hub, mcpbridge, wire | protobuf-to-JSON (the Connect protocol) | 9 / 19 |
| cap-std 3 | backup, handoff | sandboxed fs access | 9 / 14 |
| toml 0.8 | directory | parsing project manifests (`manifests.rs`) | 8 / 15 |
| snow (default-resolver, risky-raw-split) | tailnet-member, tailnet-wg | Noise protocol (WireGuard handshake) | 6 / 33 |
| chrono-tz | desk-policy only | desk time windows | 4 / 13 |
| ureq 3 (rustls) | 9 crates | blocking HTTP client | 3 / 29 |
| crypto_box | derp, derp-client | DERP box crypto | 3 / 24 |
| png | desk-node only | desktop screenshots | 3 / 10 |
| tempfile | backup, hub, node-windows, sandbox, server, xfer | temp files | 3 / 9 |
| zstd | xfer | transfer compression | 3 / 2 |
| zip 0.6 (deflate) | hub, node | artifact zips | 2 / 8 |
| hpack | tailnet-member | HTTP/2 headers (`log 0.3.9` duplicate) | 2 / 2 |
| tokio-rustls | mcpbridge, node-termux, node-windows, server | TLS listeners | 1 / 26 |
| rustls-pemfile | 4 crates | reading PEM files | 1 / 2 |
| fs2 | backlog, backup | file locks | 1 / 1 |
| qrcodegen | pairing | QR codes | 1 / 0 |
| glob | node | artifact glob patterns | 1 / 0 |
| tungstenite (handshake, rustls-tls-native-roots) | cli only | websocket client for `attach.rs` | 0 / 41 (shared with axum ws) |
| rustls (ring, std, tls12 - but see below) | cli, connect-client, derp, derp-client, tailnet-member | TLS | 0 / 13 |
| chacha20poly1305, blake2, sha2, curve25519-dalek, crc32fast, subtle, zeroize | various | crypto and checksums | 0 / 0-17 (shared) |
| chrono (clock, serde, std) | 19 crates | timestamps | 0 / 9 |
| tokio | 11 crates | async runtime | 0 / 12 |
| serde, serde_json (raw_value) | 22 / 40 crates | JSON | shared |
| base64 0.22 | 8 crates | encoding | shared |
| getrandom 0.2 | 16 crates | randomness | shared |
| flate2 (rust_backend), futures-core, futures-util, http, libc, percent-encoding, socket2, webpki-roots, windows-sys | various | misc | 0 (shared) |

Platform-specific libraries used by the node crates, such as windows-sys and libc, are mostly target-gated. node-termux and node-windows show 0 on the linux graph for the same reason.

#### 2. Duplicates and redundancy

- **Two TLS crypto providers.** Cargo.lock had both `ring` 0.17.14 and `aws-lc-rs` 1.18.2 with `aws-lc-sys` 0.46.0. `protectorate-cli` and `protectorate-connect-client` used rustls with default features, which turned on `aws_lc_rs` (fixed in `5a1fa24`; tokio-rustls defaults were also a source).
- **rcgen is a production dependency only in `derp`.** The other four are dev-only. It adds 24 packages (asn1-rs, nom, time, yasna, pem and others) and also brings a duplicate `base64 0.23` and `time`. It is used for one call, `generate_simple_self_signed`.
- **Duplicate versions** (`cargo tree -d`):
  - base64 0.22 and 0.23. The 0.23 copy comes from ureq-proto and pem.
  - getrandom 0.2, 0.3 and 0.4. 0.3 comes from rand 0.9 (tungstenite), and 0.4 from tempfile.
  - rand_core 0.6 and 0.9.
  - miniz_oxide 0.8 and 0.9. 0.8 comes from png and 0.9 from flate2.
  - log 0.3 (hpack) and 0.4.
  - syn 2 and 3.
- **Two websocket stacks.** axum's `ws` feature pulls `tokio-tungstenite` and `tungstenite`. The cli also depends on `tungstenite` directly, with `rustls-native-certs`, `rand` 0.9 and `sha1`. These are the same crate, so no duplicate library. The extras are the native-certs and rand 0.9 pieces.
- **Three HTTP stacks:** axum/hyper (server), ureq (client) and tungstenite handshake (cli). All blocking client calls are consistently ureq, and none are hand-written.
- **JSON:** `serde_json` and `prost-reflect` (with its serde feature) coexist. prost-reflect is the Connect-JSON bridge, so it is needed unless the protocol changes.
- **Compression and archives:** flate2 is used by the release crate, ureq, zip and png. Separately, zip 0.6 (deflate), zstd and flate2 are all present. zstd needs a C library (`zstd-sys`).
- **hex, rand (direct), sha1, regex, clap, anyhow, tracing:** there is no direct hex, rand or clap use. hex is hand-written. The cli uses hand-rolled args.
- **Crypto split across crates.** snow brings AES-GCM, which is not used elsewhere. crypto_box brings salsa20.

#### 3. Minimization plan, ranked by packages removed per unit of effort

| # | Change | Packages removed | Effort |
|---|---|---|---|
| 1 | Turn off rustls default features in cli and connect-client so only ring remains | aws-lc-rs, aws-lc-sys and about 2-4 build crates (about 4-6) | trivial (done) |
| 2 | Replace the single rcgen call in derp with a pre-generated or runtime-minted cert, or move cert generation into one crate that already uses ring | up to 24 | moderate (needs x509 DER; do not hand-roll TLS) |
| 3 | Shelve desktop control (section 5) | 8 | easy (feature gate) |
| 4 | Replace toml with simple key scanning in `directory/manifests.rs` | 8 | small, since it only reads dependency names. It is lossy parsing, so correctness risk is low |
| 5 | Replace cap-std in backup and handoff with canonicalize-and-prefix checks | 9 | moderate, and this hurts security. Cap-std exists to stop path traversal and symlink escapes. Not recommended |
| 6 | Drop fs2 and use `File::lock()` (stable since Rust 1.89) | 1 | trivial |
| 7 | Drop glob and use a small hand-written matcher | 1 | small |
| 8 | Use `tempfile`-free temp dirs (`std::env::temp_dir` plus random name) | 3 | small, but fine only for tests |
| 9 | Align base64 versions so ureq-proto and pem share 0.22 | 1 | only possible by upgrading to 0.23 everywhere (trivial) |
| 10 | Replace the cli's tungstenite client with axum's own ws pieces | rand 0.9, rustls-native-certs, getrandom 0.3 and about 3 more | moderate |
| 11 | Swap zstd for flate2/deflate-only transfer compression | 3, plus the C toolchain | moderate; it changes wire format and ratio |
| 12 | Unify zip with flate2 / drop png | 2-3 | small once desk is shelved |
| 13 | Replace axum with a hand-rolled HTTP/1.1 and ws server | 17 | large, and risky: it is the whole API surface. Not recommended |

Do not hand-roll: ring/rustls, chacha20poly1305, blake2, curve25519-dalek, snow, crypto_box, sha2, subtle, cap-std, rand, getrandom. Items 1, 3, 4, 6 and 7 are safe and remove about 20-25 packages combined. Item 2 plus item 10 are the next largest wins (roughly 30 more).

#### 4. Binary sizes

Only one release binary existed on disk: `target/release/protectorate-rs-termux`, at 17,586,392 bytes (about 16.8 MiB). Closure sizes for the other bins (linux): server 190 packages, cli 116, mcpbridge 104, sleeve-runner 21.

#### 5. Desktop-only crates

`protectorate-desk-node` and `protectorate-desk-policy` exist only for desktop control, and `DeskService` lives in `rust/crates/protectorate-hub/src/desk_service.rs`. The cli also has `cli/src/desk.rs`. The hub, node, node-windows and server crates depend on the two desk crates, and hub and node also depend on desk-policy.

Shelving them drops these 9 packages: `protectorate-desk-node`, `protectorate-desk-policy`, `chrono-tz`, `phf`, `phf_shared`, `siphasher`, `png`, `fdeflate` and `miniz_oxide 0.8`. That also removes a duplicate `miniz_oxide` version. Total about 1,839 lines of Rust. The Windows-specific code in `desk-node/src/windows.rs` would go too.

## Appendix B: references

External:

| Topic | URL |
|---|---|
| herdr | https://github.com/herdrdev/herdr |
| agentbaseline reference architecture | https://agentbaseline.org/reference-architecture |
| agentbaseline controls | https://agentbaseline.org/controls |
| agentbaseline source | https://raw.githubusercontent.com/agentbaseline/agentbaseline/main/software-factory/software-factory.md |
| layerfs | https://github.com/Ephemeral-AI-Lab/layerfs |
| Cloudflare inference post | https://blog.cloudflare.com/obsessive-inference-optimization/ |
| Verus | https://github.com/verus-lang/verus |
| Bend | https://github.com/bendlang/bend (also https://github.com/HigherOrderCO/Bend) |
| Headscale OIDC | https://headscale.net/stable/ref/oidc/ |
| Headscale ACLs | https://headscale.net/stable/ref/acls/ |
| Tailscale ACL syntax | https://tailscale.com/kb/1337/acl-syntax |
| biscuit-auth (Rust) | https://github.com/eclipse-biscuit/biscuit-rust |
| innernet | https://github.com/tonarino/innernet |
| boringtun | https://github.com/cloudflare/boringtun |
| Nebula | https://github.com/slackhq/nebula |
| NetBird | https://github.com/netbirdio/netbird |
| cargo-fuzz | https://rust-fuzz.github.io/book/cargo-fuzz.html |
| Kani | https://github.com/model-checking/kani |
| Miri | https://github.com/rust-lang/miri |
| loom | https://github.com/tokio-rs/loom |
| shuttle | https://github.com/awslabs/shuttle |
| proptest | https://github.com/proptest-rs/proptest |
| Creusot | https://github.com/creusot-rs/creusot |
| Prusti | https://github.com/viperproject/prusti-dev |
| TLA+ | https://github.com/tlaplus/tlaplus |
| P | https://github.com/p-org/P |
| RFC 9449 (DPoP) | https://www.rfc-editor.org/rfc/rfc9449 |
| RFC 8628 (device authorization grant) | https://www.rfc-editor.org/rfc/rfc8628 |

Local clones under `references/` (gitignored, read-only): CodexBar, cortical-stack, cortical-stacks, factory, herdr (refreshed to 43b36b6 this session), layerfs (shallow clone this session), rea, secondary, security-audit-skill, t3code, tincan-plugin, UniiChat.

Repository documents referenced: `docs/rust-migration.md`, `docs/rust-sandbox.md`, `docs/rust-termux.md`, `docs/auth.md`, `docs/install.md`, `docs/mcp.md`, `ARCHITECTURE.md`, `PROBLEMS.md`, `CLAUDE.md`, `DEV.md`.
