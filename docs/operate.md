# Operating a hub

Every command below runs against the hub's HTTP API and maps onto one API method. Global flags
come first:

```
protectorate [--home DIR] [--url URL] [--token TOKEN | --token-file PATH] [--json] <noun> <verb> ...
```

- `--home` defaults to `$PROTECTORATE_HOME`, else `~/.local/share/protectorate`.
- The URL is `--url`, else `$PROTECTORATE_URL`, else `listen_addr` from `<home>/config.json`, else
  `http://127.0.0.1:7470`. There is no unix-socket API.
- The token is `--token`, `--token-file` or `$PROTECTORATE_TOKEN`, else the owner token the CLI reads
  from `<home>/auth.json`. So a process that can read that file as the hub's user is the owner
  (REVISIT).
- `--json` prints the API's own answer instead of a table. `protectorate --version` prints the version.
  `protectorate` with no arguments prints the usage this page follows.

Harness names: `claude`, `codex`, `grok`, `pi_vendor`, `pi_local` (`pi` also works),
`antigravity` (`agy` also works).

Some behaviour in [SPEC.md](../SPEC.md) is design the code has not reached; this page describes
what the commands do now.

## Envoy

Envoy is a root sleeve that the hub starts itself, running in `<home>/envoy/` with a `CLAUDE.md` and
`AGENTS.md` there that describe this CLI.

```
envoy send <text...> [--as NAME] [--message-file FILE]   # queue a message for Envoy; prints its request id
envoy wait [REQUEST_ID] [--timeout SECS]                 # wait for Envoy's reply and print it
envoy read [--from SEQ] [--limit N]                      # Envoy's chat from a transcript seq
envoy token                                              # the envoy-only token
```

`envoy wait` waits at most 60 s per call (30 s without `--timeout`); the reply says whether the
turn `settled`, so call it again if it did not. `--message-file -` reads the message from stdin.

**Vendor general agents** (installed on the hub machine) reach Protectorate only through Envoy. Give
them the envoy-only token, which can send to Envoy and read its replies and nothing else:

- through their shell tool: `PROTECTORATE_TOKEN=<envoy-only token> protectorate envoy send --as
  <agent> ...`, then `envoy wait`;
- through MCP: `protectorate mcp --envoy [--token-file PATH] [--as NAME]`, a stdio server with the
  tools `envoy_send`, `envoy_wait` and `envoy_read`. Without `--envoy`, `protectorate mcp` offers the
  full tool set with the caller's own token.

An agent that runs as the hub's user without the token is the owner, so always give it the token.

## Sleeves

```
sleeve list | sleeve show <sleeve> | sleeve inbox [<sleeve>] [--drop]
sleeve spawn <name> (--path DIR | --new-path DIR | --parent SLEEVE [--readonly | --worktree])
      [--harness H] [--provider P] [--model M] [--effort E] [--resleeve-to H,H] [--budget N]
      [--message TEXT | --message-file FILE] [--until-done N]
sleeve send <sleeve> <text...> [--steer | --supersede] [--until-done N] [--message-file FILE]
sleeve interrupt|stop|rm|takeover|attach <sleeve>
sleeve start <sleeve> [--harness H] [--provider P] [--model M] [--effort E] [--until-done N]
sleeve resleeve <sleeve> --harness H [--provider P] [--model M] [--effort E]
sleeve transcript <sleeve> [--from SEQ] [--limit N]
sleeve wait <sleeve>... [--all | --any] [--until idle|done|blocked|parked|ended|report] [--after-seq N] [--timeout SECS]
sleeve push <sleeve> [--remote R] [--branch B] [--force]
```

A sleeve is named by its id or its name.

- **spawn** with `--path DIR` makes a root sleeve on an existing directory (a missing one is refused).
  A path that overlaps another live sleeve's claim is refused. With `--new-path DIR` the hub creates
  the directory if it is missing and claims it; it does not `git init` it, and there is no
  `projects_dir` check yet. With `--parent SLEEVE` it makes a child of that root: its own git worktree
  at `<home>/worktrees/<id>` on branch `pt/<id>` by default, or a read-only view of the root with
  `--readonly`. A root has at most 8 live children. `--harness` defaults to `claude`.
  `--resleeve-to` is the ordered list of harnesses a quota or auth failure moves the sleeve to (at
  most three harnesses in all, counting `--harness`); with none, such a failure parks the sleeve.
  `--message` is the first prompt. `--budget N` counts settled turns, and when it is reached the
  `--until-done` rounds stop.
- **send** queues a message, delivered at the next turn boundary; `--steer` sends it into the
  current turn instead; `--supersede` drops the queued messages and interrupts the running turn
  before queueing the new one. `prompt` is another name for `send`. Called with the Envoy token the
  message is mail from Envoy; any other caller sends as the owner. There are no sleeve-to-sleeve
  routing rules yet (REVISIT, "Scopes are coarse"). The reply says whether it was queued and the
  queue length.
- **--until-done N** (on spawn, send or start): when a turn ends normally with nothing queued, the
  hub queues the fixed message "Continue until done, or report status blocked/done.", at most N
  times. Stop, a budget limit and a declared `done` end it; the hub does not yet read a `STATUS:` line
  from the final message.
- **inbox** shows the messages queued for a sleeve (Envoy by default); `--drop` empties that queue.
- **interrupt** stops the current turn; **stop** ends the incarnation; **rm** removes the sleeve
  (and, for a worktree sleeve, its worktree).
- **start** starts a stopped or parked sleeve, optionally on another harness. **resleeve** moves a
  sleeve to another harness and writes a handoff brief to `<home>/briefs/<sleeve>.md`.
- **takeover** stops the managed incarnation and writes the same handoff brief, so the owner can
  work in the harness's own terminal UI. **attach** does the takeover and then runs that harness's
  own CLI with its resume flag in the sleeve's directory, in your terminal. There is no hub-owned
  terminal and no hand back yet (REVISIT).
- **transcript** prints the chat: messages, tool calls and results, approvals, turns.
- **wait** blocks until the sleeves reach a state (default `done`, which is the same as `idle`: the
  incarnation is idle with nothing queued, or the sleeve is stopped). `--any` returns when one
  does. One call waits at most 60 s; check `reached` and call again. `--until report` is not a
  real report wait yet: it returns once the sleeve has an incarnation.
- **push** has the hub push the sleeve's branch (the hub, not the sleeve, runs `git push`).

Guardrails in code resume, resleeve or park a sleeve that fails. An unrequested exit counts as a
strike, and the second strike within an hour parks the sleeve (`strike_rate_exceeded`). A crash is
resumed on the same harness's native session, at most twice in 10 minutes. A quota or auth exit
moves the sleeve to the next harness in its `--resleeve-to` list; a third automatic resleeve within
an hour parks it (`resleeve_rate_exceeded`), and an empty list parks it (`no_fallback_target`).
`sleeve list` shows the park reason; `sleeve start` brings a parked sleeve back.

## Approvals and D9

```
approval list [--pending]
approval decide <id> allow|allow-once|allow-session|deny
```

Approvals come from two places: a supervised harness asking to run a tool, and the D9 rule. The
app shows them too, inline in the chat. Envoy cannot decide approvals.

**D9.** While Envoy's current turn was started by a vendor agent's message (`envoy send --as NAME`
with the envoy-only token), these calls by Envoy wait for the owner's approval, whatever Envoy
decides:

- `sleeve rm`;
- `job run`;
- `sleeve push`;
- `sleeve spawn` past `d9_spawn_limit` spawns in that turn (default 3).

The waiting call fails after `d9_approval_timeout_secs` (default 300, at most 600), both in
`config.json`. The owner's own turns and sleeve mail do not trigger it.

## Nodes

```
node list | node show <node>
node set <node> --jobs any|owner|none [--note TEXT]
node join-token
node add <NAME:NODE_KEY>
node approve <auth_id> [--name NAME] [--fingerprint FP]
node rename <node> <new-name>
node rm <node>
node update [<node>... | --all] [--version TAG] [--channel stable|dev] [--no-wait] [--cancel ROLLOUT_ID]
```

A node is named by its id or its name. The install steps (`join-token`, `add`, `approve`) are in
[install.md](install.md#join-and-approve).

- **list** shows each node's OS and arch, link, presence (PRESENT, AWAY or UNKNOWN), job policy,
  last contact and version; **show** adds the inventory and where presence came from.
- **set** records the job policy in the journal. `any` (the default) allows jobs, `none` refuses all
  of them (queued jobs stay queued and the scheduler skips the node), and `owner` is recorded and
  shown but the scheduler does not tell callers apart yet. The note is kept with the policy.
  Inventory and updates continue under every policy.
- **rename** changes the node's name in the journal; jobs keep to the node's id.
- **rm** disconnects the node and marks it removed, so its key is no longer accepted. To bring the
  machine back, it runs `join` again and the owner adds and approves it again. `--force` is accepted
  and ignored. Run `protectorate-node uninstall` on a live machine first.
- **update** asks each approved node (or the named ones) to move to the hub's own version, or to
  `--version`. See Updates below.

The hub learns presence from each node's heartbeat (every 5 s) and inventory from its hello.

## Jobs

```
job run (--node ID | --arch ARCH) [--even-if-present] [--on-present idle|kill] [--maintenance]
      [--timeout SECS] [--cwd DIR] [--artifact PATH]... [--wait]
      [--exclusive] [--slots N] [--long-slots N] [--mem-mib N] [--parent-epoch EP]
      [--ttl-ms MS] [--wait-ms MS] -- <argv...>
job list [--node N] [--limit N] | job get|show <job> | job cancel <job>
job logs <job> [--offset N] [--limit N]
job artifact <job> <path> [--out FILE]
lease list <node> | lease acquire <node> [...] | lease renew|release <node> <lease> <epoch>
lease verify <node> <boot> <epoch> | lease unquarantine <node>
```

- `--node` names a node by id or name; `--arch` lets the hub pick the eligible node of that arch with
  the lowest id. A node is eligible when it is approved, online, not excluded after a failure, its
  job policy is not `none`, and it is not PRESENT. `--even-if-present` overrides the PRESENT hold
  for one job (UNKNOWN presence is not held).
- If someone arrives while a job runs, the job keeps running unless it was started with
  `--on-present kill`, which cancels it.
- `--timeout` is in seconds; `--cwd` is an absolute path in the node's OS; `--artifact PATH`
  (repeatable) names a file the hub fetches when the job ends and checks against its sha256.
- The node admits a job through its admission authority: `--exclusive`, `--slots`, `--long-slots`,
  `--mem-mib` and `--parent-epoch` (a child borrowing its parent's lease) shape the lease, and
  `lease ...` lists and manages leases on a node. Jobs run in a fenced group (a cgroup on Linux, a
  Job Object on Windows) that is killed when the job is cancelled or its lease is lost.
- `job run --wait` prints the log and exits with the job's exit code.
- `hub info` shows `artifact_cap` (64 MiB). A job is never re-run by itself; one that loses its node
  goes lost, then abandoned after 24 hours.

A job result is the node's claim: `job get` says which node reported it.

## Quota

```
quota [list]
```

One row per subscription (Claude, Codex, Grok, Antigravity, the local endpoint): the reading, its
windows with percent used, and a note. The readers are read-only: they read the hub user's own
login files and never refresh or write a token, so a harness no sleeve has run for a while can read
`login expired` until one does. They refresh in the background every two minutes (set
`PROTECTORATE_NO_REMOTE_QUOTA` to turn them off). A failed read keeps the last windows, marked
stale, for at most 24 hours. How the Antigravity reader works is in
[srcmd/quota.md](srcmd/quota.md). The selector in `src/quota/select.zig` is not called by the hub
yet: a quota or auth failure resleeves to the next harness in the sleeve's list, whatever the
readings say.

## History

```
history search <query...> [--mode all|any|phrase] [--sleeve S]... [--harness H]
      [--session ID] [--role user|assistant|tool-call|tool-error|compaction]...
      [--since 7d|MS] [--until 7d|MS] [--limit N]
history read --doc ID | history read <session> [--from TURN] [--to TURN] [--after DOC]
```

A keyword index (BM25) over the native session logs on the hub machine (`~/.claude/projects`,
`~/.codex/sessions`, `~/.pi/agent/sessions`, `~/.grok`) and the hub's own transcripts. It is derived
and rebuilt from those logs; `<home>/history/` can be deleted. `--since` and `--until` take `7d`,
`3h`, `30m`, `90s` or epoch milliseconds. `history read` returns at most 50 documents; `--after` is
the last document id of the previous page. Any caller with a token can search and read every
sleeve's history and transcript (REVISIT, "History and transcripts ignore the caller's reach").

## Events

```
event list [--from SEQ] [--limit N]
event read <seq>
```

Hub events: sleeve, job, node, approval, guardrail, harness and update news. The hub keeps the last
512 in memory, so a restart empties the list. `event read` marks everything up to that seq as read.

## Doctor

```
doctor [--refresh]
```

Checks this machine: each harness CLI (present, version, how to install, and whether it is
`ready`, `installed` or `missing`; ready means the binary is on `PATH` and its login files exist), `gh` login, whether bwrap or Landlock is available, `git`, and any `sleeve_devices`. The
checks run on every call (`--refresh` is accepted); nothing is cached. `protectorate-node doctor`
runs the same checks on a node. The checks look for the harness's own login files under the hub user's home and do not test
that they are valid.

## Harness logins

```
harness login <claude|codex|grok|pi|agy> [-- ARGS...]
```

Sleeves use the hub user's own harness logins: there is no separate hub login store. `harness
login` runs the vendor's own login command in your terminal, as you, so the login lands in your
home: `claude`, `codex login`, `grok`, `pi` or `agy`, with the words after `--` passed to it (for
example `harness login codex -- --device-auth` on a machine without a browser). The sandbox keeps a
sleeve's own harness directory writable (so its token refreshes reach you) and masks the other
harnesses' directories and `~/.ssh`.

## Sleeve environment

A sleeve runs with a cleared environment: `PATH`, `LANG`, `LANGUAGE`, `LC_ALL`, `LC_CTYPE`, `TERM`,
`COLORTERM`, `TZ`, `USER`, `LOGNAME`, `SHELL`, `SSL_CERT_FILE`, `SSL_CERT_DIR`,
`NODE_EXTRA_CA_CERTS`, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are copied from the hub's environment,
`HOME` is the owner's home, `TMPDIR`, `TEMP` and `TMP` point to `<home>/run/<sleeve id>/tmp`, and the
hub adds `PROTECTORATE_SLEEVE`, `PROTECTORATE_WORKSPACE`, `PROTECTORATE_ENVOY`, `PROTECTORATE_URL`,
`PROTECTORATE_JOB_ID` (and `PROTECTORATE_TOKEN` for Envoy only).

- The toolchains are whatever is on the hub's `PATH`. The hub service gets the `PATH` of the shell that
  ran `install.sh` (written into the unit), which is often thinner than your login shell's: put
  Zig, Go, Flutter and the like on it, or reinstall from a shell that has them. There is no
  `sleeve_path` setting and the hub sets no `CARGO_HOME`, `GOCACHE` or `PUB_CACHE`.
- With bubblewrap the host root is read-only, so only the workspace, the sleeve's own harness
  directory, its git directories and `TMPDIR` are writable. A build that wants to write caches under
  `~/.cargo`, `~/go` or `~/.cache` fails there; point the tool at `TMPDIR` (for example `CARGO_HOME`
  or `GOCACHE`) or run the hub with `--no-sandbox`. The Landlock fallback allows writes to the same
  places.
- Credential paths (`~/.ssh`, `~/.config/gh`, `~/.aws`, `~/.gnupg`, `~/.docker`, `~/.kube`, other
  harnesses' directories and similar) are masked. The hub's own home is not masked.
- A sleeve has no memory limit and no OOM watcher (REVISIT). `sleeve show` prints `mem_limit=null`.

## Sleeve devices

`sleeve_devices` in `config.json` lists host device paths (for example `["/dev/aipu"]`). The hub
watches them once a second and records a `device_attached`, `device_detached` or `device_reattached`
event when one appears, disappears or is replaced; `doctor` shows whether each is present. They
are not bound into the sandbox yet, so a sleeve sees only the sandbox's own `/dev`. Restart the hub
after changing the list.

## Updates and rollback

**Harnesses** update through each program's own command, only when you ask:

```
harness update [PROGRAM...]          # none means every installed one
```

A program with an active sleeve is deferred. The result (started, deferred or skipped, with a
detail) is printed and recorded as an event. There is no automatic 24-hour update.

**The hub:**

```
hub info | hub state
hub update [--check] [--channel stable|dev] [--version TAG]
hub update --rollback
hub probation [--retry-rollback]
```

`hub update` downloads the release for this OS and arch from `update_repo` (default
`hotschmoe/protectorate`) on the channel (`update_channel`, default `stable`), checks it against its
`SHA256SUMS`, stages it in `<home>/releases/<version>/`, points `releases/current` at it
(`releases/previous` keeps the old one) and writes `releases/probation.json`. It also fetches the
release's app builds into `<home>/clients/<version>/`. It does not restart the hub: restart the unit
(`systemctl --user restart protectorate.service`). `--check` only reports what it would do;
`--version` can name a release that is already staged; `--rollback` points `current` back at
`previous`, again without a restart.

Before each start, the unit runs `releases/previous/protectorate hub probation`. A release marked on
trial gets one start recorded; if it is started a second time without having become healthy, the
probation check switches `current` back to `previous`. The hub marks itself healthy once its journal has
replayed. `--retry-rollback` forces the switch back. Sleeves do not survive the restart: they
resume as after a crash. There are no journal or other snapshots, so an update that changes the
journal format cannot be undone by a rollback.

**Nodes:** `node update` tells each node to move to the hub's own version (or `--version`). The node
tries to fetch the release from GitHub itself; if that fails the hub downloads it, checks it and
streams it to the node over the fleet link. The node stages it in its own `releases/` directory,
switches `current`, and exits; its supervisor (`protectorate-node supervise`, which the systemd
unit, the launchd agent and the Windows service all run) starts the new release. The node marks the release healthy after the hub has answered its first hello; a release
that starts twice without that is switched back by `protectorate-node probation`, which the
supervisor runs before every start. The command waits up to 90 s for every node, or returns the
rollout id at once with `--no-wait` (follow it with `event list`; `--cancel ROLLOUT_ID` abandons
it). See [Windows node updates](windows-node-update.md).
Update the hub first, then the nodes: nodes move to the hub's version by default.

**Apps:** the hub offers its own release's builds of the app. `hub info` lists them as client builds:
those of `protectorate-android.apk` and `protectorate-desktop-windows-{amd64,arm64}.zip` that sit in
`<home>/clients/<version>/`. A paired app whose version differs from the hub's shows a banner; Install
downloads `GET /api/client/<name>` with its device token (any authenticated token works).

- Android installs through the system installer, which asks to confirm. The first time, Android
  asks to allow installs from Protectorate; allow it and press Install again. Every release must
  be signed with the same key as the installed app.
- Windows: the app exits, a script waits for it, copies the new files over the install
  directory and starts the app again. Under `C:\Program Files` the copy asks UAC once. A failure
  is logged to `%TEMP%\protectorate-update.log` and the old app starts again; the settings in
  `%APPDATA%\protectorate` are untouched.
- The web app is served by the hub, so its banner offers Reload.

Neither installer has run on a real device yet.

## Devices

The app's event stream starts every connection with a snapshot of the hub's state and a `live`
marker, then follows new messages. After a reconnect the app reloads the transcripts it has open.
The hub keeps the last 512 stream messages in memory; the stream's sequence restarts when the hub
does (REVISIT, "Event stream sequence").

```
device list | device add <name> | device pair <code> | device revoke <id>
pair [<name>]                  # the same as device add
token                          # prints the owner and envoy-only tokens from <home>/auth.json
```

See [install.md](install.md#pairing-a-device).

## Where state lives

The hub owns one root, `<home>` (default `~/.local/share/protectorate`). The hub does not set a
mode on it, so make it private (`chmod 700`).

| Path | What | Back up |
|---|---|---|
| `auth.json` | the hub's fleet private key, the owner and envoy-only tokens, paired devices (tokens in plain text) and open pairing codes; written with mode 0600 | yes, and keep it secret |
| `config.json` | settings you wrote (optional) | yes |
| `journal/` | the journal: every state change, in 64 MiB segments, under an exclusive file lock | yes |
| `transcripts/` | each sleeve's chat transcript, one `.jsonl` per sleeve | yes |
| `briefs/` | handoff briefs, `<sleeve>.md` | yes |
| `artifacts/` | node job logs and fetched artifacts | yes, if you need old job output |
| `worktrees/` | child sleeves' git worktrees | yes: uncommitted work lives only here |
| `envoy/` | Envoy's working directory and its `CLAUDE.md` and `AGENTS.md` | yes, if Envoy keeps files there |
| `history/` | the history index | no: derived, rebuilt from the logs |
| `releases/` | staged hub releases, `current` and `previous` links, `probation.json` | no: re-downloadable; keep it if you want to roll back |
| `clients/<version>/` | the release's app builds | no |
| `run/<sleeve id>/tmp` | each sleeve's `TMPDIR` | no |

Workspaces stay in your own directories; the hub writes nothing into a worked-on repo except the
git worktrees it creates for child sleeves (under `<home>/worktrees/`).

**Backing up.** Stop the hub (`systemctl --user stop protectorate.service`), copy the rows marked
yes, and start it again. The journal recovers a torn last frame, but a copy taken while the hub
runs can mix files from different moments, and the journal, `auth.json` and the transcripts have
to agree. To restore, put them back under an empty `<home>` and start the hub. A restore keeps the
fleet key, so nodes need no re-enrollment; without `auth.json` the hub generates a new key and every
node has to join again. The same copy moves the hub to a new machine of any architecture: point
`advertise_addrs` and the proxy at the new address. `hub export` and `hub import` do not exist.

A node keeps its state in its home: `config.json` (its name, the pinned hub key and addresses),
`node_key.hex` (its private key), `jobs/` (job logs), `releases/` and `install-manifest.json`
(default `/var/lib/protectorate-node` when installed as root, else `~/.local/share/protectorate-node`;
`%ProgramData%\protectorate-node` on Windows). Back up `node_key.hex` and `config.json` if you want
a reinstalled machine to keep its identity; otherwise it joins again as a new node.

`auth.json`, `node_key.hex` and the sleeve harness logins hold secrets. Never commit or share them.

## Logs

| Where | What |
|---|---|
| `journalctl --user -u protectorate.service` | the hub's output |
| `journalctl -u protectorate-node` | a Linux node (`--user` for a user service) |
| `sleeve transcript`, `event list`, `job logs` | what the hub saw from sleeves, itself and jobs |

A Windows node has no log file; its service output is not kept (see
[Windows node updates](windows-node-update.md)). None of these rotate beyond what journald does.

The blocking gate is `make check`; `make e2e` runs its real-binary scenarios (`scripts/zig-e2e.sh`)
and `make dst` the VOPR simulation. There is no separate background gate.
