# Coordination

How sleeves, projects and the orchestrator find each other and talk, without a
model in the loop. Everything here is hub code: it runs the same way every
time, costs no tokens, and adds nothing to any harness (no hooks, skills, MCP
config or worker AGENTS.md text; nothing is written into a worked-on repo). Mail
delivery includes brief reply guidance; Envoy's generated instructions include it too. The
decision record is `docs/memory-options.md`, section 4, decision 5.

## Identity

Every sleeve (except the orchestrator, which has its own token from
`orchestrator spawn`) gets a token of its own at spawn, named `sleeve:<name>`.
Scopes: `read`, `backlog_*`, `directory_*`, `message_*`. It is
written to `<sleeve home>/.protectorate-token` (mode 0600, inside the sandbox
at `$HOME/.protectorate-token`, never in the workspace) and the sleeve's env
has `PROTECTORATE_HUB` and `PROTECTORATE_TOKEN_FILE`, so the `protectorate`
CLI inside the sandbox works with no flags. The binary is reachable inside the
sandbox the same way it is for the orchestrator. The token is revoked when the
sleeve is killed and replaced on respawn (a sleeve that exits on its own keeps
a valid token until it is killed or respawned).

The sender of a message is the authenticated caller: a sleeve token maps to
its sleeve name, the orchestrator's to `envoy`, the owner token to `owner`, the
hub itself to `hub`. There is no `from` field to claim.

## Directory

`protectorate who` / `who <project|sleeve>` (`DirectoryService`:
`ListProjects`, `GetProject`).

For each running sleeve the hub reads, locally and without network or package
managers: the workspace's `origin` URL (normalized to `host/owner/repo`),
branch and HEAD, and manifests in the root and up to two directories down:

| File | Provides | Depends on |
|------|----------|------------|
| `go.mod` | `module` path | every `require` |
| `package.json` | `name` | `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies` |
| `Cargo.toml` | `package.name` | `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, target and workspace tables, `package =` renames |
| `pyproject.toml` | `project.name` or `tool.poetry.name` | `project.dependencies`, optional dependencies, dependency groups, poetry tables |

TOML is parsed with the `toml` crate; an unparsable manifest is skipped.
Names are prefixed with the ecosystem
(`go:`, `npm:`, `cargo:`, `pypi:`) and normalized (case and `-`/`_` for cargo
and PyPI).

A project is its normalized remote, or `path:<workspace>` without one. Project
A depends on project B when one of A's dependencies is a package B provides;
a project's own packages do not count as its dependencies. Refresh happens at
spawn and kill and every minute. Projects stay listed with no owners after
their last sleeve is gone (`directory.json`), so they can still be addressed
and notified.

## Mailbox

`protectorate msg send|reply|ack|inbox|thread|threads|wait` (`MessageService`).

```
protectorate msg send api "the schema changed" --thread t7
protectorate msg send project:acme/lib "please export Foo" --kind request
protectorate msg reply m42 "done in v0.3.0"
protectorate msg ack m42
protectorate msg inbox --unread
protectorate msg thread t7
protectorate msg threads --sleeve api
```

### Concise mail and budget

Aim for 150 words. Lead with the result or blocker; include only actionable
findings and essential evidence. Reference a file, commit or backlog note for
details, using a location the recipient can access. Do not restate the request
or prior context. No acknowledgements unless asked or ack-required; use `msg ack`
when only confirmation is needed. The hub renders this guidance once per chat
delivery batch and in non-JSON CLI inbox/thread/wait output, covering worker sleeves
without writing instruction files into their workspaces.

The 150-word target is advisory: critical evidence can exceed it without being
lost or rejected. We retain the existing 32 KiB body ceiling rather than add a
tighter hub limit. `coordinator.send` checks body bytes before persistence for
both Send and Reply (including CLI and MCP); oversize bodies are rejected with
`invalid_argument` naming the byte count and limit, never truncated. The stored
mailbox format and API responses are unchanged.

Live mailbox-wide sizes cannot be measured with a sleeve token, which only
sees its own mail. Fixture evidence is deliberately small: the binary
restart scenario sends `persist this request` (20 bytes), and the coordination
scenarios include `please export Foo` (17 bytes) and `fyi` (3 bytes), plus a
synthetic 32 KiB + 1 rejection case. These verify routing and validation, not
production verbosity. A 150-word soft target leaves room for a result, evidence
and next action; these fixtures provide no basis for rejecting larger useful
reports. Detailed reports belong in referenced files or notes.

### Message semantics

- **Addresses:** a sleeve name, `project:<id|owner/repo|name>`,
  `orchestrator` (sleeve `envoy`) or `owner`.
- **Kinds:** `note`, `request` (asks for action), `notice` (facts, usually
  from the hub). `ack_required` is optional in the API: unset, a `request`
  requires an ack and other kinds do not; an explicit false (`--ack=false`)
  turns it off for a request.
- **Threads:** `thread_id` groups messages (`t<n>`); `reply` keeps the thread
  and also acks the original. Messages are `m<n>`.
- **Dedupe:** a retry with the same `request_id` from the same sender returns
  the original message.
- **Persistence:** `mailbox.jsonl` in the state dir, an append-only log of
  every event (sent, addressed, delivered, read, acked, reminded, escalated),
  fsynced per record and replayed on start; a torn last line is cut off.
  Bodies are limited to 32 KiB.
- **Inbox:** per recipient; `--unread` leaves out messages already injected,
  shown or acked; `--thread` keeps one thread; listing marks them read unless
  `--peek`.
- **Threads:** `ListThreads` (`msg threads`, a read call) summarizes threads,
  most recently active first: participants, message count, the last message,
  the caller's unread count, messages whose required ack nobody gave, and
  escalated messages still unacked. `--sleeve` and `--project` filter it.

### Delivery

- A recipient in **chat mode** gets the message as an ordinary user message
  (`from` = `sleeve:<name>`, `owner` or `hub`) through the same path as
  `chat send`, at its next IDLE: never during a running turn, never during an
  approval or compaction. Several waiting messages go in one user message.
  Appending a message does not touch the harness prompt cache.
  ```
  [request m42 from sleeve api, thread t7, to project acme/lib, ack required]
  please export Foo

  Reply: protectorate msg reply m42 "..."
  Ack: protectorate msg ack m42
  ```
  Hub notices cannot be replied to, so they end with their own reply line in
  the body: `Reply: protectorate msg send project:<upstream> --thread <id>
  "..."` for a dependency notice, `... msg send <recipient> --thread <id>` for
  an escalation.
- A recipient in **terminal mode**, and the owner, get no injection; they read
  `msg inbox`.
- **Idle wake-up:** all unread mail, regardless of kind, wakes any idle chat
  sleeve, including Envoy. A two-second window from the oldest pending message
  coalesces a burst into one ordinary user message; a running turn, approval or
  compaction is never interrupted. No owner input is required. Envoy's stopped
  harness resumes its native session, retrying failed starts once a minute.
  Mail read with `msg inbox` or answered with `msg reply` is never injected
  again, including reminders; required-ack timers still advance toward escalation.
  Owner input and mailbox delivery remain separate
  ordinary user messages.
- **Blocking wait:** `protectorate msg wait <message-id|thread> --timeout 10m`
  calls `MessageService.Wait`, which blocks on server-side mailbox notifications,
  without client polling. A message ID returns its reply or ack; a thread returns
  incoming mail or an ack to the caller's outgoing mail. Existing outcomes return
  immediately. It does not mark mail read. Timeout defaults to 10 minutes, with
  a 24-hour maximum, and returns `timed_out: true`; cancellation ends the wait.
- A **project address** resolves to the sleeves working on the project now
  (excluding the sender). With none live the message waits, and a `request`
  also becomes a note tagged with the project (`<sender> asks: ...`, source
  the sender), and the message is stored pending with a reference to that
  note. When a sleeve for the project appears the message is delivered once,
  and says the request is already note `<id>` (`protectorate backlog show
  <id>`); the note is the only actionable copy.
- **Ack timers:** an `ack_required` message that stays unacked for the timeout
  (`hub serve --msg-ack-timeout`, default 30m, counted from delivery, or from
  addressing for terminal recipients) is delivered again once. Unacked for
  another timeout, it is escalated: a `notice` to the orchestrator if it is
  running, and always a `msg.escalated` event. It escalates once.
- **Events** (subject = message id): `msg.sent`, `msg.delivered`,
  `msg.acked`, `msg.reminded`, `msg.escalated`.

## Delegation

Native subagents are managed by the harness inside one sleeve. Drivers use
native headless protocols without supplying a replacement tool allowlist, but
this does not guarantee that every installed harness enables delegation.
Native tasks share the sleeve's workspace, sandbox, resource scope and access;
they do not get independent Protectorate identities or isolated branches.

The chat view is incomplete for native children: Claude filters nested
`parent_tool_use_id` messages, Codex filters other thread IDs and does not
normalize a complete child lifecycle, and Grok/pi show generic tool activity
without a child hierarchy. There is no native-child registry or per-child
budget. Turn interruption uses the harness protocol; recursive cancellation
of all background native work is not guaranteed.

Hub-managed sleeves are independent named workspaces/harnesses. Ordinary
sleeve tokens cannot spawn them, send chat turns, resleeve or kill by default;
they can request work from Envoy through the mailbox. Envoy can spawn/resleeve
and send mailbox assignments, but direct chat mutation and kill require
additional scopes. See [Envoy scoping](orchestrator.md#scoping).

There is no parent-child ownership, recursive cancellation, automatic result
collection or parent-exit policy. Use request/reply/ack and status/events for
coordination; IDLE means a turn settled, not that the assigned task completed.
Separate sleeves can use the same external checkout: a listed busy state does
not create an exclusive workspace reservation. Use separate checkouts for
independent write work; automatic worktree creation is not implemented.
Killing a sleeve stops its scope and forgets its record while retaining its
workspace and home. A disposable task does not imply deleting those files.
Managed-child lifecycle controls remain proposed work.

## Dependency notices

On every directory refresh the hub runs `git for-each-ref` on tags and the
default branch (`origin/HEAD`, else `main`/`master`) of each project with a
live sleeve. The first look is a baseline. After that a new tag, or an advance
of the default branch (the old tip an ancestor of the new one) that no new tag
covers, sends each dependent project one `notice` (event `project.released`):

```
Upstream release: acme/lib tag v0.2.0 (3f1c2ab), previous tag v0.1.0 (9d04e1c).
acme/api depends on it through: go:github.com/acme/lib.

Commits (2):
  3f1c2ab add Bar
  7aa91d0 add Foo
Changed files (2):
  A bar.go
  A foo.go
  2 files changed, 2 insertions(+)

Your open requests to acme/lib:
  thread t7, m42: please export Foo
Close one with: protectorate msg ack <id>
```

Facts only, bounded (20 commits, 40 files, at most 3 tags per look). The open
requests are the dependent's unacked `request` messages to that project. A
dependent with no live sleeve gets a note tagged with the project instead (once; source `hub`, body the same facts). Git runs
with hooks, fsmonitor, pager and external diff disabled, since workspaces are
written by sandboxed agents.

## What is deterministic

All of it: addressing, ordering, timers, delivery, directory, edges, notices.
Nothing here asks a model anything. Fuzzy calls (interrupt or wait, route an
unaddressed request, merge duplicates) are left to a decision model that does
not exist yet, and would sit on top of these RPCs.

## Not done

- No expiry of old messages or projects; the mailbox log is not compacted.
- Delivery to a sleeve is one pass per second, serialized; a harness that
  takes a minute to open delays other deliveries that pass.
- Paired phones get `read` (directory and threads) but not `message_*`: the
  app's MESSAGES view lists and reads every thread, and sending, replying,
  acking and marking read need the owner token.
