# Installing Protectorate

A hub is one Linux machine (amd64 or arm64). Nodes are Linux, Windows or macOS machines that dial
the hub's fleet listener, so the hub's fleet port must be reachable from every node. Examples
below use `hub.example.lan` for the hub's name, `192.0.2.10` for the hub's LAN address and
`192.0.2.20` for the reverse proxy; substitute your own.

**State of this guide.** What the real-binary e2e script (`scripts/zig-e2e.sh`, run by `make e2e`)
covers is marked; everything else was written from the code and has not been run against a
published release.

| Step | Covered how |
|---|---|
| Device pairing, Envoy, sleeves, history | e2e (`scripts/zig-e2e.sh`; `scripts/app-e2e.sh` drives the web app but still follows the old hub, see REVISIT) |
| Node `join`, rejection of an unapproved or wrong-key node, `add` and `approve`, jobs, artifacts, reconnect after a hub restart | e2e |
| Proxy listener (`--proxy-listen`, `--trusted-proxy`): a listed peer is served, another is refused | e2e |
| `hub probation`, `node probation` and its rollback of a release started twice, `node update` to the version already running | e2e |
| `hub update`, `node update` to a new release | not covered |
| `install.sh`, `install-node.sh`, `install-node.ps1`, the systemd units | not covered; `--dry-run` checks the release and prints the plan |
| Windows service (`install-service`, `supervise`) | not covered; see [Windows node updates](windows-node-update.md) |
| Hub behind a real TLS reverse proxy | not covered |
| `make release-publish` | never run |

## Release assets

Releases are built locally, not in CI (SPEC 6). `make release-assets` writes `dist/<version>/`:
`protectorate-{linux-amd64,linux-arm64,macos-arm64,windows-amd64.exe,windows-arm64.exe}` (the hub,
with the web app embedded), the same five for `protectorate-node-*`, the client builds `protectorate-android.apk` and
`protectorate-desktop-windows-{amd64,arm64}.zip`, the three installers, `VERSION` and
`SHA256SUMS`. All hub and node binaries are cross-compiled here with Zig. The x64 desktop app is built
on a Windows machine over ssh (`PT_RELEASE_WIN_HOST`), the APK on a Linux amd64 machine with the Android SDK
over ssh and signed here (`PT_RELEASE_APK_HOST`), and the arm64 desktop app as a hub job on an
arm64 Windows node (`PT_RELEASE_ARM64_NODE`, which must be online); each is skipped while its
setting is unset. The cross-builds need `zig` on PATH or `ZIG`. The hub serves its
release's client builds to the owner's devices, which offer to update themselves (SPEC 6a).
`make release-publish` uploads them with `gh release create`; it is a dry run unless `PUBLISH=1`.

### Versioning

The git tag is the version, and the only place it is written: `vX.Y.Z` for the stable channel,
`vX.Y.Z-PRE` (such as `v0.7.0-dev.1`) for the dev channel. `make version` prints what a build
here carries (`git describe`: the tag itself on a tagged commit, otherwise `vX.Y.Z-N-gHASH`,
with `-dirty` for uncommitted changes). The Makefile passes it to every build: Zig as
`-Dversion` and `-Dgit-commit`, Flutter as `--build-name`, `--build-number` and
`--dart-define=PROTECTORATE_VERSION`. The defaults in `build.zig` (`0.0.0-dev`) and
`app/pubspec.yaml` (`0.0.0+0`) are placeholders; build through the Makefile or `scripts/` so
the tag is stamped.

To release:

```
git tag v0.7.0-dev.1          # on a clean, committed HEAD
make release-assets           # refuses anything but a clean semver tag on HEAD
git push origin v0.7.0-dev.1
make release-publish          # dry run; PUBLISH=1 make release-publish uploads
```

`make release-snapshot` builds the same assets from any commit, for trying the build; never
publish one. Machine-specific release settings (`PT_RELEASE_*` and `ZIG`, listed at the top of
`scripts/release.sh`) go in the environment or the gitignored `.env.e2e`.
The default release repo is `hotschmoe/protectorate`; installers take `--repo` (`-Repo`), the hub
`release_repo` in `config.json`.

Every installer checks each download against the release's `SHA256SUMS` before using it, and
checks that the staged binary reports that release. There is no signing key, so this catches
corruption, not someone who can replace release assets. Every installer takes `--from DIR` to use
assets you built yourself instead of GitHub.

Fetch an installer at a commit, never a tag, and compare its sha256 with the one in the release
notes before running it:

```
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install.sh
sha256sum install.sh
```

## The hub

### Prerequisites

- Linux with a systemd user manager, `git`, and either `bwrap` (bubblewrap) or a kernel with
  Landlock for the sleeve sandbox. `protectorate doctor` reports which are present. `gh`, logged
  in, is shown by `doctor`. The installer itself needs `curl`, `sha256sum`, `awk`, `sed`, `grep`,
  `sort`, `mktemp` and `systemctl`.
- The harness CLIs you want, installed by their vendors' own methods (Claude Code, Codex, Grok
  Build, Antigravity, Pi). Log each one in as the hub's user with `protectorate harness login
  <claude|codex|grok|pi|agy>`, which runs the vendor's own login command (see
  [operate.md](operate.md#harness-logins)). Sleeves use that user's logins.
- For `pi_local`: Pi configured, as the hub's user, with a provider for your OpenAI-compatible
  endpoint.

### Install

Run as the user who owns the hub, not root:

```
bash install.sh --dry-run          # check the release and print the plan; writes nothing
bash install.sh                    # newest stable release
bash install.sh --channel dev      # newest release including pre-releases
bash install.sh --version vX.Y.Z   # one release
bash install.sh --from dist/vX.Y.Z # local assets from make release-assets
```

Other flags: `--home DIR` (default `~/.local/share/protectorate`), `--unit-name NAME`,
`--no-start`, `--repo OWNER/NAME`, and the listener flags below (`--listen`, `--proxy-listen`,
`--trusted-proxy` once per proxy, `--fleet-listen`), which go onto the unit's `serve` line.

What it does:

1. stages the hub in `<home>/releases/<version>/` and points `releases/current` at it (the
   previous one becomes `releases/previous`);
2. writes `~/.config/systemd/user/protectorate.service`, which runs
   `releases/previous/protectorate --home <home> hub probation` before each start (it switches back
   a release that was started twice without being marked healthy) and then
   `releases/current/protectorate --home <home> serve` with the listener flags you gave;
3. runs `loginctl enable-linger $USER`, so the hub starts at boot with nobody logged in (if that
   fails, run `sudo loginctl enable-linger $USER` yourself);
4. enables and starts the unit;
5. links `~/.local/bin/protectorate` to `releases/current/protectorate`, unless something else is
   already there (it then prints the `ln -sfn` that would replace it). With a `--home` other than
   the default, set `PROTECTORATE_HOME` in your shell profile so the CLI finds the hub.

It refuses to overwrite a unit it did not write. Logs: `journalctl --user -u protectorate.service`.

The hub creates its tokens on its first start and prints none of them. On the hub machine,
`protectorate token` prints the **owner token** and the **envoy-only token** from
`<home>/auth.json`, and the CLI reads the owner token from the same file, so it needs neither on
the hub. Other machines pass `--token`, `--token-file` or `PROTECTORATE_TOKEN` and `--url` or
`PROTECTORATE_URL`.

### Listeners and config

`serve` has three listeners:

- `--listen ADDR:PORT`: the HTTP API and the web app. Default `127.0.0.1:7470`. Keep it on
  loopback.
- `--proxy-listen ADDR:PORT --trusted-proxy IP` (repeat for each proxy): a second HTTP listener for
  a reverse proxy. Only the listed addresses may connect; any other peer is closed. Without a token
  a request reaches only the public method (pairing); the app sends its device token.
- `--fleet-listen ADDR:PORT`: where nodes dial in over Noise IK. Default `0.0.0.0:7475`. Open this
  port to your nodes' networks only. It is not an HTTP port and does not go through the proxy.

`--no-sandbox` runs sleeves without bwrap or Landlock; `--dev-token` sets a fixed owner token, for
tests. The web app is embedded in the binary; there is no `--web-dir`.

Everything else is in `<home>/config.json`, a file you create (the hub reads it at start and never
writes it; restart the hub after editing). Keys that exist, with their defaults:

```
{
  "hub_name": "protectorate",
  "listen_addr": "127.0.0.1:7470",
  "proxy_listen": null,
  "trusted_proxies": [],
  "fleet_listen": "0.0.0.0:7475",
  "advertise_addrs": [],
  "no_sandbox": false,
  "update_repo": "hotschmoe/protectorate",
  "update_channel": "stable",
  "d9_spawn_limit": 3,
  "d9_approval_timeout_secs": 300,
  "sleeve_devices": []
}
```

The listener flags override the matching keys. `advertise_addrs` is the list of `host:port` the
join token tells nodes to dial. It defaults to `127.0.0.1` and the fleet port, which only works for
a node on the hub machine, so set it to the hub's LAN address, WireGuard address or DNS name, for
example `["192.0.2.10:7475", "hub.example.lan:7475"]`. A node races all of them, so LAN and VPN
routes can be listed together. The CLI finds the hub through `listen_addr` in the same file, or
`--url`.

The drop-in way to add listener flags after install is
`systemctl --user edit protectorate.service` with an `ExecStart=` reset and a full new line; it
survives a reinstall.

### Reverse proxy

The proxy terminates TLS for `hub.example.lan` and forwards everything to the hub's
`--proxy-listen` address (`192.0.2.10:7470` here), with `--trusted-proxy 192.0.2.20`. The web
app is served from the same origin, so no CDN is needed. Its events WebSocket (`/api/events`)
needs the HTTP upgrade and a long read timeout (Traefik's default entrypoint timeout is 60 s); the
web build sends its token as `?access_token=`, which a proxy access log may record (REVISIT).
Nothing else needs a route: nodes dial the fleet port directly, so do not route it through the
proxy.

### Pairing a device

On the hub:

```
protectorate device add phone      # or: protectorate pair phone
```

It prints a pairing code (and a QR of it) that works once, for 10 minutes. Enter it in the app's
SETTINGS, or open `https://hub.example.lan/?pair=<code>` on the device. The device gets its own
token, stored in `auth.json`. `protectorate device list` shows paired devices and `protectorate
device revoke <id>` removes one; a revoked device's open event stream stays open until it
disconnects (REVISIT). The web app keeps its token in the browser; the desktop and Android builds
forget it on restart for now.

## A node

One kind of node: it takes jobs while nobody is at it and refuses them while someone is (or while
presence is unknown). The node dials the hub's fleet listener, authenticates with its own key
against the hub key it pinned at join, and the hub sends it jobs and updates over that connection.
A node the owner has not approved is dropped before it can send anything.

Every install ends with the same three steps: the owner gets a join token on the hub, the node
joins with it, and the owner approves the node's key.

### Join and approve

1. On the hub, get a join token:

   ```
   protectorate node join-token
   ```

   It prints the enrollment as JSON (the `ptjoin1:` token is the `control_key` field; `join_command`
   is the command to run on the node) and a QR code of the token. The token holds the hub's public
   key, its fingerprint and `advertise_addrs`; it is not secret, but anyone who has it can ask to
   join. `--json` leaves out the QR code.
2. On the node, `protectorate-node join ptjoin1:... --name build-arm` (the installers below do
   this for you). It pins the hub, creates the node's key (`node_key.hex`, mode 0600) and prints
   the node's `node_key` and fingerprint. The node's name defaults to its hostname.
3. On the hub, register that key and approve it:

   ```
   protectorate node add build-arm:<node_key>
   protectorate node approve <auth_id> [--name build-arm] [--fingerprint sha256:<hex>]
   ```

   `node add` prints the `auth_id` (a bare 64-hex key without a name also works).
   `--fingerprint` makes `approve` refuse if it is not the key's fingerprint, so pass the one the
   node printed.
4. On the node, `protectorate-node authorize` tests the handshake and the hello and succeeds once the
   node is approved; `protectorate-node serve` (or the service) then stays connected. `protectorate
   node list` on the hub shows the node and its presence.

An unapproved key, or a node holding another hub's key, cannot complete the handshake.

### Linux

As root, for a system service (it can then read `/dev/input` for presence and use cgroups for
jobs):

```
sudo bash install-node.sh ptjoin1:... --name build-arm --dry-run
sudo bash install-node.sh ptjoin1:... --name build-arm
```

Flags: `--version TAG | --channel stable|dev`, `--from DIR`, `--home DIR` (alias `--state-dir`; default
`/var/lib/protectorate-node`, or `~/.local/share/protectorate-node` with `--user`), `--user` (a
systemd user service, no root), `--service-name NAME`, `--wait-secs N` (how long to poll
`authorize`, default 60), `--repo OWNER/NAME`. It stages the binary in
`<home>/releases/<version>/`, points `releases/current` at it, runs `protectorate-node join`, prints
the `protectorate node add NAME:KEY` line to run on the hub, waits for approval, and runs
`protectorate-node install-service`, which writes and starts `protectorate-node.service` (it runs
`protectorate-node supervise --home <home>`). It refuses a node that is already joined and
installed; update those from the hub.

### Windows

From an elevated PowerShell:

```
powershell -ExecutionPolicy Bypass -File .\install-node.ps1 -JoinToken ptjoin1:... -Name build-win -DryRun
powershell -ExecutionPolicy Bypass -File .\install-node.ps1 -JoinToken ptjoin1:... -Name build-win
```

Parameters: `-Version`, `-Channel`, `-From`, `-Repo`, `-StateDir` (default
`%ProgramData%\protectorate-node`), `-ServiceName` (default `ProtectorateNode`), `-WaitSecs`. It
stages the binary in `<state>\releases\<version>\`, runs `join`, waits for approval, then
`protectorate-node install-service`, which registers and starts the service (a supervisor that
runs the release `releases\current.txt` names, so the node takes `node update` from the hub; see
[Windows node updates](windows-node-update.md)).

### Other machines

`protectorate-node-macos-arm64` and the manual route work the same way: `join`, add and approve on
the hub, then `protectorate-node install-service [--user]` (a launchd agent on macOS) or
`protectorate-node serve` in a terminal.

### Uninstall

`protectorate-node uninstall [--user] [--service-name NAME] [--home DIR]` stops and removes the
service (systemd unit, launchd agent or Windows service) and deletes `install-manifest.json`. The
node home, its key and its staged releases stay; remove the node from the hub with `protectorate
node rm <node>`.
