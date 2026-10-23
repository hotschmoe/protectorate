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
protectorate chat wait alice --until idle --timeout 10m
protectorate chat wait alice bob --until report --timeout 30m
```

## Results and completion

Finish every brief from inside its sleeve with a marked result:

```sh
protectorate report --status complete --summary-file result.md
printf '%s\n' 'Implementation is partial; host validation remains.' | protectorate report --status partial -
protectorate report --status blocked --summary-file blocker.md
```

Statuses are `complete`, `partial` and `blocked`. Use `partial` whenever work
remains, including validation that the brief requires. The UTF-8 summary is
optional and limited to 32 KiB, like a mail body; `--summary-file -` also reads
stdin. Only the sleeve's own identity may report. The owner and other sleeves
cannot submit a result on its behalf. Terminal sleeves can report too, and
`chat watch` exposes their report history without starting a chat driver.

Each submission records its status, summary, timestamp and chat sequence in
history. A later report replaces the current result while preserving every
earlier report in history. `sleeve show`, `sleeve ls --json` and the app's
sleeve card expose the latest result; tap its REPORT button to read the summary.
Results persist across hub restarts. A new accepted `chat send` starts a brief
and clears the current result. Mail delivery and its extra turns leave the
current result intact, including across resleeves. A fresh spawn starts without
a result.

`chat wait` waits on the hub's event bus without polling. Its default condition
is `--until idle`: the chat must be idle, outside a resleeve takeover, with no
pending or queued mail, exactly as `--exit-after-idle` requires. `--until report`
accepts any marked status and prints its summary; callers must inspect partial
and blocked results before deciding the work is done. An already satisfied
condition returns immediately. Several names wait concurrently and print each
result as it arrives, returning success only after all qualify. `--json` emits
one `WaitResponse` per sleeve. A timeout, sleeve exit, failure or removal before
the condition is met exits nonzero. `--timeout 0` (the CLI default) waits until
interrupted. Idle waits require chat mode.

The unary `ChatService.Wait` RPC uses the same event-driven wait and is exposed
to Envoy and MCP as the read-only `chat_wait` tool. Its `until` is
`WAIT_UNTIL_IDLE` or `WAIT_UNTIL_REPORT`; `timeout` is a protobuf duration,
defaulting to ten minutes when omitted and waiting until cancellation when
explicitly zero. For example, MCP arguments
`{"sleeve":"alice","until":"WAIT_UNTIL_REPORT","timeout":"1800s"}`
return the sleeve name and its result. Cancellation ends the subscription.

`protectorate events` already carries `chat.state` and `sleeve.exited`;
`sleeve.report` adds the status, summary, time and chat/brief sequences, and
`sleeve.brief` announces that a new brief cleared a result. Use `events --json`
for payloads. `sleeve.killed` announces a kill independently of whether its
record is retained; a stored result still satisfies a report wait after exit.
This remains the single owner event stream.
Mailbox reads also emit `msg.read`, so idle waits wake when pending mail is
read without starting a harness turn.

## Workspaces

The workspace is an existing directory on the hub or a git URL. It need not
be a git repository. The API requires an absolute directory path and resolves
symlinks before checking it. The CLI converts relative paths to absolute paths
on the client; that resulting path must exist on the hub. Shell `~` expansion
also happens on the client, so use an explicit hub path when calling remotely.

An existing directory, including its `.git`, is mounted read-write at its
resolved host path. Edits change the original checkout directly; no copy or
worktree is created. Selecting it does not expose its parents or siblings.
System/tool mounts are read-only, HOME is the sleeve's own writable home, and
that harness's shared auth directory is writable. The sleeve has its own
network namespace; outbound connections go only through the hub's
allowlisting egress proxy ([sandbox egress](rust-sandbox.md#egress)).
Outward symlinks do not grant access to otherwise unmounted host paths. A linked
git worktree may need a common git directory outside the workspace that is not
mounted.

Sandbox checks reject filesystem root, system directories, the real user home
and its ancestors, and paths inside or containing protected credential or
application-data paths (including `.ssh`, `.codex`, `.config`, `.cargo` and
`.local/share` under the real home). Hub state and its ancestors are refused;
within hub state only `workspaces/<sleeve-name>` and its descendants are allowed
for an ordinary sleeve. Envoy has its own orchestrator workspace exception.
The checks live in `protectorate-hub/src/workspaces.rs` and the sandbox plan
validation in `protectorate-sandbox`. They do not scan project files for
embedded secrets.

`protectorate sleeve workspaces` discovers immediate managed workspace
directories and known sleeve workspaces, filtered by the sandbox checks. It
is not a browser of every accepted hub directory. Outside hub state another
existing absolute directory can be supplied if it passes those checks.

Git URLs clone into `$PROTECTORATE_HOME/workspaces/<sleeve-name>`, with a
ten-minute timeout. If that destination already has `.git`, it is reused
without verifying the requested remote; a nonempty non-git destination fails.
Choose a new sleeve name when a separate clone is needed. The clone command
disables terminal prompts but inherits the hub process environment and git
configuration, so ambient helpers or SSH credentials may still be used.
Public repositories need no added authentication; there is no dedicated
private-repository credential-management UI. Do not embed tokens in URLs.

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

## Models and reasoning effort

Codex chats, including Envoy, support model and reasoning-effort selection at
creation and between turns. The hub queries the native paginated `model/list`
catalog: model IDs, effort values and per-model defaults come from that
installation, not a Protectorate enum. Catalog presence does not guarantee
account access; the harness can still reject an inference request.

```
protectorate chat models --harness codex       # discover before creation
protectorate chat models alice                # this sleeve's native catalog
protectorate sleeve spawn alice --workspace /srv/project --harness codex --model MODEL --effort EFFORT
protectorate chat settings alice              # requested/effective settings
protectorate chat settings alice --model MODEL --effort EFFORT
protectorate chat settings alice --native-defaults
protectorate chat settings envoy --model MODEL --effort EFFORT
```

Replace MODEL and EFFORT with advertised values. Creation also supports
`orchestrator spawn --harness codex --model MODEL --effort EFFORT`; this
respawns Envoy, so use `chat settings envoy` for an existing conversation.
In the app, Codex creation forms have native selectors, and an idle chat's
menu has MODEL AND EFFORT. Effective model and effort appear in the chat header
and sleeve information. Other harnesses explain that controls are unavailable.

At creation, an omitted effort uses `hub serve --sleeve-effort LEVEL`, which
defaults to `high`, including for Envoy and resleeves onto Codex. An explicit
effort wins. Set `--sleeve-effort=''` to leave native defaults in control. If
the chosen model does not advertise the hub's default effort, the hub logs
the fallback and uses the native effort while retaining the requested effort.
Other harnesses and terminal sleeves keep their native behavior. Existing
chats retain their persisted selection across hub restarts.

Between turns, `chat settings --model`/`--effort` replace the complete requested
selection. An omitted model uses the native session's original model; an
explicit model with omitted effort uses that model's advertised default effort.
`--native-defaults` restores both defaults captured before the first override.
Requested, effective and captured default settings are persisted separately.
Native model rerouting updates the effective model and leaves effort unknown
when the harness does not report it. `sleeve ls` includes effective model and
effort columns; `sleeve show` includes requested and native settings as well.

Updates require an idle chat with no pending approval, compaction or resleeve.
Codex uses native `thread/settings/update` without a turn or restart,
preserving the session and workspace. Older Codex versions without that
operation return an explicit error; there is no hidden restart fallback.
The driver reads native model/effort on start/resume. Codex's per-sleeve config
seeds permission/sandbox settings and does not copy the owner's `config.toml`
or model choice.

Claude, Grok and pi retain native defaults/session/config behavior; explicit
selection is rejected until those drivers implement verified controls. Their
effective effort is unknown. Model names may be absent until usage arrives;
Grok currently reports no model name. Extra harness arguments after `--` work
in terminal mode; chat rejects them and typed selection is chat-only.

The API is `ChatService.ListModels` (read) and `SetSelection` (requires
`chat_set_selection` or a matching chat scope). Ordinary sleeve and Envoy
tokens lack that write scope by default; use owner authentication or an
explicitly granted token. `SpawnSleeveRequest.selection` and
`SpawnEnvoyRequest.selection` carry creation settings. `ChatStatus.settings`
and replayable settings events separate requested/effective values;
`ChatStatus.reasoning_effort` reports the effective effort.

Resleeving preserves the request on the same harness. When moving a selected
chat with explicit selection to another harness, choose target `--model`/`--effort` or
`--native-defaults`; there is no automatic vendor mapping. The API fields are
`ResleeveSleeveRequest.selection` and `native_defaults`. A hub-applied effort
alone does not require an explicit choice when changing harnesses;
`--native-defaults` deliberately bypasses the hub effort default. See
[delegation](coordination.md#delegation) for native tasks versus sleeves.

Catalog discovery before creation runs in a temporary-home sandbox with the
native auth directory and read-only tools, without a project mount, sleeve
record or inference turn. Existing chats query their own driver. No model
traffic is proxied and no credentials are copied into settings responses.

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
- **ListModels(sleeve, harness)** queries native discovery without starting an
  inference turn. Supply a sleeve or a harness for creation-time discovery.
- **SetSelection(sleeve, selection)** replaces requested model/effort while
  idle. Unsupported drivers and invalid catalog combinations fail.

`Sleeve.chat` (`ChatStatus`) is kept current: state, harness session id, model,
`context_tokens` (the prompt size of the last model call), `context_window`,
`last_seq`, `provider` and `local_model`.
Every state change is published on EventService as `chat.state` (subject =
sleeve name).

Provider and local serving. Drivers report the provider with each usage
update: claude `anthropic`, codex `openai`, grok `xai`. pi reports the
`provider` and `baseUrl` of the `model` object in `get_state` (see
`rpc-commands.md#model-object`), and the `provider` of each assistant
message; a message with a different provider than `get_state` drops the
`baseUrl`, since pi's message carries no endpoint. The hub sets `local_model`
when the base URL host is a private address (the listener rule: loopback,
RFC 1918, 100.64.0.0/10, fc00::/7, fe80::/10), `localhost`, a name or address
in the node inventory, or a `hub serve --local-model-host` name (repeatable);
any other DNS name is not local. No base URL (a named hosted provider) is never local, so
DeepSeek, GLM, Kimi and the like over API keys show no warning. Like the model
name, both fields are empty until the first model call. Example: a pi provider `hotschmoe-local` with baseUrl
`https://llm.hotschmoe.com/v1` is a public DNS name for the user's own server,
so it is local only with `--local-model-host llm.hotschmoe.com`; a provider
`hotschmoe` at `http://192.168.10.5:18080/v1` is local by its private address.

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
2. Facts, composed in code (protectorate-handoff): git status, `diff --stat
   HEAD` and the last 10 commits (run in a read-only sandbox helper, since the
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
   a directory handle, refusing a symlinked `.protectorate`), after `.protectorate/`
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

## How turns end, and what they cost

Each model call's `usage` event carries input (uncached), output, cache-read
and cache-write tokens. A `settled` event sums its turn's calls as `usage` and
says how the turn `end`ed: completed, canceled (an interrupt was requested) or
error (the harness or its provider reported a failure during the turn). An
`error` event has a `kind`: unset when the harness or provider reported it,
`EXITED` when the harness process ended, `KILLED` when a signal nobody on the
hub asked for ended it, which is how a resource limit or the OOM killer stops a
process (the sandbox helper reports it as status 137). Chat turns have no
timeout; `chat wait --timeout` bounds a wait, not a turn.

`chat status <sleeve>` and `sleeve show` also report the log's cache use: totals,
the first model call since the harness started, and `cold_resume` when a
resumed session's first call read nothing from the cache although it should
have. The chat header shows the same as `CACHE n%` and `CACHE MISS n%`.

`protectorate report` numbers each report with a sequence kept above the last
one sent from that sleeve (a counter beside the token file, never below the
clock). The hub drops a report at or below the last sequenced report in the
log and returns the result it already holds, so a late or repeated report
cannot replace a newer one. A report without a sequence is recorded in arrival
order.

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
