# Spike: official headless drivers and native-resume handoff

Run 2026-10-19 on the dev box (aarch64 Debian), user's normal auth, throwaway
git repos under `.scratch/<harness>/` (gitignored). Question: can the hub drive
each harness through its own headless protocol for dispatched work, and can a
human then take over the same conversation in the normal TUI under tmux via
native resume (and back)?

Short answer: **yes for all four**, with three real caveats: Codex's TUI
pins threads inside a background daemon (headless must go through the daemon),
Grok's global config silently approves everything, and pi has no approval
concept of its own. Details below.

Driver scripts (Python, throwaway): `lib.py` (subprocess + log), `claude_drv.py`,
`codex_drv.py`, `codex_ws.py`, `acp_drv.py` (Grok), `pi_drv.py`,
`pi-approve.ts` (pi extension). Raw wire logs were kept in `.scratch/*.log`.
tmux used only a private socket (`tmux -L prot-drivers-<rand>`).

## Summary table

| | Claude Code 2.1.303 | Codex 0.157.1 | Grok 1.0.52 | pi 1.3.0 |
|---|---|---|---|---|
| Protocol | `claude -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio` (NDJSON) | `codex app-server` JSON-RPC 2.0 NDJSON on stdio (also WS on a unix socket) | `grok agent stdio` = ACP (JSON-RPC 2.0 NDJSON) | `pi --mode rpc` (JSONL commands/events) |
| Turn settled signal | `{"type":"result",...}` line (has `stop_reason`, `session_id`, cost, usage) | `turn/completed` notification (preceded by `thread/status/changed` -> idle) | the `session/prompt` response `{"stopReason":"end_turn"}` (also `turn_completed` update) | `{"type":"agent_settled","aborted":false}` (after `agent_end`) |
| Approval mechanism | `control_request` subtype `can_use_tool` on stdout; reply `control_response` with `{behavior:allow\|deny,updatedInput}` | server request `item/commandExecution/requestApproval` (and `item/fileChange/...`, `item/permissions/...`); reply `{decision:"accept"}` | ACP `session/request_permission` server request with options (`allow-once`, `allow-edits-session`, `reject-once`); reply `{outcome:{outcome:"selected",optionId}}` | None built in. Only via an extension: `ctx.ui.confirm` emits `extension_ui_request`, reply `extension_ui_response` |
| Native session id | UUID in `system/init` and every `result` (`eb57805d-...`) | UUIDv7 thread id from `thread/start` (`01a15335-8e61-...`) | UUIDv7 session id from `session/new` (`01a1533b-e553-...`) | UUIDv7 `sessionId` from `get_state` (`01a1533e-503d-...`) |
| Storage | `~/.claude/projects/<cwd-slug>/<id>.jsonl` | `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<id>.jsonl` | `~/.grok/sessions/<urlencoded-cwd>/<id>/{chat_history.jsonl,events.jsonl,...}` | `~/.pi/agent/sessions/<cwd-slug>/<ts>_<id>.jsonl` |
| Headless resume | `--resume <id>` keeps same id | `thread/resume {threadId}` keeps same id | `session/load {sessionId,cwd}` keeps same id (replays whole history as chunks first) | `--session <id>` (pi process arg, or `switch_session` RPC) |
| TUI resume | `claude --resume <id>` | `codex resume <id>` | `grok --resume <id>` | `pi --session <id>` |
| Headless -> TUI | works | works | works | works |
| TUI -> headless | works | **works only via the daemon socket**; a fresh `codex app-server` / `codex exec resume` fails with "already has an active writer" | works | works |
| Process startup to first output | ~1.1 s to `system/init`; 5-6 s for an `ok` turn | 0.4 s to initialize; ~4 s per `ok` turn | 0.25 s to initialize; `session/new` 20 s on first run, ~4 s afterwards | 0.45 s; ~2.5 s per turn (local LLM) |
| Cost of spike | 5 turns, ~$0.5 API-equivalent (Max sub) | 4 turns, ~15k tokens each | 5 turns, ~19k tokens each | 6 turns, local model, no quota |

## Per-harness findings

### Claude Code

Start (one process per session, can hold several turns on stdin):

    claude -p --output-format stream-json --input-format stream-json --verbose \
      --permission-prompt-tool stdio --permission-mode default [--resume <id>]
    stdin:  {"type":"user","message":{"role":"user","content":"Reply with exactly: ok"}}

Events: `system/init` (carries `session_id`, cwd, tools), `assistant` (whole
messages), `user` (tool results), `rate_limit_event`, then `result`. Without
`--include-partial-messages` there are no token deltas. Rate-limit windows
arrive in the stream for free (`rate_limit_event.rate_limit_info`, useful for P1).

Approval (trimmed):

    <- {"type":"control_request","request_id":"5cb0...","request":{"subtype":"can_use_tool",
        "tool_name":"Write","input":{"file_path":".../hello.txt","content":"hi\n"},
        "permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],
        "tool_use_id":"toolu_01Lb..."}}
    -> {"type":"control_response","response":{"subtype":"success","request_id":"5cb0...",
        "response":{"behavior":"allow","updatedInput":{...input unchanged...}}}}

The turn then continued to `result` normally.

Handoff: `claude --resume <id>` in tmux showed all prior turns and the
tool call, accepted a typed prompt, and appended to the same jsonl (same
`sessionId` on all 85 lines). Headless `--resume` afterwards remembered the TUI
turn. Gotchas:
- TUI shows a one-time "trust this folder" dialog for a never-seen cwd that
  headless `-p` skips. The hub must pre-trust or answer it (it is Claude's own
  `~/.claude.json` project record; we accepted it once in the scratch dir).
- The TUI picked "auto mode" from the user's settings while headless used
  `--permission-mode default`; modes are per-invocation, not per-session.
- No lock: opening the TUI, then running a headless turn on the same id
  works, but the open TUI does not show the new turn (it loads history at
  start) and both append to one file. Treat as unsupported/last-writer-wins.

### Codex

    codex app-server        # stdio
    -> initialize {clientInfo, capabilities:{experimentalApi:true}}; notification "initialized"
    -> thread/start {cwd, approvalPolicy:"untrusted", sandbox:"workspace-write"}   (or thread/resume {threadId, ...})
    -> turn/start {threadId, input:[{type:"text",text:"..."}]}
    <- turn/started, item/started|completed (userMessage, reasoning, agentMessage, commandExecution),
       item/agentMessage/delta, thread/tokenUsage/updated, account/rateLimits/updated,
       thread/status/changed {active|idle}, turn/completed

Schema is generated by `codex app-server generate-json-schema --out DIR`
(good for generating Go types). Policy values: `untrusted | on-request |
never | {granular}`; sandbox `read-only | workspace-write | danger-full-access`;
`approvalsReviewer: user|auto_review`.

Approval (trimmed):

    <- {"method":"item/commandExecution/requestApproval","id":0,"params":{"kind":"command",
        "threadId":"...","turnId":"...","itemId":"exec-0cb7...",
        "command":"/bin/bash -lc \"printf 'hi\\n' > hello.txt && cat hello.txt\"","cwd":"...",
        "proposedExecpolicyAmendment":[...],"availableDecisions":[...]}}
    -> {"jsonrpc":"2.0","id":0,"result":{"decision":"accept"}}
    <- serverRequest/resolved, then item/completed, turn/completed

Also `thread/status/changed` with `activeFlags:["waitingOnApproval"]` signals
that the thread is blocked on us. Note even a trivial file write goes through
the shell in `untrusted` mode, so approvals are command-shaped, not file-shaped.

Handoff, headless -> TUI: `codex resume <id>` shows the history and accepts
typed prompts (works). **Complication:** the TUI does not run its own engine.
It starts or attaches to a managed background daemon
(`~/.codex/packages/app-server-daemon/...`, running
`codex app-server --listen unix://`, here version 0.163.0 vs CLI 0.157.1,
control socket `~/.codex/app-server-control/app-server-control.sock`). It also
showed a "Background server has incompatible feature settings, restart?"
dialog on two of three launches (we chose "run without daemon this time"; the
daemon was running afterwards anyway; the third launch showed no dialog, and
keystrokes sent blind then landed in the prompt history instead).

TUI -> headless: after the TUI exits the daemon keeps the thread loaded as
its writer. A fresh `codex app-server` or `codex exec resume <id> --json`
then fails, still 10 minutes later:

    {"error":{"code":-32600,"message":"thread 01a15335-... already has an active writer"}}

`codex app-server proxy` hangs when fed JSON-RPC (it relays raw bytes and
expects WebSocket framing). Connecting to the daemon socket with a minimal
WebSocket client (`codex_ws.py`: HTTP upgrade on the unix socket, text
frames carrying the same JSON-RPC) works: `initialize` -> `thread/resume`
-> `turn/start` -> `turn/completed`, and the turn remembered the TUI turn.
So for Codex the hub should drive the daemon (or `codex app-server --listen`
that the TUI also attaches to), not private stdio processes. This is how
tincan attaches to a live TUI as well (see `harness-driving/README.md`).
`daemon stop` would clear the lock but affects every other Codex client, so
we did not use it.

### Grok

    grok agent stdio    # ACP, protocolVersion 1
    -> initialize {protocolVersion:1, clientCapabilities:{}}
    -> session/new {cwd, mcpServers:[]}   (or session/load {sessionId, cwd, mcpServers:[]})
    -> session/prompt {sessionId, prompt:[{type:"text",text:"..."}]}
    <- many session/update notifications (agent_message_chunk, tool_call, tool_call_update,
       response_completed, turn_completed) and Grok `_x.ai/*` extensions
    <- response to session/prompt: {"result":{"stopReason":"end_turn","_meta":{... usage ...}}}

Capabilities advertised: `loadSession`, `sessionCapabilities.{list,resume,close}`.

Approval: **our first attempt silently auto-approved** because the user's
`~/.grok/config.toml` has `[ui] permission_mode = "always-approve"` (the stream
showed `pending_interaction` then `interaction_resolved` with no request to
us). `grok agent stdio` has no flag to override it (`--always-approve` can
only add). Sending the slash command `/always-approve off` as a prompt
(zero tokens, official command, session-scoped; config file unchanged) turned
the gate on, and then:

    <- {"method":"session/request_permission","id":0,"params":{"sessionId":"...",
        "toolCall":{"toolCallId":"call-d10b...","kind":"edit","title":"Write `.../hello2.txt`",
        "rawInput":{"variant":"Write","file_path":".../hello2.txt","content":"hi\n"}},
        "options":[{"optionId":"allow-edits-session","kind":"allow_always"},
                   {"optionId":"allow-once","kind":"allow_once"},
                   {"optionId":"reject-once","kind":"reject_once"}]}}
    -> {"id":0,"result":{"outcome":{"outcome":"selected","optionId":"allow-once"}}}

(`session/set_mode` returned `{}` for any mode id and did not change behavior;
`session/set_config_option` only knows `model` and `reasoning_effort`.) The
TUI reopening the same session showed `always-approve` again: the toggle is
per-process, so the hub must send it on every `session/new|load`.

Handoff both directions works with `--resume <id>`; history (including the
`/always-approve off` line) was visible in the TUI, and the headless
`session/load` replays the whole history as `agent_message_chunk`s before the
new turn, so a driver must ignore updates until it sends its prompt. No lock:
TUI open + headless `session/load` + prompt both succeeded (zero-token slash
prompt only; a real concurrent turn was not tried). The TUI needed `/exit`
(Ctrl-C twice did not quit it); it prints the resume hint on exit.

### pi

    pi --mode rpc [--session <id|path>] [-e ext.ts]
    -> {"id":"s1","type":"get_state"}   -> data.{sessionId, sessionFile, model, isStreaming,...}
    -> {"id":"p1","type":"prompt","message":"..."}   <- response {disposition:"started"}
    <- agent_start, turn_start, message_start/end (system|user|assistant), tool_execution_start/end,
       turn_end, agent_end, agent_settled {aborted:false}

Message deltas arrive as `message_update` (filtered in our driver). The
session file path is returned directly, which is the easiest of the four to
locate. Observed: a `message_start` for the `system` role includes the whole
system prompt in the event.

Approval: pi writes files immediately with no gate (verified: `write` ran
with no request). The documented route is an extension
(`examples/extensions/permission-gate.ts` ships with pi; it only gates
dangerous bash). `pi-approve.ts` here gates `write|edit|bash` with
`ctx.ui.confirm`, loaded per-process with `-e`. In RPC mode that yields:

    <- {"type":"extension_ui_request","id":"38e4...","method":"confirm","title":"Approve write?","message":"{\"content\":\"hi\\n\",\"path\":\"hello2.txt\"}"}
    -> {"type":"extension_ui_response","id":"38e4...","confirmed":true}

Quirk: `tool_execution_start` is emitted before the confirm request, so a UI
must not show "running" at that event. Loading an extension alters the
harness, so gating pi means shipping our own extension; this contradicts "the
harness untouched" mildly (it is pi's supported extension point, but it is
ours code running inside it). The alternative is to treat pi as ungated and
rely on workspace isolation (sandbox/worktree), which also fits that it is the
local-LLM "free tier".

Handoff both ways works with `--session <id>`; TUI showed history, accepted a
typed prompt, then headless remembered it. No lock: a headless turn ran while
the TUI was open, and the TUI did not refresh (history is a tree via
parentId, so a later TUI turn would fork its own branch in the same file;
not tested). TUI exits with Ctrl-D.

## Concurrency and other caveats

- Only Codex enforces a single writer, and it does so for the whole daemon
  lifetime, not per live connection.
- Claude, Grok and pi allow a TUI and a headless process on one session at
  once; the open view goes stale and writers interleave in the same file.
  The hub needs its own rule: exactly one driver per session id (headless
  process OR tmux TUI), switching only when the other is stopped.
- Every handoff changed neither the session id nor the storage location, so
  the hub's record `{harness, cwd, session_id}` is sufficient. The cwd must
  match (all four key storage by cwd, Codex and Grok record it per session).
- No flag beyond the official mode choice was needed, except: Claude
  `--permission-prompt-tool stdio`/`--permission-mode`, Codex `approvalPolicy`
  and `sandbox` params, Grok the `/always-approve off` slash command, pi `-e`
  extension for approvals. All are session-scoped; no global config touched.
  Side effects of the harnesses themselves: Claude recorded folder trust in
  `~/.claude.json`; Codex TUI (re)started its user-level daemon; Grok
  rewrote nothing in config.
- Zero-token control surfaces worth using: Claude `rate_limit_event`, Codex
  `account/rateLimits/updated` (primary window 10080 min here), Grok
  `usage.json` and `response_completed.usage`.
- TUI first-run dialogs the hub must anticipate under tmux: Claude folder trust;
  Codex daemon feature-mismatch prompt; none seen for Grok or pi.

## ACP as the single protocol?

Findings only, nothing installed (`npm view` is read-only):

| Harness | ACP status |
|---|---|
| Grok | native (`grok agent stdio`), used here, works, plus many `_x.ai/*` extension notifications |
| Claude Code | no native flag (`claude --help` has no acp). Adapter `@agentclientprotocol/claude-agent-acp` 0.93.0 (updated today; predecessor `@zed-industries/claude-code-acp` 0.16.2), built on the Claude Agent SDK |
| Codex | no native `codex acp`. Adapter `@zed-industries/codex-acp` 0.16.0 (tiny npm launcher, native binary) |
| pi | adapter `pi-acp` 0.0.34 (wraps pi RPC) |
| none installed locally | only `@openai/codex` and `@earendil-works/pi-coding-agent` are in the global node dirs |

Assessment: ACP could be the single dispatch protocol only by depending on
three third-party Node adapters whose own session/resume semantics were not
verified here (do they reuse the harness's session files so the TUI can resume
them? unknown). The native protocols are close enough in shape that this is
not worth it: each is JSON-RPC or JSONL over stdio with the same five
verbs. Grok shows what ACP buys (uniform permission request shape), but also
that vendor extensions leak in anyway. Recommendation: write native drivers
for Claude, Codex and pi, use ACP only for Grok (and later Droid or other
ACP-native harnesses), and re-evaluate the adapters once the hub needs a harness
without a native headless protocol.

## Recommended minimal Go driver interface

Everything below was observed in all four.

    type Event struct {
        Kind    string          // "text", "tool_start", "tool_end", "approval", "settled", "usage", "raw"
        Text    string
        Tool    string
        Raw     json.RawMessage // original wire message, always kept
    }

    type Approval struct {
        ID     string
        Tool   string
        Input  json.RawMessage
        Allow  []string // harness-specific option ids; "allow_once" and "deny" always present
    }

    type Driver interface {
        // Open starts or resumes a native session. id == "" means new.
        Open(ctx context.Context, cwd, id string, opts OpenOptions) (Session, error)
    }

    type Session interface {
        ID() string                         // native session id, known after Open
        Send(ctx context.Context, prompt string) error
        Events() <-chan Event               // closes after Close
        Respond(ctx context.Context, approvalID string, allow bool) error
        Interrupt(ctx context.Context) error // pi abort, Codex turn/interrupt, ACP session/cancel, Claude control interrupt (not tested here)
        Close() error                       // must release any writer lock before a tmux handoff
    }

Notes for implementing it:

- `Settled` is an `Event`, not a return value: all four are asynchronous
  streams, and the hub must see settled exactly once per `Send`.
- Approvals are first-class `Event{Kind:"approval"}` with a pending `Approval`
  that the hub answers via `Respond`; unanswered approvals must be
  surfaced to the phone UI (they block the turn indefinitely in all four).
- `OpenOptions` should be small: `Mode` (supervised or auto) mapped to each
  harness's native knob; never invent modes a harness lacks (pi supervised
  needs the extension, otherwise unsupported).
- Handoff protocol: `Close()` the headless session, wait for the process
  (or Codex thread detach) to end, then `tmux new-session ... <harness resume
  cmd>`; reverse: send the harness's exit command in tmux, wait for the pane
  to die, then `Open(cwd, id)`. Keep exactly one owner per session id.
- Codex needs a different transport (daemon unix-socket WebSocket) than the
  other three; hide that inside the driver, and let the hub own the daemon
  lifecycle decision (use the user's, or start a private one with its own
  `CODEX_HOME`-free socket via `codex app-server --listen`).
- Generate Go types from `codex app-server generate-json-schema` and from
  ACP's schema; for Claude and pi write small hand-rolled structs (we only
  touch about 8 message types each).

## Quota used

| Harness | Turns | Approx cost |
|---|---|---|
| Claude (Max, Opus 5.5) | 5 | reported total cost ~$0.35 per process x several, ~$0.5 API-equivalent; ~25-50k cached input tokens per turn |
| Codex (GPT-6-Sol, ChatGPT Pro Lite) | 4 (+2 failed resumes) | ~15k tokens/turn; weekly window showed 0% used at start |
| Grok (4.7) | 5 (+2 zero-token slash commands) | ~19k input tokens/turn (mostly system prompt), ~100 output |
| pi (local `hotschmoe-dd`) | 6 | none |

## Open items not tested

- Mid-turn interrupt and steering in each protocol.
- Denying an approval (only allow paths were exercised; Claude's deny shape
  is `{behavior:"deny",message}`).
- A real concurrent turn from both sides in Grok and Claude.
- Whether the Claude and Codex ACP adapters write session files the TUI can resume.
- Headless Claude via the Agent SDK vs `claude -p`: same CLI underneath, so the
  `--resume` behavior is expected to match.
