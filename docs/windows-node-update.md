# Windows node updates

How `protectorate node update` reaches a Windows node, the one-time reinstall of older Windows
installs, and the manual check for a real Windows machine. [operate.md](operate.md) "Updates and
rollback" has the overview.

## What runs

```
service protectorate-node (LocalSystem, auto start, SCM restarts it 5 s, 5 s, then every 60 s)
  "%ProgramFiles%\protectorate-node\protectorate-node.exe" supervise --service
      --state-dir "%ProgramData%\protectorate-node" --listen-tcp 127.0.0.1:7476
    before each start: releases\previous\protectorate-node.exe probation --state-dir ...
    then, in a kill-on-close job: releases\current\protectorate-node.exe serve --state-dir ...
      --listen-tcp 127.0.0.1:7476 (and member, jobs, the console idle probe in jobs of their own)
```

- `%ProgramFiles%\protectorate-node\protectorate-node.exe` is the supervisor: the release that was
  installed. `node update` never touches it, so it changes only with a reinstall. Its log lines
  and the node's go to `%ProgramData%\protectorate-node\node.log`.
- The node is the release `releases\current` names. On `Update` it stages
  `releases\<tag>\protectorate-node.exe` (checked against `SHA256SUMS`, then run once with
  `--version`), writes `releases\probation.json`, moves `previous` and then `current`, replies,
  and exits 75. The supervisor, which outlives it, runs the previous release's `probation` and
  starts the new one 1 s later.
- Nothing is overwritten. A pointer moves by making `.current-new`, renaming `current` to
  `.current-old`, renaming `.current-new` to `current` and removing `.current-old` (the same for
  `previous`). A crash in between is finished by the next process that opens the store, the
  supervisor before its next start included.
- Rollback is the same as on Linux: the release on trial switches back when the hub has not made
  contact within the window, and `probation`, run from the previous release before each start,
  switches back a release that has started three times without contact.
- Permissions: everything runs as LocalSystem, which may create directory symlinks. The state dir
  (releases included) is limited to SYSTEM and Administrators by `install-service`, so only they
  can change the binaries the service starts. Jobs also run as LocalSystem (REVISIT "No runner
  account"), so a job can change them too, as root can on Linux.

## Reinstall an older Windows node once

A node installed by v0.7.0-dev.5 or older runs `serve --service` straight from `%ProgramFiles%`
and answers `unsupported` to `node update`. Check with `sc.exe qc protectorate-node`; if
`BINARY_PATH_NAME` contains `supervise --service`, it is already the new layout.

From an elevated PowerShell on the node, with the new release's `protectorate-node-windows-<arch>.exe`
and `SHA256SUMS` downloaded into a new, empty directory:

```
$s = "$env:ProgramData\protectorate-node"
Get-FileHash -Algorithm SHA256 .\protectorate-node-windows-amd64.exe   # must match SHA256SUMS
.\protectorate-node-windows-amd64.exe --version                        # protectorate-node <tag>, member: embedded
& "$s\releases\current\protectorate-node.exe" uninstall --state-dir $s  # the old release removes its service
.\protectorate-node-windows-amd64.exe install-service --state-dir $s
sc.exe qc protectorate-node                                             # supervise --service
```

`uninstall` keeps the state dir (identity, member keys, jobs, releases). `install-service` copies
the new binary into `releases\<tag>\`, points `current` at it and `previous` at the old release,
copies it under `%ProgramFiles%` as the supervisor and starts the service. The node keeps its
name and id; `protectorate node ls` shows it on the new release.

## Manual check on a real Windows node

Not yet run on a real machine. Use a spare Windows box joined to a test hub, not someone's
desktop: the check stops and restarts the service and runs jobs. It needs two releases built with
the supervisor, called `A` (installed) and `B` (newer) here, published on a channel the hub
reads, and the hub already on `B`. Run the PowerShell steps elevated on the node and the
`protectorate` steps on the hub. Record each result.

1. Install `A` with `install-node.ps1` (or the reinstall above). Check:
   - `sc.exe qc protectorate-node`: `BINARY_PATH_NAME` is the `%ProgramFiles%` copy with
     `supervise --service`; `SERVICE_START_NAME` is `LocalSystem`.
   - `cmd /c dir /a "$env:ProgramData\protectorate-node\releases"`: `current [A]` and
     `previous [A]` are `<SYMLINKD>`.
   - `icacls "$env:ProgramData\protectorate-node\releases\A"`: only SYSTEM and Administrators.
   - `protectorate node ls`: the node is online on `A`, with presence (not a failed reading).
   - `protectorate job run --node ID -- whoami`: `nt authority\system`.
2. Update: `protectorate node update NODE --version B`. It reports `A -> B` and waits for
   the confirmation. Check:
   - `node.log`: `switched from A to B`, `exiting 75`, the supervisor's `the node exited 75`,
     `release on probation`, `start <n>, release B`, then `the hub made contact; release B stays`.
   - `releases`: `current [B]`, `previous [A]`, no `.current-*` or `.previous-*` left;
     `releases\probation.json` has `contacted_ms`.
   - `protectorate node ls`: online on `B`, presence still read (the idle probe runs in the
     console session from the supervised node).
   - `tasklist /fi "imagename eq protectorate-node.exe"`: two processes, the supervisor and the
     node; `tasklist /fi "imagename eq member*"`: one member.
3. Same release again: `protectorate node update NODE --version B` reports it current or reused,
   with no restart in `node.log`.
4. Rollback when the hub never makes contact: `protectorate node update NODE --version A
   --probation 120 --no-wait`, then stop the hub at once and keep it down for 3 minutes. Check in
   `node.log`: `the hub made no contact in time; switched back from A to B`, then a start of `B`.
   Start the hub; `node ls` shows `B`, and `probation.json` has `rolled_back_ms`.
5. Rollback of a release that cannot start: make one that exits at once, put it on trial by hand
   and restart the service.

   ```
   $r = "$env:ProgramData\protectorate-node\releases"
   sc.exe stop protectorate-node
   New-Item -ItemType Directory "$r\v9.9.9-broken" | Out-Null
   Copy-Item "$env:SystemRoot\System32\whoami.exe" "$r\v9.9.9-broken\protectorate-node.exe"
   $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
   $prev = (Get-Item "$r\previous").Target
   Set-Content -Encoding ascii "$r\probation.json" ('{"version":"v9.9.9-broken","previous":"' + $prev + '","switched_ms":' + $now + ',"deadline_ms":' + ($now + 600000) + '}')
   cmd /c rmdir "$r\current"
   cmd /c mklink /D "$r\current" v9.9.9-broken
   sc.exe start protectorate-node
   ```

   Within about 30 s `node.log` shows three starts of `v9.9.9-broken`, each exiting nonzero, then
   `release v9.9.9-broken rolled back to <previous>` and a start of `<previous>`; `current`
   names `<previous>` and `node ls` shows the node online on it. Update back to `B` with
   `node update`, then remove `$r\v9.9.9-broken` by hand.
6. Stop and start: `sc.exe stop protectorate-node` leaves no `protectorate-node.exe` or member
   process (`tasklist`); `sc.exe start protectorate-node` brings the node back on `current`.
7. Supervisor crash: `taskkill /f /pid <supervisor pid>`, the `protectorate-node.exe` whose
   command line has `supervise` (`Get-CimInstance Win32_Process -Filter
   "name='protectorate-node.exe'" | Select-Object ProcessId,CommandLine`). The node and member die
   with it, and the SCM starts the service again after 5 s (60 s once it has failed twice that
   day).
8. Reboot the node. The service starts on its own and the node comes back on `current`.
9. Busy: start a long job (`protectorate job run --node ID -- ping -n 120 127.0.0.1`), then
   `node update` answers `busy` and nothing in `releases` changes.

If a step fails, keep `node.log`, the `releases` listing and `probation.json`, and the node can be
put back with the reinstall above.
