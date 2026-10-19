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
at logon as that user:

```
protectorate service windows-task --trigger logon --write .
schtasks /Create /TN protectorate-node /XML protectorate-node.xml
```

(See docs/install.md.) An agent started by hand from an SSH session does not
qualify, even though the same user owns it. A locked screen shows the lock
screen in screenshots, and input does not reach windows of higher-integrity
programs (run as administrator) unless the agent is elevated too.

### The desktop lease

The desktop is a lease resource named `desk:<node>`, separate from the node
lease, so a build job on the machine is not blocked by someone driving its
screen and the reverse.

```
protectorate lease acquire --node desk:office --ttl 30m   # prints lease-xxxxxxxx
protectorate desk office --lease lease-xxxxxxxx click 400 300
protectorate lease release lease-xxxxxxxx
```

`click`, `move`, `type`, `key` and `run` need the id of a live lease on exactly
that desktop; `screenshot` and `windows` do not, so anyone can watch. The hub
checks the lease before relaying, and the node agent does not accept desk
calls from anywhere else. Holding the id is the proof: leases do not identify
a caller beyond that, so do not share the id with agents that should not
drive. The env var `PROTECTORATE_DESK_LEASE` supplies `--lease`. A desktop
lease cannot be used as a job target.

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
