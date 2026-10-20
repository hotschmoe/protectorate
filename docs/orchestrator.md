# Orchestrator (Envoy)

Envoy is the orchestrator. It is not engine code, but an ordinary sleeve named `envoy`: a harness (Claude,
Codex, Grok or pi, whichever has quota to spare) running in chat mode (see
[chat.md](chat.md)) in the usual bwrap and systemd-scope sandbox. Its workspace is `$PROTECTORATE_HOME/orchestrator/` and
its only tool is the `protectorate` CLI, authenticated with a scoped token. The
harness is untouched: it only finds normal project context files and a CLI.

```
protectorate orchestrator spawn [--harness auto|claude|codex|grok|pi] [--scopes a,b] [--add-scopes c]
protectorate orchestrator status
protectorate orchestrator scopes --add chat_send --remove events
protectorate quota suggest [--for small|medium|large]
```

`spawn` is a thin client of the owner-only `SleeveService.SpawnEnvoy` RPC: the
hub does the work in process, so it needs the owner token but not the hub
machine, and the app's ENVOY tab starts Envoy with the same call. The RPC is
owner-only whatever the scopes and is not an MCP tool, so Envoy's own token can
never call it. `--harness auto` takes the top of `quota suggest`
(`QuotaService.Suggest`, also an MCP tool). Only FRESH quota counts as capacity;
STALE and UNKNOWN providers are listed but never picked, and the local LLM
through pi is always the fallback. `orchestrator status` shows the sleeve and
the token, including its expiry.

## What a spawn does

1. Picks the harness, stops a running `envoy`, and revokes every earlier
   `orchestrator-envoy` token.
2. Mints a new token and writes it to `orchestrator/.protectorate-token` (0600).
   The sleeve gets `PROTECTORATE_TOKEN_FILE` and `PROTECTORATE_HUB`, so the CLI
   inside needs no flags.
3. Regenerates `AGENTS.md`, `CLAUDE.md` (a symlink to it) and `hosts.md`.
4. Starts the sleeve in chat mode. If anything fails after the token was
   minted, the new token is revoked.

Chat mode is what lets the hub reach Envoy: escalations (an ack-required message
that stayed unacked) and other mailbox notices are injected into its chat as
ordinary user messages at its next idle (see [coordination.md](coordination.md)).
The app's ENVOY tab (the first one) is a dedicated chat with Envoy and a selector for any other chat-mode sleeve. Watch it with `protectorate chat watch envoy`, and answer its approval requests
with `chat respond`. There is no terminal mode for Envoy.

For Codex, choose model/effort on creation with `--harness codex --model MODEL
--effort EFFORT`, using values from `protectorate chat models --harness codex`.
To change an existing idle Envoy without recreating it, use `protectorate chat
settings envoy --model MODEL --effort EFFORT` or its chat menu's MODEL AND
EFFORT action. See [model settings](chat.md#models-and-reasoning-effort) for
native defaults, other-driver limits and scope requirements. Envoy's default
token lacks `chat_set_selection`; owner authentication or an explicit grant
is required. This feature does not require restarting the hub or Envoy.

`AGENTS.md` holds the role, the working loop, the token's name and scopes, a table
of every hub tool with whether this token may call it, and the CLI reference
walked from the cobra command tree. It is rebuilt on every spawn; put standing
notes in `NOTES.md` next to it. Claude Code reads
`CLAUDE.md`; Codex, Grok and pi read `AGENTS.md`. `hosts.md` is the hub's node
inventory.

## Staying cache-friendly

Envoy is a long conversation, and a harness's prompt cache is keyed on the
exact prefix, context files included. So the hub keeps that prefix still:

- `AGENTS.md` has no timestamp and says nothing about the token's id or
  expiry. It is byte-identical across respawns unless the CLI, the tool
  catalog, the hub URL or the scopes change (the harness is not named, so a resleeve keeps it too), and it is not even
  rewritten (mtime kept) when nothing changed.
- `hosts.md` changes whenever a node does, so it is never part of the loaded
  context: `AGENTS.md` only mentions it and Envoy reads it on demand. The hub
  refreshes the copy in the workspace every minute.
- The token is renewed in place. The hub watches Envoy's token and, once less
  than a quarter of its life is left (or it is past its end), mints a new one
  with the same scopes and lifetime, replaces the token file atomically, and
  revokes the old one five minutes later. The harness is not restarted: the CLI
  reads `PROTECTORATE_TOKEN_FILE` on every call. A command that streams for
  longer than the grace period (`chat watch`, `events`) keeps the token it
  started with until it ends.

The hub adds its own binary's directory read-only to every sleeve and puts it
last on `PATH`, so `protectorate` runs inside the sandbox. Sleeves share the
host network, so `127.0.0.1` reaches the hub. The binary must be installed
under the name `protectorate`.

The orchestrator can read and edit the owner's notes with `protectorate backlog ...`, so you can talk to it about them (docs/backlog.md).

## Scoping

Default scopes are `read`, `events` and the explicit names of the mutating,
non-destructive tools matching `backlog_*`, `lease_*`, `message_*`,
`sleeve_spawn_sleeve`, `sleeve_resleeve_sleeve` and `grant_admin`. In
practice: read everything, stream events, create and update notes, send,
reply to and ack mailbox messages (escalations land in Envoy's chat and must be
answerable), spawn sleeves, and move a sleeve to another harness (`sleeve
resleeve`, see [chat.md](chat.md#resleeve)), which Envoy may do without asking.
`grant_admin` mints a task grant (`job_*` and `node_admin:<node>`, at most
24h) for one `shared` node by name, and only when the hub runs with
`--orchestrator-admin-grants` (off by default); `care` nodes are granted by
the owner only ([node-work.md](node-work.md)). Not granted by
default:

- token management, raw session creation, respawn and node add (owner-only
  whatever the scopes), `sleeve_kill_sleeve`, `session_kill_session` and
  node remove;
- the terminal;
- `session_*` creation. `session_create_session` and `session_respawn_session`
  start processes directly on the host, outside any sandbox, so a token that
  could call them would escape isolation. Envoy starts work only through
  `sleeve_spawn_sleeve`, which is sandboxed. (The hub also refuses these RPCs
  for names starting with `sleeve-`.) Listing and capturing sessions is covered
  by `read`.

A denied call fails with `permission_denied` and is written to the audit log
(`protectorate audit tail`) as `token:orchestrator-envoy`. The role text also
tells Envoy to ask the user before any destructive action, even one its token
would allow. That is a convention for the model; the token is the enforcement.

## Changing scopes in place

```
protectorate orchestrator scopes --add sleeve_kill_sleeve
protectorate orchestrator scopes --remove sleeve_kill_sleeve
protectorate token scopes sleeve:worker --add sleeve_spawn_sleeve
```

Run these commands with owner authentication (see [auth.md](auth.md)); Envoy
cannot change its own scopes. `TokenService.UpdateTokenScopes` changes the
existing token's grants without rotating its secret or restarting Envoy. The
next request uses the new scopes; existing streams retain their admission.
Every change is audited with its before/after scopes. `orchestrator scopes`
selects the newest active Envoy token, including during the renewal grace
period. Older grace-period tokens keep their own grants until revoked.

`--add-scopes` extends the defaults, or the explicit `--scopes` list; it does
not extend the running token's custom grants. To preserve custom grants, supply
their complete list with `--scopes` and add the new scope with `--add-scopes`.
These flags still apply when creating or explicitly respawning Envoy.
Scopes use tool names (see [auth.md](auth.md)). The owner-only spawn API is
`SleeveService/SpawnEnvoy`, with `SpawnEnvoyRequest.scopes`, `add_scopes`,
`harness` and `ttl`. Automatic renewal keeps the updated scopes, including an
empty set. No hub restart is needed. In-place updates leave `AGENTS.md` alone
to preserve its prompt prefix; its spawn-time scope table may be stale. The
API is authoritative; `orchestrator status` shows the current grants.

Envoy can assign work with `msg send <sleeve> ...`; the mailbox delivers it
into chat according to [delivery rules](coordination.md#delivery). Direct
`chat send`, interrupt and approval responses need additional chat scopes,
which Envoy does not receive by default. `terminal` permits typing into sleeve
terminal sessions; host sessions need `terminal:host` or the owner token.
Neither terminal grant is needed for mailbox assignments. Envoy can kill a
sleeve it spawned without a global kill grant. The hub records `parent: envoy`
on new workers and exposes it in `sleeve ls` and `sleeve show`. This ownership
right survives Envoy token renewal, cannot kill Envoy itself, and grants no
rights over older workers whose records lack a parent. An explicit
`sleeve_kill_sleeve` grant permits killing sleeves generally, not only children.

## External bridges are the same mechanism

"AI as orchestrator" and "AI talking to the orchestrator" are one path: a scoped
token plus the CLI or the MCP bridge. A Claude or Grok bot on another machine
does the same thing Envoy does, without the sleeve:

```
protectorate token create claude-bridge --scope read --scope 'backlog_*' --scope sleeve_spawn_sleeve
protectorate mcp --token-file ~/.protectorate/claude-bridge.token --allow 'quota_*' --allow 'backlog_*'
```

`quota_suggest` and `quota_get_quota` are available to either. See
[mcp.md](mcp.md).

## Security notes

The outside reviews warned that a sleeve able to spawn others tends to collect
holes until it is the hub. Envoy keeps its sandbox and gets only a revocable,
expiring, audited token. Its harness's shared auth directory is mounted
read-write so native login refresh works; the sandbox is not credential-free.
The owner's real home and SSH key directory are not mounted as general access.
Envoy can still spend quota by spawning sleeves, so the spawn scope is the thing
to watch in the audit log.

## Always available

Envoy's record outlives its harness. After a hub restart or a harness exit
nothing is restarted on a timer: the next owner message, or an escalation,
reopens the same native session (`--resume`) with the same AGENTS.md, so the
provider's cache prefix still matches while it is warm. Nothing is compacted
automatically. `hub serve --start-envoy auto|claude|codex|grok|pi` starts
Envoy at boot only when the hub has no Envoy at all; it is off by default,
because starting happens from the app. Low-priority mail waits for the owner's
next message instead of starting turns ([coordination.md](coordination.md#delivery)).

## Renaming from Poe

Before the rename the orchestrator was the sleeve `poe` with the token
`orchestrator-poe`. When the hub loads, a sleeve record named `poe` becomes
`envoy`: its state directory (chat log, native session, environment file) is
renamed, not copied, and nothing is deleted. A running harness is stopped
first, as a hub restart does anyway; the next message to Envoy resumes its
native session. If `envoy` already exists, `poe` is left untouched and the hub
logs why. Tokens named `orchestrator-poe` are revoked; when the renamed sleeve
has no token of its own, one named `orchestrator-envoy` with the same scopes
and remaining life is written to its token file first. The mailbox keeps its
old messages addressed to `poe` as history.
