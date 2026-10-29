# Authentication, scopes and audit

Every way into the hub goes through one decision point,
`protectorate-hub/src/access.rs`: the Connect interceptor (unary and
streaming) and the WebSocket terminal, on every hub listener. Network location
is never identity; being on the LAN or the VPN grants nothing.

## Network

The hub is reachable two ways only, and never from the internet:

- **Loopback**: plain HTTP on `hub serve --listen` (default
  `127.0.0.1:7470`) on the hub machine, for the CLI, sleeves, Envoy and the
  MCP bridge. The owner token and `/debug/` work only here.
- **Through the trusted proxy**: a reverse proxy on the LAN terminates TLS
  with an ordinary public certificate and forwards plain HTTP to each
  `hub serve --proxy-listen IP:PORT` (a private address). That listener
  closes every TCP connection whose peer is not a `--trusted-proxy` address
  before reading a byte, so a client can never send a token to the hub in the
  clear, and it refuses the owner token. Production: Traefik on stronghold
  (192.168.10.50) serves `https://protectorate.hotschmoe.com` with its Let's
  Encrypt certificate behind its lan-only middleware, and the hub runs with
  `--proxy-listen 192.168.10.44:7470 --trusted-proxy 192.168.10.50`.

Clients use the platform's public roots for HTTPS; there is no private CA.

## Credentials

Every credential travels as `Authorization: Bearer <token>`, and the hub
tells them apart by prefix.

- **Owner token**: `$PROTECTORATE_HOME/token`, created by the installer and
  passed with `hub serve --owner-token-file`. It is break-glass: the hub
  accepts it only on its loopback `--listen`, never through the proxy, so it
  works only from the hub host. It never leaves that machine.
- **Device tokens** (`pd1.<base64url of 32 random bytes>`): the app on web
  and Android, desktop builds and a remote CLI. A device token is a bearer
  secret kept by the client: the app stores it with the hub's profile (on the
  web in that origin's local storage), the CLI reads it from
  `PROTECTORATE_TOKEN` or a 0600 `--token-file`. Anyone holding it is that
  device until it is revoked. The hub stores only `hex(SHA-256(token))`; the
  device ID is the first 16 hex digits of that hash, and a presented token is
  checked by hashing it, finding the device by ID and comparing the full hash
  in constant time. Devices have Full access: every ordinary RPC, host and
  sleeve terminals, and device management; owner-only RPCs stay excluded
  except those marked `device_allowed`.
- **Capabilities** (`pc1.`): everything else (sleeves, Envoy, scripts)
  holds a capability the hub minted: an HMAC-SHA256 macaroon,
  `pc1.<base64url caveats>.<base64url tag>`. Its caveats name the principal
  (`sleeve:<name>`, `orchestrator-envoy` or a script name),
  the scopes, an optional task and the issue and expiry times. Only the hub
  holds the key (in `identity.json`), so only the hub can mint or verify one.

Unknown, expired, revoked and malformed credentials all fail as
`unauthenticated` (HTTP 401). A valid credential without the needed scope
fails as `permission_denied` (HTTP 403 on the terminal), and the message names
the tool.

Streams and the terminal authenticate once at connect. A browser cannot set
WebSocket headers, so the terminal also takes the token as the subprotocol
`bearer.<token>` offered next to `protectorate.v1`; the hub never selects it.
There is no `?token=` fallback.

## Devices

DeviceService admits the owner and devices, never a capability.

1. Add a device: `protectorate devices add NAME` on the hub, or "Add device"
   in the app's DEVICES view on any device. The hub returns the token once.
   With `--url https://HUB` the CLI also prints a web link
   (`https://HUB/#/pair?url=...&token=...`), an app link
   (`protectorate://pair?url=...&token=...`) and a terminal QR of the web
   link, or of the app link with `--app`. The links carry the token, so they
   are bearer secrets as well.
2. Open the link on the new device, or paste the token. The app saves the
   hub and its token and removes the token from the address bar.
3. Review DEVICES or `protectorate devices ls` (name, when and by whom added,
   last use, flushed at most about once a minute). Rename entries; names need
   not be unique and cannot use sleeve or orchestrator identities (`envoy`,
   `orchestrator-envoy`, `sleeve:...`).
4. Revoke a lost device immediately: its record is deleted, its next request
   fails and its open event streams and terminals close within a second.

Devices share the owner's mailbox; their audit actor is the device
(`token:<name>`).

### Capability lifetime, renewal and revocation

A capability expires: one hour for sleeves and Envoy (`hub serve
--capability-ttl`), one hour by default and at most 24 hours for
`token create`. Renew before expiry:

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
Envoy gets the sleeve scopes intersected with Envoy's.

## Scopes

Default deny: a capability has exactly its scopes, and nothing is implied
(`read` does not include `events`; no glob includes the terminal).

| Scope | Grants |
|-------|--------|
| `full` | devices only |
| `read` | every RPC marked `NO_SIDE_EFFECTS`, derived from the proto descriptors |
| `events` | the EventService stream |
| `terminal` | the WebSocket terminal on `sleeve-*` sessions, read-write or read-only |
| `terminal:readonly` | the terminal only with `readonly=1` |
| `terminal:host` | the terminal, also read-write on sessions outside a sleeve sandbox |
| `<glob>` | `path.Match` glob over tool names |

The canonical glob form is the MCP tool name: the snake_case service name
without `Service`, an underscore and the snake_case method, for example
`session_kill_session`, `backlog_*`, `*`. Globs must match at least one tool
when the token is created. `*` covers every hub RPC except owner-only ones,
but not the terminal.

RPCs marked `(protectorate.v1.owner_only_rpc)` (TokenService create, list and
revoke, AuditService, `CreateSession`, `RespawnSession`, `SpawnEnvoy`,
`ReleaseService.UpdateHub`) accept only the owner whatever the scopes.
`UpdateHub` also carries `(protectorate.v1.device_allowed)`, so the app's
update button works from a device. `RenewToken` admits only
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
| Connect unary and server-streaming RPCs | authenticate the bearer, then scope for the procedure |
| `GET /ws/terminal/{name}` | device token or capability with a terminal scope (header or `bearer.` subprotocol), or the owner token on loopback |
| `GET /debug/...` | owner token, loopback listener only |

Nothing is unauthenticated.

## Approval gates

Agents are the holders of hub-minted capabilities whose principal is Envoy
(`orchestrator-envoy`) or a sleeve (`sleeve:<name>`), and of any lineage Envoy
minted. When an agent makes a high-impact call the hub opens
an approval request instead of running it: any RPC marked `destructive` in
the proto (`DeleteNote`, `KillSleeve` even of its own children,
`RestoreCheckpoint` and the rest), and launching a sleeve whose harness
executable or profile changed (see harness pinning in `docs/sleeves.md`).
Calls by the owner, devices and capabilities the
owner minted are never gated.

The call fails with `aborted` and a detail naming the request id. Repeating
the same call with header `protectorate-approval: <id>` runs it once after the
owner approves, and fails with `permission_denied` after a denial or expiry;
the CLI and MCP bridge repeat it every two seconds and print the id once. An
approval binds to the caller and the exact request bytes and admits one
execution. A call identical to a denied one is denied again for ten minutes.
Requests expire after ten minutes and live only in hub memory.

The owner decides from a device (the app's ACTION APPROVAL banner,
fed by `approval.requested` and `approval.decided` events) or with
`protectorate approvals ls | approve <id> | deny <id>`
(`ApprovalService`, owner and devices only). The audit log records each
request (`approval.request`), decision (`DecideApproval`, with the decider and
what was decided) and expiry (`approval.expired`), and the call's own entry
when it runs.

## Audit log

`$PROTECTORATE_HOME/audit/audit-YYYY-MM.jsonl`, JSON lines opened `O_APPEND`
with mode 0600. Logged: every mutating RPC (including denied and
unauthenticated attempts), and every terminal attach. Fields: `time`, `actor` (`owner`, `token:<principal or device
name>` or `unauthenticated`), `token_id` (the device ID, the first 16 hex digits of
its token's SHA-256, or the capability lineage ID), `procedure`, `subject`, `outcome`, `code`,
`task`, `detail`, `run_id` (the run of the sleeve that made the call or that
it concerns), `seq`, `prev` and `hash`; every field is part of the hashed
line. Tokens, env values and request bodies are never written.
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
protectorate devices add NAME [--url https://HUB] [--app]
protectorate devices ls | rename ID NAME | revoke ID
protectorate audit tail -n 50
protectorate audit ls --run RUN_ID
protectorate audit verify
protectorate approvals ls | approve ID | deny ID
```

Every command authenticates with the token in `--token-file` or
`PROTECTORATE_TOKEN_FILE`, then `PROTECTORATE_TOKEN`. A token file holding a
capability is renewed in place past half its life; any other token in it,
such as a device token, is used as it is. `--json devices add` prints
`{"device", "token", "web", "app"}` (the links only with `--url`). From
another machine, pass `--hub https://HUB` with a device token. On the hub
host the installer's owner token is used as a last resort by
`protectorate mcp`.
