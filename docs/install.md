# Install

## One-line hub install

On Linux (x86_64, aarch64), as the user that will run the hub; no sudo, nothing is deleted, re-running upgrades:

```
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/v0.4.2/install-hub.sh | bash
```

Pass flags with `bash -s -- --listen 0.0.0.0:7470 --public-url http://NAME:7470`; also `--channel stable|dev`, `--version TAG`, `--release-repo OWNER/REPO`, `--release-dir DIR`, `--home DIR`, `--dry-run` (downloads and verifies into a temporary directory, installs nothing). Stable is the default and installs GitHub's latest non-prerelease; dev installs the rolling `dev` prerelease. An explicit `--version` selects that release tag. The script downloads `SHA256SUMS`, `VERSION`, all five platform binaries and `protectorate-web-<version>.tar.gz`, and verifies every asset against the sums before installing anything.

Releases live in `${XDG_DATA_HOME:-~/.local/share}/protectorate/releases/<version>/`, with binaries, `web/`, the checksummed web archive, `VERSION`, `SHA256SUMS` and `release.json`. `current` is an atomically replaced symlink to the active version; `previous` records the preceding install. The executable is installed atomically as `~/.local/bin/protectorate`, preserving `protectorate.previous` on upgrade. The script writes and enables the `protectorate-hub` systemd user unit, and runs `hub tailnet install`. Missing bwrap, tmux, git or a systemd user manager are listed with the package to install; it installs no system packages. Then it prints the next steps: `hub tailnet init`, `protectorate login <harness>` and, when linger is off, the `enable-linger` hint.

`hub serve` defaults `--binary-dir` to `releases/current` and `--web` to `releases/current/web`, so ADD NODE and node updates use verified releases on a fresh install. `--release-repo` defaults to `hotschmoe/protectorate`; `--update-channel` defaults to `stable`. Explicit `--binary-dir` and `--web` remain development overrides. `--release-dir` changes the release root for both installation and the hub.

When upgrading an existing installation, the installer preserves its systemd unit and prints a migration notice for explicit `--web` or `--binary-dir` flags. Remove those flags from `~/.config/systemd/user/protectorate-hub.service` so the release pointer controls installs and the web app; add `--release-dir` if using a custom root, then run `systemctl --user daemon-reload && systemctl --user restart protectorate-hub`. Old `web-*`, `web`, `bin` and `downloads` paths are left for you to reclaim after reviewing them.

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

Upgrading a v0.3 hub: since v0.4 the hub keeps `nodes`, `tokens`, `sleeves`, `leases` and `desk-policies` state in `.json` files (durations as integer nanoseconds) and ignores the old `.yaml` files, so an unconverted hub starts with no nodes, tokens or sleeves. Stop the hub and convert each file before starting v0.4 (the one known v0.3 hub was converted with a one-off tool that keeps the originals as `*.yaml.pre-json`).

Upgrading a v0.4.0 hub to v0.4.1 needs no state conversion: restart the hub on the new binary and run `protectorate node update --all`. Existing sleeves keep their old cache layout until respawned; reclaim a sleeve's old private cache copies with `protectorate sleeve kill NAME --drop-caches` before respawning, and review leftovers with `protectorate sleeve prune` (it removes nothing without `--yes`). Copy-on-write caches need bubblewrap 0.9 or newer first on the hub service's PATH (see [sleeves](sleeves.md)); with Debian's 0.8 the hub keeps copying caches. Codex sleeves now default to reasoning effort `high` (`hub serve --sleeve-effort`).

Log in to each harness once on the hub. This runs the harness's own login in your terminal and stores the credential in `~/.protectorate/auth/<harness>`, which every sleeve shares:

```
protectorate login claude
protectorate login codex                  # headless box: protectorate login codex -- --device-auth
protectorate login grok                   # or: protectorate login grok -- --device-auth
protectorate login pi                     # type /login in pi, then exit it
protectorate login --status               # logged in / missing, from file presence only
```

Relay fallback needs no domain or certificate. The hub runs its own DERP relay (TCP 7483, TLS with a self-signed certificate it generates in `tailnet/`) and STUN (UDP 3478). Clients trust the relay by the certificate hash carried in the DERP map, so nothing is installed on nodes or phones. Open TCP 7483 and UDP 3478 on the hub for anything outside the LAN; the `--server-url` host must be an address or name those clients can reach. Upgrading from a config without `derp_listen` needs no change (it defaults to `0.0.0.0:7483`); restart the hub. The stock Tailscale app needs a version that understands the `sha256-raw:` pin (current releases do).

## Add a node

A browser cannot install software, so the hub serves a one-line installer and the app hands you the line. The hub needs:

- a tailnet (`hub tailnet init`; the `--server-url` must be reachable from the new machine),
- the current verified GitHub release (the hub installer prepares it; subsequent hub updates fetch all platform builds),
- a listen address the new machine can reach, for example `hub serve --listen 0.0.0.0:7470` on the LAN or a reverse proxy in front of it. When machines reach it under another name than you open the app with, pass `--public-url http://NAME:7470`.

New app connections use device approval and cannot issue node enrollment keys. On the hub, run `protectorate --token-file ~/.protectorate/token node invite officepc --url http://HUB:7470` (use your custom home's token path when applicable). Choose a lowercase DNS label for the name, then paste the printed Windows, Linux or Termux command on the new machine. The single-use enrollment key expires after an hour; treat it like a password until used. After enrollment, open the approved app's NODES tab to follow the node and inspect any attention message.

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

From the CLI the same invite is `protectorate node invite <name> [--tier care] [--pool P] [--expires 2h] [--url http://HUB:7470]`, which prints the lines for each platform and the manual `node install` line.

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

## Update the hub and nodes

```
protectorate hub update                     # fetch and verify the latest release on the configured channel
protectorate hub update --channel dev       # force the rolling development build
protectorate hub update --version v0.4.2     # select an explicit release tag
protectorate node ls                         # VERSION column; (!) marks a node that differs from the hub
protectorate node update <node>              # or: node update --all [--force]
```

Build with `make build` and `make cross` so installs and updates retain the
Tailscale feature selection. Runtime profiles and the node's default 128 MiB
soft Go memory limit are documented in [footprint.md](footprint.md).

The hub checks for releases roughly hourly and on demand, using conditional requests. Nothing updates by itself. The overview reports the current version, available version, channel and release notes; Update hub streams download, verification and restart progress. The hub verifies all fleet assets against the release's `SHA256SUMS` before switching `current`, replaces its executable atomically and restarts through the supervisor or with its original arguments. A failed health check rolls back the executable and release pointer; successful updates retain two releases and prune older managed releases. SHA-256 verification fails closed; release signature verification remains a security-audit item.

`node update` asks the hub (`NodeService.UpdateNode`, also behind the app's node update actions), which sends the matching `protectorate-<os>-<arch>` from its current verified release over the file transfer (zstd where it shrinks, size and SHA-256 checked). After updating the hub, run `node update --all` or use Update N nodes in the app to bring the fleet onto that release. Release updates require the owner or a Full device token. Explicit `--binary-dir` remains available for local development builds. The node then:

1. checks size and SHA-256 again, copies the file next to its own binary and runs `<new> version --json` (it must report the node's OS and architecture);
2. swaps it in and keeps exactly one previous binary: `protectorate.exe.old` / `protectorate.old` next to it, overwritten by the next update. Linux and Termux copy the old one aside and rename the new one over it atomically; Windows cannot overwrite a running exe, so it renames the running exe to `.old` and puts the new one in place;
3. answers, shuts down and restarts: `systemctl --user restart` for the unit `node install` wrote, `schtasks /Run` for the Windows logon task (after it exits; the task's restart-on-failure is the fallback), `sv restart` for the Termux service, exit status 75 under a supervisor that restarts on failure (a systemd system unit or Windows service), and otherwise by starting itself again from the same path with the same arguments.

The hub streams the progress (sending, verifying, swapping, restarting) and waits up to 3 minutes until the node reports the new version, or reports that it came back on the old one. The new binary is on trial, recorded in `protectorate.exe.update` next to the binary (a marker file that is rewritten, never deleted): the first hub poll that reaches it confirms the update. If the new binary is started twice without the hub reaching it, or four minutes pass without contact, it puts the `.old` binary back (on Windows the failed one stays as `.failed`) and restarts. A node that has no supervisor and whose new binary dies before running any code stays down; that is why `version --json` is run first. The hub's own node is updated through `hub update`.

## Back up and restore the hub

```
protectorate hub backup [--to DIR]                        # default DIR: $PROTECTORATE_HOME/backups
protectorate hub serve --backup-dir DIR --backup-every 6h # the same, on a schedule (6h is the default interval)
protectorate hub restore FILE --home DIR                  # into an empty or missing DIR only
```

A backup is one `protectorate-hub-<UTC time>.tar.zst` plus `<name>.sha256` (`sha256sum -c` works on it). The hub keeps running. Its supervisor briefly stops and reaps headscale while copying its database and WAL, then restarts it before compression begins. File locks coordinate scheduled and standalone backups, and release on cancellation or process exit. Other files are copied on their own: the hub replaces them atomically or only appends to them (a torn last line is cut off on load). The archive ends with `MANIFEST.json`: the size and SHA-256 of every file, the hub version and the home it came from. Restore verifies those hashes and checks the SQLite header and page alignment; headscale recovers the WAL when it opens the restored database. Format 2 archives contain JSON state; restore older archives with the binary that created them, then migrate their state before starting the new hub.

In the backup: `token`, `tokens.json`, `nodes.json`, `sleeves.json`, `leases.json`, `desk-policies.json`, `desk-use.json`, `directory.json`, `mailbox.jsonl`, `notes.json`, `audit/`, `ca/`, `jobs/<id>/job.json` and `job.log` with `jobs/request-ids.jsonl`, each sleeve's chat log and its request ids, and `tailnet/` (database and WAL, `config.json`, the DERP certificate and key that nodes pin, the noise key, the hub's tsnet state). Not in it: `bin/` (run `hub tailnet install`), `auth/` and `login/` (harness logins; run `protectorate login` again), `node-files/`, job artifacts, and sleeve workspaces and homes. It holds secrets (the owner token, the tailnet keys): keep the backup directory as private as the home.

With `--backup-dir`, `hub serve` makes a backup at start unless the newest one in the directory is younger than `--backup-every`, then one every interval. A failed backup is logged and tried again. **Old backups are never deleted, not by the hub and not by `hub backup`; pruning is manual** (for example `find DIR -name 'protectorate-hub-*.tar.zst*' -mtime +30`, review, then remove).

Restore on the same or a new machine:

```
protectorate hub restore protectorate-hub-20261020T093000Z.tar.zst --home ~/.protectorate
protectorate hub tailnet install          # the headscale binary is not in the backup
protectorate hub serve ...                # the same flags as before, PROTECTORATE_HOME unchanged
```

`restore` refuses a home that has any file in it. Before it writes anything it checks the sidecar digest (when present) and the manifest (every file's size and SHA-256, no entry the manifest does not list, no path outside the home); afterwards it runs SQLite's integrity check on the restored headscale database. The enrolled nodes are in the restored database and the DERP certificate that nodes pin is unchanged, so with the hub back at the same address they reconnect without enrolling again. A hub restored at a different address is not covered: nodes still hold the old control URL.

Targets: any directory, including a share mounted on the hub box (NFS, SMB). The NAS (`/mnt/storage/...` on b70-nas-server) is the intended off-box copy; pushing a backup to a node with the `node put` transfer is a later option, so for now mount it on the hub or copy the finished archive there with `scp`. The backup is a plain file, so any copy tool works.

## Phone client

Connect the phone to the same LAN as the hub. Device approval is LAN-only
until the security audit; keep the hub off the public Internet.

Release and dev APKs are signed with one release key, so a newer APK installs
over the previous one. CI reads it from the repository secrets
`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and
`ANDROID_KEY_PASSWORD`; local builds use it when `PROTECTORATE_ANDROID_SIGNING`
names a properties file with `storeFile`, `storePassword`, `keyAlias` and
`keyPassword`, and otherwise sign with the machine's debug key (such an APK
cannot update one signed elsewhere; uninstall first). Keep an off-box backup of
the keystore: without it, future APKs cannot update installed apps.

1. Install and open the native app, or open the hub's web client in the phone
   browser. On the hub, `protectorate pair --app` displays a native-app link
   and QR; `protectorate pair` displays the web link. These contain the hub
   URL, never a credential.
2. Scan the QR or choose ADD HUB and enter its URL, for example
   `http://192.168.10.44:7470`. URLs without a scheme default to HTTPS; for a
   plain HTTP LAN hub enter `http://` explicitly. If a bare LAN address does
   not connect, use the suggested `:7470` port.
3. Connect and keep the large approval code visible. On an already trusted
   app, check that the banner's device name, platform, IP and code match,
   then choose APPROVE. For the first phone, run
   `protectorate devices approve CODE` on the hub. DENY refuses the request.
4. The phone connects automatically after approval. A code expires after ten
   minutes; RETRY creates a fresh request. If the hub cannot be reached,
   check the URL, port and LAN connection, then retry.

Use DEVICES on any trusted app, or `protectorate devices ls`, to review
last use. Rename a device for a clearer label or revoke it when lost or no
longer needed. Revocation also closes its active streams. Existing paired
devices continue to work; old links containing a token must be replaced by a
new URL-only link and a fresh approval.

Tailnet client enrollment (`protectorate client invite --name phone`, then
`protectorate client register KEY`) remains a separate network setup step;
it does not replace device approval or authorize exposing the hub publicly.

## Desktop client (Windows)

CI builds the native Windows app on every push (artifact `desktop-windows` of
the `ci` run) and releases attach it as
`protectorate-desktop-windows-x64-<version>.zip`. Unzip anywhere and run
`protectorate.exe`; it needs no installer.

1. Choose ADD HUB and enter the reachable LAN URL, for example
   `http://192.168.10.44:7470`, or open the URL-only link from
   `protectorate pair --app`. Enter `http://` explicitly for a plain HTTP hub;
   the default scheme is HTTPS.
2. Choose CONNECT and compare the displayed code with the approval banner on
   a trusted phone or desktop. Approve the matching request there, or run
   `protectorate devices approve CODE` on the hub using its owner token.
3. The desktop saves its own Full device token and opens the hub. No owner
   token needs to be pasted. Review, rename or revoke it through DEVICES.

For a CLI on another machine, run
`protectorate login --hub URL --name officepc --platform windows-cli`, approve
the displayed code on a trusted device, then use normal commands against the
same hub. Harness logins on the hub (`protectorate login codex`, for example)
remain separate from authorizing a client device.

## Troubleshooting the tailnet

`protectorate tailnet status` shows the hub's own view: its tailnet name and IPs, then one row per peer with tags, online flag, how the hub reaches it, last WireGuard handshake, last seen and rx/tx bytes. PATH reads `direct <ip:port>` for a UDP path, `DERP protectorate` when traffic goes through the hub's relay, and `-` when no path exists yet. `--json` prints the same data for scripts; the `tailnet_get_status` MCP tool and `TailnetService/GetStatus` carry it too (read scope). It fails with a precondition error when the hub runs without a tailnet.

- A peer that stays on DERP while both sides are on one LAN usually has UDP blocked between them; the relay still works.
- A peer with `online no` and an old handshake is down or revoked; `node ls` also lists nodes that were invited but never joined.
- A node killed without a clean shutdown makes the relay see a dropped connection; those expected disconnects are not logged, other relay errors are.
- A node that reaches the hub only through a VPN or a non-default route (a WireGuard interface with a host route to the hub, no default route through it) joins and works: the tailnet client never binds its sockets to the default-route interface. Upstream tailscale does so when it cannot set `SO_MARK` (any non-root process), which sent the control dial out the wrong interface and hung `node join` until its timeout with the hub seeing nothing. `internal/tailnet` turns that bypass off, as `tailscaled` does in netstack-only mode. A kernel without policy routing (`ip rule` fails with "Operation not supported") is fine.
- Linux desk support for a graphical session owned by another user (for example Hyprland under `kids` with the node installed as `alarm`) would need access to that user's Wayland socket. Not built.

## Versions

The version is derived from release tags; nothing else defines it. `make version` prints it (`git describe --tags --match 'v[0-9]*' --always --dirty`), excluding the moving `dev` tag, and every build takes it from there: `make build` and `make cross` stamp the Go binaries, `make app-build-web`, `make apk`, `make app-build-linux` and `make app-build-windows` pass `--build-name` (the x.y.z of the nearest release tag, `0.0.0` without one), `--build-number` (commits on HEAD, so it only grows; `make version-app` prints `name+number`) and `--dart-define=PROTECTORATE_VERSION=<version>` to Flutter. `app/pubspec.yaml` holds the placeholder `0.0.0+0`, which every build overrides. `make check-version` fails if the placeholder changed or a make target, script or CI workflow builds without those flags; `make check` runs it with the rest of the checks. Tag with `git tag vX.Y.Z` and build from a full clone (CI uses `fetch-depth: 0`).

The app shows its own version in the hub list and the hub's in the overview, and lights VERSION SKEW on the overview when it differs from the hub's or any node's does.
