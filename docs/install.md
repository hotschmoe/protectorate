# Install

## One-line hub install

On Linux (x86_64, aarch64), as the user that will run the hub; no sudo, nothing is deleted, re-running upgrades:

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/v0.2.0/install-hub.sh | bash
```

Pass flags with `bash -s -- --listen 0.0.0.0:7470 --public-url http://NAME:7470`; also `--version TAG`, `--home DIR`, `--dry-run` (downloads and verifies into a temporary directory, installs nothing). The script downloads the release's `SHA256SUMS`, the binary for this machine, `protectorate-web-<version>.tar.gz` and the other platforms' builds, and checks each against the sums. It installs `~/.local/bin/protectorate`, the web client into `~/.local/share/protectorate/web-<version>` (with `web` a symlink to it), the other builds into `~/.local/share/protectorate/bin`, writes and enables the `protectorate-hub` systemd user unit (an existing unit is kept as it is), and runs `hub tailnet install`. Missing bwrap, tmux, git or a systemd user manager are listed with the package to install; it installs no system packages. Then it prints the next steps: `hub tailnet init`, `protectorate login <harness>` and, when linger is off, the `enable-linger` hint.

`hub serve` uses `~/.local/share/protectorate/web` for `--web` and `~/.local/share/protectorate/bin` for `--binary-dir` when they exist and the flag is not given, so ADD NODE and node updates work on a fresh install.

## Build from source

Needs Go. Build and copy the result:

```
make cross        # bin/protectorate-{linux,windows}-{amd64,arm64}[.exe], bin/protectorate-android-arm64
make build        # bin/protectorate for this machine
```

`protectorate service ...` only prints or writes service definitions. It never enables or starts anything, and `--write DIR` / `--install` refuse to overwrite existing files. Arguments after `--` go to `hub serve` / `node serve`.

## Hub (Linux)

```
install -m755 bin/protectorate-linux-amd64 ~/.local/bin/protectorate
protectorate hub tailnet install
protectorate hub tailnet init --server-url http://LAN-IP:7480        # or an https URL
protectorate service systemd hub --binary ~/.local/bin/protectorate --install -- --web ~/protectorate/app/build/web
systemctl --user daemon-reload && systemctl --user enable --now protectorate-hub
sudo loginctl enable-linger $USER                                    # keep running when logged out
```

The unit uses `KillMode=control-group` so a killed hub cannot orphan headscale. Prefer the user unit when the hub runs sleeves (they use `systemd-run --user`). For a headless box without a user session, `--user-name NAME` renders a system unit for `/etc/systemd/system`.

Log in to each harness once on the hub. This runs the harness's own login in your terminal and stores the credential in `~/.protectorate/auth/<harness>`, which every sleeve shares:

```
protectorate login claude
protectorate login codex                  # headless box: protectorate login codex -- --device-auth
protectorate login grok                   # or: protectorate login grok -- --device-auth
protectorate login pi                     # type /login in pi, then exit it
protectorate login --status               # logged in / missing, from file presence only
```

Relay fallback needs no domain or certificate. The hub runs its own DERP relay (TCP 7483, TLS with a self-signed certificate it generates in `tailnet/`) and STUN (UDP 3478). Clients trust the relay by the certificate hash carried in the DERP map, so nothing is installed on nodes or phones. Open TCP 7483 and UDP 3478 on the hub for anything outside the LAN; the `--server-url` host must be an address or name those clients can reach. Upgrading from a config without `derp_listen` needs no change (it defaults to `0.0.0.0:7483`); restart the hub. The stock Tailscale app needs a version that understands the `sha256-raw:` pin (current releases do).

## Add a node from the web app

A browser cannot install software, so the hub serves a one-line installer and the app hands you the line. The hub needs:

- a tailnet (`hub tailnet init`; the `--server-url` must be reachable from the new machine),
- the builds for every platform: run `make cross` on the hub box and start the hub with `--binary-dir bin` (its own platform is served from its own executable),
- a listen address the new machine can reach, for example `hub serve --listen 0.0.0.0:7470` on the LAN or a reverse proxy in front of it. When machines reach it under another name than you open the app with, pass `--public-url http://NAME:7470`.

Open the web app on the new machine (or anywhere), with the hub entered under the owner token (`cat ~/.protectorate/token` on the hub; a paired device token cannot add or remove nodes). On the NODES tab press ADD NODE, type a name (a lowercase DNS label: a-z, 0-9 and `-`), check the machine type (preselected from the browser's platform: WINDOWS, LINUX or TERMUX) and press CREATE INVITE. The dialog shows the line to paste, with COPY (and a QR code on a phone), and a SETUP panel that follows the node live: INVITED (key issued), JOINED (the machine enrolled in the tailnet), FIRST METRICS (the hub reached the agent), READY. When something needs fixing the panel turns to ATTENTION and says what, for example an agent of another version than the hub's, or a Windows agent that is not in the interactive desktop session. The lines hold a single-use key that expires after an hour; treat them like a password until used.

```
# Windows: a normal PowerShell (Windows PowerShell 5.1 or pwsh 7), not "Run as administrator"
$env:PROTECTORATE_ENROLL_KEY='KEY'; $env:PROTECTORATE_NODE_NAME='office-arm'; irm 'http://HUB:7470/install.ps1' | iex

# Linux (x86_64, aarch64) and Termux (aarch64), as the user the agent runs as
curl -fsSL 'http://HUB:7470/install.sh' | PROTECTORATE_ENROLL_KEY='KEY' sh -s -- --name 'box'
```

`irm | iex` cannot pass arguments, so on Windows the key and the name both come from the environment; the script clears the key from the session when it is done. The key is never part of a URL. The installers are generated by the hub with its own URLs baked in and fetch the build from the same hub:

| | `install.ps1` | `install.sh` |
|---|---|---|
| detects | the machine's architecture from the registry's machine environment, then `RuntimeInformation.OSArchitecture`, then `PROCESSOR_ARCHITEW6432` / `PROCESSOR_ARCHITECTURE` (an x64 PowerShell emulated on ARM64 still gets the ARM64 build) | `uname -s`, `-m` and `-o`, and `TERMUX_VERSION` for Termux |
| downloads | `GET /dl/protectorate-windows-<arch>.exe` into `%LOCALAPPDATA%\protectorate\download` | `GET /dl/protectorate-<linux|android>-<arch>` into `~/.cache/protectorate` |
| verifies | `GET /dl/<file>.sha256` against `Get-FileHash` | the same against `sha256sum` or `shasum -a 256` |
| runs | `node install --control <control URL> --name <name>` | the same |

The Windows script refuses an elevated PowerShell: the agent becomes a logon task of the signed-in user and runs in that user's desktop session, which is what desk commands need, and it says so as it installs. `install.sh --print-target` prints the build it would fetch and exits. `/install.sh`, `/install.ps1` and `/dl/...` are the only unauthenticated endpoints besides the web app's files; none of them carries a secret. Everything that issues keys or removes nodes is owner-only.

From the CLI the same invite is `protectorate node invite <name> [--tier family] [--pool P] [--expires 2h] [--url http://HUB:7470]`, which prints the lines for each platform and the manual `node install` line.

A node that never joined shows as PENDING. Its detail page offers REISSUE KEY (a new key; the old one stops working) and CANCEL INVITE (the key is expired and the entry removed).

## Remove a node

Node detail, REMOVE NODE (or `protectorate node rm <name>`). Type the name to confirm, then choose:

- running jobs: LET THEM FINISH (`--jobs wait`, up to `--wait-timeout`, default 1h) or CANCEL THEM (`--jobs cancel`). From the CLI without `--jobs`, a node with running jobs is refused. Either way the node is drained first: no new job or lease goes to it, and it shows REMOVING;
- ALSO UNINSTALL (`--uninstall`): a reachable agent unregisters its logon task, systemd user unit or termux service and stops. It deletes nothing and reports the paths left for you to delete. An offline node is skipped; run `protectorate node uninstall` on it later.

Then the hub expires a pending invite key, revokes the tailnet enrollment (the machine is expired and deleted in headscale), releases leases on the node and its desktop, and removes the inventory entry. A revoked machine that starts again from its stored state is never registered again and the hub cannot reach it; it needs a new invite. Outside a tailnet the hub and its agents share one node token, so there is nothing per node to revoke; removing the entry stops the hub from calling the agent. Removal is owner-only and audited.

## Install a node by hand

On the hub, `protectorate node invite <name>` prints the control URL and a single-use key (valid one hour by default). On the machine, run the release binary once:

```
# Windows (PowerShell, as the user who stays logged in; not elevated)
$env:PROTECTORATE_ENROLL_KEY = "KEY"
.\protectorate-windows-amd64.exe node install --control http://HUB:7480 --name office

# Linux
PROTECTORATE_ENROLL_KEY=KEY ./protectorate-linux-amd64 node install --control http://HUB:7480 --name box

# Termux
PROTECTORATE_ENROLL_KEY=KEY ./protectorate-android-arm64 node install --control http://HUB:7480 --name phone
```

The key may also come from `--key-file FILE` or `--key KEY` (visible in the process list, so prefer the other two). `node install` does, in order:

| | Windows | Linux | Termux |
|---|---|---|---|
| binary | `%LOCALAPPDATA%\Programs\protectorate\protectorate.exe` | `~/.local/bin/protectorate` | `$PREFIX/bin/protectorate` |
| enroll | `node join` into `$PROTECTORATE_HOME/tsnet`, skipped when already enrolled (no key needed on a re-run) | same | same |
| startup | logon task `protectorate-node` for the current user, registered with `schtasks /Create /XML ... /F`, then started | `~/.config/systemd/user/protectorate-node.service`, `systemctl --user enable`, then `restart` | termux-services run script and Termux:Boot script, `sv-enable` |

It is idempotent: run it again with a newer binary to update the binary and the task or unit (the running agent is stopped for the copy and started again). The Windows task needs no elevation. It starts `conhost.exe --headless protectorate.exe node serve --home DIR`, with no `cmd.exe` wrapper. `--headless` (Windows 10 1809 and later) gives the agent a console host without any window, so nothing is ever shown, whichever console host is the system default. Hiding the window from inside the process does not work when Windows Terminal is the default host (the process only has a pseudo window, and `ShowWindow` does not reach the real one), which is why the agent no longer tries; a GUI-subsystem build is not used because it would break the CLI in a terminal. Re-run `node install` after updating to move an older task (which used `--hide-console`) to the new form. `--elevated` registers the task with the highest available privileges; that needs a terminal opened with Run as administrator, and is only for driving elevated windows (Windows blocks input from a normal process to an elevated window, UIPI). Arguments after `--` are passed to `node serve`, for example `-- --watch D:\build`.

`node install --dry-run` prints every action, including the file contents it would write, and changes nothing. `node install --home DIR` uses another state directory. On Linux the install prints (and does not run) `sudo loginctl enable-linger $USER`, needed to keep the agent running while logged out.

`protectorate node uninstall` stops the agent and unregisters the task, unit or service. It never deletes anything: it prints the binary, definition and state paths you may delete yourself, and the `node rm` command for the hub. The hub can ask an agent to do the same (REMOVE NODE with ALSO UNINSTALL); the agent then only unregisters and exits on its own. `protectorate node doctor` checks the machine: interactive session, screen lock, startup registration, enrollment and (with `--hub URL`) hub reachability; see docs/desk.md.

## Nodes (manual steps)

On the hub: `protectorate node invite <name>`. It prints the control URL and key; run `node join` with them on the node, then run `node serve` as a service.

Linux:

```
protectorate node join --control URL --key-file FILE --name NAME   # or env PROTECTORATE_ENROLL_KEY, or --key KEY
protectorate service systemd node --binary ~/.local/bin/protectorate --install
systemctl --user daemon-reload && systemctl --user enable --now protectorate-node
```

Windows (x86-64: `protectorate-windows-amd64.exe`, ARM: `protectorate-windows-arm64.exe`). `node serve` runs as a real Windows service, with no wrapper. Use one `PROTECTORATE_HOME` for both `node join` and the service:

```
protectorate.exe node join --control URL --key KEY --name NAME
protectorate.exe service install-windows --home C:\ProgramData\protectorate > install.cmd
REM review install.cmd, then run it from an elevated prompt
```

Do not rely on an SSH session for the long-running agent: its lifetime depends on how it was started. A plain foreground run dies with the session, `Start-Process -WindowStyle Hidden` from an SSH PowerShell also died when the session ended, while a process created through WMI (`Invoke-CimMethod Win32_Process -MethodName Create`) or one left over from an interrupted session kept running unsupervised. Use a service or scheduled task. Without service rights, `node install` (above) registers the logon task; `service windows-task --trigger logon --write .` plus `schtasks /Create /TN protectorate-node /XML protectorate-node.xml` is the manual equivalent. The logon task is also the only way `protectorate desk` can work: screen and input need the agent in the logged-in user's session, not session 0 (see docs/desk.md).

### Android (Termux)

`protectorate-android-arm64` is a static binary; copy it into Termux and run it as the Termux user (not root). Android runs no sleeves, so a phone is a build/test node: hub jobs, `job test-cross`, metrics.

```
# in Termux
cp /sdcard/Download/protectorate-android-arm64 $PREFIX/bin/protectorate && chmod +x $PREFIX/bin/protectorate
pkg install git                     # needed for git-source jobs; test-cross works without it
# on the hub
protectorate node invite phone      # prints the control URL and key; the control URL must be reachable from the phone
# in Termux
PROTECTORATE_ENROLL_KEY=KEY protectorate node join --control URL --name phone
pkg install termux-services         # then restart Termux
protectorate service termux --install && sv-enable protectorate-node
# on the hub
protectorate node show phone         # invite already recorded it in the inventory
```

Without `--name`, the node name comes from the device model (`ro.product.model`) because Android reports the hostname `localhost`. Install the Termux:Boot app and open it once so the boot script runs after a reboot; the generated script takes a wake lock. `tmux` (`pkg install tmux`) is only needed to run the session tests there.

Verified in `termux/termux-docker:aarch64` (a normal Linux kernel with the Termux rootfs and the Android `/system/bin/sh`), as the container's `system` user. The container runs on the hub host with `--network host`, because the hub's own tailnet connection to the docker bridge address (172.17.0.1) left via the LAN interface and never connected. The hub used `--server-url http://127.0.0.1:PORT` and loopback listen/DERP/STUN addresses:

```
docker run -d --name test-prot-android-1 --network host \
  -v $PWD/bin/protectorate-android-arm64:/data/local/tmp/protectorate:ro \
  -v $STATE:/data/data/com.termux/files/home/state \
  termux/termux-docker:aarch64 /system/bin/sh -c 'sleep 100000'
docker exec -u system -e HOME=/data/data/com.termux/files/home -e PREFIX=/data/data/com.termux/files/usr \
  -e PATH=/data/data/com.termux/files/usr/bin:/system/bin -e PROTECTORATE_HOME=.../home/state \
  test-prot-android-1 sh -c 'cd $HOME && ./protectorate node serve'      # pkg install refuses root
protectorate job test-cross --goos android --goarch arm64 ./...         # all packages pass; tmux-less and sleeve tests skip
```

Not verified on a real phone: Android 11+ restricts netlink for apps (tsnet's interface and route discovery may degrade), `/proc/stat` and `/proc/loadavg` may be unreadable (the node reports a CPU warning and drops the load average rather than failing), and battery optimisation or Doze can kill the process or freeze the network even with the wake lock. A node killed that way is shown unreachable until `node serve` runs again. Once it runs again the hub sees it fresh within a poll or two (the node pings the hub on start); no hub restart is needed. Disable battery optimisation for Termux and keep the Termux notification.

## Update nodes

```
make cross                                   # on the hub box: bin/protectorate-<os>-<arch>[.exe]
protectorate hub serve --binary-dir bin      # where the hub finds those builds (default: the directory of its own executable)
protectorate node ls                         # VERSION column; (!) marks a node that differs from the hub
protectorate node update <node>              # or: node update --all [--force]
```

Nothing updates by itself. `node update` asks the hub (`NodeService.UpdateNode`, owner-only, also behind the app's "Update node" button), which sends one binary per node over the file transfer (zstd where it shrinks, size and SHA-256 checked): its own executable for nodes of its own OS and architecture, the matching `protectorate-<os>-<arch>` file from its `--binary-dir` for the rest. The hub box therefore needs the `make cross` output (or the CI artifacts) in that directory. The node then:

1. checks size and SHA-256 again, copies the file next to its own binary and runs `<new> version --json` (it must report the node's OS and architecture);
2. swaps it in and keeps exactly one previous binary: `protectorate.exe.old` / `protectorate.old` next to it, overwritten by the next update. Linux and Termux copy the old one aside and rename the new one over it atomically; Windows cannot overwrite a running exe, so it renames the running exe to `.old` and puts the new one in place;
3. answers, shuts down and restarts: `systemctl --user restart` for the unit `node install` wrote, `schtasks /Run` for the Windows logon task (after it exits; the task's restart-on-failure is the fallback), `sv restart` for the Termux service, exit status 75 under a supervisor that restarts on failure (a systemd system unit or Windows service), and otherwise by starting itself again from the same path with the same arguments.

The hub streams the progress (sending, verifying, swapping, restarting) and waits up to 3 minutes until the node reports the new version, or reports that it came back on the old one. The new binary is on trial, recorded in `protectorate.exe.update` next to the binary (a marker file that is rewritten, never deleted): the first hub poll that reaches it confirms the update. If the new binary is started twice without the hub reaching it, or four minutes pass without contact, it puts the `.old` binary back (on Windows the failed one stays as `.failed`) and restarts. A node that has no supervisor and whose new binary dies before running any code stays down; that is why `version --json` is run first. The hub's own node is updated by replacing the hub binary and restarting the hub. Updating is owner-only (the `NodeService.ApplyUpdate` and `UpdateNode` RPCs are not offered to scoped tokens or the MCP bridge).

## Phone client

```
protectorate client invite --name phone      # follow the printed Tailscale app steps
protectorate client register <key>           # for the app's registration key
protectorate pair --app                      # QR for the native app; without --app, the web link
```

## Troubleshooting the tailnet

`protectorate tailnet status` shows the hub's own view: its tailnet name and IPs, then one row per peer with tags, online flag, how the hub reaches it, last WireGuard handshake, last seen and rx/tx bytes. PATH reads `direct <ip:port>` for a UDP path, `DERP protectorate` when traffic goes through the hub's relay, and `-` when no path exists yet. `--json` prints the same data for scripts; the `tailnet_get_status` MCP tool and `TailnetService/GetStatus` carry it too (read scope). It fails with a precondition error when the hub runs without a tailnet.

- A peer that stays on DERP while both sides are on one LAN usually has UDP blocked between them; the relay still works.
- A peer with `online no` and an old handshake is down or revoked; `node ls` also lists nodes that were invited but never joined.
- A node killed without a clean shutdown makes the relay see a dropped connection; those expected disconnects are not logged, other relay errors are.
- A node that reaches the hub only through a VPN or a non-default route (a WireGuard interface with a host route to the hub, no default route through it) joins and works: the tailnet client never binds its sockets to the default-route interface. Upstream tailscale does so when it cannot set `SO_MARK` (any non-root process), which sent the control dial out the wrong interface and hung `node join` until its timeout with the hub seeing nothing. `internal/tailnet` turns that bypass off, as `tailscaled` does in netstack-only mode. A kernel without policy routing (`ip rule` fails with "Operation not supported") is fine.
- Linux desk support for a graphical session owned by another user (for example Hyprland under `kids` with the node installed as `alarm`) would need access to that user's Wayland socket. Not built.

## Versions

The version is the git tag; nothing else defines it. `make version` prints it (`git describe --tags --always --dirty`), and every build takes it from there: `make build` and `make cross` stamp the Go binaries, `make app-build-web`, `make apk` and `make app-build-linux` pass `--build-name` (the x.y.z of the nearest tag, `0.0.0` without one), `--build-number` (commits on HEAD, so it only grows; `make version-app` prints `name+number`) and `--dart-define=PROTECTORATE_VERSION=<version>` to Flutter. `app/pubspec.yaml` holds the placeholder `0.0.0+0`, which every build overrides. `make check-version` fails if the placeholder changed or a make target, script or CI workflow builds without those flags; `make check` runs it with the rest of the checks. Tag with `git tag vX.Y.Z` and build from a full clone (CI uses `fetch-depth: 0`).

The app shows its own version in the hub list and the hub's in the overview, and lights VERSION SKEW on the overview when it differs from the hub's or any node's does.
