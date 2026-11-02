# A Rust-only hub-to-node network

**Status:** design for owner review, 2026-11-03. No implementation or cutover is authorized by this document.

**Recommendation:** investigate a small Rust node listener carrying the existing `pt-frame` protocol over mutually authenticated Noise/TCP. Address each node by an owner-approved public key and dial a short list of private addresses. For the roaming laptop, use its fixed address on the existing `192.168.22.0/24` WireGuard network, with a home-LAN DHCP reservation as an optional second address.

Keep headscale and member until that arrangement has been proved on the real machines and the owner has reviewed the result. If the WireGuard route, node firewall, or background service cannot reliably accept the hub’s connection, keep headscale rather than building a new traversal and relay system.

The lower-code alternative is system OpenSSH carrying `pt-frame`. It removes Go from this repository and has an existing proof of concept, but requires an external SSH server and its configuration on each node. Choose it over Noise if the owner prefers fewer Rust dependencies and less protocol code to a self-contained Rust node service.

## Scope and evidence

The scope is the hub-to-node network. Flutter remains Dart, and the owner’s existing WireGuard implementation remains an external system component. A system SSH or independently installed tailnet client may also contain non-Rust code.

“Private network” means that the application connects over the home LAN, the owner’s WireGuard network, or another explicitly approved private route. A WireGuard packet sent from a roaming laptop crosses the public internet encrypted to the existing VPN gateway; this proposal adds no public Protectorate listener. An independently managed tailnet qualifies only if its control, discovery and relay behavior is acceptable to the owner.

Evidence labels used below:

- **[V]** Verified in the current repository code or lockfile.
- **[D]** Reported by repository documentation or an external primary source.
- **[I]** Inference, estimate, or proposed behavior; it is not implemented or verified on the live network.

The current [SPEC 3.2](SPEC.md:140) and decision D39 still select headscale. Today’s direction narrows the older requirement for hole punching and relay recorded in [REVISIT.md:305](REVISIT.md:305). It does not itself change the specification.

## What the current network provides

| Function | Current behavior and evidence | Needed under today’s direction? |
|---|---|---|
| Device identity | **[V]** Headscale records machines and their keys; member persists tsnet state and pins a control-key fingerprint. The hub’s public node ID is currently the headscale ID; the node agent also reports its own ID and journal epoch. [headscale.rs:193](crates/pt-fleet/src/headscale.rs:193), [member/main.go:209](go/cmd/member/main.go:209), [msg.rs:121](crates/pt-frame/src/msg.rs:121). | **Yes.** A name or IP must not be authority to run jobs. |
| Stable addressing | **[V]** Headscale allocates overlay IPv4 and IPv6 addresses; the poller dials the first listed address. [headscale.rs:61](crates/pt-fleet/src/headscale.rs:61), [poller.rs:231](crates/pt-fleet/src/poller.rs:231). | **Yes, in effect.** The hub needs at least one current locator per node, but Protectorate need not allocate a new overlay address if the existing WireGuard address is reachable. |
| Name resolution | **[V]** Headscale MagicDNS and local DNS override are disabled. Member can match an IP, peer hostname, or first DNS label from its netmap; the poller normally uses an IP. [headscale.rs:80](crates/pt-fleet/src/headscale.rs:80), [control.go:108](go/cmd/member/control.go:108). | **Convenience only.** Owner names and optional private DNS are useful; MagicDNS is not required. |
| Enrollment | **[V]** The node displays an auth ID and control-key fingerprint as QR/text. The owner approves; the hub checks the fingerprint and registers, tags and renames through headscale’s local CLI. [join.rs:138](crates/protectorate-node/src/join.rs:138), [enroll.rs:171](crates/pt-fleet/src/enroll.rs:171). | **Yes.** Keep owner approval and scan/paste, with an authenticated exchange of public keys in both directions. |
| Encryption and authentication | **[V]** tsnet supplies a userspace WireGuard link; node member identifies the caller as `tag:hub`, and hub member dials only a `tag:node` peer. [member/main.go:130](go/cmd/member/main.go:130), [node.go:72](go/cmd/member/node.go:72), [control.go:108](go/cmd/member/control.go:108). | **Yes.** Authenticate both ends and protect job traffic, including on the physical LAN. |
| NAT traversal and endpoint discovery | **[D]** tsnet exchanges endpoints, probes direct paths and can traverse some NATs. **[V]** This configuration disables STUN in its DERP map; it should not be described as generic STUN-assisted traversal. [network-without-go.md:35](docs/design/network-without-go.md:35), [headscale.rs:91](crates/pt-fleet/src/headscale.rs:91). | **No additional Protectorate traversal**, if the existing VPN and routes make every required node reachable. The existing WireGuard VPN may still need its own NAT mapping and keepalives. |
| Relay | **[V]** Headscale enables its embedded, client-verified DERP and supplies a private, IP-pinned map. [headscale.rs:62](crates/pt-fleet/src/headscale.rs:62). | **Not required** if direct connectivity through the approved private routes is actually available. There is no replacement fallback when those routes fail. |
| Overlay ACL | **[V]** One rule permits `tag:hub` to `tag:node:7475`; nodes cannot assign themselves tags. The hub member has no node-facing tailnet listener. [policy.hujson:1](go/policy.hujson:1), [member/main.go:168](go/cmd/member/main.go:168). | **Yes, as a property.** Replace it with authenticated hub-to-node access and separately enforced physical-network rules. |
| Presence and health | **[V]** Member reports peer online, active path, relay and handshake information. The hub also polls node presence every 15 seconds, samples every 60 seconds, and inventory every 10 minutes. [control.go:151](go/cmd/member/control.go:151), [poller.rs:29](crates/pt-fleet/src/poller.rs:29). | **Yes.** Preserve both link health and *human* presence; the latter gates jobs. Direct-versus-DERP status disappears. |
| Reconnect | **[V]** Member is supervised with backoff; the poller discards failed connections and redials. Node disco pings help the hub rediscover a restarted member. [member.rs:89](crates/protectorate-node/src/member.rs:89), [poller.rs:335](crates/pt-fleet/src/poller.rs:335), [node.go:120](go/cmd/member/node.go:120). | **Yes.** Redial candidates after interface changes, sleep, restart and broken connections. |
| Administration and recovery | **[V]** The hub can expire or delete headscale nodes, snapshots headscale state for updates, and supervises both children. [headscale.rs:274](crates/pt-fleet/src/headscale.rs:274), [lib.rs:169](crates/pt-fleet/src/lib.rs:169). | **Yes, with different machinery.** Fleet records, key revocation, backup and rollback remain necessary. |
| Job transport | **[V]** `pt-frame` has job submission, status, cancellation, log and artifact fetch, collection acknowledgment, inventory, presence and update requests. [msg.rs:37](crates/pt-frame/src/msg.rs:37). | **Yes.** These are application functions, not services supplied by SSH or WireGuard. |

**Current qualification:** the control-key check is a separate fetch from tsnet’s own fetch, so it does not fully bind tsnet’s connection against an active intermediary. The repository records this limitation in [REVISIT.md:118](REVISIT.md:118).

**Current network qualification:** the private DERP map disables public DERP for that map, but the repository also records a possible public bootstrap-DNS fallback after control-name resolution fails. Do not describe the current code as proving that every packet always stays private. [REVISIT.md:123](REVISIT.md:123).

## Addressing the roaming laptop

### Recommended arrangement

**[I]** Give the laptop one stable WireGuard peer address within the existing `192.168.22.0/24`, if it does not already have one. Record that address and the node’s pinned public key in a hub-owned fleet directory.

Optionally reserve a home-LAN DHCP address for the laptop and record it as a second candidate. A reservation helps only if the router recognizes the laptop consistently; Wi-Fi MAC randomization can defeat a reservation.

A conceptual record is:

```text
node ID:          7                         # example only
owner name:       laptop
node public key:  <approved key>
candidates:
  - <last successfully verified address>:7475
  - <fixed 192.168.22.x WireGuard address>:7475
  - <home-LAN DHCP reservation>:7475
```

The addresses are locators. The public key authenticates the node; the owner-assigned ID remains stable when an address, name, or approved key changes. **[V]** The existing core uses a numeric `NodeId`, so a hub-allocated, durable, never-reused `u64` fits the current model. [model.rs:23](crates/pt-core/src/model.rs:23).

The hub tries the last successfully authenticated address first, then the remaining approved candidates with short, bounded attempts. A failed or wrong-key candidate is rejected and reported; it does not prevent trying the next candidate. A successful address is cached only after the node’s key has been verified.

Do not automatically add an address merely because a node asserts it. Limit candidates and resolved DNS answers to owner-approved private networks and the node service port. This bounds accidental dials to unrelated services, including after DHCP churn or DNS changes.

Before relying on the WireGuard address, verify all of the following on the actual topology:

1. The laptop has a fixed peer address and its WireGuard client is active when away.
2. The hub has a route to that address, either directly or through the home VPN gateway.
3. The gateway’s peer configuration routes the laptop address to the correct peer.
4. The laptop’s WireGuard `AllowedIPs` and return route include the hub’s source network.
5. The laptop firewall permits the hub’s connection on the VPN interface.
6. If the laptop is behind NAT, its existing VPN connection establishes and maintains a usable mapping.
7. The node service binds to the intended private interfaces and starts again after sleep or interface changes.

**[D]** WireGuard documents persistent keepalives for peers behind NAT; 25 seconds is a common starting point, not a guarantee for every network. [WireGuard quick start](https://www.wireguard.com/quickstart/).

These facts are not established by the repository. In particular, a stable tunnel address does not help while the VPN is off, the laptop is asleep, UDP is blocked, the return route is missing, or an inbound host firewall refuses the service.

If WireGuard is intended to work while the laptop is also on home Wi-Fi, test the VPN gateway endpoint and any required hairpin route there. Otherwise, the reserved LAN address supplies the home candidate. No particular `.172` or `.176` subnet is assumed here.

### Other locator options

| Option | Assessment |
|---|---|
| Last-seen address | **[I]** Cheap and useful for reconnects, but cannot discover a new DHCP lease by itself. Keep it only after successful key verification. |
| DHCP reservation | **[I]** Simple home-LAN solution; does not provide the roaming VPN address and depends on router configuration and stable client identity. |
| Private DNS | **[I]** A router-managed hostname can follow DHCP leases without new Protectorate code. It must resolve from the hub on each relevant network, and its answers remain untrusted until the key verifies. |
| mDNS/DNS-SD | **[I]** Useful only on multicast-capable local links unless additional infrastructure exists. VPNs, VLANs, sleep and multicast filtering limit it; `.local` is not a general roaming solution. A hub-side mDNS receiver or OS daemon also receives node-originated multicast, so it changes the literal “no node traffic to a hub listener” boundary. |
| UDP beacon | **[I]** A node’s periodic unsolicited beacon requires a hub-side receiver. A hub-initiated discovery query with bounded replies can preserve request/reply direction, but needs an additional node listener, firewall treatment and discovery code. Replies are hints, never identity. |
| Node-initiated reverse dial | **[I]** Solves some cases where the hub cannot initiate, but requires a node-facing listener on the hub or a separate jump host. A node can then initiate connection attempts and send protocol bytes to that service. Authentication, rate limits, isolation and a revised trust decision are required. |
| Existing independent tailnet | **[I]** Can supply stable addresses where already installed. It remains a separate service and may use public control or relays; it is not automatically within the owner’s private-only direction. Protectorate must still authenticate the node. |

Do not build discovery or reverse dial for the first version. Add either only if measured routing failures make the fixed WireGuard address and LAN reservation insufficient.

The laptop may send WireGuard packets to the existing VPN gateway. If that gateway runs on the hub itself, it is a node-facing network service and must be recorded as an explicit infrastructure exception to the literal listener rule. It is not a Protectorate node-to-hub API.

## What the link must do beyond carrying bytes

### Existing capabilities and gaps

**[V]** A frame has a length, kind, request ID and payload, capped at 1 MiB. Invalid lengths and kinds are rejected. [codec.rs:1](crates/pt-frame/src/codec.rs:1).

**[V]** Frame IDs do not currently provide concurrent multiplexing. `NodeClient` performs one request at a time and treats a frame for another ID as a protocol error. The node serves requests sequentially on each connection. [client.rs:1](crates/pt-fleet/src/client.rs:1), [serve.rs:330](crates/protectorate-node/src/serve.rs:330).

**[I]** For the first replacement, use a small number of independently authenticated hub-opened connections per node:

- A reserved control connection for `Hello`, presence, status and cancellation.
- A job/log or artifact connection when work needs it.
- A separate connection for a future interactive terminal if that feature is requested.

This provides concurrent logical streams without inventing a multiplexing protocol. Cap connections and reserve control capacity so a slow transfer cannot indefinitely block cancellation or presence. Add single-connection multiplexing only if measured connection overhead warrants it.

**[V]** The protocol has no node PTY RPC today. The PTY in [SPEC 3.3](SPEC.md:174) is for a hub-hosted sleeve. **[I]** If a node terminal becomes a product requirement, its dedicated connection needs open, input, output, resize and close operations, bounded output, and explicit disconnect behavior. Reattaching to a live PTY is separate work; do not imply that job-log recovery already implements it.

### Jobs, streams and recovery

**[V]** The node journals a job before launching it. It retains job state and uses attempts and tokens; the hub records the node’s journal epoch when submitting. [jobs/mod.rs:226](crates/pt-node/src/jobs/mod.rs:226), [SPEC.md:760](SPEC.md:760).

**[I]** Preserve these rules:

- A broken TCP connection never automatically means that a submitted job failed to start.
- Reconnect, verify the node key and journal epoch, then ask for the job’s status.
- Retry a submission only under the existing attempt, token and epoch fencing rules.
- If the epoch changed or the result cannot be established, report **Uncertain**; do not silently launch another attempt.
- Keep the resident job runner independent of a particular network connection.
- A node restart can interrupt or lose a running job; reconnecting cannot promise that the process survived. [jobs/mod.rs:180](crates/pt-node/src/jobs/mod.rs:180).

**[V]** `FetchLog` and `FetchArtifact` accept offsets. The hub appends log slices at its saved offset and retries eligible calls after reconnecting. [msg.rs:171](crates/pt-frame/src/msg.rs:171), [jobs.rs:737](crates/pt-hub/src/jobs.rs:737), [jobs.rs:1100](crates/pt-hub/src/jobs.rs:1100).

**[V]** Artifact recovery is narrower: a new artifact collection opens the destination with `File::create` and starts at offset zero. A dropped individual fetch can be retried, but a restarted collection is not durable offset resume. [jobs.rs:1242](crates/pt-hub/src/jobs.rs:1242).

**[I]** Add durable artifact progress before claiming full resume: retain a bounded partial file, its committed offset and expected identity; after reconnect, recheck the epoch and token, recompute the partial hash if needed, and request the next slice. Check final size and SHA-256 before publishing it. The hash verifies consistency with what the node reported; a compromised node can still lie about the artifact.

**[D]** Cancellation currently can acknowledge collection before fetching the final log tail. Correct that before making cancellation and resumed output a cutover guarantee. [REVISIT.md:445](REVISIT.md:445).

### Presence, flow control, versions and updates

- **Presence and heartbeat:** **[I]** Use successful authenticated polling as link health. Keep human presence separate: `UNKNOWN` must still prevent ordinary jobs. The existing detector enforces that rule. [presence.rs:43](crates/pt-node/src/presence.rs:43).
- **Backpressure:** **[I]** Bound frame size, fetch size, pending writes, connection count and unauthenticated handshake time. Use socket backpressure rather than unbounded queues. Continue draining and truncating a job log at its configured cap so a slow hub does not block the job.
- **Version negotiation:** **[V]** `Hello` currently reports protocol 3, and the hub requires an exact match. [msg.rs:7](crates/pt-frame/src/msg.rs:7), [client.rs:113](crates/pt-fleet/src/client.rs:113). **[I]** Exchange supported protocol ranges and optional capabilities inside the authenticated channel; refuse an incompatible peer before effects. Keep one application implementation and use additive changes for normal rolling updates.
- **Update delivery:** **[V]** Today the hub sends `Update`; the node fetches from GitHub itself, verifies against release `SHA256SUMS`, stages, restarts and awaits hub contact on probation. A node with running jobs refuses the update. [update.rs:1](crates/protectorate-node/src/update.rs:1), [update.rs:140](crates/protectorate-node/src/update.rs:140).
- **Offline-node update:** **[D]** The spec calls for the hub to stream verified bytes to a node without internet, but this is not implemented. [SPEC.md:544](SPEC.md:544), [REVISIT.md:272](REVISIT.md:272).
- **[I]** Add a bounded hub-to-node upload on a hub-opened connection, with offset, expected size and digest, staging, final verification and the existing probation/rollback behavior. This also lets nodes remain off the internet. The existing checksum policy is not release signing.

## Transport choices

### Dependency and code estimates

**[V]** The root [Cargo.lock](Cargo.lock:5) currently has **one** registry package/version entry, `libc`; [DEPENDENCIES.md:7](DEPENDENCIES.md:7) lists it as the only shipped third-party Rust crate. The older report’s statement that rustls was already in use describes another codebase or earlier state, not this workspace.

Published lockfile counts below include recorded dev, build, optional and target dependencies. They are context, **not** the number of crates selected for a particular build, incremental additions, or binary sizes:

- **[D]** `russh` 0.63.3: **396** registry entries, measured by the earlier [network proposal](docs/design/network-without-go.md:52).
- **[D]** `snow` 0.10.0: **139** registry entries in its [published Cargo.lock](https://docs.rs/crate/snow/0.10.0/source/Cargo.lock).
- **[V]** `rustls` 0.23.46: **133** registry entries in the locally cached published crate’s lockfile.

**[I]** Resolve and count the chosen version, features and supported targets before adoption. Pin every resulting third-party crate and document it under the repository’s dependency rules.

The following owned-code estimates are rough planning ranges, excluding tests and code removed. They are not measured diffs. Common fleet directory, enrollment, job recovery, update and service work could add roughly **1,500–3,000 lines** regardless of transport.

| Transport | Additional dependencies and transport-specific owned code | Platform and failure considerations |
|---|---|---|
| **System OpenSSH + `pt-frame`** | **0 new Rust registry crates**; external `ssh`, `sshd`, their crypto libraries and maintenance remain. **[I]** Roughly 700–1,500 owned lines for fleet integration, process lifecycle, pinning and local IPC. | Linux has an existing proof of concept. Windows needs OpenSSH Server setup, forced-command verification and authenticated service IPC; current `--stdio` explicitly fails on Windows. macOS and Termux need separate service and background-lifecycle work. A stopped or misconfigured sshd, incorrect account permissions or a blocked port makes the node offline. |
| **Embedded `russh`** | **[D]** Its published 0.63.3 lock has 396 entries; its dependencies include Tokio. **[I]** Roughly 1,000–2,000 owned lines for client/server integration, authentication policy, limits and lifecycle. [russh crate](https://docs.rs/crate/russh/0.63.3). | Potentially avoids system sshd and supplies SSH channels, but adds a large async and crypto supply chain to a blocking-thread workspace. Windows, macOS and Termux integration remain unproved. It still needs private routes and correct server authorization. |
| **Noise + TCP + `pt-frame`** | **[D]** `snow` 0.10.0 has 139 entries in its published lock; actual selected features will differ. **[I]** Roughly 700–1,400 owned lines for handshake integration, encrypted records, deadlines and listener lifecycle. Use the library’s crypto, not new cryptographic primitives. [Snow documentation](https://docs.rs/snow/0.10.0/snow/). | Fits the existing Rust service and blocking sockets without sshd. Linux and Windows need real-device proof; Android/Termux runtime and macOS are not established. Our handshake policy and record handling become security-sensitive code. |
| **TLS 1.3 + `rustls`** | **[V]** Rustls is absent from this workspace; its cached 0.23.46 published lock has 133 entries. **[I]** Roughly 900–1,700 owned lines for mutual authentication, certificates, pinning and lifecycle. | Standard TLS tooling is an advantage, but self-signed certificate generation, mutual verification, expiry and provider/target choices add work. Custom verifiers must still validate handshake signatures; accepting any certificate is not pinning. Rustls documents `aws-lc-rs` and `ring` provider choices. [rustls documentation](https://docs.rs/rustls/0.23.46/rustls/). |
| **Plain `pt-frame` inside WireGuard** | **0 new Rust crates** if using the existing OS VPN. **[I]** Roughly 200–500 transport lines, plus VPN, routing and firewall administration. | Only equivalent if the tunnel is genuinely end-to-end between the hub and each node, peer identity is correctly bound to the service, and the owner accepts that any sufficiently privileged process with tunnel access may impersonate the hub. A VPN terminating at a gateway leaves the gateway-to-hub LAN leg outside that tunnel. Plain frames on a LAN candidate are not protected. |

For every row, a **completed** replacement can remove the repository’s `go/` network tree, embedded headscale and member, and the dedicated Traefik routes for headscale control and DERP. The normal app/API Traefik route remains. External OpenSSH, WireGuard, or a separately installed tailnet remains where selected.

**[D]** Previous arm64 measurements were about **49.2 MB raw headscale** and **18.7 MB raw trimmed member**, approximately **18 MB and 7 MB gzipped** respectively. These are component measurements, not the expected exact reduction in the final binaries; the embedded assets are compressed and a replacement may add code and dependencies. [network-trust-language.md:159](docs/design/network-trust-language.md:159).

### Why recommend Noise for the Rust service?

**[I]** Noise is the provisional choice because it gives the node one Rust service, authenticates the application independently of how the private route is provided, and avoids implementing a VPN, SSH server or certificate authority. It is not the smallest possible dependency change.

Use a reviewed Noise implementation such as `snow`. Consider `Noise_IK_25519_ChaChaPoly_BLAKE2s`, with the hub as initiator and the node’s approved static public key known before dialing. The node checks the initiator’s static public key against its approved hub key. [Noise IK pattern](https://noiseprotocol.org/noise.html#interactive-handshake-patterns-fundamental).

A concrete first protocol would:

1. Bind a fixed Protectorate transport identifier and version into the Noise prologue.
2. Complete the handshake with no job or other application data in handshake messages.
3. Verify both approved static keys before processing `pt-frame`.
4. Carry frames as a byte stream in bounded authenticated Noise records.
5. Use fresh handshake state and fresh session keys on every connection.
6. Close on a malformed record, failed authentication, deadline, or resource limit.
7. End long-lived sessions at a defined byte/time limit and reconnect with a fresh handshake.

**[I]** A 32 KiB plaintext record limit is comfortably below Noise’s 65,535-byte message limit; larger `pt-frame` messages are split across records and reassembled by the existing frame reader. Specify and fuzz the record-length encoding and EOF behavior before implementation.

The current node `--listen-tcp` path trusts a caller-supplied `tag:hub` header on local loopback. Do not expose that listener on the LAN or treat its header as authentication. Integrate authenticated transport into the node service, or use authenticated local IPC if a separate front end is ever introduced. [serve.rs:330](crates/protectorate-node/src/serve.rs:330), [REVISIT.md:277](REVISIT.md:277).

### Why keep OpenSSH as the fallback?

**[V]** The feature-gated SSH proof of concept pins the host key, disables interactive behavior and forwards frames through `serve --stdio`. Its Linux test exercises wrong host and client keys, a job continuing after session close, and later log and artifact retrieval. [ssh_poc.rs:34](crates/protectorate-node/src/ssh_poc.rs:34), [ssh_poc test:127](crates/protectorate-node/tests/ssh_poc.rs:127).

**[D]** The earlier report records that this test passed in its investigation. This review did not run it. [network-without-go.md:169](docs/design/network-without-go.md:169).

**[I]** A production SSH version must use a dedicated hub key, pinned host keys, a forced node command and disabled forwarding, and must authenticate the bridge to the resident service. SSH itself does not provide job recovery or solve DHCP roaming. Windows service IPC and Termux lifecycle remain substantial work.

## Trust boundaries and key lifecycle

### What a node can reach

**[V]** The current overlay permits the hub to initiate node connections, while nodes can reach headscale control and DERP and exchange WireGuard/discovery packets with the hub member. The node’s responses are untrusted. [SPEC.md:40](SPEC.md:40).

**[D]** The old investigation also observes that overlay policy does not isolate unrelated physical LAN paths: without network separation, a node can see the same LAN front door as another host. [network-trust-language.md:304](docs/design/network-trust-language.md:304).

**[I]** Under direct SSH, Noise or TLS, a node can answer hub-opened connections and can send malformed data on them. Its own key permits it to impersonate that node if the key is stolen. It does not by itself give the node the hub’s private key or authority to impersonate the hub to other nodes.

The node may also initiate unrelated connections through whatever LAN or VPN access its operating system has. An installed WireGuard interface can expose routes to local processes in a way the current userspace tsnet member does not. Application authentication does not replace network segmentation.

For a meaningful return-only boundary, verify the actual hub, proxy and gateway rules for IPv4 and IPv6. Where hostile nodes share a Layer 2 LAN with trusted clients, source-IP rules alone can be spoofed; use appropriate isolation and anti-spoofing, or explicitly record the weaker boundary for owner review. Do not claim that a forced SSH command or a Noise key enforces network isolation.

A reverse-dial design would add a node-facing service that accepts unsolicited connection attempts and parses their traffic. That is a different boundary even with mutual authentication. It is outside this recommendation.

### Enrollment

**[I]** Preserve scan-to-join in spirit, without a node calling a hub endpoint:

1. The node generates its long-lived key pair locally using a cryptographic OS random source.
2. It shows a QR and text containing its public key, fingerprint, proposed name and private address candidates.
3. The owner scans or pastes that record into the authenticated app, verifies it, names the node and approves it.
4. The hub allocates a durable fleet ID and a dedicated hub-to-this-node key pair.
5. The app shows the hub **public** key and fingerprint; the owner transfers it to the node through a locally authenticated method, such as paste, a file, or a second scan where a camera exists.
6. The node accepts only that approved hub key. The hub dials a candidate and verifies the node key.
7. Mark enrollment complete only after an authenticated `Hello` succeeds; retain an intelligible pending state if either side is interrupted.

The second transfer is necessary: a one-way node-to-phone scan cannot deliver a hub public key to an otherwise disconnected node. Neither private key leaves its originating machine.

**[V]** The current node ID and journal epoch use `RandomState` and are explicitly documented as unsuitable for secrets. Do not use that helper for keys, enrollment challenges or tokens. [pt-node/lib.rs:49](crates/pt-node/src/lib.rs:49).

### Storage, revocation and rotation

- **[I]** Store hub private keys in hub-owned state, inaccessible to sleeves; back them up with the fleet directory. Store each node private key under its service account with platform-appropriate permissions.
- **[I]** Use a separate hub key per node to limit exposure of any one key. A compromised hub with access to all hub keys can control the fleet.
- **[V]** Current jobs can run as root or LocalSystem, so local permissions do not protect a node’s key from a sufficiently privileged job on that node. [REVISIT.md:267](REVISIT.md:267).
- **[I]** Removing a node immediately disables its directory record and closes its active hub connections. Stop scheduling it and preserve uncertainty about effects already sent.
- **[I]** For a stolen node, also revoke its WireGuard peer or other network access at the relevant gateway. Revoking the application record alone does not remove the device from the underlying LAN or VPN.
- **[I]** To rotate a hub key, approve and install the new public key on the node, prove a connection with it, then remove the old key. If the old key is suspected compromised, verify the replacement through an owner-controlled local channel.
- **[I]** To rotate a node key, verify the new public key out of band, temporarily allow old and new keys, prove the new one, then retire the old one. A temporary second private port or a brief service interruption is acceptable; do not silently trust a changed key.
- **[I]** An offline or stolen node cannot be trusted to remove an old hub key. Restrict its network access separately and complete local cleanup only when the owner controls it again.

## Migration plan

Every phase below can be delivered or abandoned independently. None requires a permanent dual-backend abstraction or compatibility shim.

### Phase 0: Review and topology inventory

**[I]** Keep the current product unchanged. Record the actual hub, gateway and node interfaces, VPN peer addresses, routes, firewall rules, sleep behavior and supported machines.

Verify whether the laptop already has a fixed WireGuard address and whether the hub can reach it in both directions. Record the current job, update and recovery behavior as the baseline.

**Reversal:** no product change. **Exit:** the owner has a concrete network map and this design to review.

### Phase 1: Make private addressing reliable

**[I]** Configure or confirm the WireGuard peer address and, if useful, a home-LAN reservation or private DNS name. Test LAN-to-VPN and VPN-to-LAN movement, including laptop sleep and wake.

This phase is useful with headscale still in service and requires no new Protectorate transport.

**Reversal:** restore the previous router, DNS and VPN configuration. **Exit:** the hub can repeatedly reach the intended private node endpoint after the network is ready.

### Phase 2: Ship common recovery improvements

**[I]** Independently improve durable artifact resume, final log collection on cancellation, and hub-delivered node updates over the existing hub-opened protocol. Keep the current headscale/member network in production.

Implement these application behaviors once, so the eventual transport can reuse them.

**Reversal:** use the existing release rollback procedure and its snapshots. **Exit:** real-process end-to-end tests show the improvements through the current network.

### Phase 3: Isolated transport trial

**[I]** Build a test-only Noise prototype using separate state, keys and ports on sacrificial instances. Compare its resolved dependency graph and binary sizes with the existing OpenSSH proof of concept.

Exercise the real laptop route, Windows service, Linux service, firewall policy, reboot, key rotation and job recovery. This is a trial, not a second production fleet backend.

**Reversal:** stop the isolated trial and retain the existing production network. **Exit:** the results, dependency list and remaining platform gaps are concrete enough for an owner decision.

### Phase 4: Owner review and full cutover

**[I]** Only after the owner reviews the trial and chooses replacement:

1. Drain jobs or record their uncertain state; stop new dispatch.
2. Preserve recoverable old hub and node state, release binaries, keys, headscale snapshot and router configuration.
3. Install the new hub and nodes as fresh installations and enroll them again.
4. Ship one complete network implementation; do not retain headscale compatibility code.
5. Remove the headscale/member build and embedding paths, Go network tree and dedicated control/DERP routes as one migration, subject to the repository’s file-deletion rule in [CLAUDE.md:7](CLAUDE.md:7).
6. Keep old backups and artifacts until rollback is no longer needed.

Fresh installation means old headscale IDs and historical fleet state need not be mapped into the new directory. Archive old history separately; do not present a newly allocated ID as the identity of an old job.

**Reversal:** stop the new fleet, restore the old release and preserved configuration or reinstall and re-enroll the old fleet. Reconcile any effects performed after the snapshot before dispatch resumes; restoring an old journal must not silently replay jobs.

### Phase 5: Operational acceptance

**[I]** After cutover, observe reconnects, updates, offline nodes and recovery for an owner-chosen period. Keep the previous release and protected rollback material available during that period.

**Reversal:** use the documented full-fleet rollback above. No old and new authority should simultaneously dispatch jobs to the same node.

## Exit criteria before removing headscale

The owner reviews the results and approves replacement only when these checks pass:

- **Private reachability:** the real laptop is reachable on home Wi-Fi and through its WireGuard peer address after moving networks. DHCP churn and a stale last-seen address do not select the wrong machine.
- **Recovery target:** in repeated trials, the hub reconnects within 30 seconds after the node’s OS, VPN and routes report ready. Sleeping, powered-off or VPN-blocked nodes are reported offline.
- **No new public service:** packet and listener checks show only the approved private application paths. Existing WireGuard internet transport and any approved tailnet infrastructure are documented separately.
- **Authentication:** wrong node key, wrong hub key, revoked key and changed key fail closed. A stale address can fall through to a different candidate only after its key verifies.
- **Direction and isolation:** verify that nodes cannot initiate connections to hub API or proxy listeners under the approved network boundary, including IPv6 and applicable gateway paths. Record any intentional exception.
- **Enrollment and rotation:** scan/paste, the return transfer of the hub public key, interruption, retry, planned rotation, lost-key recovery and stolen-node revocation work on real devices.
- **Jobs:** interrupt a connection before and after submission, while a job runs, during cancellation and during log and artifact collection. Verify epoch and token fencing, no duplicate launch, final output handling and durable resume.
- **Bounds:** slow readers, oversized or malformed frames, handshake floods and unavailable peers do not create unbounded memory, disk, threads or waiting time.
- **Updates:** a node without internet receives an update from the hub, verifies it, restarts, is contacted on probation and rolls back when contact does not occur.
- **Platforms:** Linux and Windows fleet scenarios pass on actual machines, including Windows presence and service restart. Preserve the Android/Termux build target and test its real runtime before claiming support; test macOS when it is offered.
- **Verification:** use real-binary end-to-end scenarios and fuzz the new record and state-machine parsers, consistent with [CLAUDE.md:29](CLAUDE.md:29).
- **Release and rollback:** fresh install, backup, full rollback and re-enrollment have been rehearsed. The selected dependency versions and actual target-specific graph are documented.

## Decision and main risk

**[I]** Prefer fixed WireGuard addressing plus a pinned-key Noise/TCP service for the proposed Rust-only network. Keep OpenSSH as the practical alternative if its external server and platform setup are acceptable.

The main risk is operational: headscale currently supplies path discovery and a private relay, while the replacement depends on the real VPN, routes, inbound node firewall and service lifecycle all working together. The earlier investigation records that inbound firewall and certificate upkeep dominated a previous router-only cutover. [network-trust-language.md:59](docs/design/network-trust-language.md:59).

If the laptop cannot reliably accept hub-initiated traffic over the existing private routes, or if a node-facing firewall rule is impractical, keep headscale. If Noise’s selected dependencies or security-sensitive integration prove disproportionate, choose the tested OpenSSH direction instead. If automatic traversal or relay again becomes a requirement, review that as a separate design before replacing the current network.