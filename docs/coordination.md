# Coordination

How sleeves, projects and the orchestrator find each other and talk, without a
model in the loop. Everything here is hub code: it runs the same way every
time, costs no tokens, and adds nothing to any harness (no hooks, skills, MCP
config or AGENTS.md text; nothing is written into a worked-on repo). The
decision record is `docs/memory-options.md`, section 4, decision 5.

## Identity

Every sleeve (except the orchestrator, which has its own token from
`orchestrator spawn`) gets a token of its own at spawn, named `sleeve:<name>`.
Scopes: `read`, `backlog_*`, `directory_*`, `lease_*`, `message_*`. It is
written to `<sleeve home>/.protectorate-token` (mode 0600, inside the sandbox
at `$HOME/.protectorate-token`, never in the workspace) and the sleeve's env
has `PROTECTORATE_HUB` and `PROTECTORATE_TOKEN_FILE`, so the `protectorate`
CLI inside the sandbox works with no flags. The binary is reachable inside the
sandbox the same way it is for the orchestrator. The token is revoked when the
sleeve is killed and replaced on respawn (a sleeve that exits on its own keeps
a valid token until it is killed or respawned).

The sender of a message is the authenticated caller: a sleeve token maps to
its sleeve name, the orchestrator's to `poe`, the owner token to `owner`, the
hub itself to `hub`. There is no `from` field to claim. Leases follow the same
rule: a sleeve's lease is always held by that sleeve.

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

TOML is read by a small tolerant reader (`internal/directory/toml.go`), since
no TOML library is in `go.mod`. Names are prefixed with the ecosystem
(`go:`, `npm:`, `cargo:`, `pypi:`) and normalized (case and `-`/`_` for cargo
and PyPI).

A project is its normalized remote, or `path:<workspace>` without one. Project
A depends on project B when one of A's dependencies is a package B provides;
a project's own packages do not count as its dependencies. Refresh happens at
spawn and kill and every minute. Projects stay listed with no owners after
their last sleeve is gone (`directory.json`), so they can still be addressed
and notified.

## Mailbox

`protectorate msg send|reply|ack|inbox|thread` (`MessageService`).

```
protectorate msg send api "the schema changed" --thread t7
protectorate msg send project:acme/lib "please export Foo" --kind request
protectorate msg reply m42 "done in v0.3.0"
protectorate msg ack m42
protectorate msg inbox --unread
protectorate msg thread t7
```

- **Addresses:** a sleeve name, `project:<id|owner/repo|name>`,
  `orchestrator` (sleeve `poe`) or `owner`.
- **Kinds:** `note`, `request` (asks for action; `--ack` is the default),
  `notice` (facts, usually from the hub).
- **Threads:** `thread_id` groups messages (`t<n>`); `reply` keeps the thread
  and also acks the original. Messages are `m<n>`.
- **Dedupe:** a retry with the same `request_id` from the same sender returns
  the original message.
- **Persistence:** `mailbox.jsonl` in the state dir, an append-only log of
  every event (sent, addressed, delivered, read, acked, reminded, escalated),
  fsynced per record and replayed on start; a torn last line is cut off.
  Bodies are limited to 32 KiB.
- **Inbox:** per recipient; `--unread` leaves out messages already injected,
  shown or acked; listing marks them read unless `--peek`.

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
  Hub notices omit the Reply line.
- A recipient in **terminal mode**, and the owner, get no injection; they read
  `msg inbox`.
- A **project address** resolves to the sleeves working on the project now
  (excluding the sender). With none live the message waits, and a `request`
  also becomes a backlog item for the project (`<sender> asks: ...`). When a
  sleeve for the project appears the waiting messages are delivered to it.
- **Ack timers:** an `ack_required` message that stays unacked for the timeout
  (`hub serve --msg-ack-timeout`, default 30m, counted from delivery, or from
  addressing for terminal recipients) is delivered again once. Unacked for
  another timeout, it is escalated: a `notice` to the orchestrator if it is
  running, and always a `msg.escalated` event. It escalates once.
- **Events** (subject = message id): `msg.sent`, `msg.delivered`,
  `msg.acked`, `msg.reminded`, `msg.escalated`.

## Leases

`protectorate lease acquire --wait [--priority N]` (`AcquireLease` with
`wait`). Without `--wait` a held resource is an error, as before. With it the
caller gets a ticket (`ticket-<id>`, state QUEUED) and is served first in first
out, higher priority first, per resource (a waiter for a free resource is not
stuck behind a waiter for a held one). When the resource frees (release or
expiry) the hub creates the lease for the waiter at once, holds it for a claim
window (2 minutes), and sends the sleeve a `notice` saying so. `lease renew
<id>` with no `--ttl` claims it for the ttl originally asked for; unclaimed it
expires and passes to the next ticket. `lease release <ticket>` leaves the
queue; tickets older than a day are dropped. The queue is kept in
`leases.yaml` and survives restarts. Events: `lease.queued`, `lease.granted`,
`lease.dropped`. Agents never negotiate hardware between themselves.

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
dependent with no live sleeve gets a backlog item instead (once). Git runs
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
- Paired phones get `read` (directory and threads) but not `message_*`; the
  app has no mailbox view yet.
