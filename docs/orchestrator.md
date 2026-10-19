# Orchestrator (Poe)

Poe is not engine code. It is an ordinary sleeve named `poe`: a harness (Claude,
Codex, Grok or pi, whichever has quota to spare) running in chat mode (see
[chat.md](chat.md)) in the usual bwrap and systemd-scope sandbox. Its workspace is `$PROTECTORATE_HOME/orchestrator/` and
its only tool is the `protectorate` CLI, authenticated with a scoped token. The
harness is untouched: it only finds normal project context files and a CLI.

```
protectorate orchestrator spawn [--harness auto|claude|codex|grok|pi] [--scopes a,b] [--add-scopes c]
protectorate orchestrator status
protectorate quota suggest [--for small|medium|large]
```

Run `spawn` with the owner token on the hub machine. `--harness auto` takes the
top of `quota suggest` (`QuotaService.Suggest`, also an MCP tool). Only FRESH
quota counts as capacity; STALE and UNKNOWN providers are listed but never
picked, and the local LLM through pi is always the fallback.

## What a spawn does

1. Picks the harness, stops a running `poe`, and revokes every earlier
   `orchestrator-poe` token.
2. Mints a new token and writes it to `orchestrator/.protectorate-token` (0600).
   The sleeve gets `PROTECTORATE_TOKEN_FILE` and `PROTECTORATE_HUB`, so the CLI
   inside needs no flags.
3. Regenerates `AGENTS.md`, `CLAUDE.md` (a symlink to it) and `hosts.md`.
4. Starts the sleeve in chat mode. If anything fails after the token was
   minted, the new token is revoked.

Chat mode is what lets the hub reach Poe: escalations (an ack-required message
that stayed unacked) and other mailbox notices are injected into its chat as
ordinary user messages at its next idle (see [coordination.md](coordination.md)).
Watch it with `protectorate chat watch poe`, and answer its approval requests
with `chat respond`. There is no terminal mode for Poe.

`AGENTS.md` holds the role, the working loop, the token's id and scopes, a table
of every hub tool with whether this token may call it, and the CLI reference
walked from the cobra command tree. It is rebuilt on every spawn, so it cannot
go stale; put standing notes in `NOTES.md` next to it. Claude Code reads
`CLAUDE.md`; Codex, Grok and pi read `AGENTS.md`. `hosts.md` is the hub's node
inventory.

The hub adds its own binary's directory read-only to every sleeve and puts it
last on `PATH`, so `protectorate` runs inside the sandbox. Sleeves share the
host network, so `127.0.0.1` reaches the hub. The binary must be installed
under the name `protectorate`.

## Scoping

Default scopes are `read`, `events` and the explicit names of the mutating,
non-destructive tools matching `backlog_*`, `lease_*`, `message_*`,
`sleeve_spawn_sleeve` and `sleeve_resleeve_sleeve`. In practice: read
everything, stream events, add and update backlog items, send, reply to and
ack mailbox messages (escalations land in Poe's chat and must be answerable),
spawn sleeves, and move a sleeve to another harness (`sleeve resleeve`, see
[chat.md](chat.md#resleeve)), which Poe may do without asking. Not granted by
default:

- token management, raw session creation, respawn and node add (owner-only
  whatever the scopes), `sleeve_kill_sleeve`, `session_kill_session` and
  node remove;
- the terminal;
- `session_*` creation. `session_create_session` and `session_respawn_session`
  start processes directly on the host, outside any sandbox, so a token that
  could call them would escape isolation. Poe starts work only through
  `sleeve_spawn_sleeve`, which is sandboxed. (The hub also refuses these RPCs
  for names starting with `sleeve-`.) Listing and capturing sessions is covered
  by `read`.

A denied call fails with `permission_denied` and is written to the audit log
(`protectorate audit tail`) as `token:orchestrator-poe`. The role text also
tells Poe to ask the user before any destructive action, even one its token
would allow. That is a convention for the model; the token is the enforcement.

## Widening it

```
protectorate orchestrator spawn --add-scopes sleeve_kill_sleeve     # allow kill
protectorate orchestrator spawn --scopes read,events,backlog_add_item   # replace the defaults
```

Scopes use the tool names from `protectorate mcp` (see [auth.md](auth.md)).
Widening takes effect on the next spawn, which revokes the old token. Granting
`terminal` lets Poe type into any session, which Poe needs to hand a prompt to
another sleeve; it is a large grant, so it is off by default.

## External bridges are the same mechanism

"AI as orchestrator" and "AI talking to the orchestrator" are one path: a scoped
token plus the CLI or the MCP bridge. A Claude or Grok bot on another machine
does the same thing Poe does, without the sleeve:

```
protectorate token create claude-bridge --scope read --scope 'backlog_*' --scope sleeve_spawn_sleeve
protectorate mcp --token-file ~/.protectorate/claude-bridge.token --allow 'quota_*' --allow 'backlog_*'
```

`quota_suggest` and `quota_get_quota` are available to either. See
[mcp.md](mcp.md).

## Security notes

The outside reviews warned that a sleeve able to spawn others tends to collect
holes until it is the hub. Poe keeps its sandbox and gets only a revocable,
expiring, audited token; the sandbox holds no harness credentials or SSH keys.
Poe can still spend quota by spawning sleeves, so the spawn scope is the thing
to watch in the audit log.
