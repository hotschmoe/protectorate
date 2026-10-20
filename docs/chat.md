# Chat

A sleeve in chat mode (the default for claude, codex, grok and pi) runs its
harness through the harness's own headless protocol, inside the sleeve sandbox
and systemd scope, so limits and cgroup usage work as for any sleeve. The hub
normalizes what the harness emits into events in a per-sleeve log, and any
number of clients watch it. The harness's own session store stays the full
record; the log is the view. Design: ARCHITECTURE.md, "Harness sessions".

```
protectorate sleeve spawn alice --workspace ~/src/proj --harness claude   # chat mode
protectorate chat watch alice                  # replay, then follow
protectorate chat send alice "run the tests"
protectorate chat interrupt alice
protectorate chat respond alice <approval-id> --allow
protectorate chat compact alice
```

The workspace is a directory on the hub or a git URL (cloned into
`workspaces/<name>` in the hub's state directory; the hub has no git
credentials, so the repository must be readable without a login). `protectorate
sleeve workspaces` lists what the hub can reuse: directories under its
`workspaces/` and the workspaces of known sleeves. A directory under
`workspaces/` can only be used by a sleeve of the same name; any other absolute
path outside the state directory is accepted if it passes the sandbox checks.

In the app, the SLEEVES tab shows every sleeve in a fence per node (the hub's
node first; sleeves run only on the hub's machine today, so other nodes get a
fence only if they report sleeves). NEW SLEEVE, or the + on the hub's fence,
opens the start dialog: EXISTING (the list above), CLONE (a git URL) or PATH
(an absolute directory on the hub). The sleeve name is suggested from the
repository or directory and the new sleeve opens when it starts. RAW SESSIONS
on the same tab opens the owner-only unsandboxed tmux sessions.

`--mode terminal` spawns the TUI under tmux instead (always for `shell`). A
sleeve keeps its mode for life; a terminal sleeve has no chat.

A chat harness runs autonomous: the sandbox is the boundary. `sleeve spawn
--supervised` runs it in its asking mode instead (Claude `--permission-mode
default`, Codex `approvalPolicy` `untrusted` with its `workspace-write`
sandbox, Grok `/always-approve off`; pi has no gate), so approvals arrive as
`ApprovalRequest` events and `chat respond` answers them.

## API

`ChatService` (`proto/protectorate/v1/chat.proto`). `Watch` needs the `read`
scope (a sleeve's own token may watch only its own sleeve); every call is
covered by `chat_*`.

- **Watch(sleeve, after_seq)** replays stored events with `seq > after_seq`,
  then streams live ones, with no gap or duplicate at the boundary.
- **Send(sleeve, text, request_id)** starts a turn. A retry with the same
  `request_id` (8-128 chars) returns the original `seq`, for an hour and across a
  hub restart (`chat.jsonl.request-ids` beside the log). During a turn it
  steers on harnesses that can (Codex, pi) and fails with `failed_precondition`
  on the rest (Claude, Grok): interrupt first.
- **Interrupt** cancels the running turn; a no-op when idle.
- **Respond(approval_id, allow, note)** answers an `ApprovalRequest`.
- **Compact** runs the harness's native compaction and returns after the
  harness confirmed it. `failed_precondition` unless the chat is IDLE, or when
  there is too little to compact; `unimplemented` for Grok, where `/compact`
  over ACP is not verified.

`Sleeve.chat` (`ChatStatus`) is kept current: state, harness session id, model,
`context_tokens` (the prompt size of the last model call), `context_window`,
`cache_expires_at` (Claude only), `last_seq`, `provider` and `local_model`.
Every state change is published on EventService as `chat.state` (subject =
sleeve name).

Provider and local serving. Drivers report the provider with each usage
update: claude `anthropic`, codex `openai`, grok `xai`. pi reports the
`provider` and `baseUrl` of the `model` object in `get_state` (see
`rpc-commands.md#model-object`), and the `provider` of each assistant
message; a message with a different provider than `get_state` drops the
`baseUrl`, since pi's message carries no endpoint. The hub sets `local_model`
when the base URL host is loopback, private (RFC1918, fc00::/7 including the
tailnet ULA fd7a:115c:a1e0::/48), link-local, CGNAT (100.64.0.0/10, tailnet),
a single-label name, `.local`, `.localhost` or `.ts.net`, or a host or name in
the node inventory or in `hub serve --local-model-host` (repeatable). No base URL (a named hosted provider) is never local, so
DeepSeek, GLM, Kimi and the like over API keys show no warning. Like the model
name, both fields are empty until the first model call. Example: a pi provider `hotschmoe-local` with baseUrl
`https://llm.hotschmoe.com/v1` is a public DNS name for the user's own server,
so it is local only with `--local-model-host llm.hotschmoe.com`; a provider
`hotschmoe` at `http://192.168.10.5:18080/v1` is local by its private address.
A tailnet with a custom
MagicDNS suffix other than `.ts.net` is only recognised by IP or inventory entry.

## Resleeve

```
protectorate sleeve resleeve alice --to codex      # or --to auto
protectorate sleeve resleeve alice --interrupt     # do not wait for the turn
```

`SleeveService.ResleeveSleeve` (MCP `sleeve_resleeve_sleeve`, owner and the
orchestrator's default scopes) moves a chat sleeve to another harness on the
same workspace under the same name. The outgoing harness is never asked
anything. Decision record: `docs/memory-options.md`, sections 3 and 4.

1. The hub takes the chat over: `Send` and `Compact` fail with
   `failed_precondition` (the mailbox waits), then it waits for IDLE. A turn
   still running after `--wait` (default 5m), or at once with `--interrupt`,
   is interrupted; one that does not settle 30s later is closed under it.
2. Facts, composed in code (`internal/handoff`): git status, `diff --stat
   HEAD` and the last 10 commits (run in a read-only bwrap jail, since the
   repository config is agent-written), the hub notes the sleeve wrote or
   tagged with its project, open ack-required messages, whether
   `.protectorate/NOTES.md` exists, and from the outgoing harness's own
   transcript (read-only, inside the sleeve home: Claude
   `.claude/projects/*/<id>.jsonl`, Codex `.codex/sessions/Y/M/D/rollout-*-<id>.jsonl`,
   Grok `.grok/sessions/*/<id>/chat_history.jsonl`, pi
   `.pi/sessions/*_<id>.jsonl`) the final assistant message, the latest user
   message, the four before it and the first one. Each item is clipped (keeping
   its end), the whole within 16 KiB, as t3code budgets its handoffs.
   Harness-added context (a message wholly wrapped in tags) is left out.
3. Summary: a throwaway helper sleeve (`sum-<name>`, empty workspace, hidden
   from `sleeve ls`, never a mailbox recipient) runs the best subscription
   from `quota suggest` other than the outgoing harness, in its asking mode
   with every tool request denied, and answers Goal / State / Unfinished /
   Next / Gotchas from the facts. With no subscription to spare, or when it
   fails, pi (the local LLM) writes it; failing that the handoff holds the
   facts only. It runs while the outgoing turn winds down, on the facts known
   when the resleeve started; the file's facts are read after the turn ended.
4. `<workspace>/.protectorate/HANDOFF.md` is overwritten (atomically, through
   an `os.Root`, refusing a symlinked `.protectorate`), after `.protectorate/`
   is added to the repository's `info/exclude`; a copy goes to
   `<state dir>/handoffs/<name>/<time>-<from>-to-<to>.md`.
5. The sleeve is stopped and spawned again on the new harness: same name,
   workspace, limits, env and supervision, a fresh identity token, and the
   chat log continues (a `Handoff` event marks the switch). If the new
   harness does not start, the old one is spawned again on its own native
   session and the call fails.
6. The first `Send` is a plain user message from `hub`: "Continue the task in
   this workspace. Read .protectorate/HANDOFF.md first ...".

`Sleeve.handoff` keeps the record (who summarized; `summarizer_local` makes the
app show LOCAL SUMMARY). Events: `resleeve.started`, `sleeve.resleeved`, and
`resleeve.warning` when pi or no model wrote the summary. The outgoing
harness's native session stays in the sleeve home; resleeving back later
starts a new session that reads the newer handoff.

## The log

`<state dir>/sleeves/<name>/chat.jsonl`, one proto-JSON `ChatEvent` per line,
`seq` from 1. A new sleeve with a reused name moves the old log aside as
`chat.jsonl.<unix time>`. A partial last line left by a crash is cut off on
open.

Assistant text is coalesced: the first delta of a message reserves its `seq`;
live watchers receive the deltas, all carrying that `seq`, and the stored event
is the whole message under the same `seq`. A watcher that attaches mid-message
first gets the text so far, then the following deltas. To resume after a
disconnect, pass the last `seq` seen; if the last event was a text delta, pass
its `seq` minus one and replace the message with that `message_id`. A watcher
that falls 4096 events behind is cut with `resource_exhausted` and resumes the
same way.

State, highest priority first: AWAITING_APPROVAL, WORKING, COMPACTING, IDLE;
FAILED and EXITED when the driver is gone. The driver starts at spawn, stays
open until the sleeve is killed or the hub stops, and restarts on the next Send
after a crash or a hub restart, resuming the native session. A session id is
only resumed once a turn was sent, since harnesses store a session only then.

A hub restart while a turn, an approval or a compaction was running leaves the
sleeve FAILED, with an error event "hub restarted mid-turn" (or
"mid-compaction") in the log, as soon as the hub is up. The turn is not
replayed; the next Send opens the harness again and resumes the same native
session (`--resume`, `thread/resume`, `session/load`, `--session`). An idle
sleeve comes back IDLE.

## Per harness

| | Claude | Codex | Grok | pi |
|---|---|---|---|---|
| Autonomy in a sleeve | `bypassPermissions` | sleeve `config.toml`: never / danger-full-access | `/always-approve on` | no gate |
| Steer mid-turn | no | yes | no | yes |
| Approvals | yes | yes (none under the sleeve config) | yes | no |
| Context window | from `result.modelUsage` | `modelContextWindow` | unknown | model's `contextWindow` |
| Cache expiry | 5m or 1h from the usage buckets | no | no | no |
| Manual compact | yes | yes | no | yes |

Native auto-compaction is left at each harness's defaults; when one happens the
log gets a Compaction event with `native=true`. Context and cache numbers come
from per-call usage and are zero until the first call.

## Verified live (2026-10-20, ms-r1, real logins via `protectorate login`)

One chat sleeve per harness on a scratch git workspace, prompt "Read the file
hello.txt in the current directory and reply with only its first word":

| | Tool call seen | Answer | Context / window | Cache read on 2nd call | Manual compact |
|---|---|---|---|---|---|
| Claude | `Bash` head | banana | 23677 / 1000000 | 23554 | 23681 -> 1978 |
| Codex | `commandExecution` | banana | 14372 / 475000 | 12288 | 14372 -> 8377 |
| Grok | `read_file` | banana | 16594 / unknown | 16384 | unimplemented |
| pi (hotschmoe-local / hotschmoe-dd) | `read` | banana | 1724 / 200000 | 1679 | "nothing to compact" (too small) |

pi's thinking and answer arrive as separate events under one message id. Hub
restart: drivers die with the hub (`--die-with-parent`, no orphans), and the
next Send resumed Claude's native session after its compaction (it recalled
the earlier answer; context 24468, consistent with the compacted history).

Verified in the dogfood run (`docs/dogfood-2026-10-20.md`): a real Claude
approval from a `--supervised` sleeve answered through `Respond`, and two
resleeves (Claude to Codex while idle, Codex to Claude interrupting a turn),
whose new harness read the handoff on its first turn.

Not verified: Grok compaction over ACP, Claude cache expiry against the
subscription TTL, approvals from Codex and Grok, reconnect mid-message from
the app against a real stream, the local-model summary fallback with a real
pi.
