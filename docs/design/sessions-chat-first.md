# Sessions, chat-first (2026-11-01)

The owner wants to work through chat: mostly with Envoy, sometimes in a sleeve's own chat, and in a
real terminal only as a last resort. SPEC 3.3 and 3.4 already say headless first, the transcript is
the chat, and terminal takeover happens at a turn boundary through native resume. This report
checks that plan against t3code (a chat-first GUI), herdr (a terminal agent herder), our old
drivers, and the harness binaries installed on the dev hub today.

**Citation prefixes:**
- `t3:` is `references/t3code` at 1451e7cb.
- `hd:` is `references/herdr` at 43b36b6.
- `pr:` is `git show dev-hotschmoe:rust/crates/<path>`.
- `pidoc:` is `references/secondary/pi/packages/coding-agent/docs`.

**Tags:**
- [V]: read in code.
- [H]: seen in `--help` output on 2026-11-01 (claude 2.1.316, codex-cli 0.164.0, grok 1.0.67, pi 1.4.0).
- [I]: inferred and not yet tested.

## 1. What t3code does

**Session model.** t3code is event-sourced:
- The orchestrator turns commands into events without doing I/O. One SQLite transaction then
  commits the events, the projections, the command receipt and the outbox effects. An effect
  worker runs the effects and feeds the results back in as commands
  [V `t3:apps/server/src/orchestration-v2/EventSink.ts:806`, `t3:docs/internals/overview.md:53-67`].
- The object graph is Project > AppThread > Run (one user-visible turn) > ExecutionNode tree. Beside
  it sit ProviderSession, ProviderThread, ProviderTurn, RuntimeRequest, ContextHandoff and
  Checkpoint [V `t3:docs/orchestration-v2/core-graph-and-data-model.md:5-27`].
- The native id is `ProviderRef {driver, nativeId, strength}`
  [V `t3:packages/contracts/src/orchestrationV2.ts:99,866`].

**Per-harness drivers.**

| Harness | How t3code drives it |
|---|---|
| Codex | `codex app-server` [V `t3:apps/server/src/provider/codexLaunchArgs.ts:12-15`]; `initialize` with `experimentalApi:true` and `optOutNotificationMethods:["turn/diff/updated"]` [V `t3:apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:226-230,1768`]; then `thread/start` or `thread/resume`, `turn/start`, `turn/steer`, `turn/interrupt`, `thread/compact/start`, `thread/read`, `thread/fork` (same file, :6219-6945). The schema is downloaded from the openai/codex repo at a pinned ref by `t3:packages/effect-codex-app-server/scripts/generate.ts:15-18,616-709`, not generated from the binary |
| Claude | **the Claude Agent SDK** (`query()`, Node), not `claude -p` [V `t3:.../ClaudeAdapterV2.ts:45,811`]. Options: `includePartialMessages:true`, `sessionId` on the first turn and `resume` afterwards, `canUseTool`, and a preset `claude_code` system prompt with `append` (:1098-1203) |
| Grok | ACP: `grok agent stdio`, with `agent --always-approve stdio` for full access or `--permission-mode auto\|default agent stdio` [V `t3:packages/provider-grok/src/server/acpSupport.ts:54-64`] |
| Pi | `pi --mode rpc ... --extension <t3 ext>` [V `t3:packages/provider-pi/src/server/mcpInjection.ts:265-280`]. The injected extension creates approvals with `ctx.ui.confirm` (`mcpExtensionSource.ts:317-333`). We would never do this, because it means installing our code into the harness |

**Event normalization.**
- Adapters emit `ProviderAdapterV2Event` [V `t3:packages/provider-core/src/server/ProviderAdapter.ts:79-175`].
  Its kinds are `app_thread.created`, `provider_session.updated`, `provider_thread.updated`,
  `provider_turn.updated`, `node.updated`, `subagent.updated`, `message.updated`,
  `turn_item.updated`, `command_output.delta`, `runtime_request.updated`, `plan.updated` and
  `turn.terminal`. `turn.terminal` carries a status (completed, interrupted, cancelled or failed
  with a failure) and a `threadDisposition: reusable|broken`.
- Persisted items are upserted whole by id. They are `OrchestrationV2TurnItem`
  [V `t3:packages/contracts/src/orchestrationV2.ts:1332-1534`], with these kinds:
  - conversation: `user_message`, `assistant_message`, `reasoning`, `proposed_plan`, `todo_list`;
  - requests: `user_input_request`, `approval_request`;
  - tools: `file_change`, `command_execution`, `file_search`, `web_search`, `dynamic_tool`;
  - checkpoints and interrupts: `checkpoint`, `run_interrupt_request`, `run_interrupt_result`;
  - notices and errors: `system_notice`, `error`;
  - session structure: `compaction`, `handoff`, `fork`, `thread_created`, `subagent`.
- Item status is one of `idle|pending|running|waiting|completed|failed|cancelled|interrupted`
  (:1211).
- Failure classes are `usage_limit`, `provider_error`, `transport_error`, `permission_error`,
  `validation_error` and `unknown` (:1233).
- `command_output.delta` is "never persisted": only clients showing that row see it
  (ProviderAdapter.ts:123-133).
- An older union, `ProviderRuntimeEvent` (`t3:packages/contracts/src/providerRuntime.ts:152-200`,
  `session.*`, `turn.*`, `item.*`, `content.delta`, `request.opened|resolved`, ...), is nearly dead.

**Approvals.**
- A `runtime_request.updated` arrives together with an `approval_request` item. The request status
  is `pending|resolved|expired|cancelled`. The decision is
  `accept|acceptForSession|acceptAlways|decline|cancel`
  [V `t3:packages/contracts/src/providerPolicy.ts:50`].
- Per harness:
  - Codex: the decision is the JSON-RPC result to `item/commandExecution/requestApproval`,
    `item/fileChange/requestApproval` or `item/permissions/requestApproval`, among others
    [V `CodexAdapterV2.ts:5032-5454`].
  - Claude: SDK `canUseTool`, answered with `{behavior:"allow",updatedInput}` or
    `{behavior:"deny"}`, plus `interrupt:true` on cancel [V `ClaudeAdapterV2.ts:2539`].
  - ACP: answers `session/request_permission` with `{outcome:{outcome:"selected",optionId}}`, picked
    by option kind (`allow_once`, `allow_always`, `reject_once`)
    [V `t3:packages/provider-acp/src/server/adapter.ts:912-922,5223`].
- Runtime modes are `approval-required|auto-accept-edits|auto|full-access`, with full-access as the
  default (providerPolicy.ts:25-32).

**Interrupt and resume.**

| Harness | Interrupt | Resume |
|---|---|---|
| Codex | `turn/interrupt {threadId,turnId}` | `thread/resume {threadId, excludeTurns:true}` |
| Claude | SDK `interrupt()`; the process group is killed on close | pre-allocated `sessionId`, later `resume` |
| ACP | `session/cancel`, then a kill on timeout (`AcpSessionRuntime.ts:2332-2366`) | `session/load`, or `session/resume` (:2179-2205) |
| Pi | `{type:"abort"}` | `switch_session` |

**Persistence and recovery.**
- The store is SQLite `statev2.sqlite` with the event log `orchestration_v2_events`, about 15
  projection tables and an effect outbox
  [V `t3:apps/server/src/persistence/Migrations/055_OrchestrationV2.ts:20-298`].
- Raw provider frames go only to rotating NDJSON debug logs (10 MiB x 10 files) and are not app
  state [V `t3:apps/server/src/config.ts:127,142`].
- **No provider process outlives the server**
  [V `t3:apps/server/src/orchestration-v2/ProviderRuntimeRecoveryService.ts:529`]. At startup:
  - in-flight runs and items are marked `cancelled`;
  - streaming messages are closed;
  - pending approvals become `expired`/`not_resumable`;
  - process-bound outbox effects are cancelled (:249-601, `EffectOutbox.ts:120-132,494`).
- Resume is lazy, on the next turn. If resume fails, it falls back to a fresh native session plus a
  context handoff (`ProviderTurnStartService.ts:523-704`).
- Optionally, it sends "Continue where you left off." up to 3 times (`RestartContinuation.ts:31-33`).
  That is a hub-composed prompt, which our hands-off rule forbids.

**Tool calls and diffs in the UI.**
- Consecutive work items are folded into one collapsed row with a summary such as "Read N files,
  Changed N files, Ran N commands", keeping the top 2 categories
  [V `t3:packages/client-runtime/src/work-log/presentation.ts:559-564,647`;
  `t3:apps/web/.../MessagesTimeline.logic.ts:1583-1720`].
- While a run is live, the row shows the latest tool instead.
- **Diffs come from git, not from the harness:**
  - every run gets a baseline and a post-run checkpoint, written as a tree into
    `refs/t3/checkpoints/...` using a temporary index (`CheckpointService.ts:161-266`,
    `GitVcsDriver.ts:796`);
  - the diff panel shows turn N-1 to N (`DiffPanel.tsx:308-324`);
  - Codex's own `turn/diff/updated` is opted out.
- Rollback runs `git restore`, `git clean -fd` and `git reset` in the user's worktree
  (`GitVcsDriver.ts:1072+`).

**Terminal.**
- t3code has a per-thread `$SHELL` PTY (node-pty) rendered with libghostty-vt in WASM
  (`t3:apps/server/src/terminal/Manager.ts:453-457`).
- **It never hands a session to the harness TUI.** There is no `claude --resume` or `codex resume`
  anywhere.
- The terminal is used for logins and installs (`apps/web/src/onboarding/providerReadiness.logic.ts:109,115`).

## 2. What herdr does that still matters

herdr is a TUI-first multiplexer: a `herdr server` daemon owns the PTYs (`hd:src/server/handoff.rs:86-102`).
Most of it is the pane world we are leaving. Five pieces still apply:

1. **Wait semantics** [V `hd:src/api/schema/agents.rs:34-42`, `hd:src/api/wait.rs:177-535`]:
   - `agent.wait {until[], timeout_ms}` defaults to `Idle|Done|Blocked`. It checks the current
     state first, then follows `events_after(seq)`, so no transition slips between the two.
   - `Done` means "idle and not yet seen" (`hd:src/app/api_helpers.rs:96-106`), which is our
     `Idle{seen}`.
   - `agent.prompt` with a wait first waits for the agent to **react** to the prompt, with a stall
     timeout, and only then for the target state. That is our prompt-ack step (Sent to Confirmed).
2. **Update handoff:**
   - The old server pauses its readers, sends a JSON manifest per pane, then sends the `dup()`ed
     PTY master fds over SCM_RIGHTS in batches of 64 [V `hd:src/server/handoff.rs:25-29,395-421`].
   - The handshake is `validated -> restored -> ready -> committed -> owned`, and the old server
     rolls back on any failure before commit [V `hd:src/server/headless/lifecycle.rs:60-234`].
   - We execve in place (D17), so no socket is needed, but the same commit point applies. Our
     headless sleeves are pipe fds, which pass the same way.
3. **Resume argv per harness** [V `hd:src/agent_resume.rs:199-310`]: `claude --resume <id>`,
   `codex resume <id>`, `grok --resume <id>`, `pi --session <path>`. herdr *types* the command into
   a fresh shell (`hd:src/app/agent_resume.rs:241-300`); we exec the argv directly (D18).
4. **Session identity:** herdr learns it from installed hooks
   (`hd:src/integration/claude_settings.rs:26-63` edits `~/.claude/settings.json`). We assign the id
   instead (`--session-id` for Claude and Pi) or take it from the protocol (Codex `thread/start`,
   ACP `session/new`). We never install hooks.
5. **Screen status rules** are per-harness TOML files [V `hd:src/detect/manifests/*.toml`,
   `hd:src/detect/manifest.rs:440-470,1094-1111`]:
   - matches come from regions such as `bottom_non_empty_lines(N)`, `prompt_box_body`, `osc_title`
     and `osc_progress`;
   - working to idle needs 3 confirmations at 100 ms (`hd:src/pane/agent_detection.rs:5-76`).

   We need this only for Antigravity panes and possibly for takeover panes (section 5). Skip the
   remote manifest updates (`hd:src/detect/manifest_update.rs:16`).

herdr's dependencies are tokio, ratatui, portable-pty (patched) and ghostty-vt built with Zig
(`hd:Cargo.toml:26-64`). None of these is worth taking.

## 3. A normalized transcript event model for Protectorate

**Design rules:**
- One append-only entry stream per sleeve. Every entry carries `seq`, `time`, `g` (the incarnation)
  and `turn` (a hub turn number).
- Text deltas are fanned out live under a reserved seq and persisted once, whole, when the block
  closes. That is the old chatlog design [V `pr:protectorate-chatlog/src/lib.rs:309-349,502-508`]
  and t3code's "never persisted" deltas.
- Status changes are new entries that point at an item `id`. The client folds them. We don't
  upsert, because append-only matches the journal and replay.
- Outputs are bounded (16 KiB, with a `truncated` flag), as in the old drivers.

| Kind | Fields | Shown as |
|---|---|---|
| `session_info` | harness, native_id, model, provider, effort, mode (headless or pane), g | a thin divider line ("Claude opus, g3") |
| `user_message` | id, text, from (owner, envoy, mail:<sleeve>, ga:<vendor>), request_id, acked | a chat bubble; ticks when the harness echoes it |
| `assistant_text` | id, text (deltas while streaming), final | a chat bubble |
| `reasoning` | id, text | collapsed |
| `tool_call` | id, tool kind (command, read, edit, search, web, mcp, subagent, other), name, summary (300 chars), input (bounded) | one row in the turn's collapsed work group |
| `tool_result` | id, status (completed, failed, declined, cancelled), output summary, truncated, duration_ms | folds into its call |
| `file_change` | per turn: [{path, op, added, removed}], diff_ref | "Changed N files"; tap for the diff |
| `approval_request` | id, tool kind, summary, options (allow once, allow for session, deny) | a card with buttons, and a push to the phone |
| `approval_decision` | id, decision, by (owner, envoy, rule, autonomous, expired) | folds into the card |
| `turn_started` | turn, native_turn_id, cause (user_message id, mail, resume) | the start of a work group |
| `turn_settled` | turn, stop (end_turn, cancelled, error, max_turns), usage {input, output, cache_read, cache_write, context_tokens, context_window}, duration_ms | a footer line: tokens, context % |
| `error` | class (usage_limit, auth, provider, transport, protocol, unknown), text | a red notice; G1, G2, G14 read the class |
| `compaction` | trigger (auto or manual), tokens_before, tokens_after | a divider |
| `notice` | text (retry, model rerouted, warning, takeover started or ended) | grey text |

`file_change` is computed by the hub from git at turn boundaries, as t3code does, because Claude and
Pi report no patches for shell edits. To keep the worked-on repo unpolluted, write the snapshot
trees into a hub-owned object directory: `GIT_OBJECT_DIRECTORY=<sleeve state>/objects` with the
repo's own objects as `GIT_ALTERNATE_OBJECT_DIRECTORIES`, and a private `GIT_INDEX_FILE`. No refs
go into the repo [I].

**Native events to entries.** "old" refers to the old driver's line numbers.

| Entry | Claude stream-json | Codex app-server | Grok ACP | Pi rpc |
|---|---|---|---|---|
| session_info | `system/init` session_id, plus `message.model` (old `claude-driver:296-308,337`) | `thread/start` or `thread/resume` result `thread.id`, `model`, `reasoningEffort`; `thread/settings/updated`; `model/rerouted` (old `codex-driver:199-235,700-721`) | `session/new` or `session/load` `sessionId` (old `grok-driver:323-347`) | `get_state` session id, model, provider, contextWindow, thinkingLevel (old `pi-driver:136-161`) |
| user_message ack | the `user` echo with `--replay-user-messages` [H]; not passed by the old driver | `turn/start` result `turn.id` (old `:393-395`) | first `session/update` after `session/prompt` [I] | `response` to `prompt`, `data.disposition` [V `pidoc:rpc.md:58-69`] |
| assistant_text | `assistant` `text` blocks (old `:358-367`); deltas need `--include-partial-messages` [H], which the old driver did not pass | `item/agentMessage/delta`, final in `item/completed` agentMessage (old `:728-739`) | `agent_message_chunk` (old `:532-538`) | `message_update` `text_delta`; authoritative `message_end` [V `pidoc:json.md:62-72`] |
| reasoning | `thinking` blocks (old `:361-366`) | `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta` | `agent_thought_chunk` | `message_update` `thinking_delta` |
| tool_call | `assistant` `tool_use` block (old `:369-376`); skip `parent_tool_use_id != null` subagent traffic (old `:332`) | `item/started` with ThreadItem `commandExecution`, `fileChange`, `mcpToolCall`, `dynamicToolCall`, `webSearch`, `imageView`, `collabAgentToolCall`, `subAgentActivity` (schema [H]; old `:740-755`) | `tool_call` (title, kind, rawInput) (old `:539-545`) | `tool_execution_start` (old `:574-577`) |
| tool_result | `user` `tool_result` block, `is_error` (old `:382-410`) | `item/completed`, status `completed\|failed\|declined` (old `:756-768` handles only commandExecution) | `tool_call_update` status `completed\|failed` (old `:546-563`) | `tool_execution_end` `isError`, `durationMs` [V `pidoc:json.md:101-107`] |
| file_change | none native; hub git diff | `fileChange` item (+ `turn/diff/updated`, which the hub need not use); hub git diff | `tool_call` kind `edit` with diff content [I]; hub git diff | hub git diff |
| approval_request | `control_request` `can_use_tool` (old `:411-446`) | server requests `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `item/permissions/requestApproval` (old `:647-692`); also `mcpServer/elicitation/request` and `item/tool/requestUserInput`, which the old driver refuses with -32601 | `session/request_permission` with options (old `:589-640`) | none: Pi has no gate |
| approval_decision | `control_response` `{behavior:"allow",updatedInput}` or `deny` (old `:187-208`) | JSON-RPC result `{decision:"accept"\|"decline"}`, or `{permissions,scope}` (old `:426-450`) | `{outcome:{outcome:"selected",optionId}}` by option kind (old `:735-760`) | n/a |
| turn_started | first event after the send (no native turn id) | `turn/started` `turn.id` | `session/prompt` sent (no native id) | `agent_start` |
| turn_settled | `result`: stop_reason or subtype, is_error, `modelUsage[].contextWindow`; usage from `assistant.message.usage` per message id (old `:340-356,447-493`) | `turn/completed` `completed\|interrupted\|failed`; `thread/tokenUsage/updated` (old `:785-843`) | `session/prompt` response `stopReason`; usage from `_x.ai/session_notification` (old `:500-525,568-587`) | `agent_settled {aborted}`, not `agent_end`; usage from `message_end.message.usage` (old `:599-639`, `pidoc:json.md:45-56`) |
| error | `result.is_error`, malformed line (old `:480-482`) | `error` notification, failed turn, `turn.error` [I] | JSON-RPC error on `session/prompt` | `response success:false` for `prompt`; `auto_retry_end success:false` [V `pidoc:json.md:161-170`] |
| compaction | `system/compact_boundary` `pre_tokens`, `post_tokens`, `trigger` (old `:309-321`) | `item/completed` `contextCompaction`; `thread/compacted` (old `:769-781`) | not verified; `/compact` is refused by the old driver (`grok-driver:296`) | `compaction_start`, `compaction_end` (old `:641-660`) |
| notice | `system/api_retry` [I] | `warning`, `model/rerouted`, `configWarning` | none | `auto_retry_start`, `queue_update` |
| interrupt (input) | `control_request {subtype:"interrupt"}` (old `:174-186`) | `turn/interrupt {threadId,turnId}` (old `:410-424`) | `session/cancel` notification (old `:235-257`) | `{type:"abort"}` (old `:433-440`) |

Steering: Codex `turn/steer` with `expectedTurnId` and Pi `steer` are native. For Claude and Grok,
the hub queues the message until `turn_settled`.

## 4. Terminal takeover per harness

The common flow:
1. Wait for `turn_settled` with no pending approval, and hold incoming mail.
2. End the headless process and **wait for it to exit**.
3. Exec the TUI argv in the same sandbox, with the same private home, the same cwd path and the same
   permission mode.
4. On handback, wait for the TUI to exit, then relaunch headless on the same native id.
5. Verify that the native id the harness reports matches (old Claude driver `:296-307` already
   kills on a mismatch).
6. **Backfill** the turns the owner did in the TUI into the chat from the harness's own read path,
   tagged `mode: pane`:
   - Claude: read the native JSONL;
   - Codex: `thread/turns/list` or `thread/read`, without `excludeTurns`;
   - Grok: the `session/load` replay, which the old driver drops before the first send
     (`grok-driver:6-7,520`);
   - Pi: `get_messages`.

| Harness | Headless to TUI | TUI to headless | Gotchas |
|---|---|---|---|
| Claude | `claude --resume <id>` (same `--permission-mode`, `--settings`) [H] | `claude -p --input-format stream-json --output-format stream-json --verbose --replay-user-messages --include-partial-messages --permission-prompts host --permission-prompt-tool stdio --permission-mode manual --resume <id>` [H; old argv `pr:protectorate-hub/src/chat.rs:1330-1368`] | see below |
| Codex | `codex resume <id>` [H] | `codex app-server`, `initialize`, `thread/resume {threadId}` | see below |
| Grok | `grok --resume <id>` (`-r`), same `--always-approve` or `--permission-mode` [H] | `grok agent [--always-approve] --no-leader stdio`, then `session/load` [H] | see below |
| Pi | `pi --session <id>` [H] | `pi --mode rpc --session <id>` (create with `--session-id <uuid>`) [H] | see below |

**Claude:**
- The store is `~/.claude/projects/<cwd-slug>/<id>.jsonl`, keyed by the cwd, so the bwrap mount
  path must be identical in both modes.
- There is no lock file. Per `--bg` help, resuming a session that is still running "starts a copy
  and says so" [H], so a lingering headless process forks the conversation. Hence the wait for
  exit.
- Headless sessions are tagged `"entrypoint":"sdk-cli"` and TUI ones `"cli"` (3631 vs 60036 lines
  on this host). The `/resume` picker may filter on that, but an explicit id should not [I].
- `--system-prompt-snapshot` is on by default: the first prompt is recorded and reused on every
  resume until compaction [H]. That keeps the cache warm across the switch, but an edited
  `--append-system-prompt` won't take effect until then.
- `--permission-prompt-tool` is not in the option list. Only `--permission-prompts` mentions it [H],
  so treat it as unstable.

**Codex:**
- `~/.codex/thread-writer-locks/` exists [V on this host], which means one writer per thread. The
  app-server must be gone, or must have released the thread, before the TUI resumes it.
- The TUI may go through the shared app-server daemon (`codex agents`,
  `~/.codex/app-server-daemon/daemon.pid` [V on this host]). A daemon started inside the sleeve
  could outlive the PTY and keep the lock. Kill the scope on handback [I].
- `resume --include-non-interactive` [H] shows that app-server threads are hidden from the picker by
  default. An explicit id is fine [I].
- **Alternative:** `codex resume <id> --remote unix://PATH` [H] attaches the TUI to *our* running
  app-server when that server uses `--listen unix://PATH`. No restart, and the chat keeps streaming.
  Unknown: which client receives approval requests. Worth a spike.

**Grok:**
- The leader model: `~/.grok/leader.sock`, and `agent leader --relay-on-demand` is "passed by leaders
  auto-spawned from interactive clients" [H]. Also `~/.grok/active_sessions.json` + `.lock`
  [V on this host].
- A TUI may leave a leader running that owns the session. Use `--no-leader` for stdio [H], give the
  TUI a per-sleeve `--leader-socket` under the private home, and kill leftovers in the scope.
- Approval mode: the old driver set it with a hub-typed `/always-approve on|off` prompt
  (`grok-driver:348-361`), which SPEC 3.4 replaces with the flag. The TUI needs the same flag.

**Pi:**
- The store is `~/.pi/agent/sessions/<cwd>/` [V `pidoc:sessions.md:50-54`]. No lock is documented,
  so wait for exit.
- Never pass `--no-session` (the old auxiliary Pi did, `chat.rs:105-114`).
- Sessions are trees: TUI `/tree` navigation can move the active leaf that rpc then resumes [I].

**Antigravity** stays pane-only (D13).

## 5. Recommendations

**Adopt:**
1. **The entry model in section 3** as the only chat format, from hub to app. `pt-drivers` maps each
   protocol to it. Envoy reads the same stream through `protectorate envoy read`. Deltas are
   ephemeral and coalesced at close (chatlog), and everything else is append-only with an item id.
2. **Collapsed work groups with a summary line** per turn ("Read 3 files, ran 2 commands, changed 4
   files"), computed in Dart from `tool kind`, as in t3code. Raw tool output opens on tap.
   Command-output streaming is shown only to a client that opened the row, and never journaled.
3. **Diffs from git at turn boundaries**, hub-side, with snapshot objects outside the worked-on repo
   (section 3). Don't trust or require harness patches. No rollback: t3code's revert runs
   `git clean -fd` in the user's tree, which we don't do.
4. **Error classes on `error` entries** (`usage_limit`, `auth`, ...) so guardrails G1 and G2 read a
   field instead of text.
5. **Restart rule from t3code, minus the auto-prompt:**
   - in-flight turns become `Uncertain`, then `cancelled` after the probe;
   - pending approvals become `expired`, since the process that asked is gone;
   - resume is native.
   - Never send "continue where you left off": a nudge, if any, comes from Envoy.
6. **herdr's wait:** check the current state, then follow `events_after(seq)`. The default `until`
   is `idle|done|blocked`. `send --wait` first waits for the prompt ack with a stall timeout.
7. **The takeover flow in section 4, with backfill.** That way the chat stays complete even when the
   owner worked in the TUI. Hold mail and guardrail timers G6 to G8 during takeover. The owner is at
   the keys.
8. **No VT model on the hub for takeover panes.** Keep the PTY and a raw byte ring, and let the
   client's terminal widget render it. Derive takeover-pane activity from the native log tail
   (Claude JSONL, Codex rollout, Pi JSONL), which is deterministic, instead of screen rules.
   Blocked is shown as "owner at terminal". Only Antigravity needs screen rules, and a few
   herdr-style rules on OSC title and progress plus the bottom lines suffice. This simplifies SPEC
   3.3's "VT screen model" and needs an owner nod.
9. **Protocol drift check in `doctor`:** run `codex app-server generate-json-schema --out <tmp>` [H]
   and diff the method lists (today 106 client requests, 10 server requests, 85 notifications)
   against the set the driver handles. Unknown server requests get a JSON-RPC error and an `error`
   entry, as in the old driver (`codex-driver:667-670`). This uses no codegen and no dependency.
10. **Launch flags to add to today's argv:**
    - Claude: `--replay-user-messages` and `--include-partial-messages`;
    - Grok: `--no-leader`;
    - Pi: `--session-id <uuid>` at create.
11. **Threads:** per child, one reader thread (lines capped at 8 MiB), one writer thread (`LineInput`
    as today) and one waiter. All events go to the core thread over a channel. Approval answers
    are effect records (Record before effect).

**Skip:**
- the Claude Agent SDK and anything Node (we speak the same stream-json the SDK uses);
- t3code's 15 projection tables (our journal plus `apply` already is the event source);
- checkpoint refs in the repo, and rollback;
- harness extensions or hooks for approvals or state (t3code's Pi extension, herdr's Claude hooks);
- typed resume commands;
- remote detection manifests;
- ghostty-vt, portable-pty and tokio;
- OpenCode and Cursor drivers.

**Risks:**
- Codex app-server is marked `[experimental]` and broad. The drift check limits the damage.
- Claude's `--permission-prompt-tool stdio` is hidden. If it disappears, approvals fall back to
  `--permission-prompts none` plus a permission mode, so supervised Claude would lose per-tool
  approval. Test this at every Claude update.
- Session-store contention at takeover: a Codex daemon or a Grok leader can survive the TUI.
  Mitigation: kill the scope, and verify the native id on relaunch.
- Backfill parses native logs, which are private formats. Keep it read-only and best-effort: a
  failed parse yields one `notice`, never a broken chat.
- Pi tree navigation in the TUI can change which branch rpc resumes. Check `get_state` and show it
  in `session_info`.
