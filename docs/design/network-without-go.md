# Network without Go

Investigation, 2026-11-01. This is a proposal and an isolated experiment; SPEC 3.2 and the
shipping headscale/member path remain in force until the owner chooses a migration. The worktree
was inspected, not the live hub or the other machines. External claims below link to primary
documentation. No WAN listener or public relay is assumed.

## Recommendation

Use the system OpenSSH client on the hub and an OpenSSH server on each node. The hub opens an
SSH session and the node runs `protectorate-node serve --stdio` as the forced command for a
dedicated hub key. Carry the existing `pt-frame` protocol over that session. Maintain an
owner-approved fleet directory of stable IDs, names, reachable addresses and pinned host keys.
Use the LAN and the owner's already-established site-to-site WireGuard routes first; an existing
tailnet is another private route if available. Neither route requires Protectorate to implement a
VPN. This removes Go **from this repository** after a full migration; system OpenSSH, an existing
Tailscale installation or VPN software remain external dependencies and may themselves contain Go
or other languages. The existing Flutter applications remain Dart; "Rust-only" here refers to the
hub/node repository code no longer needing Go, not the removal of the Flutter client.

This fits the reported topology better than a second overlay: agent-world and officewin are on
the LAN, and msr1-kids has a site-to-site route. Actual routes, return paths, host firewalls and
officewin's configuration have not been checked in this investigation. A node without a route is
offline until it acquires private connectivity or a separate private jump/relay is designed. SSH
does not reproduce automatic hole punching, relay, roaming or virtual addressing. No solution can
reach a node outside these private networks while also forbidding any new WAN exposure.

The main qualification is platform work: today's stdio bridge is Unix-only and today's node
service owns its socket and job privileges differently from an sshd account. Do not remove the
current network until Windows, local IPC, enrollment, firewall enforcement and recovery are
implemented and tested. If automatic traversal from otherwise unreachable nodes becomes a hard
requirement, evaluate a private iroh relay or a separate restricted SSH jump host again; either
adds a reachable process and requires an explicit trust-boundary decision.

## What exists today

| Requirement | Current implementation |
|---|---|
| Identity, address, name | `pt-fleet/src/headscale.rs` configures sequential `100.64.0.0/10` and `fd7a:115c:a1e0::/48` addresses. Headscale stores node IDs and approved names. MagicDNS and local DNS override are off; the member's own DIAL accepts a tailnet address or peer name. `poller.rs` uses the first assigned IP. Public `Node.id` is the headscale ID, while `Hello` also carries the agent's separate node ID and journal epoch. |
| Enrollment | `protectorate-node join` launches member, displays its registration ID, name and headscale control-key fingerprint as QR/text, and waits. An owner scans/pastes it. `pt-fleet/enroll.rs` checks the fingerprint against the local headscale, registers, tags and renames through the local CLI, and records approval. The node pins the control key on later starts. This is not a general SSH host-key enrollment mechanism. |
| Connectivity | tsnet runs in userspace, without a TUN or root. Members exchange endpoints through headscale, try direct WireGuard/disco paths and fall back to the headscale-hosted DERP relay. The DERP map is private and IP-pinned; its STUN port is disabled, so do not promise generic STUN-assisted traversal from the present configuration. The node emits a disco ping after restart. Direct versus relay and online state are observable through member STATUS. |
| Direction and trust | `go/policy.hujson` allows `tag:hub` to `tag:node:7475`; a node member refuses DIAL, and the hub member has no peer listener. The node's member checks the peer tag and the agent checks the forwarded header again. Nodes still contact the separately sandboxed headscale control/DERP service. This policy constrains the overlay; it does not by itself isolate other physical LAN paths to the hub. |
| Keys | Headscale distributes device public keys and enforces membership; member persists tsnet keys and checks the pinned headscale control fingerprint at each start. Headscale can explicitly expire/delete a node. WireGuard session rekeying is not the same as rotation of the long-lived node or control identity. There is no Protectorate owner-facing scheduled identity-key rotation workflow in the inspected code; do not assume a particular expiry period for these registered/tagged nodes. A changed or missing control pin fails closed and requires recovery. |
| Jobs and artifacts | The hub records effects before sending `SubmitJob`; the node journals before exec and retains attempt/token tombstones. The hub opens the connection, polls `JobStatus`, and pulls bounded `FetchLog` and `FetchArtifact` chunks plus end metadata. The hub checks artifact size and SHA-256 and treats results as node claims. A session can end while the resident node process continues the job; journal epochs and tokens support reconciliation after reconnect. `pt-frame` bounds individual frames and treats node bytes as untrusted. |

The current Rust fleet layer supervises headscale and member, manages enrollment, polls headscale
and member for status, and exposes a `NodeClient` bound to a Unix stream from member. The node's
`member.rs` supervises its tsnet child; `join.rs` implements the scan flow. The design's measured
stripped arm64 binaries were approximately 49 MB for headscale and 19 MB for member before
compression; these are historical measurements from `network-trust-language.md`, not new builds.

## Options and dependency accounting

Counts below are **not binary sizes or exact incremental production dependencies**. For repeatable
apples-to-apples context, count each registry package/version in the published crate's `Cargo.lock`,
including all recorded targets, dev/build dependencies and optional features. Download the exact
crate from `https://crates.io/api/v1/crates/NAME/VERSION/download`, extract its `Cargo.lock` to
stdout and count `source = "registry+..."` lines. These lockfile totals overstate a particular
minimal build, can include several versions of the same crate, and overlap the existing workspace.
Re-resolve and count the actual target/features before adopting any crate. The present workspace
lock has 16 registry package/version entries; `go/go.mod` lists 139 direct and indirect Go module
requirements. Neither number counts an OS's native libraries or programs.

| Option | Dependency footprint | Platforms, access and operational gaps |
|---|---|---|
| System OpenSSH over existing private routes | **0 new Rust registry crates.** The OS/Termux `ssh`, `sshd`, crypto libraries and their updates are real external dependencies. An SSH client process is started by the hub for a session; sshd and the node process run on the node. | Linux and macOS have OpenSSH packages; Windows 11 has an optional OpenSSH Server feature, often needing administrator setup, service configuration and firewall rules. Termux can run a user-owned sshd on a high port, subject to Android background limits. A transport-only Unix setup can be unprivileged, but the current system-service presence detection and privileged maintenance jobs still need their existing privileges. Windows stdio is not implemented. sshd must be reachable, configured and patched. [OpenSSH](https://man.openbsd.org/sshd), [Windows installation](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse), [Termux package issue](https://github.com/termux/termux-packages/issues/16057). |
| SSH over the owner's existing tailnet | **0 new Rust registry crates** in Protectorate; an independently installed tailnet client and its control/relay infrastructure remain dependencies and may contain Go. | Where already installed and permitted, the tailnet can provide stable private addresses and its own traversal while SSH still pins the node and authenticates the hub. Its ACLs, device enrollment, client service/privileges and possible external coordination are managed outside Protectorate; the product cannot assume they enforce SPEC's one-way rule. This does not satisfy a requirement that all runtime software, rather than this repository, be Go-free. |
| Rust SSH, e.g. `russh` 0.63.3 | **396** entries in its published lock; not an actual selected-feature or incremental count. The library supports client/server and selectable crypto backends. | Potentially portable, and an embedded server could remove system sshd, but we would own authentication policy, key parsing/rotation, channel limits, server hardening and lifecycle on every OS. It adds a substantial crypto/async supply chain without solving NAT. Windows and Termux integration would still require proof. [russh](https://github.com/Eugeny/russh). |
| Our own kernel/native WireGuard hub-and-spoke | **0 new Rust crates** if configuring the OS tools/driver; OS kernel, tools and Windows driver/service are additional dependencies. A Rust control plane and secure RPC are still needed. | Static per-node overlay addresses and WireGuard public keys are straightforward, but peers behind NAT need to initiate to a privately reachable hub and maintain a mapping, for example with `PersistentKeepalive`. This is not a relay or hole-punching system. Kernel interfaces/routes/firewall configuration need elevated privileges or equivalent capabilities; Windows needs driver/service setup, and Android needs the VPN API/app, not an ordinary rootless Termux TUN. WireGuard is bidirectional: restrict hub services separately; source IP alone is not the node protocol's authorization. [WireGuard quick start](https://www.wireguard.com/quickstart/), [Windows project](https://www.wireguard.com/install/). |
| `boringtun` 0.7.1 userspace WireGuard | **153** entries in its published lock. | Library supports Linux, Windows, Android and other targets, but implements WireGuard packets, **not** an IP/TCP stack or enrollment, addressing, NAT discovery or relay. A TUN still needs privileged setup; rootless application-only use needs an additional userspace network stack and substantial integration. Its CLI supports Linux/macOS, not Windows. [BoringTun](https://github.com/cloudflare/boringtun). |
| `iroh` 1.2.0 | **489** entries in its published lock; the relay server is an additional component and its own dependency footprint is not included. | Rust QUIC streams, public-key endpoint identity, direct paths and fallback through a relay; library APIs can avoid a TUN and root. Runs on desktop/mobile targets, but this particular Termux/Windows integration is unproved. Must implement owner approval, authorization, names, endpoint persistence, service lifecycle and one-way policy. Explicitly disable public discovery/default public relays and configure a private relay or direct addresses; the private relay must itself be reachable through an allowed private network. Its endpoint/relay connections introduce a node-facing service boundary like control/DERP. [iroh documentation](https://docs.rs/iroh/1.2.0/iroh/), [private relay configuration](https://github.com/n0-computer/iroh/blob/main/iroh/src/endpoint.rs). |
| `innernet` 2.0.0 | **179** client and **217** server published-lock entries, counted separately and not additive because they overlap. | Supplies coordination, addresses, WireGuard and invitations, but uses system WireGuard interfaces and administrative setup. Upstream describes Linux/macOS as tested and other platforms as limited; no demonstrated Windows/Android path here. It is not a drop-in userspace socket or a DERP-equivalent relay, and its CIDR associations do not automatically implement the required one-way trust. [innernet](https://github.com/tonarino/innernet). |
| `libp2p` 0.56.0 | **429** entries in its published lock, not a selected-feature count; relay service cost is additional. | Rust transport building blocks, peer IDs, discovery, relay and hole punching are configurable, not an approved fleet product. We would implement pairing, private discovery, relay deployment, policy and recovery, then verify desktop/mobile behavior. More moving pieces than this topology needs. [rust-libp2p](https://github.com/libp2p/rust-libp2p). |
| Official `tailscale-rs` reference at `5dfd5a0` | Reference workspace lock has **650** registry package/version entries, including unrelated workspace/dev/optional features. The earlier design measured about **218** external crates in one normal dependency tree; this is a different scope. | The inspected 0.6.1 README marks NAT traversal incomplete, private DERP and Android unsupported. It still needs a coordination server, so it does not remove headscale Go from the hub. Reassess when private relay and Android work. [tailscale-rs](https://github.com/tailscale/tailscale-rs). |

For every alternative, counts include neither a complete deployment nor the cost of integrating
the frame protocol and fleet state. In particular, a small-looking WireGuard library does not
provide the node-facing TCP stream that tsnet supplies today.

## SSH enrollment, addressing and restrictions

The fleet directory should map a Protectorate-owned stable ID and owner-chosen name to an ordered
list of private hostnames/IPs, port, SSH account and pinned host public keys. An address is a
locator, never the identity. DHCP reservations or private DNS keep the common case simple; a
changed address must still present the approved key. Do not trust an unverified `ssh-keyscan` or
accept-on-first-use. Pass a private known-hosts file, `StrictHostKeyChecking=yes`, an explicit
identity, `IdentitiesOnly=yes`, batch mode, no agent/X11/port forwarding and no user ssh config.
OpenSSH's [host-key options](https://man.openbsd.org/ssh_config) support this; verify the exact
Windows server's accepted restrictions and command quoting.

Enrollment needs an authenticated exchange **in both directions** without a node-to-hub API:

1. On the node, display a QR/text record containing its proposed name, addresses, SSH account,
   port and complete public host key plus fingerprint. The owner verifies/scans this locally and
   approves it in the hub app. The hub creates a private per-node client key in hub-owned state.
2. Convey that hub public key to the node through an authenticated owner-controlled channel,
   such as a second QR/transfer or an install bundle, and verify its fingerprint on the node.
   A single node-to-phone scan cannot deliver the hub key back to an otherwise disconnected node.
   Install the key with a per-key forced command and `restrict`/equivalent forwarding and TTY
   restrictions; do not alter other users' keys or sshd behavior. The hub private key never
   leaves the hub. Record approval only after a pinned connection succeeds.
3. On a changed node host key, fail closed and require owner verification. For planned rotation,
   add an owner-verified new host key while the old one remains valid, test it, then retire the
   old key; recovery after loss requires a fresh out-of-band verification. For the hub client key,
   install and test the new authorized public key before removing the old one. An offline or
   compromised node cannot be relied upon to revoke a leaked hub key; also deny it in the hub
   directory and constrain access at the network boundary. Automatic `UpdateHostKeys` is not a
   substitute for owner approval in this proposal.

Only the owner may enroll, approve, rotate or revoke. Protect fleet keys/config from sleeves and
from ordinary node job identities. A node running a root-equivalent job can change its own sshd;
separate account/file permissions alone do not make a compromised node trustworthy. Continue to
treat every job result as an untrusted claim. Session rekeying is handled by SSH, but long-lived
host/client keys need the lifecycle above.

Direct SSH opens no node-initiated hub listener. It also **does not enforce** the whole one-way
rule on the underlying LAN: a node may independently reach hub or Traefik listeners. Enforce
stateful return-only rules for node zones at every relevant hub/proxy interface, including IPv6,
the site-to-site VPN, and proxy routes; use proper network isolation/anti-spoofing where a hostile
peer can impersonate another LAN address. Do not equate an SSH forced command with a host firewall.
Check existing LAN paths separately from headscale's overlay policy.

For an unreachable node, prefer the owner's private WireGuard/tailnet. As a future alternative, a
node may maintain an outbound SSH reverse tunnel to a **separate**, private jump host, with a
dedicated key, no shell, no general forwarding, a fixed loopback-only `permitlisten`, and a hub
account allowed to reach only that port. The hub connects through the jump and separately pins
both jump and node host keys; node SSH remains end-to-end. The jump is an extra node-facing
listener and failure domain, and must not be a listener on the hub under SPEC 2.4. It is not
implemented or tested here. Reconnection, node-to-jump abuse and network isolation need review.

## Proposed full migration, after approval

1. **Fleet state and `pt-fleet`:** introduce durable IDs and an owner-only directory with name,
   approved host keys, private endpoint(s), account and client key. Map existing headscale IDs to
   durable IDs before removing headscale so old jobs/history still refer to the same node. Replace
   headscale enrollment/admin and member STATUS/DIAL/supervisors with SSH process lifecycle,
   pinned connections, timeouts and health derived from actual `Hello`/polls. Adapt `NodeClient`
   to separate SSH stdin/stdout with cancellation, per-call deadlines and the existing poisoned
   stream, frame bounds and token/epoch checks. Retain the poller and job RPCs, but replace their
   assumptions about headscale IDs, peer online state and IP allocation.
2. **Node:** replace `join.rs` with the two-way owner enrollment and retire `member.rs`, its blob,
   configuration and supervision. Keep the resident agent and `pt-frame`; make stdio bridge to an
   already-running service. The existing Unix mode-0600 socket cannot be shared blindly between a
   system service and an sshd account: give only the intended identity access and authenticate
   the local peer. On Windows implement a restricted named pipe or equivalently authenticated
   local IPC; an unauthenticated loopback port plus a self-asserted `tag:hub` header is not
   sufficient. Verify forced-command execution, quoting, reconnect, presence and Job Objects on
   actual Windows; test macOS and Termux separately.
3. **Installers and release:** change `install-node.sh` and `.ps1` to detect/provision OpenSSH,
   dedicated account/authorized key and private ingress rules with explicit privilege requirements;
   preserve the existing node state and update rollback. Change `install.sh`'s embedded-blob
   checks, node/hub build scripts and `pt-embed` wiring. Remove `scripts/net-build.sh`, the Go
   stage, Go asset transfer and embed checks from `scripts/release.sh`, then remove the `go/`
   module as a single migration. Release artifacts no longer embed headscale/member; OpenSSH is
   installed and updated independently. These removals are future migration work, not performed
   in this proof of concept.
4. **SPEC:** replace 3.2's headscale topology with pinned SSH, directory, enrollment and private
   routing/relay policy; update principles 2.4 and 2.6, 3.5, 6, backup/move/update rules and the
   relevant owner decisions and corrections as a consistent full migration. Specify that physical
   network isolation is a separate required control. Record the extra SSH processes and the
   external system dependency.
5. **Cutover:** inventory routes and host keys without changing machines in this spike; preserve
   node job journals and hub IDs. Drain or reconcile in-flight jobs and uncertain effects, install
   and verify the new connection node by node, then fully switch the fleet. Do not silently retry
   an uncertain job. Exercise offline nodes, reboot, host/client key rotation, stolen-key
   revocation, large artifacts and rollback. Remove headscale only after all supported platforms
   and one-way controls pass. Keep rollback assets and state until the owner accepts cutover.

## Proof of concept

The feature-gated `pt-ssh-poc` binary is an experimental hub-side client. Its companion Linux e2e
starts the real `protectorate-node` resident agent and a separate unprivileged sshd bound to a
loopback high port, with fresh keys, private configuration and a forced stdio command. It checks
that a changed host key and an unauthorized client key fail. It then submits a token-fenced job,
closes the first SSH session while the job runs, reconnects, checks node identity and journal
epoch, pulls the job log and artifact through `pt-frame`, verifies the artifact size and SHA-256,
and checks its contents. The requested SSH command is intentionally invalid, so success requires
the server's forced command. The test uses `StrictModes no` only for its isolated scratch sshd;
production must enforce appropriate ownership and permissions.

Run with the required build environment and system `ssh`, `ssh-keygen`, and `/sbin/sshd`:

```sh
export CARGO_TARGET_DIR=/home/hotschmoe/.cache/pt-target/next-netpoc CARGO_BUILD_JOBS=4
cargo test --workspace --features protectorate-node/ssh-poc --test ssh_poc -- --nocapture
```

The test skips if one of those programs is missing. It does not configure or use the system sshd,
the user's SSH directory, any VPN, or another machine. It does not establish production enrollment,
Windows behavior, NAT traversal, fleet scheduling or firewall isolation. Its fixed job deliberately
sets `even_if_present` for the experiment. The stand-alone client is not a production SSH client:
it has fixed demo requests and needs production cancellation/deadlines and lifecycle integration.

**Result in this worktree:** an independent smoke check started the actual unprivileged OpenSSH
9.2 server on loopback with in-memory keys and configuration. A strictly pinned, public-key SSH
connection executed the forced command and the server was stopped. The e2e above was then run
outside the sandbox (2026-11-01) and passed in 5 s: changed host key and unauthorized client key
refused, a token-fenced job submitted, the session closed mid-job and reopened, the log and the
artifact pulled through `pt-frame`, and the artifact's size and SHA-256 matched. No sshd was left
running. Clippy with the feature is clean.
