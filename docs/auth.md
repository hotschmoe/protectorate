# Authentication, scopes and audit

Every way into the hub goes through one decision point, `internal/auth`
(`Authorizer`): the Connect interceptor (unary and streaming), the WebSocket
terminal, and the tailnet listener. Nothing else reads a token.

## Credentials

- **Owner token**: `$PROTECTORATE_HOME/token`, created by `hub serve`. Full
  access, never expires, the only credential that may call `TokenService`.
- **Scoped tokens**: issued by the owner, stored as SHA-256 hashes in
  `$PROTECTORATE_HOME/tokens.yaml` (0600, rewritten atomically; the hub is its
  only writer). The secret (`ptk_` + 64 hex) is shown once and cannot be
  recovered. Each token has id, name, scopes, created, expires (optional),
  last_used (persisted at most about once a minute) and revoked.

Unknown, expired, revoked and malformed credentials all fail as
`unauthenticated` (HTTP 401). A valid token without the needed scope fails as
`permission_denied` (HTTP 403 on the terminal), and the message names the
scope that would have admitted the call. Comparisons are constant time.

## Scopes

Default deny: a token has exactly the scopes it was created with, and nothing
is implied (`read` does not include `events`; no glob includes the terminal).

| Scope | Grants |
|-------|--------|
| `read` | every RPC marked `NO_SIDE_EFFECTS`, derived from the proto descriptors, so new read RPCs are covered automatically |
| `events` | the EventService stream |
| `terminal` | the WebSocket terminal, read-write or read-only |
| `terminal:readonly` | the terminal only with `readonly=1` |
| `<glob>` | `path.Match` glob over tool names |

The canonical glob form is the MCP tool name: the snake_case service name
without `Service`, an underscore and the snake_case method, for example
`session_kill_session`, `backlog_*`, `*`. `protectorate mcp` lists the same
names. Globs must match at least one tool when the token is created, so typos
are rejected rather than silently granting nothing. Procedure paths are not
scopes. `*` covers every hub RPC except owner-only ones, but not the terminal.

Leases and jobs follow the same rules: `lease_*` and `job_*` are globs like any
other, `read` admits `GetJob`, `ListJobs`, `WatchJob` and `ListLeases`, and
`SubmitJob`, `CancelJob`, `AcquireLease`, `RenewLease` and `ReleaseLease` are
audited. Treat `job_submit_job` as shell access: the command runs as the node
agent's user on any node the caller can target, and source paths are read from
the hub's disk. Node agents accept the job RPCs only from the hub (tailnet
WhoIs `tag:hub`, or the node token); the hub never serves `NodeAgentService`.

Services marked `(protectorate.v1.owner_only)` (TokenService) and RPCs marked
`(protectorate.v1.owner_only_rpc)` accept only the owner token whatever the
scopes. The RPC form covers anything that runs code as the hub's OS user or
aims the hub's credentials: `CreateSession` and `RespawnSession` (the command
is unsandboxed and could read `$PROTECTORATE_HOME/token`) and `AddNode` (the
hub polls the address with the node token). Owner-only RPCs match no scope
glob, so granting them is rejected at token creation; `*` and `session_*` do
not include them.

The terminal is the same hole by another door. Typing into a session that runs
as the hub user is code execution as that user, so a `terminal` token may
attach read-write only to `sleeve-*` sessions, which run in the sleeve
sandbox. Read-write attach to any other session needs the owner token; a
scoped token gets 403 with a hint to attach with `readonly=1`, which still
works. `job_submit_job` is likewise shell access (on nodes, not the hub) and
stays a deliberate grant.

The hub sends its node token only to an address the owner added: inventory
addresses must be a bare host or IP with an optional port (and optional
`http(s)://`), with no path, credentials or query, and node polls and job calls
never follow redirects.

## Entry points

| Entry point | Check |
|-------------|-------|
| Connect unary and server-streaming RPCs | interceptor: authenticate, then scope for the procedure |
| `GET /ws/terminal/{name}` | bearer header, `token.<value>` subprotocol or `?token=`; needs `terminal` (or `terminal:readonly` with `?readonly=1`) |
| Tailnet listener | WhoIs first (hub and `tag:client` only, never `tag:node`), then exactly the checks above |

Decision: tailnet devices must also present a token. WhoIs proves which device
is calling, not that its owner meant this request, and a compromised phone or
laptop on the tailnet would otherwise have full access. The tailnet identity is
recorded in the audit log next to the token.

## Audit log

`$PROTECTORATE_HOME/audit/audit-YYYY-MM.jsonl`, JSON lines opened `O_APPEND`
with mode 0600. Logged: every mutating RPC (any RPC not marked
`NO_SIDE_EFFECTS`, including denied and unauthenticated attempts) and every
terminal attach. Not logged: reads and event streams. Fields: `time`, `actor`
(`owner`, `token:<name>` or `unauthenticated`), `token_id`, `tailnet`,
`procedure`, `subject` (the request's `name` or `id`, nothing else from the
body), `outcome` (`ok`, `denied`, `error`) and `code` (Connect code).
Secrets and request bodies are never written. `protectorate audit tail [-n 50]`
reads it on the hub machine.

Limits to keep in mind:

- Append-only is a convention of the hub process, not enforced. Anything
  running as the same OS user can edit or truncate the file. True tamper
  resistance needs the log shipped off-host or to a root-owned append-only
  location (for example `chattr +a` on the directory, or forwarding to a
  separate machine). Sleeves only see their workspace mounts, so an agent in a
  sleeve cannot reach `$PROTECTORATE_HOME`.
- Entries are written after the call returns, so a hub crash mid-call can lose
  the line. A failed write is reported on the hub's stderr and does not undo
  the call.
- Unauthenticated mutating attempts are logged too, so an attacker who can
  reach the port can grow the log.

## CLI

```
protectorate token create phone --scope read --scope terminal --scope 'session_*' --ttl 720h
protectorate token ls
protectorate token revoke <id>
protectorate audit tail -n 50
```

Every command takes the token from `--token-file`, then env
`PROTECTORATE_TOKEN`, then the local owner token file, so a scoped token also
works against a remote hub (`--hub` or `PROTECTORATE_HUB`). `protectorate pair`
mints a device token (`read`, `events`, `terminal`, `session_*`, `backlog_*`,
90 days) instead of embedding the owner token.

## Shared node token

Outside a tailnet, the hub and its node agents share a bearer. It is never a
flag, because arguments are readable by every local user in the process list.
Give `hub serve` and `node serve` `--node-token-file PATH` (mode 0600), or set
env `PROTECTORATE_NODE_TOKEN`. The file wins over the env. The one-time
enrollment key in `node join --key` is still an argument; it expires (default
1h) and the process exits once joined.
