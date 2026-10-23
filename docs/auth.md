# Authentication, scopes and audit

Every way into the hub goes through one decision point,
`protectorate-hub/src/access.rs`: the Connect interceptor (unary and
streaming) and the WebSocket terminal, on every hub listener. Node agents read
no credential at all: their listener admits only the hub's client certificate
(see [Node certificates](#node-certificates)). Network location is never
identity; being on the LAN or the VPN grants nothing.

## Credentials

There are no long-lived bearer secrets on clients.

- **Owner token**: `$PROTECTORATE_HOME/token`, created by the installer and
  passed with `hub serve --owner-token-file`. It is break-glass: the hub
  accepts it only on its plain loopback `--listen`, never on a TLS listener,
  so it works only from the hub host. It never leaves that machine.
- **Devices** (the app on web and Android, desktop builds, a remote CLI):
  during pairing the client creates a P-256 key and registers its public
  half. Every request then carries a proof instead of a token (below). The
  private key never leaves the device: a non-extractable WebCrypto key kept in
  IndexedDB on the web, the Android Keystore on Android, a 0600 key file for
  the CLI (`$HOME/.protectorate/credentials/`, owner-only ACL on Windows).
  Desktop app builds have no OS key store the app can rely on and keep the key
  with the app's preferences. Approved devices have Full access: every
  ordinary RPC, host and sleeve terminals, and device management; owner-only
  RPCs stay excluded except those marked `device_allowed`.
- **Capabilities**: everything else (sleeves, Envoy, admin grants, scripts)
  holds a capability the hub minted: an HMAC-SHA256 macaroon,
  `pc1.<base64url caveats>.<base64url tag>`. Its caveats name the principal
  (`sleeve:<name>`, `orchestrator-envoy`, `grant:<node>` or a script name),
  the scopes, an optional task and the issue and expiry times. Only the hub
  holds the key (in `identity.json`), so only the hub can mint or verify one.

Unknown, expired, revoked, replayed and malformed credentials all fail as
`unauthenticated` (HTTP 401). A valid credential without the needed scope
fails as `permission_denied` (HTTP 403 on the terminal), and the message names
the tool.

### Device proofs

```
Authorization: Proof <device id>.<unix seconds>.<nonce>.<signature>
```

The signature (ECDSA P-256 SHA-256, raw r||s or DER) covers
`<METHOD>\n<path>\n<unix seconds>\n<nonce>`; Connect calls sign `POST` and the
procedure path. The device ID is the first 8 bytes of SHA-256 over the public
key, in hex. The hub rejects a proof more than 60 seconds from its clock and
any nonce it has seen in that window (a bounded in-memory cache). Streams and
the terminal authenticate once at connect. A browser cannot set WebSocket
headers, so the terminal also takes the proof as the subprotocol
`proof.<proof>` next to `protectorate.v1`; the hub never selects it. There is
no `?token=` fallback.

Revoking a device deletes its record: the next request fails and its open
event streams and terminals close within a second.

### Capability lifetime, renewal and revocation

A capability expires: one hour for sleeves and Envoy (`hub serve
--capability-ttl`), one hour by default and at most 24 hours for
`token create` and admin grants (4 hours by default). Renew before expiry:

- **Held by the hub**: for sleeves and Envoy the hub writes the token file
  (`PROTECTORATE_TOKEN_FILE`, 0600, in the sleeve home or Envoy workspace)
  and rewrites it with a fresh token at half its lifetime, so an idle sleeve
  never loses access.
- **By the holder**: `TokenService.RenewToken`, authenticated with the
  current token, returns a fresh one with the same caveats, including any the
  holder appended. The CLI and the MCP bridge read a token file on every call
  and renew it in place once it is past half its life; this works from inside
  a sandbox through the egress relay to the hub. `protectorate token renew`
  does it by hand. An expired token cannot be renewed.

The hub keeps one record per lineage (every token minted, renewed or narrowed
from one mint shares its ID) so it can list and revoke it. Killing, exiting or
replacing a sleeve revokes its lineage and removes its token file; respawning
Envoy revokes the previous one. Revocation also closes open streams.

### Attenuation and delegation

A holder narrows a capability offline by appending caveats:
`protectorate token attenuate --scope message_* --ttl 10m` prints the narrowed
token. A `scopes` caveat can only narrow: a call must be allowed by every
scope set in the token, so appending `*` grants nothing. An `exp` caveat can
only shorten the life, and renewal keeps it. When the hub mints on behalf of a
caller, the result is limited to what the caller holds: a sleeve spawned by
Envoy gets the sleeve scopes intersected with Envoy's, and an admin grant
minted by Envoy gets only the `job_*` tools Envoy holds (plus the
`node_admin:<node>` the grant rule gives).

## Scopes

Default deny: a capability has exactly its scopes, and nothing is implied
(`read` does not include `events`; no glob includes the terminal).

| Scope | Grants |
|-------|--------|
| `full` | approved devices only |
| `read` | every RPC marked `NO_SIDE_EFFECTS`, derived from the proto descriptors |
| `events` | the EventService stream |
| `terminal` | the WebSocket terminal on `sleeve-*` sessions, read-write or read-only |
| `terminal:readonly` | the terminal only with `readonly=1` |
| `terminal:host` | the terminal, also read-write on sessions outside a sleeve sandbox |
| `node_admin:<glob>` | only on admin grants: work on `care` nodes the glob matches when the owner minted the grant, and `job run --admin` there; see [node-work.md](node-work.md) |
| `<glob>` | `path.Match` glob over tool names |

The canonical glob form is the MCP tool name: the snake_case service name
without `Service`, an underscore and the snake_case method, for example
`session_kill_session`, `backlog_*`, `*`. Globs must match at least one tool
when the token is created. `*` covers every hub RPC except owner-only ones,
but not the terminal. Treat `job_submit_job` as shell access on every node the
caller can target. File transfer (`node_put_file`, `node_get_file`) is a
separate grant, confined to the node's file root unless the agent runs with
`--file-unconfined`.

RPCs marked `(protectorate.v1.owner_only_rpc)` (TokenService create, list and
revoke, AuditService, `CreateSession`, `RespawnSession`, `AddNode`,
`InviteNode`, `RemoveNode`, `SetNodeTier`, `SpawnEnvoy`, `UpdateNode`,
`ReleaseService.UpdateHub`) accept only the owner whatever the scopes.
`UpdateHub` and `UpdateNode` also carry `(protectorate.v1.device_allowed)`, so
the app's update buttons work from a paired device. `RenewToken` admits only
capabilities. DeviceService admits the owner and devices, never a
capability.

Identity checks inside handlers come on top of scopes. A sleeve can watch
only its own chat, list or capture only its own session, report only for
itself (`ChatService.Report`, which needs no scope for its own sleeve) and
never browse workspaces. A parent may kill the sleeves it spawned without
`sleeve_kill_sleeve`; the parent is recorded as `owner`, `envoy`,
`sleeve:<name>` or `token:<name>` with its lineage ID. Read-write attach to a
session outside a sleeve sandbox needs the owner, a device or
`terminal:host`.

## Entry points

| Entry point | Check |
|-------------|-------|
| Connect unary and server-streaming RPCs | authenticate, then scope for the procedure |
| `DeviceService.StartDeviceAuth`, `PollDeviceAuth` | unauthenticated, private source address, source-IP and global rate limits |
| `NodeService.JoinNode` | the single-use, expiring join code and a signature over it by the joining key |
| `GET /ca.pem`, `/install.sh`, `/install.ps1`, `/dl/...` | none: public certificate, installers and builds |
| `GET /ws/terminal/{name}` | device proof (header or subprotocol), owner token on loopback, or a capability with a terminal scope |
| `GET /debug/...` | owner token, loopback listener only |
| Node agent listener | mutual TLS with a hub-CA client certificate naming the hub |

## Device approval

1. On the new app, enter or scan the hub URL and connect. For another
   machine's CLI, run `protectorate login --hub URL --name NAME`.
2. The new device shows a short code. On an already trusted device compare
   the code, name, platform and source IP, then approve or deny; or run
   `protectorate devices approve CODE` on the hub host.
3. The poll that reports `approved` also returns the device record once. The
   device code only learns the outcome: what the device can do still needs its
   private key. Requests expire after ten minutes.
4. Review DEVICES or `protectorate devices ls`; rename entries, and revoke a
   lost device immediately.

Start is limited to ten requests per source IP and sixty globally per minute,
polling to 120 per IP and 1,200 globally; at most 100 requests may be pending.
The source IP is the TCP peer, or with `hub serve --trusted-proxy ADDR` the
rightmost `X-Forwarded-For` address from that proxy. Device names cannot use
sleeve or orchestrator identities.

Trusted devices share the owner's mailbox; their audit actor is the device.

## Approval gates

Agents are the holders of hub-minted capabilities whose principal is Envoy
(`orchestrator-envoy`) or a sleeve (`sleeve:<name>`), and of any lineage Envoy
minted, such as its admin grants. When an agent makes a high-impact call the hub opens
an approval request instead of running it: any RPC marked `destructive` in
the proto (`DeleteNote`, `KillSleeve` even of its own children,
`RestoreCheckpoint` and the rest), minting an admin grant (after the grant's
own checks pass), an admin job, and launching a sleeve whose harness
executable or profile changed (see harness pinning in `docs/sleeves.md`).
Agents cannot reach care nodes at all: placement refuses them without an
owner-minted grant. Calls by the owner, approved devices and capabilities the
owner minted are never gated.

The call fails with `aborted` and a detail naming the request id. Repeating
the same call with header `protectorate-approval: <id>` runs it once after the
owner approves, and fails with `permission_denied` after a denial or expiry;
the CLI and MCP bridge repeat it every two seconds and print the id once. An
approval binds to the caller and the exact request bytes and admits one
execution. A call identical to a denied one is denied again for ten minutes.
Requests expire after ten minutes and live only in hub memory.

The owner decides from an approved device (the app's ACTION APPROVAL banner,
fed by `approval.requested` and `approval.decided` events) or with
`protectorate approvals ls | approve <id> | deny <id>`
(`ApprovalService`, owner and devices only). The audit log records each
request (`approval.request`), decision (`DecideApproval`, with the decider and
what was decided) and expiry (`approval.expired`), and the call's own entry
when it runs.

## Audit log

`$PROTECTORATE_HOME/audit/audit-YYYY-MM.jsonl`, JSON lines opened `O_APPEND`
with mode 0600. Logged: every mutating RPC (including denied and
unauthenticated attempts), every terminal attach, and every call made with an
admin grant. Fields: `time`, `actor` (`owner`, `token:<principal or device
name>` or `unauthenticated`), `token_id` (the device ID, which is its key ID,
or the capability lineage ID), `procedure`, `subject`, `outcome`, `code`,
`task`, `detail`, `run_id` (the run of the sleeve that made the call or that
it concerns), `seq`, `prev` and `hash`; every field is part of the hashed
line. Secrets, proofs, env values and request bodies are never written.
`protectorate audit tail` reads it on the hub machine;
`AuditService.ListAuditEntries` (owner only) serves the app's AUDIT view,
filtered by actor, procedure, subject, outcome and run id.

The log is a hash chain. `seq` numbers entries from 1 across every monthly
file, `prev` is the previous entry's hash, and `hash` is SHA-256 over the
line's exact bytes with an empty `hash` value. After each append the writer
records the newest `seq` and `hash` in `audit/head.json`; writers in every
process (the hub and terminal sleeve runners) serialize on `audit/.lock` and
continue from the last line on disk. `protectorate audit verify`
(`AuditService.VerifyAudit`) walks the chain from entry 1 and fails, naming
the file and line, when an entry was edited, removed or reordered, when the
log ends before the recorded head, or when the head is missing. `GetStatus`
and `ListAuditEntries` report the head, so a client can remember it.

Limits: the chain makes tampering evident, not impossible; anything running as
the same OS user can rewrite the whole log and its head, which only a head
remembered elsewhere or a root-owned append-only location catches (sleeves
cannot reach `$PROTECTORATE_HOME`). An entry is written after its call
returns; unauthenticated attempts are logged too, so a caller who can reach
the port can grow the log.

## CLI

```
protectorate token create backup --scope read --scope 'backlog_*' --ttl 24h
protectorate token ls
protectorate token revoke <id>
protectorate --token-file FILE token renew
protectorate --token-file FILE token attenuate --scope backlog_create_note --ttl 10m
protectorate login --hub https://hub.example --name officepc
protectorate devices ls | pending | approve CODE | deny CODE | rename ID NAME | revoke ID
protectorate audit tail -n 50
protectorate audit ls --run RUN_ID
protectorate audit verify
protectorate approvals ls | approve ID | deny ID
```

Every command authenticates with the capability in `--token-file` or
`PROTECTORATE_TOKEN_FILE` (renewed in place), then `PROTECTORATE_TOKEN`, then
the device key `login` saved for the selected hub. On the hub host the
installer's owner token is used as a last resort by `protectorate mcp`.
`protectorate pair` prints a QR code and URL containing only the hub URL and
its CA.

## Node certificates

A node has no shared secret with the hub. `node join` creates a P-256 key on
the node, signs the invite code with it and sends the public key; the hub CA
returns a certificate naming `ID.NAME.node.protectorate.internal`, where `ID`
is new on every join. The hub trusts only the certificate of the node's
current join, so `node rm` revokes it; it renews certificates in place a month
before their one-year expiry. The join code works once and expires (default
1h).
