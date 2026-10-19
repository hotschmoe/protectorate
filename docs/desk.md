# Files and desktop on other machines

The `protectorate` binary on a PC or server lets agents work it. Anything that
can be done in a shell stays a shell: `job run`, or plain `ssh`. Two small
command sets cover what a shell cannot: moving files, and driving a GUI.
Layering for programs such as ArchiCAD or ENERCALC (PROBLEMS.md P11): use the
program's own API first, then files in and files out around a batch run, and
screen and input only as the last resort.

## File transfer

```
protectorate node put <node> <local> <remote> [-f] [--no-compress]
protectorate node get <node> <remote> <local> [-f] [--no-compress]
```

Files and directories both work. The hub relays; it stores nothing. `<remote>`
and `<local>` name the destination itself, not a directory to copy into.
Without `-f` an existing destination is an error.

- **Streaming and memory.** The transfer is a sequence of 4 MiB chunks over
  plain unary calls (put) or one server stream (get), so it works over HTTP/1.1
  and memory stays at a few chunks on every machine.
- **Compression.** Each chunk is compressed with zstd only when that saves
  more than about 6 percent. Files with an already-compressed extension (zip,
  gz, png, jpg, mp4, pdf, docx, ...) are never compressed, and after three
  chunks in a row that do not shrink the sender stops trying. Chunks travel
  compressed through the hub unchanged.
- **Verification and atomicity.** The sender ends with the byte count and the
  SHA-256 of the uncompressed stream. The receiver writes to a temporary file
  next to the destination, checks both, then renames it into place; any
  mismatch or interruption leaves the destination untouched. A directory is a
  tar stream: it is unpacked into a temporary directory and renamed, replacing
  an existing one only with `-f`. Symlinks and special files are skipped (the
  count is reported); the receiver rejects absolute names, `..`, links and
  devices in the archive.
- **Path rule.** On the node, a relative path is under the node's file root
  (`$PROTECTORATE_HOME/node-files`, or `node serve --file-root DIR`), and an
  absolute path must resolve inside it too, symlinks of existing ancestors
  included. `node serve --file-unconfined` lifts the rule for nodes where the
  agent is trusted like a shell: then any path the agent's OS user can reach is
  allowed. The hub's own node follows the confined rule with
  `$PROTECTORATE_HOME/node-files`. Jobs, by contrast, run any command as the
  agent's user and are not confined; the file root exists because file calls
  are a narrower grant than `job_submit_job` and should stay narrower.
  The check is lexical plus symlink resolution at call time, not a sandbox: a
  program on the node that races symlink creation could still win. Treat the
  file root as a convenience boundary, not a security boundary against local
  users of the node.
- **Scopes.** `node_put_file` and `node_get_file` (globs: `node_put_*`,
  `node_get_*`). `read` does not include them. `*` does. Sleeves use the CLI
  with their own token; the hub API is `NodeService.PutFile` and `GetFile`.

## Desktop commands

```
protectorate desk <node> screenshot [-o file.png]
protectorate desk <node> windows
protectorate desk <node> --lease ID click X Y [--button left|right|middle] [--double]
protectorate desk <node> --lease ID move X Y
protectorate desk <node> --lease ID type "text"
protectorate desk <node> --lease ID key ctrl+s
protectorate desk <node> --lease ID run <program> [-- args]
```

Coordinates are pixels of the screenshot (origin at the top-left of the
virtual screen, all monitors). `windows` lists visible top-level windows with
title, position, size and process id. A key chord is zero or more of ctrl, alt,
shift, win followed by one key: a letter, digit, F1 to F24, or a name such as
enter, tab, esc, space, backspace, delete, home, end, pageup, pagedown, left,
right, up, down. Write `--` before a program's own arguments and before
negative coordinates.

Implemented on Windows only, with system calls and no cgo: GDI for the screen
(encoded as PNG), `SendInput` for mouse and keyboard, `EnumWindows` for the
window list and `CreateProcess` for `run`. Linux and Android nodes answer
`unimplemented`.

### The interactive-session requirement

Windows runs services and everything started by sshd in session 0, which has
no visible desktop. Screen and input only work for a node agent started in the
logged-in user's own session. The agent checks its session and window station
and, when it is not on `WinSta0` in a session other than 0, every desk command
fails with `failed_precondition` naming the fix. Fix it by starting the agent
at logon as that user, which one command does (see docs/install.md):

```
protectorate node install --control URL --key KEY
```

An agent started by hand from an SSH session does not qualify, even though the
same user owns it. The logon task needs no elevation. Input does not reach
windows of higher-integrity programs (run as administrator) because of UIPI,
unless the agent is elevated too: `node install --elevated`, from a terminal
opened with Run as administrator, registers the task with the highest
available privileges.

### Locked screens and unattended machines

Screenshots of a locked screen show the lock screen, and input fails: when the
screen is locked or at the logon screen, the user's desktop does not receive
input. A machine agents are meant to drive must therefore stay logged in and
unlocked. The options, none of which protectorate changes for you:

- a dedicated account that is always logged in (Windows auto-logon), used only
  for this;
- turn off lock on idle: no screen saver with "on resume, display logon
  screen", sleep set to never, lock on wake off;
- a physical or virtual display that stays attached, so the session keeps a
  desktop.

`protectorate node doctor`, run on the node, reports it before an agent trips
over it:

```
ok    session        interactive desktop session (desk commands can run)
FAIL  screen         locked or showing the logon screen ...
ok    startup        logon task registered, agent running (pid 4120)
ok    enrollment     enrolled (...)
info  hub            not checked: pass --hub URL ...
```

It checks the session and window station (the `desk` precondition), the input
desktop via `OpenInputDesktop` (locked, logon screen or screen saver), the
logon task and agent process (`schtasks /Query`, `tasklist`), the tailnet
enrollment, and, when `--hub URL` is given, that the hub answers over HTTP. It
exits nonzero when a check fails and changes nothing. On Linux it checks the
systemd user unit and linger; on Termux the service scripts.

### The desktop lease

The desktop is a lease resource named `desk:<node>`, separate from the node
lease, so a build job on the machine is not blocked by someone driving its
screen and the reverse.

`click`, `move`, `type`, `key` and `run` run under a lease on exactly that
desktop; `screenshot` and `windows` need none, so anyone can watch.

**Automatic lease.** A simple sequence needs no lease call:

```
protectorate desk office click 400 300      # stderr: desk lease lease-xxxxxxxx (auto, ... expires ...)
protectorate desk office type "hello"
protectorate desk office key ctrl+s
```

When a mutating command names no lease and the desktop is free, the hub takes
a 2 minute lease for the caller, holder `auto:<owner or token:NAME>`, and
returns its id and expiry (response headers `Protectorate-Desk-Lease` and
`Protectorate-Desk-Lease-Expires`; the CLI prints them on stderr). Every
further desk command from the same token renews it for 2 minutes, and it is
released on expiry, or at once when the node refuses the command that created
it. When another holder has the desktop, the command fails naming the holder
and when the lease expires. "Same caller" means the same token: agents that
share one token share its auto lease, so give each agent its own token.
Passing the id of an auto lease with `--lease` renews it too.

**Explicit lease**, for a long session or to keep others out:

```
protectorate lease acquire --node desk:office --ttl 30m   # prints lease-xxxxxxxx
protectorate desk office --lease lease-xxxxxxxx click 400 300
protectorate lease release lease-xxxxxxxx
```

An explicit lease is not renewed by use and blocks the automatic lease. The
hub checks the lease before relaying, and the node agent does not accept desk
calls from anywhere else. Holding the id is the proof: leases do not identify
a caller beyond that, so do not share the id with agents that should not
drive. The env var `PROTECTORATE_DESK_LEASE` supplies `--lease`. A desktop
lease cannot be used as a job target.

## Availability while away

A workstation someone uses by day can lend its desktop to agents at night. A
per-node policy, set from the hub with the owner token, decides when
`desk:<node>` can be had:

```
protectorate desk policy office-pc --window 19:00-07:00 --idle 15m --lock-after
protectorate desk policy office-pc              # show the policy and the state
protectorate desk policy office-pc --off        # always available again
```

`--tz ZONE` (an IANA name such as `Europe/Berlin`) fixes the zone of the
window; the default is the node's own local time. A window whose start is
after its end spans midnight. Without a policy nothing changes: the desktop is
always available, as before.

**Rule.** The node is available when the time is inside the window **and** it
has seen no physical input for at least the idle period. The node agent
installs low-level keyboard and mouse hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`
on a locked OS thread with its own message loop) and keeps the time of the last
event that does not carry the injected flag, so input from `desk click|type|key`
(which is `SendInput`) and from other programs is not mistaken for the user.
After the agent starts, the user is assumed to have been there at that moment,
so the first idle period is measured from the start. If the hooks cannot be
installed the node says so on stderr and keeps that assumption. Screen
states count too: a locked screen, or an agent that is not in the logged-in
user's session, is unavailable.

**What the hub does.** It stores the policy (`desk-policies.yaml` in the hub
home) and sends it to the node, which also keeps a copy (`desk-policy.json`
in its home) for restarts. `desk:<node>`, explicit or automatic, is refused
with the node's reason while it is unavailable. For nodes with a policy the hub
asks the node every 3 seconds.

**When the user comes back.** The node reports unavailable at once (reason
`user active`). The hub ends every lease on that desktop within about 3 seconds
and sends the holder a notice message saying the user returned. Agents are
refused until the idle period has passed again inside the window. Independently
of the hub, the node refuses every input command (`click`, `move`, `type`,
`key`, `run`) while unavailable, checks again between pieces of a long `type`,
and the refusal also ends the caller's lease. So no input is injected after the
user is back, whether or not the hub has caught up; `screenshot` and `windows`
are not gated.

**`--lock-after`.** The node calls `LockWorkStation` when the window ends, and
when an agent's lease ends or expires inside the window and no command from
another lease follows within a minute. It never locks while the user is active
(physical input within the idle period) and never locks an already locked
screen. Consequence: once locked, the desktop is unavailable to agents until
the user unlocks it, so a lock at night ends that night's agent work.

**The screen must be unlocked.** Agents cannot drive a locked desktop, and
nothing here unlocks it. For the overnight use the user leaves the screen
unlocked when leaving: no screen saver with "on resume, display logon screen",
sleep set to never, no lock on idle (see "Locked screens and unattended
machines"). The user accepts the unlocked workstation because the office is
physically secure; without `--lock-after` it stays unlocked after the window
too.

**Status.** `protectorate node ls` has a DESK column and `node <name>` a `desk`
line: `available` (no policy), `away` (inside the window and idle), `in use by
<holder>`, `user active`, `outside window`, `unavailable: not an interactive
session`, `unavailable: screen locked`. `desk policy <node>` adds the detail
and the time since the last physical input. `protectorate node doctor`, run on
the node, prints the policy evaluation (it cannot measure input, which only the
running agent tracks, and assumes the user is idle). The same fields
(`NodeStatus.desk`, `desk_holder`, `DeskState`, `DeskPolicy`) are in the proto
for the app.

Limits: a node must run an agent that has these calls (`node update`); an older
one shows `unknown` and, under a policy, is not granted. Input arriving through
remote desktop or accessibility tools that does not carry the injected flag
counts as the user. The hub's own node takes no policy.

## Trust model

- Hub to node: the same as jobs. The agent accepts these calls only from the
  hub (tailnet WhoIs `tag:hub`, or the shared node token).
- Who may ask the hub: scopes. File calls are `node_put_file` and
  `node_get_file`; desktop calls are `desk_*` (or one of `desk_screenshot`,
  `desk_list_windows`, `desk_click`, `desk_move`, `desk_type`, `desk_key`,
  `desk_run`). None is part of `read`. `desk_run` starts any program in the
  user's session, `desk_type` and `desk_key` can do anything the logged-in
  user can do on screen; treat these grants like shell access to the desktop,
  and screenshots like any screen share (they can show other windows).
- Audit: every put (first and last chunk, plus any failed chunk), every get,
  and every desktop call is logged with the node name as subject. Paths, typed
  text and screenshots are never written to the log.
