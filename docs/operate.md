# Operating a hub

Every command below runs on the hub as the hub's user and maps onto one hub API method over the
unix socket `<home>/run/api.sock`. Global flags come first:

```
protectorate [--home DIR] [--socket PATH] [--json] <noun> <verb> ...
```

`--home` defaults to `$PROTECTORATE_HOME`, else `~/.local/share/protectorate`. `--json` prints the
API's own answer instead of a table. `PROTECTORATE_TOKEN`, when set, is sent as the bearer token;
without it, a caller of the hub's own uid on the socket is the owner. `protectorate` with no
arguments prints the full usage, which this page follows.

Harness names: `claude`, `codex`, `grok`, `pi_vendor`, `pi_local`.

## Envoy

Envoy is a root sleeve that the hub starts itself, running in `<home>/envoy/` with its own
`CLAUDE.md` describing this CLI. Its harness, provider and model are the `envoy` key of
`config.json` (Claude Code by default).

```
envoy send <text...> [--as NAME]           # queue a message for Envoy; prints its request id
envoy wait [REQUEST_ID] [--timeout SECS]   # wait for Envoy's reply (default 600 s) and print it
envoy read [--from SEQ]                    # Envoy's chat from a transcript seq
envoy token                                # the envoy-only token
```

**Vendor general agents** (installed on the hub machine) reach Protectorate only through Envoy.
Give them the envoy-only token, which can send to Envoy and read its replies and nothing else:

- through their shell tool: `PROTECTORATE_TOKEN=<envoy-only token> protectorate envoy send --as
  <agent> ...`, then `envoy wait`;
- through MCP: `protectorate mcp --envoy [--token-file PATH] [--as NAME]`, a stdio server with
  the tools `envoy_send`, `envoy_wait` and `envoy_read`. The token comes from `--token-file` or
  `PROTECTORATE_TOKEN`; the agent's name from `--as` or the client's own name.

An agent that runs as the hub's user without the token is the owner on the socket (REVISIT), so
always give it the token.

## Sleeves

```
sleeve list | sleeve show <sleeve>
sleeve spawn <name> (--path DIR | --parent SLEEVE [--readonly]) [--harness H]
      [--provider P] [--model M] [--resleeve-to H,H] [--budget TOKENS] [--message TEXT]
sleeve send <sleeve> <text...> [--steer]
sleeve interrupt|stop|rm|takeover <sleeve>
sleeve start <sleeve> [--harness H] [--provider P] [--model M]
sleeve resleeve <sleeve> --harness H [--provider P] [--model M]
sleeve transcript <sleeve> [--from SEQ] [--limit N]
sleeve wait <sleeve> [--until idle|done|blocked|parked|ended] [--timeout SECS]
```

A sleeve is named by its id or its name.

- **spawn** with `--path DIR` makes a root sleeve on that directory (one root per directory). With
  `--parent SLEEVE` it makes a child of that root: its own git worktree and branch by default, or
  a read-only view of the root with `--readonly`. A root has at most 8 live children. `--harness`
  defaults to `claude`. `--resleeve-to` is the ordered list automatic resleeves walk (default
  `pi_local`). `--message` is the first prompt.
- **send** queues a message, delivered at the next turn boundary; `--steer` sends it into the
  current turn instead.
- **interrupt** stops the current turn; **stop** ends the incarnation; **rm** removes the sleeve.
  Stopping or removing a root stops its children in the same step.
- **start** starts a stopped or parked sleeve, optionally on another harness. **resleeve** moves a
  sleeve to another harness, with a continuation brief in `.protectorate/BRIEF.md`.
- **transcript** prints the chat: messages, tool calls and results, approvals, turns.
- **wait** blocks until the sleeve reaches a state (default `idle`, 600 s) and exits non-zero if
  it does not.
- **takeover** (the terminal view) is not built; it answers `unavailable`.

Guardrails in code interrupt, resume, resleeve or park a sleeve that fails (quota exhausted,
crash, silent, looping and so on). A third resleeve within an hour parks it; `sleeve list` shows
the park reason.

## Approvals and D9

```
approval list [--pending]
approval decide <id> allow|allow-session|deny
```

Approvals come from two places: a supervised harness asking to run a tool, and the D9 rule. The
app shows them too, inline in the chat.

**D9.** While Envoy's current turn was started by a vendor agent's message, these calls by Envoy
wait for the owner's approval, whatever Envoy decides:

- `sleeve rm`;
- `job run`;
- `sleeve spawn` past `d9_spawn_limit` spawns in that turn (default 3).

A node job with `--maintenance` from any sleeve, Envoy included, always waits for the owner. The
waiting call fails after `d9_approval_timeout_secs` (default 900). Pending approvals live in
memory: a hub restart forgets them.

## Nodes

```
node ls | node show <node>
node add [REGISTRATION]
node approve <auth_id> [--name NAME] [--fingerprint FP]
node rename <node> <new-name>
node rm <node> [--force]
node update (<node>... | --all) [--version TAG] [--channel stable|dev] [--probation SECS] [--no-wait]
```

- **ls** shows each node's OS and arch, link, presence (PRESENT, AWAY or UNKNOWN), last contact and
  version; **show** adds the inventory and where presence came from.
- **add** with no argument prints the join command for a new machine, the hub's control-key
  fingerprint and pending registrations; with the registration text that `join` printed, it
  records it. **approve** registers, tags and names the node. See [install.md](install.md).
- A node is named by its id or its name. A name is never digits alone (`join`, **approve** and
  **rename** refuse one), so digits always mean the id.
- **rename** renames the node in headscale (its tailnet name) and in the hub's enrollment record
  (`tailnet/enrolled.json`). The name follows the `join --name` rules (1 to 63 lowercase letters,
  digits and inner hyphens), may not be digits alone (that reads as an id) and may not be any
  other machine's on the tailnet, the hub's own member included. The machine keeps the hostname
  it joined with; only the hub's name for it changes. Jobs keep to the node's id, so a rename
  moves no job.
- **rm** deletes the node from headscale, which revokes its tailnet key, and drops its enrollment
  record. Its machine cannot come back on the old key: to rejoin it runs `join` again and the
  owner approves it. Queued jobs that could run only on that node fail with `node N was
  removed` (`job show` says so); so does any job pinned to it that would otherwise wait queued
  later, such as one whose approval was still pending, or one left queued by a hub crash in the
  middle of the removal. An online node,
  or one with jobs in flight (submitting, running, collecting, cancelling or lost), is refused
  unless `--force`; run `protectorate-node uninstall` on a live machine first. Forced, its jobs
  in flight are left to the lease: they go lost, then abandoned, and the node may still finish
  them on its own. Its old jobs stay in `job ls` under its id.
- A name freed by **rm** is free at once: removing a stale entry, then renaming its successor
  into that name, is the way to tidy up after a machine was reinstalled and joined again.
- Both write headscale first, then the record. If the record cannot be written after a rename,
  the hub puts headscale's name back and says so (and raises an alarm event if that fails too).
  If it cannot be dropped after a removal, the node is already gone from headscale and the
  error says so and names its id; finish with `node rm <id>` (by id: its name is free, and
  another node may hold it by then). Repeating a rename whose record write was cut short by a
  crash repairs the record. Both are node events (`event list`).
- **update**: see Updates below.

The hub polls presence and the node list every `fleet.poll_secs` (15 s), samples every 60 s and
inventory every 10 minutes.

## Jobs

```
job run (--node ID | --arch ARCH) [--even-if-present] [--on-present idle|kill]
      [--maintenance] [--timeout SECS] [--cwd DIR] [--artifact PATH]... -- <argv...>
job ls [--node N] [--limit N] | job show|cancel <job> | job log <job> [--offset N] [--limit N]
job artifact <job> <path> [--out FILE]
```

- `--node` names a node by id or name, and the hub keeps its id: a later rename does not move
  the job, a node not on the tailnet is refused, and a job pinned to a node that is removed
  before it starts fails; `--arch` lets the hub pick an AWAY node of that arch (the
  least loaded one).
- A node refuses to start a job while someone is present or presence is UNKNOWN.
  `--even-if-present` overrides that for one job.
- If someone arrives mid-job, the job drops to idle priority and keeps running; `--on-present
  kill` kills it instead.
- `--timeout` defaults to 1 hour; a log is capped at 64 MiB. A job never re-runs by itself.
- Without `--cwd` a job runs in its own directory on the node. `--cwd` is an absolute path in
  the node's OS (`/srv/x`, `C:\dev\x` or `C:/dev/x`); the hub checks it when `--node` names a
  node it has met, otherwise (`--arch`) the node does and the job fails there.
- `--artifact PATH` (repeatable) names a file inside the job's directory (the `--cwd`), relative
  or absolute for the node; it is fetched when the job ends, and `job artifact` writes it to
  `--out` or stdout after checking its sha256. A path outside that directory is refused.
- `job log` lags a running job by up to 10 s.

A job result is the node's claim: `job show` says which node reported it.

## Quota

```
quota list
```

One row per subscription (Claude, Codex, Grok, the local endpoint): the reading, its windows with
percent used, and a note. The readers are read-only: they use the hub's harness logins (below)
and never refresh or print a token, so a harness no sleeve has run for a while can read `login
expired` until one does. They run every `quota_poll_secs` (60; 0 turns them off); a reading is
refreshed after 5 minutes and becomes stale after 10. A response with a malformed usage window,
or with no window at all, is reported as an error.
Resleeve targets are weighted by fresh readings; known-exhausted candidates are skipped, and the
sleeve parks if all eligible candidates are exhausted. A stale reading keeps its last windows:
under the usable minimum it counts as exhausted, and with spare left the fallback may still
choose it. A candidate with no reading (`unknown`: no login, or not read yet) can still be
selected. A failed reading (`error`) cannot; it parks the same way an exhausted window does. A
window whose reset time has already passed still blocks while it is over that line; a reset at
or before the epoch is ignored and the percent still counts. A model-scoped Claude window
(the weekly Sonnet or Opus cap, shown as `wk <model>`) is displayed and does not by itself
block a Claude resleeve. Pi with a vendor login and Antigravity have no meter and are never
chosen. A local
endpoint may use HTTPS or loopback HTTP; other HTTP
endpoints are reported as misconfigured and are not contacted.

## History

```
history search <query...> [--mode all|any|phrase] [--sleeve S]... [--harness H]
      [--session ID] [--role user|assistant|tool-call|tool-error|compaction]...
      [--since 7d|MS] [--until 7d|MS] [--limit N]
history read --doc ID | history read <session> [--from TURN] [--to TURN]
```

A keyword index over the native session logs of the sessions the hub launched, plus the hub's own
transcripts. `--since` and `--until` take `7d`, `3h`, `30m`, `90s` or epoch milliseconds. The
owner can search everything; Envoy too; a sleeve only its own runs. The index is derived and
rebuilt in the background if it fails to open. Catch-up commits bounded batches, and failed
commits are retried. Below each configured root, discovery and reopening ignore symbolic links
and non-regular files.

## Events

```
event list [--from SEQ] [--limit N]
event read <seq>
```

Hub events: sleeve, job, node, approval, doctor and update news. `event list` prints the newest
`--limit` (500), or with `--from` the ones from that seq on, oldest first. `event read` marks
everything up to that seq as read. Events live in memory (the last 500).

## Doctor

```
doctor [--refresh]
```

Checks this machine: each harness CLI (path, version, install method, login present), `gh`
login, the sandbox prerequisites (`bwrap`, `systemd-run --user`, linger), disk, clock, `curl` and
the embedded headscale and member. The result is cached in `<home>/doctor.json` and gates which
harnesses dispatch and resleeve may use; a harness marked `NO` under ELIGIBLE is skipped. It runs
at start, every 6 hours, and on `--refresh`. It checks that the hub's login files exist (below),
not that they are valid. A login is a file in that store only: `ANTHROPIC_API_KEY` and the like in
the hub's environment do not count, because the sandbox clears the environment and a sleeve never
sees them. Nodes have no doctor yet.

## Harness logins

```
harness login <claude|codex|grok|pi> [-- ARGS...]
```

Sleeves never use your own logins. The hub keeps one login per harness in `<home>/logins/`, and
every sleeve shares all of them, whatever its own harness (a Claude sleeve can run `codex exec`):
each directory is bound read-write into the sandbox, so a token one sleeve refreshes reaches the
others (a copy per sleeve would fork the refresh token, and the first refresh would log the other
copies out). `harness login` runs on the hub machine, on
your terminal, without the API:

- `claude`, `codex`, `grok`: the vendor's own login command (`claude auth login`, `codex login`,
  `grok login`) pointed at the store; words after `--` go to it, for example
  `harness login codex -- --device-auth` on a machine without a browser.
- `pi`: copies `auth.json`, `models.json` and `settings.json` from your `~/.pi/agent` (API keys
  and your local endpoint's provider). An OAuth entry there is refused, since a copy would fork
  it; log that one in with `PI_CODING_AGENT_DIR=<home>/logins/.pi/agent pi` and `/login`.
  Run it again after you change your Pi models.

Then `doctor --refresh`. Doctor names the command for a harness with no login.

## Sleeve toolchains

A sleeve builds with your toolchains, from a private home and a cleared environment:

- `PATH` is the hub's CLI dir, the hub's own `PATH`, then `~/.cargo/bin`, `~/go/bin`,
  `/usr/local/go/bin` and the dirs in `config.json` `sleeve_path`, where they exist. The hub
  service often has a thinner `PATH` than your shell: list Go and Flutter there, for example
  `"sleeve_path": ["/home/you/.local/go/bin", "/home/you/development/flutter/bin"]`. A seeded
  `~/.profile` restores it in `bash -lc`, which otherwise resets it.
- Rust: `RUSTUP_HOME` and `CARGO_HOME` are yours, read-only except cargo's `registry` and `git`.
  A toolchain a project asks for that you have not installed fails; `rustup` cannot install.
- Go (`GOMODCACHE`, `GOCACHE`) and Dart (`PUB_CACHE`) caches are yours, read-write. The Flutter SDK
  is read-only except its `bin/cache`.
- Builds start warm and every sleeve fills the same caches. The cost is that a sleeve can damage
  them, and a corrupt cache hurts all of them; the tools lock for concurrent use.
- `CARGO_BUILD_JOBS`, `GOMAXPROCS`, `GOFLAGS=-p=N` and `FLUTTER_TEST_CONCURRENCY` follow the
  scope's CPU quota (`N` = quota / 100), since tools count host cores and a 4 GiB scope runs out
  of memory otherwise.
- git commits as your global `user.name` and `user.email`, copied into the sleeve's `~/.gitconfig`
  when it has none; later changes to yours do not reach a sleeve that already has one.

Not available: the systemd user bus (`systemd-run --user` in a test) and long unix socket paths.

## Updates and rollback

**Harnesses** update natively, through each program's own command (`claude update`, `codex
update`, `grok update`, `pi update self`), once every `harness_update_hours` (24). A program is
updated only while none of its sleeves is starting or mid-turn.

```
harness update [PROGRAM...]          # claude, codex, grok, pi; none means every installed one
```

**The hub:**

```
hub info | hub state
hub update [--check] [--channel stable|dev] [--version TAG]
hub update --rollback
```

`hub update` downloads the release from GitHub, checks it against `SHA256SUMS`, stages it in
`<home>/releases/<version>/`, snapshots headscale's database, switches `releases/current` on
probation and restarts. The new hub keeps the release once its health check passes within 2
minutes; a release that keeps failing to start is switched back by `hub probation`, which the
unit runs before each start. Before the rollback it restores the headscale database snapshot;
nodes enrolled after the snapshot, including during the failed trial, are lost and must be
enrolled again. A configured fleet must be running so the database can be snapshotted. The service
must run the snapshot-aware `hub probation` before each start and stop if it fails; regenerate an
older unit with the current installer before relying on this rollback. `--check` only reports what
it would do; `--rollback` goes back to `releases/previous`. The channel defaults to
`release_channel` in `config.json` (`stable`).
Sleeves do not survive the restart: they resume as after a crash.

**Nodes:**

`node update` asks each node to fetch the release from GitHub itself, verify it, stage it and
restart through its service manager. A node confirms its release after it has fully answered a
hub request other than Hello. A node that has not done so within the
probation window (`--probation`, default 600 s) reverts to its previous binary. The command waits
for every node unless `--no-wait`. Update the hub first, then one node per OS and arch, then the
rest. A node with running jobs answers `busy`; once an update is admitted, new jobs answer `busy`
until the restart.

A Windows node updates the same way. The SCM cannot run a step before each start or restart a
service from inside it, so the service runs a supervisor, `protectorate-node supervise --service`,
from the copy under `%ProgramFiles%\protectorate-node`; that copy is never switched. The supervisor
does what the systemd unit does: before each start it runs `releases\previous\protectorate-node.exe
probation`, then it starts `releases\current\protectorate-node.exe serve` in a kill-on-close job,
and it starts the node again 1 s after it exits 75 (update or rollback), 5 s after any other exit. A
release that keeps failing to start is switched back after three starts, as on Linux. The running
`.exe` is never overwritten: each release has its own directory. A rename cannot replace a directory
link on Windows, so `current` and `previous` change by renaming the old link aside and the new one
into place. A crash between the two leaves no `current`; the supervisor finishes the switch before
its next start. A failed or hung probation (killed after 60 s) does not keep the node down: the
supervisor counts the start itself and starts `current`. A Windows node refuses an update to a
release older than the supervisor (its `--version` lacks `supervise: 1`). The supervisor itself is
the release that was installed and changes only with a reinstall. A Windows node installed by
v0.7.0-dev.5 or older (`sc.exe qc protectorate-node` shows `serve --service`) runs `serve` straight
from `%ProgramFiles%`, answers `unsupported` and must be reinstalled once; see [Windows node
updates](windows-node-update.md), which also has the manual check for a real Windows machine.

Neither hub nor node switches releases while a release is on trial; wait for confirmation or
automatic rollback. Without `--version`, an update is available only when the newest release on
the selected channel has higher semver precedence than the running release. `--version` can select
an older release, and the result identifies a downgrade. If the running version cannot be compared,
specify `--version` explicitly.

The hub talks only to nodes at its own node protocol (`node ls` shows "reinstall the node"
otherwise). When a release bumps the protocol, update the nodes first, from the old hub, and the
hub last. A node the old hub cannot update is reinstalled with the new release.

Both update paths are tested end to end against a stand-in release server, and have run on the
real fleet: `hub update` from v0.7.0-dev.1 to dev.2 and dev.3, `node update` from dev.2 to dev.3.
The Windows supervisor loop and its pointer switch are tested end to end on Linux
(`supervise` with `PT_TEST_POINTER_ASIDE=1`); on a real Windows node they have not run yet.

## Devices

```
device list | device add <name> | device pair <code> | device revoke <id>
```

See [install.md](install.md#pairing-a-device).

## Where state lives

The hub owns one root, `<home>` (default `~/.local/share/protectorate`, mode 0700):

| Path | What |
|---|---|
| `journal/` | the journal: snapshots and segments of every state change |
| `config.json` | settings; written with every default on first start |
| `tokens` | the hub secret and the owner token's hash |
| `devices.json` | paired devices, tokens stored as SHA-256 |
| `doctor.json` | the cached doctor report |
| `harness-updates.json` | the last run of each harness's update command |
| `envoy/` | Envoy's working directory |
| `logins/` | the harness logins every sleeve shares (`harness login`) |
| `sleeves/<id>/` | a sleeve's private `home/` (native session logs, links to `logins/`), `transcript.jsonl`, checkpoints |
| `worktrees/` | child sleeves' git worktrees |
| `jobs/<job>/<attempt>/` | node job logs and fetched artifacts |
| `history/` | the history index (derived; keep it private and out of backups) |
| `tailnet/` | headscale's database and keys, the hub member's state, `enrolled.json`, `pending.json` |
| `net/` | the embedded headscale and member, unpacked by SHA-256 |
| `releases/` | staged hub releases and the `current` and `previous` links |
| `run/` | `api.sock` and `bin/`, bound into every sandbox |

Workspaces stay in your own directories; nothing is written into a worked-on repo.

A node keeps its state in `/var/lib/protectorate-node` (Linux) or the `--state-dir` it was
installed with (Windows): `node.json` (from join), `member/` with its keys and `control-key.pin`,
`jobs.jsonl` (the job journal), `work/`, `releases/` and `install-manifest.json`.

`tokens`, `devices.json`, `tailnet/` and the sleeve homes hold secrets. Never commit or share
them.

## Logs

| Where | What |
|---|---|
| `journalctl --user -u protectorate.service` | the hub: `<seconds since start> <level> <message>` lines (`serve --log-level`) |
| `<home>/tailnet/headscale/headscale.log` | headscale |
| `<home>/tailnet/member.log` | the hub's member |
| `journalctl -u protectorate-node` | a Linux node, member lines prefixed `member:` |
| `<state>\node.log` | a Windows node under its service |
| `<state>/join-member.log` | member during `join` |
| `sleeve transcript`, `event list`, `job log` | what the hub saw from sleeves, itself and jobs |

None of these rotate yet.
