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
node update (<node>... | --all) [--version TAG] [--channel stable|dev] [--probation SECS] [--no-wait]
```

- **ls** shows each node's OS and arch, link, presence (PRESENT, AWAY or UNKNOWN), last contact and
  version; **show** adds the inventory and where presence came from.
- **add** with no argument prints the join command for a new machine, the hub's control-key
  fingerprint and pending registrations; with the registration text that `join` printed, it
  records it. **approve** registers, tags and names the node. See [install.md](install.md).
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

- `--node` names a node by id or name; `--arch` lets the hub pick an AWAY node of that arch (the
  least loaded one).
- A node refuses to start a job while someone is present or presence is UNKNOWN.
  `--even-if-present` overrides that for one job.
- If someone arrives mid-job, the job drops to idle priority and keeps running; `--on-present
  kill` kills it instead.
- `--timeout` defaults to 1 hour; a log is capped at 64 MiB. A job never re-runs by itself.
- Without `--cwd` a job runs in its own directory on the node. `--artifact PATH` (repeatable)
  names a file inside that directory to fetch when the job ends; `job artifact` writes it to
  `--out` or stdout after checking its sha256.
- `job log` lags a running job by up to 10 s.

A job result is the node's claim: `job show` says which node reported it.

## Quota

```
quota list
```

One row per subscription (Claude, Codex, Grok, the local endpoint): the reading, its windows with
percent used, and a note. The readers are read-only: they use your existing logins and never
refresh or print a token. They run every `quota_poll_secs` (60; 0 turns them off), and a
reading is kept 5 minutes. Resleeve targets are weighted by them.

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
rebuilt in the background if it fails to open.

## Events

```
event list [--from SEQ] [--limit N]
event read <seq>
```

Hub events: sleeve, job, node, approval, doctor and update news. `event read` marks everything up
to that seq as read. Events live in memory (the last 500).

## Doctor

```
doctor [--refresh]
```

Checks this machine: each harness CLI (path, version, install method, login present), `gh`
login, the sandbox prerequisites (`bwrap`, `systemd-run --user`, linger), disk, clock, `curl` and
the embedded headscale and member. The result is cached in `<home>/doctor.json` and gates which
harnesses dispatch and resleeve may use; a harness marked `NO` under ELIGIBLE is skipped. It runs
at start, every 6 hours, and on `--refresh`. It checks that login files exist, not that they are
valid. Nodes have no doctor yet.

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
unit runs before each start. `--check` only reports what it would do; `--rollback` goes back to
`releases/previous`. The channel defaults to `release_channel` in `config.json` (`stable`).
Sleeves do not survive the restart: they resume as after a crash.

**Nodes:**

`node update` asks each node to fetch the release from GitHub itself, verify it, stage it and
restart through its service manager. A node that is not contacted by the hub within the
probation window (`--probation`, default 600 s) reverts to its previous binary. The command waits
for every node unless `--no-wait`. Update the hub first, then one node per OS and arch, then the
rest. A node with running jobs answers `busy`; a Windows node answers `unsupported`.

The hub talks only to nodes at its own node protocol (`node ls` shows "reinstall the node"
otherwise). When a release bumps the protocol, update the nodes first, from the old hub, and the
hub last. A node the old hub cannot update is reinstalled with the new release.

Both update paths are tested end to end against a stand-in release server, and have run on the
real fleet: `hub update` from v0.7.0-dev.1 to dev.2 and dev.3, `node update` from dev.2 to dev.3.

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
| `sleeves/<id>/` | a sleeve's private `home/` (harness logins and native session logs), `transcript.jsonl`, checkpoints |
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
