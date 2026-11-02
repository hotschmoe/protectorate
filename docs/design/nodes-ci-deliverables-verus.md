# Nodes, git/CI, deliverables, doctor, install, Verus, UI

Design investigation for the rebuilt Protectorate, 2026-11-01. Scope: the
node daemon, git and CI, the deliverable list, `doctor`, one-line install,
Verus scope and the UI direction. Networking, enrollment crypto and the
tailnet belong to another investigator; this document assumes the hub
reaches each node over a tailnet RPC the hub initiates and nothing more.

Evidence key: **[V]** verified in the repo, git history or the Verus clone at
`references/verus` (head 39095af, 2026-10-30); **[D]** documented behaviour of
an OS or product recalled from its documentation, not re-tested here;
**[I]** inference or recommendation.

Sources read: the owner's session doc, PROBLEMS.md, ARCHITECTURE.md, DEV.md,
docs/design.md, docs/mcp.md, the pre-removal docs (`git show 4f199cc^:docs/
install.md`, `docs/node-work.md`), the Go tree at `bc9b87a^` (internal/node,
internal/desk) and the Rust node crates at `4f199cc^`.

## What went wrong with the old fleet [V]

The fleet was removed on 2026-10-29 (commit 4f199cc, "one hub machine runs
sleeves"). The history before it is a list of upkeep:

- Own CA, node certificates valid a year with renewal, mutual TLS on 7475,
  CA pinning in every invite line, a browser trust store for the app.
- Node address tracking: nodes rebinding on DHCP change and reporting their
  address back over their own certificate (the one node-to-hub call), with a
  no-NAT requirement and a `node doctor` to explain when it could not.
- Host firewall rules on every node (`ufw allow from HUB to 7475`), router
  rules per zone, and "stays invited" states when they were missing.
- Windows: a hidden logon task through `conhost.exe --headless`, a guardian
  process for updates, a second `protectorate-runner` account whose password
  was sealed with user-scoped DPAPI from an elevated interactive session
  (failed from SSH), and uninstall fixes on 10-24 and 10-26 (stop before
  unregister; recognise the installer's own task).
- Per-platform service generators (systemd user units with linger, Termux
  services plus Termux:Boot, Windows task XML), three node executables.

The redesign below removes each cause rather than patching it: identity and
reachability come from the tailnet (no CA, no certificates, no firewall
rules, no address reports); one node binary; one install mode per OS; an
install manifest that makes uninstall exact.

---

## 1. Node daemon

### 1.1 Inventory and live load

Reuse the collectors that already exist and were fuzzed [V]: Linux sysfs PCI
and platform devices (amdgpu, panthor, mali, xe, Intel NPU), Windows SetupAPI
plus PDH "GPU Engine" and "GPU Adapter Memory", `nvidia-smi`, `rocm-smi`,
`xpu-smi`, `/proc/stat`, `/proc/meminfo`, `/proc/self/mounts`,
`GetSystemTimes`, `GlobalMemoryStatusEx`, `GetDiskFreeSpaceEx`. The Rust
versions at `4f199cc^:rust/crates/protectorate-node-metrics` are the ones to
pull.

Two cadences, both hub-initiated:

| Call | Interval | Content |
|---|---|---|
| `Inventory` | on join, every 10 min, on demand | hostname, OS and version, arch, CPU model and count, RAM, GPUs/NPUs with VRAM and driver, every filesystem with size, toolchains with versions (`rustc`, `cargo`, `go`, `zig`, `node`, `python`, `dart`, `flutter`, `git`, MSVC via `vswhere`), node version |
| `Sample` | 15 s shared, 60 s care | CPU %, load, memory available, swap, per-filesystem free, GPU utilisation/VRAM/temperature, job-root usage, `presence` (1.2) |

Rules [I]: every string from a node is untrusted: length-capped (names 128
bytes, paths 4096), control characters replaced, never used as a path or a
shell word on the hub. Directory sizes (job roots, caches) are measured on
the slow interval, never per request. Unknown is `-1` or unset and never
counts as spare capacity (keep the old rule).

### 1.2 Human-present detection

The node reports facts; the hub applies policy. Report:

```
presence { state: PRESENT | AWAY | UNKNOWN, idle_secs, locked: bool|unknown,
           source: "logind" | "evdev" | "x11" | "wayland" | "wts" | "hid",
           sampled_at }
```

`PRESENT` means physical input within the window (hub default 10 min, the
hub owns the number). `UNKNOWN` is treated as present on care machines and
as away on shared machines; that asymmetry is the whole policy.

**Linux**, in order of preference, all from a non-root system service:

1. logind over the system D-Bus: enumerate `org.freedesktop.login1.Manager
   .ListSessions`, then each session's `Active`, `State`, `Type`,
   `LockedHint`, `IdleHint`, `IdleSinceHint` [D]. Readable by any user, no
   root. Caveat [D]: `IdleHint` is only meaningful if the session's desktop
   calls `SetIdleHint`; GNOME does, KDE does, sway and bare X sessions
   generally do not, so an `IdleHint=false` that never changes is "unknown",
   not "present". Use `IdleSinceHint` monotonic changes as the signal.
2. `/dev/input/event*` read directly: open each device, poll, timestamp the
   last event. Needs membership of the `input` group (udev default mode
   0660 root:input) [D], which the installer grants to the service user; no
   root at run time. This is the most reliable source and works on any
   desktop or none. Filter injected events the same way the old Windows code
   did: none exist on evdev except from `uinput` devices, which are listed by
   name and skipped.
3. Desktop-specific, only when the node runs inside the user's session
   (it does not by default): X11 `XScreenSaverQueryInfo` idle ms [D];
   Wayland `ext-idle-notify-v1` [D] (needs a compositor connection, so a
   session process); GNOME `org.gnome.Mutter.IdleMonitor.GetIdletime` and
   KDE `org.freedesktop.ScreenSaver.GetSessionIdleTime` on the session bus
   [D]. Not implemented in the first version; 1 and 2 cover the fleet.

**Windows**:

- `GetLastInputInfo` reports input for the calling process's session only
  [D]; from a service in session 0 it reports nothing useful. The old desk
  code worked around this with `WH_KEYBOARD_LL`/`WH_MOUSE_LL` hooks from a
  logon task in the interactive session, filtering `LLKHF_INJECTED` [V].
  That required the task design we are dropping.
- From a service: `WTSEnumerateSessions`, then `WTSQuerySessionInformation
  (WTSSessionInfoEx)` which returns `WTSINFOEXW` with `SessionFlags`
  (`WTS_SESSIONSTATE_LOCK`/`UNLOCK`) and `LastInputTime` [D, medium
  confidence on `LastInputTime` from a non-SYSTEM caller; verify on
  officewin]. Running the service as LocalSystem (1.3) removes the privilege
  question. If the call fails, report `UNKNOWN`.
- Also useful: an interactive session in state `WTSActive` with the console
  (`WTSGetActiveConsoleSessionId`) [D]; no active session means nobody is
  there regardless of input time.

**macOS**: the IORegistry `IOHIDSystem` property `HIDIdleTime` (nanoseconds
since last HID event), readable through IOKit or `ioreg -c IOHIDSystem`
without root and from a LaunchDaemon [D]. `CGEventSourceSecondsSinceLastEvent
Type` needs a GUI session and is not used. Lock state: `CGSessionCopyCurrent
Dictionary` is session-only; report `locked: unknown` from the daemon [I].

Keep the old guard semantics [V]: the node enforces its own presence policy
on anything that touches the desktop or care tier, whatever the hub believes;
with desktop control shelved, that is only "care nodes refuse jobs while
PRESENT unless the job carries an owner grant".

### 1.3 Safe job execution

One service mode per OS, installed once, no second mode:

| OS | Service | Runs as | Job isolation | Admin jobs |
|---|---|---|---|---|
| Linux | systemd system unit, `Delegate=yes`, `ProtectSystem=strict`, `ProtectHome=read-only` plus the job root | dedicated user `protectorate` | user+mount+pid+ipc+uts+net namespaces (our sandbox helper, pulled from protectorate-sandbox), one cgroup per job under the delegated subtree: `cpu.max`, `memory.max`, `pids.max`, `cgroup.kill` to cancel [D, cgroup v2, kernel 5.14+] | `sudo -n` rules for the service user, allowlisted commands only, under an owner grant |
| Windows | Windows service, LocalSystem (one elevated install) | SYSTEM; jobs as local standard account `protectorate-runner` via `CreateProcessAsUser` in a Job Object (CPU rate, memory, `KILL_ON_JOB_CLOSE`) [V old code reusable] | the Job Object; runner password sealed with machine-scope DPAPI, ACL SYSTEM-only [I] | run as SYSTEM, flagged, grant required |
| macOS | LaunchDaemon, `UserName=_protectorate` | `_protectorate` | separate user `_protectorate-job` via the daemon running as root only at spawn is not possible without root; first version: same user, `sandbox-exec` profile (deprecated but present) [D]; report `isolation: weak` | none |
| Termux | `termux-services` `sv` | the Termux user | none; plain jobs only with `--allow-plain-jobs` [V old rule] | none |

Why the Linux change [I]: the old node needed a user systemd manager with
linger and `systemd-run --user`; a system unit with `Delegate=yes` lets the
node manage its own cgroup subtree without root and without the user
manager, and starts at boot with nothing else configured. Unprivileged user
namespaces are still required for the mount/pid isolation (Ubuntu 24.04's
AppArmor restriction must be relaxed for the service profile; `doctor`
checks `kernel.apparmor_restrict_unprivileged_userns` and
`user.max_user_namespaces`). A node that cannot isolate refuses sandboxed
jobs rather than running them plain, as before.

Limits, kept from the fuzzed implementation [V]: default timeout 1 h, cap
24 h; 64 MiB per job log, 256 MiB retained across jobs, 4096 retained jobs,
retention 1 h after end so a restarted hub can reattach; cancel sends
SIGTERM/`cgroup.kill`/`TerminateJobObject`, 3 s grace, then SIGKILL of the
whole group. The job records `mode` (sandboxed, plain, admin) and the
isolation the node reported. Nothing retries a job by itself.

Output as untrusted bytes [I]: the node stores the log and the hub pulls
`ReadLog(job, offset, max)` (hub-initiated, like everything). The reply is a
frame `[u8 kind][u32 BE len][bytes]`, len <= 1 MiB; the hub parser is a Verus
module (6). The hub never converts job output to `String` without lossy
conversion, strips ESC and C0/C1 controls before a sleeve or the UI sees it
(terminal injection into a harness TUI is the real risk), and stores the raw
bytes separately for download. Inventory strings follow the same rule. Exit
codes: 124 timeout, 130 cancel, 137 killed (keep).

Work shipping: `--git repo@commit` as a thin bundle of missing objects into a
warm per-repo worktree on the node, `--artifact` for files, results pulled
back as a bounded archive (xfer rules: entry and byte caps, no `..`, no
absolute paths, no symlink escape) [V rules exist].

---

## 2. Git and CI

**Recommendation: `gh` on the hub against GitHub; no local git server; CI
across architectures is Protectorate's own node jobs; plain binaries.**

Git hosting:

- GitHub already holds the repos and the releases the installers verify.
  `gh` is one binary with its own login (`gh auth login`, `gh auth status`,
  `gh auth setup-git` for HTTPS pushes) [D], which is exactly the hands-off
  rule applied to a tool.
- Inside a sleeve, `gh` and git authenticate with a fine-grained PAT limited
  to the owner's repositories (contents, pull requests, issues; 90-day
  expiry) exposed as `GH_TOKEN`, provided by the sandbox the way harness
  credentials are (read-only, live, never copied) [I]. The sleeve's egress
  proxy admits `github.com` and `api.github.com`. Envoy and sleeves then use
  every GitHub feature without any code of ours.
- Backup against losing GitHub: a hub job runs `git clone --mirror` /
  `git remote update` for every repo in the directory into
  `$HOME/backups/git/` on a schedule and after each default-branch advance
  the directory already observes [V directory exists]. That is the whole
  "backup to GitHub or from GitHub" story: GitHub is primary, the hub holds
  bare mirrors, no server.
- Forgejo/Gitea/GitLab rejected for now: a second service with its own
  database, auth, TLS, runners, upgrades and backups, all of which are the
  upkeep the owner just removed from the fleet. If GitHub becomes a problem
  later, Forgejo as a push mirror is an afternoon's addition, and nothing
  above has to change.

CI across architectures:

| | Forgejo Actions / Woodpecker / GitLab runners | Protectorate node jobs |
|---|---|---|
| Scheduler | their own, runner polls the server (node-to-server traffic, which our model forbids on the node side) | the hub, hub-initiated, inventory-aware (arch, toolchain, free memory, warm worktree, presence) |
| Trust model | runner holds a server token; the server trusts runner output | node is untrusted; output is bytes; hub audits |
| Isolation | docker by default; Forgejo runner also has `host` and `lxc` labels, Woodpecker a `local` backend [D, medium] | our sandbox (1.3), the same code as sleeves |
| Windows/Android | docker not available; host executors only | native, same path as Linux |
| Trigger | push webhook (needs the server reachable from GitHub, so not LAN-only) | hub rule: directory sees a default-branch or tag advance, or a sleeve asks |
| Extra deliverables | server + runner binaries, configs, upgrades | none |

So: a hub rule "on default-branch advance of project P, run `make test` on
one node per arch in P's matrix, post the result as a note and as a GitHub
commit status through `gh api`" [I]. Sleeves get the same through `job run
--on os=linux,arch=arm64 -- make test`. Containers only when a job asks
(`--image`) on a node that already runs Docker; never required, never on
Windows or Termux. Toolchains are installed by the owner once per node
(rustup, flutter) and reported in inventory; `--needs rustc>=1.90` selects.

---

## 3. Deliverables

Today's release [V DEV.md]: five Linux binaries per arch (`protectorate-rs`,
`protectorate`, `protectorate-mcpbridge`, sleeve runner, sandbox helper), the
web archive, the Windows desktop zip, `VERSION`, `SHA256SUMS`; before the
fleet removal also three node executables. Proposed:

```
protectorate-linux-{amd64,arm64}          hub binary: serve | cli | mcp | sleeve-runner
                                          | sandbox-helper (self re-exec) | doctor;
                                          embeds the Flutter web build and (other
                                          investigator) headscale
protectorate-node-{linux-amd64,linux-arm64,windows-amd64.exe,windows-arm64.exe,
                   android-arm64}         node binary: serve | install | uninstall | doctor
                   (macos-arm64 only once a Mac exists to build and sign it)
protectorate-desktop-linux-{amd64,arm64}.tar.gz, protectorate-desktop-windows-{amd64,arm64}.zip,
protectorate-android.apk (signed with the owner's key; recipe in memory)
install.sh, install-node.sh, install-node.ps1   in the repo, fetched by tag
VERSION, SHA256SUMS, SHA256SUMS.sig
```

Two binaries, as asked. Notes:

- The sleeve runner and both sandbox helpers become subcommands of
  `protectorate` that re-exec `/proc/self/exe` [I]; the "refuses to serve
  without the helper beside it" check disappears. The hub binary is Linux
  only; it never needs to build for Windows.
- The CLI lives in the hub binary and runs on the hub: sleeves get it
  mounted at `/opt/protectorate/bin/protectorate` as today [V]; Envoy is a
  sleeve. A remote human uses the Flutter app or `ssh hub protectorate ...`.
  No Windows CLI build.
- Vendor general agents (GrokBot, Claude Duet, ChatGPT Dot) installed on a
  desktop or phone: I could not verify what each one can connect to today,
  and nothing here should depend on a guess. What is verifiable: Claude Code
  and Codex register stdio MCP servers (`claude mcp add`, `codex mcp add`)
  [V docs/mcp.md]; Claude Desktop registers stdio servers through its config
  file [D]; cloud-side agents can only reach a network endpoint. So the hub
  serves MCP over streamable HTTP itself (behind Traefik, device token as
  bearer, LAN only) and a local `protectorate mcp` stdio bridge exists for
  harnesses on the hub. Any agent that can speak remote MCP gets the whole
  API; one that cannot gets nothing until it can, and we do not write a
  plugin per vendor.
- **Is MCP better than the CLI?** Honestly, marginally, and only because it
  is free: the bridge derives every tool from the proto [V], so it costs no
  hand-written code. What MCP adds: the model sees a typed schema instead of
  reading `--help` (fewer tokens, fewer malformed calls), arguments are
  validated before any side effect, `readOnlyHint`/`destructiveHint` let the
  client gate calls, and a process with no shell (a desktop app, a cloud
  agent) can use it. What the CLI keeps: it works in every harness that has a
  shell, composes with pipes and scripts, needs no registration and is what
  Envoy drives. Keep the CLI primary, MCP as the same API for clients that
  cannot run a shell. Do not build features in one that are not in the
  other; both are generated from the proto.
- The hub serves the Flutter web build: yes, embedded in the binary
  (`include_dir`-style; about 5 MB), one origin, no CORS, one file to update
  [I]. `PROTECTORATE_WEB_DIR` overrides for development without Flutter.
- Flutter desktop: Linux and Windows are built today [V]; macOS has a
  project dir but no builder or signing path; ship it when there is a Mac.

---

## 4. `doctor`

`protectorate doctor [--json]` on the hub, `protectorate-node doctor` on a
node (also run by the hub over RPC). Output is a list of checks:

```
{ id, status: ok|warn|fail|skip, summary, detail, fix, checked_at }
```

Checks, hub:

| id | What | Never |
|---|---|---|
| `harness.<name>.binary` | path, `--version`, sha256 (pinning) for claude, codex, grok, pi, gemini/agy | |
| `harness.<name>.credentials` | file present (`~/.claude/.credentials.json`, `~/.codex/auth.json`, `~/.grok/auth.json` [V quota crate]), mode 0600, expiry timestamp if the file carries one, and **valid** = the read-only quota reader succeeds (no refresh, no redeem) | prints a token, account id, email or any field value; reports names of fields and times only |
| `gh.auth` | `gh auth status` exit code; `gh api user` login; PAT expiry if GitHub reports it | the token |
| `sandbox.userns`, `sandbox.cgroup`, `sandbox.systemd`, `sandbox.tmux`, `sandbox.git` | unprivileged user namespaces allowed, cgroup v2 with delegation, `systemd-run`, tmux and git versions | |
| `disk.home`, `disk.workspaces`, `disk.tmp` | free bytes and percent against thresholds | |
| `clock` | `NTPSynchronized` from timedatectl, offset against the `Date` header of a GitHub response [I] (OAuth and JWT validity depend on it) | |
| `egress.<host>` | the egress proxy can reach each provider host | |
| `release` | running version vs latest, trial marker state | |

Node: the same shape with `isolation`, `input-group`, `presence.source`,
`toolchains.*`, `disk.*`, `clock`, `service` (unit/service registered and
enabled, install manifest consistent).

How it feeds dispatch [I]: the hub runs the cheap checks on start and every
10 minutes, credential validity every hour, and stores `doctor.json`. A
harness is **eligible** only when its binary and credentials are `ok` and
quota shows remaining capacity; `quota suggest`, resleeve-on-guardrail and
the future dispatch-by-quota choose only among eligible harnesses. A check
turning `fail` sends one mail to the owner with the `fix` line and is never
retried by an LLM. Nothing in this path asks a model anything.

---

## 5. One-line install

UX only; the enrollment owner specifies the device code.

Hub (Linux, per user, no sudo except `loginctl enable-linger` once):

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/vX.Y.Z/install.sh | bash
```

Node, after `protectorate node add laptop` on the phone shows `WXYZ-1234`:

```
curl -fsSL https://protectorate.hotschmoe.com/node.sh | sh -s -- WXYZ-1234      # Linux, macOS, Termux
irm https://protectorate.hotschmoe.com/node.ps1 | iex; node-join WXYZ-1234       # Windows, elevated once
```

Rules:

- **Signing without a new dependency**: `SHA256SUMS` is signed with
  `ssh-keygen -Y sign` and verified with `ssh-keygen -Y verify` against an
  allowed-signers line embedded in the script and printed in the README [D,
  OpenSSH 8.1+ ships on Linux, macOS and Windows 10+]. The release key lives
  only on the owner's machine. The hub re-serves the release assets it
  installed (nodes never fetch from the internet, as before [V]) and the
  node script verifies the same signature, so a compromised hub cannot ship
  a node a different binary.
- The script pins the version in its URL (fetched by tag), verifies, stages
  under `releases/<version>/` with `current`/`previous` links (keep [V]),
  writes **one install manifest** (`install.json`: every file, unit, task,
  user, group membership and sudoers line it created, with the version),
  then starts the service. `--dry-run` prints the manifest it would write.
- Upgrade: hub-initiated. `hub update` as today for the hub [V]; for nodes
  the hub pushes the verified node binary and signature over the RPC, the
  node checks the signature, swaps atomically, re-execs, keeps `previous`,
  and the next hub poll confirms or the node rolls back. No guardian
  process, no node-side downloads.
- Uninstall that works: `protectorate-node uninstall` reads the manifest and
  removes exactly those items in reverse order (stop, disable, unregister,
  delete files, remove the user and group lines) and refuses anything the
  manifest does not list. It prints the plan, asks, and `--yes` skips the
  question. The two old Windows bugs were an unlisted task and a wrong order
  [V]; the manifest makes both impossible.
- No firewall step and no certificate step appear in any script. Both were
  the bulk of the old upkeep and both are removed by the tailnet (assumption
  on the other investigator's design).
- Phone flow: ADD NODE shows the code and the one line for each OS, with a
  QR of the line; the hub shows the node as `joining` until its first
  successful poll, then `ok`, with the failure reason in between (keep the
  "why is it still invited" lesson [V]).

---

## 6. Verus

Verified facts about Verus today [V, `references/verus` 39095af]:

- `cargo verus verify|focus|build` exists; crates opt in with
  `[package.metadata.verus] verify = true`; `--rlimit N` is passed after
  `--` (default infinity); `cargo build` still works on annotated crates.
- During verification the driver sets `--cfg verus_only --cfg
  verus_keep_ghost` (and `verus_keep_ghost_body`); plain cargo never sets
  them. `unexpected_cfgs` must list them in `[workspace.lints.rust]`.
- Attribute syntax for existing code: `#[verus_spec(requires ... ensures
  ...)]`, `#[verus_verify(external_body | rlimit(n) | ...)]`, `proof!{}` and
  `proof_decl!{}` inside bodies. `builtin_macros/src/attr_rewrite.rs`
  explicitly supports `#[cfg_attr(pred, verus_spec(..))]` ("code that is also
  built without Verus gates its specifications this way").
- `vstd`, `verus_builtin` and `verus_builtin_macros` are published to
  crates.io (`0.0.0-2026-10-25-0158`) as an erased build "for linking with
  non-Verus Rust code". The `verus_only` cfg must guard only `use` lines and
  config attributes; guarding code with it is unsound (guide, erasure.md).
- Toolchain pinned by the Verus release: `1.99.1` at this head.

Shipping shape (rule 8) [I, test in the first module]:

```toml
[dependencies]
vstd = { version = "=0.0.0-2026-10-25-0158", optional = true }
[features]
verus = ["dep:vstd"]
[package.metadata.verus]
verify = true
```

```rust
#![forbid(unsafe_code)]            // except the one helper, see below
#[cfg(verus_only)] use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires buf.len() <= u32::MAX as usize,
    ensures  match r { Ok((f, n)) => n <= buf.len() && f.len as usize <= MAX_FRAME, Err(_) => true }
))]
pub fn decode(buf: &[u8]) -> Result<(Frame, usize), NeedMore> { ... }
```

`verify.sh` runs `cargo verus verify --features verus -p <each crate> --
--rlimit 20`, then again with `--rlimit 10`; a proof that passes the first
and fails the second is "fragile" and fails CI (rule 7). Plain `cargo build`
never enables the feature, so the shipped binaries contain no Verus crate
and the file is byte-identical to the verified one. `proof!` blocks are
gated with `#[cfg(verus_keep_ghost)]` on the statement; they are ghost code
by construction, so the erasure warning does not apply to them. Pin the
Verus release zip URL and its sha256 in `verify.sh`; CI job on Linux amd64
only; run on every push to the working branch.

Two cheap rules for every parser and writer crate (rule 3):
`[profile.release.package.<crate>] overflow-checks = true` (the `dist`
profile inherits release [V]), and a fuzz target per module comparing the
new implementation against the old one or a second simple implementation
(the existing libFuzzer setup [V]).

Modules that qualify, each with its top-level contract:

| Module | Reads/writes | Property to prove |
|---|---|---|
| `frame` (node log and RPC body frames, 1.3) | bytes from an untrusted node | `decode` never indexes past `buf`; `consumed <= buf.len()`; a declared `len > MAX_FRAME` is an error, never an allocation; `decode(encode(f)) == Ok((f, encode(f).len()))`; a strict prefix yields `NeedMore`, never a frame |
| `logtail` (chat.jsonl and job-log tail reader, today's `chatlog` with `offsets: Vec<u64>` and `seek` [V]) | offset/size arithmetic | offsets strictly increasing and every offset `<= size`; after `append(n)` the new offset equals the old size and `size` grows by exactly `n`; `read(after)` returns `[offsets[after], size)` with no overflow (`u64` checked add); `watch` never re-emits an event |
| `index` writer (sleeve registry / chat index snapshot) | writes a file the hub and backup/restore read | the written bytes parse back to an equal index; records are length-prefixed and a truncated tail is detected, never read as a record |
| `wire::streaming` envelope (`[flags u8][len u32 BE]` [V]) | bytes from any client | same contract as `frame`; `len <= limit` or error before any copy |
| `sums` (`SHA256SUMS` and `install.json` manifest parser used by every installer and by node update) | release bytes before signature verification is complete | never panics on any input; every returned entry has exactly 64 hex digits and a relative path with no `..`, no empty component, no absolute root; duplicate paths rejected |
| `xfer` bounds (job artifacts in/out) | archive bytes from a node | entry count and total bytes stay under the caps at every step; every extracted path is confined under `dst` |
| `audit` chain walk | our own JSONL read back | `verify` accepts iff `seq == i` for all `i` and `entry[i].prev == entry[i-1].hash`; the hash itself is `external_body` |
| `backup` snapshot writer/reader | writes `.pbackup` that `restore` reads | round trip; the length table is consistent with the byte stream; a truncated archive is detected before any file is written |
| `presence` window math | timestamps from the node | `idle_secs` computed with saturating subtraction; a future `last_input` yields `UNKNOWN`, never `PRESENT` |

Hand-off: if the networking investigator adds a wire codec for the tailnet
RPC envelope, it belongs in this table with the `frame` contract.

Explicitly excluded (rule 2): all Flutter code; harness drivers (vendor JSON
through serde, compared against the vendor's own schema and fuzzed by
differential runs); quota readers (floats, vendor JSON); metrics collectors
(floats and OS glue; fuzzed with golden outputs, which already exist [V]);
the sandbox launcher and every FFI call (syscalls, Windows API) marked
`external_body`; the hub's RPC handlers, Envoy, mail, coordination (glue);
headscale (Go). Verus is also not used to prove anything about time, the
network or the tailnet.

`unsafe` (rule 6): `#![forbid(unsafe_code)]` in every crate except two kinds.
(a) A verified module may hold one private helper with `#[allow(unsafe_code)]`
whose bounds Verus proves at every call (`get_unchecked` after a proven index
check). (b) The OS layers (sandbox helper, Windows process and job-object
code, evdev) cannot avoid FFI; rule 6 cannot apply to them literally. They
get `#![deny(unsafe_op_in_unsafe_fn)]`, clippy
`undocumented_unsafe_blocks`, no pointer arithmetic on bytes that came from
outside the process, and nothing else in the workspace may contain
`unsafe`, enforced by a `make check` grep. This is an owner decision (see
the end).

---

## 7. UI direction

Superseded on 2026-11-02 by SPEC section 9: the owner wants a clean, professional desktop app, not a
TUI. The rules and mockup below are kept as history.

The current design doc [V docs/design.md] is already amber VFD, JetBrains
Mono and an IBM-engineer brief, but it draws instruments: bezels with lit
edges, glass windows, glow, arc gauges, DSEG7 digits, radius 10. The owner's
new brief is a 3270 session on a System/370 console, not a hi-fi front panel.
Keep the palette tokens; change the drawing rules:

1. One typeface, one size for everything but titles: JetBrains Mono 13 px
   on a 4 px grid; no DSEG7, no 40 px readouts. Emphasis is colour and
   reverse video, never size.
2. No bezels, no glass, no glow, no shadows, no gradients, radius 0 on
   every element. Boxes are single rules (1 px) in `rule`; double rules only
   around the focused pane.
3. Every pane has a legend in its top rule (`- SLEEVES ---------`) and a
   status word at the right end of the same rule (`3 WORKING  1 BLOCKED`).
4. Density: rows are 20 px, no padding above 4 px, no cards, no empty
   states with illustrations; an empty table prints `(none)`.
5. Function-key legend as the bottom line of every screen, 3270 style:
   `F1 HELP  F2 SPAWN  F3 KILL  F5 REFRESH  F7/F8 PAGE  F9 NODES  F12 ENVOY`,
   bound to the keys on desktop, shown as a tappable strip on the phone.
6. Meters are character bars (`[########........] 52%`), 16 cells, zone
   colours; unknown is `[ -------------- ]`. No arcs, no lamps, no blinking;
   "needs you" is reverse video in `warn`.
7. Colour carries meaning only: `lit` text, `dim` secondary, `info` for
   selection and links, `warn` for waiting-on-you, `alarm` for failed or
   unreachable, `ok` only for a status word. No decorative colour.
8. Tables everywhere a list is shown: fixed-width columns, right-aligned
   numbers with tabular figures, left-aligned identifiers in their own case,
   uppercase for states and legends. Sorting by clicking the header rule.
9. Navigation is a tree in the left pane (workspaces > sleeves > children)
   and a command line at the bottom (`===>` prompt, as on a 3270) that takes
   the same commands as the CLI; the phone keeps the tree as its first
   screen and the command line behind a key.
10. Light mode is the line-printer rendering of the same screen (buff bars,
    black and red ribbon), unchanged from today; it inherits rules 1 to 9.

Main screen, 1200 px and up:

```
 PROTECTORATE 0.7  hub stronghold  up 11d 04:12   quota C 62% X 41% O 88% L -   09:41:07
=-- WORKSPACES ----------------------=-- PANE: herdr-port / claude ------- WORKING 00:14 -=
 protectorate              2 sleeves |> turn 9  tokens 41.2k  cache 93%  budget [####....]  |
   envoy            claude  IDLE     |                                                      |
   la/app           codex   WORKING  | user  run make test on arm64 and report              |
     > fuzz-wire    pi      WORKING  | tool  job run --on arch=arm64 -- make test           |
 herdr-port                1 sleeve  |       job j-4f2a on ms-r1 ... exit 0 in 3m12s         |
   herdr-port       claude  WORKING  | agent Tests pass on arm64. Opening PR #41.            |
 notes                     3 open    |                                                      |
                                     |-- INPUT ---------------------------------------------|
=-- NODES ------------ 4 UP 1 AWAY -=| ===> _                                               |
 NAME       ARCH   LOAD  MEM   DISK  |=-- MAIL / EVENTS ------------------------ 2 UNREAD -=|
 ms-r1      arm64  [##..] 41%  [#..] | 09:40 la/app -> envoy   request: approve pin change   |
 officepc   amd64  [....]  8%  [##.] | 09:38 directory         main advanced: protectorate  |
 officewin  amd64  PRESENT    [#..]  | 09:31 job j-4f2a        exited 0 (ms-r1)             |
 kidsbox    arm64  CARE  PRESENT     |                                                      |
 b70-nas    amd64  [###.] 77%  gpu1 BUSY                                                    |
=-- QUOTA --------------------------=|                                                      |
 claude  5h [######..] 62%  r 02:11  |                                                      |
 codex   wk [#######.] 88%  r 4d     |                                                      |
 grok    wk [####....] 41%  r 2d     |                                                      |
 F1 HELP  F2 SPAWN  F3 KILL  F5 REFRESH  F7/F8 PAGE  F9 NODES  F10 QUOTA  F12 ENVOY
```

Phone: the WORKSPACES tree alone, then the pane on tap; NODES and QUOTA
behind F9/F10 on the key strip; the `===>` line behind a key.

---

## Rejected alternatives

- Nodes as per-user services (old design): fewer privileges but needs
  linger, a user systemd manager, DISPLAY-less sessions, and on Windows a
  logon task with a guardian. One system service per OS is less code.
- Windows node as a virtual service account (`NT SERVICE\name`, no
  password): attractive, but spawning jobs as another user and querying
  other sessions' input needs privileges a virtual account may lack; not
  verified, so LocalSystem with an explicit runner account wins for now.
- Containers for jobs: solves toolchains on Linux only, adds Docker to
  every node, impossible on Windows/Termux. Toolchains are installed once
  by the owner and reported by inventory.
- Forgejo/Gitea/GitLab plus Woodpecker: a second scheduler, runner polling
  (node-to-server), docker default, webhooks that need WAN; all upkeep.
- A separate MCP binary and a Windows CLI: both fold into the hub binary
  and the hub's HTTP endpoint.
- minisign/cosign for release signing: one more tool on every machine;
  `ssh-keygen -Y` is already there.
- Verus on quota, metrics, drivers or the sandbox: floats, vendor schemas or
  FFI glue; rule 2.
- The VFD instrument look: beautiful, but it is the "bubbly" the owner means.

## Risks

- `WTSINFOEXW.LastInputTime` from a service and `HIDIdleTime` from a
  LaunchDaemon are [D] not [V]; verify on officewin before relying on them,
  and the `UNKNOWN` rule covers the failure.
- Unprivileged user namespaces are disabled or restricted on some distros
  (Ubuntu 24.04 AppArmor); `doctor` must say so and the node must refuse
  sandboxed jobs rather than run plain.
- LocalSystem on Windows is a large privilege for a daemon that executes the
  hub's requests; the hub-only RPC path and the runner account are what
  contain it. An owner who prefers a least-privilege service accepts losing
  cross-session presence detection.
- A fine-grained PAT in every sleeve is a long-lived secret exposed to
  agents; scope it to the owner's repos, expire it at 90 days, and let
  `doctor` report its expiry.
- The optional-`vstd` shipping arrangement is inferred from the Verus
  sources, not from a working project; prove it with the `frame` module
  first and fall back to an unconditional erased `vstd` dependency if the
  cfg gating fights the macros.
- Verus pins a specific nightly-ish toolchain (`1.99.1` today); `verify.sh`
  must use its own toolchain and never the workspace's.
- Embedding the web build in the hub binary ties the Rust release to a
  Flutter build in CI; `PROTECTORATE_WEB_DIR` keeps local builds possible.

## Owner decisions

1. Windows node as LocalSystem with a runner account (recommended) or a
   least-privilege service that reports presence `UNKNOWN`.
2. Linux node as a system service (one `sudo` at install) rather than the
   old no-sudo user service.
3. FFI `unsafe` in the OS crates as an explicit exception to rule 6, with the
   guards above, or no evdev/Windows-API code at all.
4. `gh` with a fine-grained PAT inside sleeves, or `gh` only in Envoy and
   sleeves push through the hub (more code, smaller blast radius).
5. Embed the Flutter web build in the hub binary.
6. macOS node and desktop: build when a Mac exists, or drop from the list.
7. The `===>` command line and F-key legend in the Flutter app (rule 5 and
   9); they are the largest visible change from today's app.
