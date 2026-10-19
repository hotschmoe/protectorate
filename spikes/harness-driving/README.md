# Research: how reference projects drive harnesses

Read-only survey of `references/` (2026-10-19). File paths are relative to
`references/`.

## Summary

**No mature orchestrator here drives a harness through its TUI.** They all use
the harness's structured headless protocol and use PTYs only for
user-facing terminals.

| Harness | Structured protocol | Resume handle | Used by |
|---------|--------------------|---------------|---------|
| Codex | `codex app-server` JSON-RPC (stdio / ws / unix socket): `thread/start`, `turn/start`, `turn/steer`, `turn/interrupt`, `thread/resume`, approval requests | thread id | t3code `apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts`; tincan attaches to a *live* TUI's app-server socket (`tincan-plugin/cmd/tincan/codex_websocket.go`) |
| Claude Code | Agent SDK `query()` / `claude -p --output-format stream-json`, `canUseTool` for approvals | session id (`--resume`) | t3code `ClaudeAdapterV2.ts` |
| Grok (+ ACP agents) | ACP over stdio (`grok agent stdio`) | session id | t3code `packages/provider-grok`, `packages/provider-acp` |
| pi | `pi --mode rpc` JSONL: `prompt`, `steer`, `follow_up`, `abort`, `get_state`, `switch_session`, events end in `agent_settled` | session file (`--session`) | t3code `packages/provider-pi` |
| Droid | `droid exec --input-format stream-jsonrpc --output-format stream-jsonrpc`, or ACP | session id | factory `docs/cli/droid-exec/overview.mdx` |
| OpenCode | `opencode serve` HTTP + SDK | session id | t3code `packages/provider-opencode` |

What a structured protocol gives that tmux screen-scraping can't: typed
approval requests, a reliable "turn settled" signal, steering, token usage,
and durable resume ids.

## t3code (closest analogue to the hub)

- One Node server per machine (an "environment") owns provider processes,
  PTYs, git and state. Web, Electron and React Native clients are thin, with
  shared logic in `packages/client-runtime`.
- Transport: Effect RPC over WebSocket, JSON serialization
  (`apps/server/src/ws.ts`). Per-thread subscriptions with replay cursors.
- `ProviderAdapterV2` (`packages/provider-core/src/server/ProviderAdapter.ts`):
  open/ensure/resume thread, start/steer/interrupt turn, respond to runtime
  request, snapshot, rollback, fork. It emits a normalized event union, and
  capability flags mean orchestration never branches on provider.
- Event-sourced SQLite (`~/.t3/userdata/statev2.sqlite`). Provider processes
  die with the server and are recovered by resuming the native thread ref.
- Four permission modes mapped onto each provider's *native* enforcement.
  It never emulates a mode the provider lacks.
- Remote: each machine is its own environment, and clients walk a list of
  routes (LAN, Tailscale, SSH tunnel, T3 Connect relay). Threads never move
  between environments.
- Exposes `/mcp` so outside agents can create, message and wait on threads.

## factory

- `droid daemon --remote-access` dials out to a relay, so remote machines
  ("Droid Computers") need no inbound ports.
- `cloudSessionSync` mirrors CLI sessions to the web.
- Mobile story in their VPS guide: SSH from Termius.

## pi

- RPC mode is described above. There's no built-in approval gate; it would
  have to come from extension `confirm` dialogs.
- `packages/env`: a small Rust daemon (`pi-env serve`) deployed over SSH and
  verified by SHA-256. It speaks a framed stdio protocol (fs, exec, pty,
  watch). The agent's *tools* run remotely while the harness and credentials
  stay local. This closely matches our "hub holds logins, nodes do work"
  model (P2/P3).
- `packages/durable`: commit-before-display transcripts, `resume()`,
  idempotent `requestId`, `watch()` for remote viewers. It's independent of
  `pi --mode rpc`.

## Messaging

- **tincan**: hosted rooms, per-agent identities, a durable inbox over SSE,
  and per-harness wake delivery (Claude via hooks, Codex via the app-server
  socket).
- **mcp_agent_mail**: an MCP server with an inbox/outbox per agent per project,
  advisory file-reservation leases with a pre-commit guard, and markdown in git
  plus a SQLite FTS index.

## CodexBar

- It has a cross-platform **Linux CLI** (`codexbar`): `usage --json`,
  `serve` (HTTP JSON), `guard` (gate automation on remaining quota) and
  `sessions --json-v2` (discovers live Codex, Claude and pi sessions locally and
  over SSH).

## Implications for Protectorate

1. **Use a hybrid session model.**
   - *Human-attached* sessions run as the harness TUI under tmux, reachable
     from the phone. This is how the user works today.
   - *Dispatched* work (backlog, orchestrator delegation) uses each
     harness's structured protocol, supervised by the hub, which stores
     `{harness, cwd, native_session_id}`.
   - Both meet through native resume: `claude --resume <id>`,
     `pi --session`, `codex resume`. A human can take over a dispatched
     session in tmux, or hand a tmux session back to dispatch.
2. **Consider ACP as the one dispatch protocol.** Grok and Droid speak it
   natively, and adapters exist for Claude and Codex. One Go ACP client could
   drive every harness. Each adapter is a dependency to evaluate, though.
3. **The pi-env pattern** (a tiny daemon deployed over SSH, speaking stdio)
   independently confirms `spikes/ssh-agentless`.
