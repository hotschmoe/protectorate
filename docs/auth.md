# Authentication, scopes and audit

Every way into the hub goes through one decision point, `internal/auth`
(`Authorizer`): the Connect interceptor (unary and streaming), the WebSocket
terminal, and the tailnet listener. Nothing else reads a token.

## Credentials

- **Owner token**: `$PROTECTORATE_HOME/token`, created by `hub serve`. Full
  access, never expires, the only credential that may call `TokenService`.
- **Scoped tokens**: issued by the owner, stored as SHA-256 hashes in
  `$PROTECTORATE_HOME/tokens.json` (0600, rewritten atomically; the hub is its
  only writer). The secret (`ptk_` + 64 hex) is shown once and cannot be
  recovered. Each token has id, name, scopes, created, expires (optional),
  last_used (persisted at most about once a minute) and revoked.

- **Admin grants**: tokens minted per task by `protectorate grant admin`
  (`GrantService.Admin`): scopes `job_*` and `node_admin:<node>`, default 4h,
  at most 24h, stored with the task and the minter. The owner mints them for
  any node; the orchestrator only for a `shared` node by name and only when
  the hub runs with `--orchestrator-admin-grants`. Every call made with a
  grant is audited with its task. See [node-work.md](node-work.md).

- **Sleeve tokens**: minted by the hub itself for every sleeve at spawn (name
  `sleeve:<name>`, scopes `read`, `backlog_*`, `directory_*`, `lease_*`,
  `message_*`), written into the sleeve's home, revoked on kill and replaced on
  respawn. They are how the hub knows who sent a message. Despite `read`, a
  sleeve token can watch only its own sleeve's chat and list or capture only
  its own session; the hub checks the authenticated identity, not the request.
  See `docs/coordination.md`.

Unknown, expired, revoked and malformed credentials all fail as
`unauthenticated` (HTTP 401). A valid token without the needed scope fails as
`permission_denied` (HTTP 403 on the terminal), and the message names the
scope that would have admitted the call. Comparisons are constant time.

## Scopes

Default deny: a token has exactly the scopes it was created with, and nothing
is implied (`read` does not include `events`; no glob includes the terminal).

The owner can change an existing token through `TokenService.UpdateTokenScopes`
or `protectorate token scopes <id|name> --add read --remove events`. Names must
identify exactly one active token; use its ID when names overlap. Additions and
removals are exact scope strings, removals win when both name the same scope,
and removing every scope is allowed. The ID, secret, expiry and running sleeve
stay unchanged. The next authenticated request uses the new scopes; an existing
stream keeps its admission. Revoked and expired tokens cannot be changed.
Scope updates cannot add `node_admin:` grants; continue to use `grant admin`.
Each update is audited with the token ID and the before/after scopes.

Each newly spawned sleeve records its authenticated parent in `sleeves.json`:
`owner`, `envoy`, `sleeve:<name>`, or `token:<name>`, together with the
parent token ID. `sleeve ls`, `sleeve show` and the API expose the parent label.
A parent may kill its children without `sleeve_kill_sleeve`; other callers
still need the existing global kill scope. Envoy and sleeve identities retain
this right across identity-token renewal or resleeving. Arbitrary tokens are
matched by ID. The ownership exception never admits killing `envoy`, including
Envoy killing itself. Global scopes and the reserved-name rules are unchanged.
Existing records without a parent gain no ownership rights; their creator is
unknown and is not guessed during migration.

| Scope | Grants |
|-------|--------|
| `read` | every RPC marked `NO_SIDE_EFFECTS`, derived from the proto descriptors, so new read RPCs are covered automatically |
| `events` | the EventService stream |
| `terminal` | the WebSocket terminal, read-write or read-only |
| `terminal:readonly` | the terminal only with `readonly=1` |
| `terminal:host` | the terminal, also read-write on sessions outside a sleeve sandbox (shell as the hub user) |
| `node_admin:<glob>` | only on admin grants (`grant admin`, never `token create`): work on `care` nodes whose name the glob matches when the owner minted the grant, and `job run --admin` there; never an RPC by itself; see [node-work.md](node-work.md) |
| `<glob>` | `path.Match` glob over tool names |

The canonical glob form is the MCP tool name: the snake_case service name
without `Service`, an underscore and the snake_case method, for example
`session_kill_session`, `backlog_*`, `*`. `protectorate mcp` lists the same
names. Globs must match at least one tool when the token is created, so typos
are rejected rather than silently granting nothing. Procedure paths are not
scopes. `chat_*` covers every ChatService call, including `chat_watch`; `read`
alone is enough to watch a chat but not to send, answer an approval or compact. `*` covers every hub RPC except owner-only ones, but not the terminal.

Leases and jobs follow the same rules: `lease_*` and `job_*` are globs like any
other, `read` admits `GetJob`, `ListJobs`, `WatchJob` and `ListLeases`, and
`SubmitJob`, `CancelJob`, `AcquireLease`, `RenewLease` and `ReleaseLease` are
audited. Treat `job_submit_job` as shell access: the command runs as the node
agent's user on any node the caller can target, and source paths are read from
the hub's disk. Node agents accept the job RPCs only from the hub (tailnet
WhoIs `tag:hub`, or the node token); the hub never serves `NodeAgentService`.

File transfer and desktop control are separate grants, none part of `read`:
`node_put_file`, `node_get_file` (`node_put_*`, `node_get_*`) and `desk_*`
(`desk_screenshot`, `desk_list_windows`, `desk_foreground`, `desk_click`, `desk_move`,
`desk_type`, `desk_key`, `desk_run`). File calls are confined to the node's
file root unless the agent runs with `--file-unconfined`, which makes them as
broad as a job; the mutating desk calls also need a lease on `desk:<node>`.
Node agents take these from the hub only, like jobs. Puts are audited at the
first and last chunk (and any failing chunk), gets and every desk call once,
with the node name as subject and never the path, text or image. See
docs/desk.md.

Services marked `(protectorate.v1.owner_only)` (TokenService, AuditService) and RPCs marked
`(protectorate.v1.owner_only_rpc)` accept only the owner token whatever the
scopes. The RPC form covers anything that runs code as the hub's OS user or
aims the hub's credentials: `CreateSession` and `RespawnSession` (the command
is unsandboxed and could read `$PROTECTORATE_HOME/token`), `AddNode` (the
hub polls the address with the node token), `InviteNode` (it issues a tailnet
enrollment key), `RemoveNode` (it revokes a node and can make it uninstall)
and `SetNodeTier` (it decides which machines jobs may reach). Owner-only RPCs match no scope
glob, so granting them is rejected at token creation; `*` and `session_*` do
not include them.

The terminal is the same hole by another door. Typing into a session that runs
as the hub user is code execution as that user, so a `terminal` token may
attach read-write only to `sleeve-*` sessions, which run in the sleeve
sandbox. Read-write attach to any other session needs the owner token or the
explicit `terminal:host` scope; any other token gets 403 with a hint to attach
with `readonly=1`, which still works. `protectorate pair --host-shell` adds
`terminal:host` for a phone you want to type into host sessions from. `job_submit_job` is likewise shell access (on nodes, not the hub) and
stays a deliberate grant.

The hub sends its node token only to an address the owner added: inventory
addresses must be a bare host or IP with an optional port (and optional
`http(s)://`), with no path, credentials or query, and node polls and job calls
never follow redirects.

## Entry points

`SleeveService.BrowseWorkspaces` requires `read` or its explicit tool scope
(`sleeve_browse_workspaces`); the owner and owner-paired devices can call it.
As with the existing per-sleeve chat/session read restrictions, the handler
also checks the authenticated identity: `sleeve:<name>` tokens are always
denied, including root listing, even if granted `read` or `sleeve_*`.
Other scoped callers, including Envoy, follow their scopes. Granting browsing
to a non-sleeve token exposes names, paths and git metadata beneath the
configured browser roots. It does not execute repository programs.

| Entry point | Check |
|-------------|-------|
| Connect unary and server-streaming RPCs | interceptor: authenticate, then scope for the procedure |
| `GET /install.sh`, `GET /install.ps1`, `GET /dl/protectorate-<os>-<arch>[.exe][.sha256]` | none: generated installers and the builds, no secrets; the enrollment key travels in the environment of the command the owner pastes |
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
body, or what the handler names, such as a new job's id), `outcome` (`ok`,
`denied`, `error`), `code` (Connect code), `task` (the admin grant's task)
and `detail` (what the handler adds: a job's node and mode, and for admin
jobs and jobs under a grant the command, cut at 512 bytes). Secrets, env
values and other request bodies are never written. Calls made with an admin
grant are logged whether or not they mutate. `protectorate audit tail [-n 50]`
reads it on the hub machine. `AuditService.ListAuditEntries` (owner only,
like `TokenService`) serves it to the app's AUDIT view: newest first, pages of
at most 500 with a cursor to older entries, filtered by actor, procedure and
subject (substring, any case) and outcome. One page reads at most about
200,000 lines, so a rare filter can return a short page and a cursor.

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
protectorate token scopes <id|name> --add sleeve_spawn_sleeve --remove events
protectorate audit tail -n 50
```

Every command takes the token from `--token-file`, then env
`PROTECTORATE_TOKEN`, then the local owner token file, so a scoped token also
works against a remote hub (`--hub` or `PROTECTORATE_HUB`). `protectorate pair`
mints a device token (`read`, `events`, `terminal`, `session_*`, `sleeve_*`, `chat_*`, `backlog_*`,
90 days) instead of embedding the owner token.

Run `make test` to verify scope changes, persistence, audit and care-tier
authorization against a real temporary hub. On Linux with tmux, bwrap and a
systemd user manager, the same Go harness also verifies persisted parent
ownership and cross-sleeve denial. Runtime prerequisites produce explicit
subscenario skips; the core authorization checks always run.

## Shared node token

Outside a tailnet, the hub and its node agents share a bearer. It is never a
flag, because arguments are readable by every local user in the process list.
Give `hub serve` and `node serve` `--node-token-file PATH` (mode 0600), or set
env `PROTECTORATE_NODE_TOKEN`. The file wins over the env. The one-time
enrollment key in `node join --key` is still an argument; it expires (default
1h) and the process exits once joined.
