# Windows node updates

How `protectorate node update` reaches a Windows node, and a manual check for a real Windows
machine. [operate.md](operate.md) "Updates and rollback" has the overview. The code is
`src/node/service.zig` (service and supervisor), `src/node/serve.zig` (the `update` request) and
`src/update/store.zig` (staging and probation).

## What runs

```
service ProtectorateNode (LocalSystem, auto start; the SCM restarts it after 5 s, up to three times
                          a day)
  "<exe>" supervise --home "<state>"
     before each start: the probation check (the same logic as `protectorate-node probation`)
     then: <state>\releases\<tag>\protectorate-node.exe serve --home "<state>"
           where <tag> is the content of <state>\releases\current.txt
```

- `install-service` runs `sc.exe create ProtectorateNode binPath= ... start= auto` with no `obj=`,
  so the service runs as LocalSystem, sets the failure actions (restart after 5 s, counter reset
  after a day) and starts it. `<exe>` is the `protectorate-node.exe` that ran `install-service`: with
  `install-node.ps1` it is the first release, `<state>\releases\<tag>\protectorate-node.exe`. The
  default state directory is `%ProgramData%\protectorate-node`. Nothing sets permissions on the
  state directory.
- `<exe>` is the supervisor. `node update` never changes it, so it changes only with a reinstall.
  The supervisor re-reads `releases\current.txt` before every start, and falls back to
  `releases\current\protectorate-node.exe` and then to itself. It starts the node again after any
  exit, waiting 0.5 s and doubling up to 10 s; a run of more than 30 s resets the wait.
- Windows has no `current` link from the update code: `releases\current.txt` and
  `releases\previous.txt` hold the tag names. (`install-node.ps1` also tries `mklink /D current`,
  which the supervisor does not need.)
- On an update request the node stages the new release in `releases\<tag>\protectorate-node.exe`
  (from GitHub, or from pieces the hub streams over the fleet link when GitHub fails; the sha256 is
  checked against the release's `SHA256SUMS`), writes `previous.txt`, `current.txt` and
  `probation.json` (`healthy: false`, `attempts: 0`), answers the hub with `restarting`, and exits
  its `serve`. The supervisor runs the probation check, which counts this start
  (`attempts: 1`), and starts the new release.
- The new release sets `healthy: true` once the hub has answered its first `Hello`: that contact is the
  only proof. A release that exits again before that point is switched back: the next probation
  check, seeing `healthy: false` and `attempts: 1`, rewrites `current.txt` to the previous tag. There
  is no timer: a release that keeps running but never reaches the hub stays on trial.
- The node has no log file. Under the service its output is not kept, so use `protectorate node
  show <node>` and `protectorate event list` on the hub, or stop the service and run
  `protectorate-node serve --home <state>` in a console.
- Jobs also run as LocalSystem, as jobs on a Linux system service run as root.

## Reinstall to replace the supervisor

`node update` never replaces the supervisor binary, and no remote command does. To give a node a
new supervisor, run the installer steps again from an elevated PowerShell on the node, with the new
release's `protectorate-node-windows-<arch>.exe` and `SHA256SUMS` downloaded into a new, empty
directory:

```
$s = "$env:ProgramData\protectorate-node"
Get-FileHash -Algorithm SHA256 .\protectorate-node-windows-amd64.exe   # must match SHA256SUMS
.\protectorate-node-windows-amd64.exe --version                        # protectorate-node <tag> (<commit>)
.\protectorate-node-windows-amd64.exe uninstall --home $s               # stops and deletes the service
```

Then copy the new exe to `$s\releases\<newtag>\protectorate-node.exe`, write `<newtag>` to
`releases\current.txt` (and the old tag to `previous.txt`), and run
`$s\releases\<newtag>\protectorate-node.exe install-service --home $s`. `uninstall` keeps the state
directory (identity, key, jobs, releases). The node keeps its name and id, and `protectorate node
list` shows it on the new release.

Without anyone at the machine, a `--maintenance` job can do the same, but the job runs under the
service and uninstalling stops the service, which kills the job. So the job only stages the files
and a script and hands the script to Task Scheduler, which runs it as SYSTEM outside the service:

1. One job downloads the new exe and `SHA256SUMS` into `%ProgramData%\protectorate-node\reinstall\`,
   checks the hash (`certutil -hashfile ... SHA256`) and runs it with `--version`.
2. A job writes `reinstall.cmd` there, which runs the current release's `uninstall --home`, copies
   the new exe into `releases\<newtag>\`, updates `current.txt` and `previous.txt`, and runs
   `install-service --home`, logging to `reinstall.log`.
3. Two jobs hand it to Task Scheduler and end at once:

   ```
   protectorate job run --node ID --maintenance -- schtasks /create /f /tn pt-reinstall /ru SYSTEM /sc once /st 23:59 /tr C:\ProgramData\protectorate-node\reinstall\reinstall.cmd
   protectorate job run --node ID --maintenance -- schtasks /run /tn pt-reinstall
   ```

4. The node drops off and comes back on the new release; `reinstall.log` says what happened. Delete
   the task afterwards (`schtasks /delete /tn pt-reinstall /f`, as a job).

Never run yet; try it on a spare machine first.

## Manual check on a real Windows node

Not yet run on a real machine for the Zig program. Use a spare Windows box joined to a test hub,
not someone's desktop: the check stops and restarts the service and runs jobs. It needs two
releases, called `A` (installed) and `B` (newer), published on a channel the hub reads, and the
hub already on `B`. Run the PowerShell steps elevated on the node and the `protectorate` steps on
the hub. Record each result.

1. Install `A` with `install-node.ps1`. Check:
   - `sc.exe qc ProtectorateNode`: `BINARY_PATH_NAME` is `"<exe>" supervise --home "<state>"`;
     `SERVICE_START_NAME` is `LocalSystem`.
   - `<state>\releases\current.txt` holds `A`.
   - `protectorate node list`: the node is online on `A`, with presence (not UNKNOWN).
   - `protectorate job run --node ID --wait -- whoami`: `nt authority\system`.
2. Update: `protectorate node update NODE --version B`. It reports `A -> B`. Check:
   - `current.txt` holds `B` and `previous.txt` holds `A`.
   - `probation.json` ends with `"healthy": true` once the hub has answered the new release.
   - `protectorate node list`: online on `B`, presence still read.
   - `tasklist /fi "imagename eq protectorate-node.exe"`: two processes, the supervisor and the
     node.
3. Same release again: `protectorate node update NODE --version B` reports it already on that
   release, and nothing restarts.
4. Rollback of a release that cannot run: make one that exits at once, put it on trial by hand and
   restart the service.

   ```
   $r = "$env:ProgramData\protectorate-node\releases"
   sc.exe stop ProtectorateNode
   New-Item -ItemType Directory "$r\v9.9.9-broken" | Out-Null
   Copy-Item "$env:SystemRoot\System32\whoami.exe" "$r\v9.9.9-broken\protectorate-node.exe"
   $prev = (Get-Content "$r\current.txt").Trim()
   Set-Content -Encoding ascii "$r\previous.txt" $prev
   Set-Content -Encoding ascii "$r\current.txt" "v9.9.9-broken"
   Set-Content -Encoding ascii "$r\probation.json" ('{"active":"v9.9.9-broken","previous":"' + $prev + '","healthy":false,"attempts":0}')
   sc.exe start ProtectorateNode
   ```

   Within a few seconds `current.txt` names `$prev` again and `node list` shows the node online on it.
   Remove `$r\v9.9.9-broken` by hand.
5. Stop and start: `sc.exe stop ProtectorateNode` leaves no `protectorate-node.exe` process
   (`tasklist`); `sc.exe start ProtectorateNode` brings the node back on `current.txt`'s release.
6. Supervisor crash: `taskkill /f /pid <supervisor pid>`, the `protectorate-node.exe` whose command
   line has `supervise` (`Get-CimInstance Win32_Process -Filter "name='protectorate-node.exe'" |
   Select-Object ProcessId,CommandLine`). The SCM starts the service again after 5 s. Check
   whether the node process dies with its supervisor or stays behind as an orphan, and record it.
7. Reboot the node. The service starts on its own and the node comes back on `current.txt`'s
   release.

If a step fails, keep the `releases` listing, `current.txt`, `previous.txt` and `probation.json`;
the node can be put back with the reinstall above.
