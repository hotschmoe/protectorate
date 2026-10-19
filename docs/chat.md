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

`--mode terminal` spawns the TUI under tmux instead (always for `shell`). A
sleeve keeps its mode for life; a terminal sleeve has no chat.

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

Not verified: Grok compaction over ACP, Claude cache expiry against the
subscription TTL, approvals from a real harness (sleeves run autonomous),
reconnect mid-message from the app against a real stream.
