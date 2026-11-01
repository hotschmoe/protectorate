# Installing Protectorate

A hub is one Linux machine (amd64 or arm64). Nodes are Linux or Windows machines the hub reaches
over its own tailnet. Examples below use `hub.example.lan` for the hub's name, `192.0.2.10` for the
hub's LAN address and `192.0.2.20` for the reverse proxy; substitute your own.

**State of this guide.** No GitHub release has been published from `next` yet, so nothing here
has run against a real release. What has run is marked: each step says whether it was tested by
an e2e test, run on a real machine, or only dry-run.

| Step | Tested how |
|---|---|
| `install.sh`, real path | against a scratch home with stand-in `systemctl` and `loginctl` only |
| `install.sh --dry-run`, `install-node.sh --dry-run`, `install-node.ps1 -DryRun` | dry runs on real machines |
| hub as a user service with linger | run on a real hub, with a hand-written unit rather than `install.sh`'s |
| hub API and web app behind a TLS reverse proxy | run on a real hub |
| headscale behind a TLS reverse proxy | a stand-in Go reverse proxy in `scripts/net-e2e.sh` only |
| device pairing | e2e (`crates/protectorate/tests/api.rs`, `scripts/app-e2e.sh` for the web app) |
| node `join`, approval, system service (Linux) | real Linux nodes run as system services, enrolled LAN-direct |
| node service (Windows) | install, run and uninstall checked by hand, not through a hub |
| `make release-publish` | never run |

## Release assets

Releases are built locally, not in CI (SPEC 6). `make release-assets` writes `dist/<version>/`:
`protectorate-linux-{amd64,arm64}` (the hub, with the web app, headscale and member embedded),
`protectorate-node-linux-{amd64,arm64}`, `protectorate-node-windows-{amd64,arm64}.exe` (each with
member embedded), the three installers, `VERSION` and `SHA256SUMS`. The Windows assets are built
natively on a Windows machine over ssh (`PT_RELEASE_WIN_HOST`; unset, they are skipped), and the
glibc 2.31 amd64 build needs `zig` on PATH or `ZIG`. `make release-publish` uploads them with
`gh release create`; it is a dry run unless `PUBLISH=1`. It has never been run.

### Versioning

The git tag is the version, and the only place it is written: `vX.Y.Z` for the stable channel,
`vX.Y.Z-PRE` (such as `v0.7.0-dev.1`) for the dev channel. `make version` prints what a build
here carries (`git describe`: the tag itself on a tagged commit, otherwise `vX.Y.Z-N-gHASH`,
with `-dirty` for uncommitted changes). The Makefile passes it to every build: cargo as
`PROTECTORATE_BUILD_VERSION`, Flutter as `--build-name`, `--build-number` and
`--dart-define=PROTECTORATE_VERSION`. The versions in `Cargo.toml` (`0.0.0`) and
`app/pubspec.yaml` (`0.0.0+0`) are placeholders, and `make check-version` (part of `make check`)
fails if they change or if any build path in the Makefile or `scripts/` skips the tag.

To release:

```
git tag v0.7.0-dev.1          # on a clean, committed HEAD
make release-assets           # refuses anything but a clean semver tag on HEAD
git push origin v0.7.0-dev.1
make release-publish          # dry run; PUBLISH=1 make release-publish uploads
```

`make release-snapshot` builds the same assets from any commit, for trying the build; never
publish one. Machine-specific release settings (`PT_RELEASE_WIN_HOST`, `PT_RELEASE_WIN_DIR`,
`ZIG`, `PT_RELEASE_REPO`) go in the environment or the gitignored `.env.e2e` (see the Makefile).
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

- Linux with a systemd user manager, `bwrap`, `systemd-run`, `git`, `curl`, `gzip` and
  `sha256sum`. `gh`, logged in, is checked by `doctor`.
- The harness CLIs you want, installed by their vendors' own methods (Claude Code, Codex, Grok
  Build, Pi). After the install, log each one in for the hub with `protectorate harness login
  <claude|codex|grok|pi>` (see [operate.md](operate.md#harness-logins)); your own logins are
  never used by sleeves.
- For `pi_local`: Pi configured with a provider for your OpenAI-compatible endpoint, named in the
  hub's `config.json` as `pi_local_provider`, with `pi_local_model`. Both default to empty, and
  until they are set doctor reports `pi_local` as not configured and nothing spawns or resleeves
  onto it (the default `resleeve_to` is `["pi_local"]`, so a sleeve that must resleeve parks).

### Install

Run as the user who owns the hub, not root:

```
bash install.sh --dry-run          # check the release and print the plan; writes nothing
bash install.sh                    # newest stable release
bash install.sh --channel dev      # newest release including pre-releases
bash install.sh --version vX.Y.Z   # one release
bash install.sh --from dist/vX.Y.Z # local assets from make release-assets
```

Other flags: `--home DIR` (default `~/.local/share/protectorate`), `--no-start`, `--repo OWNER/NAME`.

What it does:

1. stages the hub in `<home>/releases/<version>/` and points `releases/current` at it (the
   previous one becomes `releases/previous`);
2. writes `~/.config/systemd/user/protectorate.service`, which runs
   `releases/previous/protectorate hub probation` before each start and then
   `releases/current/protectorate --home <home> serve`;
3. runs `loginctl enable-linger $USER`, so the hub starts at boot with nobody logged in (if that
   fails, run `sudo loginctl enable-linger $USER` yourself);
4. enables and starts the unit.

It refuses to overwrite a unit it did not write. Logs: `journalctl --user -u protectorate.service`.

On its first start the hub prints the **owner token** and the **envoy-only token** once, to its
log. The CLI on the hub needs neither: on the unix socket, callers running as the hub's user are
the owner. `protectorate envoy token` prints the envoy-only token again.

### Listeners

By default `serve` listens on the unix socket `<home>/run/api.sock` and on `127.0.0.1:7470`, and
the fleet is off. The unit `install.sh` writes passes no other flags; add them with a drop-in
(`systemctl --user edit protectorate.service`), which survives a reinstall:

```
[Service]
ExecStart=
ExecStart=%h/.local/share/protectorate/releases/current/protectorate --home %h/.local/share/protectorate serve --proxy-listen 192.0.2.10:7470 --trusted-proxy 192.0.2.20
```

The drop-in itself is untested; the real hub ran with these flags in a hand-written unit.

- `--proxy-listen ADDR --trusted-proxy IP`: the listener for the reverse proxy. Connections from
  any other address are refused. Without a token a request reaches only the public methods
  (pairing); the app sends its device's `pd1.` token.
- `--listen ADDR|off`: the loopback TCP listener.
- `--web-dir DIR`: serve the web app from disk instead of the embedded build.
- `--log-level error|warn|info|debug`.

The hub never listens on the tailnet.

### Fleet settings

The fleet (embedded headscale plus the hub's member) starts when `fleet.listen` is set, in
`<home>/config.json` or with `--fleet-listen`. The hub writes `config.json` with every default on
its first start; restart it after editing. An excerpt:

```
"fleet": {
  "listen": "192.0.2.10:7480",
  "udp_port": 7483,
  "server_url": "",
  "derp_ipv4": "",
  "trusted_proxies": [],
  ...
}
```

- **LAN-direct** (what the real nodes use today): leave `server_url` empty. Nodes then use
  `http://<listen>` as the control URL and DERP runs over plain HTTP.
- **Behind the TLS proxy** (untested with a real proxy): set `server_url` to
  `https://hub.example.lan`, `derp_ipv4` to the proxy's IPv4, and `trusted_proxies` to
  `["192.0.2.20"]` (a bare address or a prefix such as `192.0.2.0/24`), then have a host firewall admit only the proxy to the `listen` port.

`--fleet-dir DIR` points at an existing tailnet directory to adopt it; nodes keep their pinned
control key. `--fleet-udp-port N` and `--fleet-server-url URL` override the config keys.

### Reverse proxy

The proxy terminates TLS for `hub.example.lan` and has two routes:

- headscale's paths to `192.0.2.10:7480`: `/ts2021`, `/key`, `/machine/ping-response`,
  `/bootstrap-dns`, `/health`, `/derp` and `/derp/...`. The `/ts2021` and `/derp` requests are
  HTTP upgrades that stay open for a long time: forward them over HTTP/1.1 and make sure the
  entrypoint's read timeout does not cut them (Traefik's default is 60 s). This route is proven
  only against the stand-in proxy.
- everything else to the hub's proxy listener, `192.0.2.10:7470`.

The web app is served by the hub from the same origin, so no CDN is needed. Its events WebSocket
carries the token as `?access_token=`, which a proxy access log may record (REVISIT).

### Pairing a device

On the hub:

```
protectorate device add phone
```

It prints a pairing code that works once, for 10 minutes. Enter it in the app's SETTINGS, or open
`https://hub.example.lan/?pair=<code>` on the device. The device gets a `pd1.` token, stored on the
hub only as a hash. `protectorate device list` shows paired devices and `protectorate device
revoke <id>` removes one. The web app keeps its token in the browser; the desktop and Android
builds forget it on restart for now.

## A node

One kind of node: it takes jobs while nobody is at it and refuses them while someone is (or while
presence is unknown). The node never calls the hub; it registers with headscale and answers
connections the hub opens on tailnet port 7475.

`SERVER_URL` below is the hub's headscale URL: `http://192.0.2.10:7480` LAN-direct, or
`https://hub.example.lan` behind the proxy.

### Linux

As root (the node is a system service, so it can read `/dev/input` and make job scopes):

```
sudo bash install-node.sh SERVER_URL --name build-arm --dry-run
sudo bash install-node.sh SERVER_URL --name build-arm
```

Flags: `--version TAG | --channel stable|dev`, `--from DIR`, `--state-dir DIR` (default
`/var/lib/protectorate-node`), `--repo OWNER/NAME`. It stages the binary in
`<state>/releases/<version>/`, points `releases/current` at it, runs `protectorate-node join`, then
`protectorate-node install-service`, which writes and starts `protectorate-node.service`. It
refuses a node that is already joined and installed; update those from the hub. Only dry runs of
the script have happened; the real nodes were set up without it.

### Windows

From an elevated PowerShell:

```
powershell -ExecutionPolicy Bypass -File .\install-node.ps1 -ServerUrl SERVER_URL -Name build-win -DryRun
powershell -ExecutionPolicy Bypass -File .\install-node.ps1 -ServerUrl SERVER_URL -Name build-win
```

Parameters: `-Version`, `-Channel`, `-From`, `-Repo`, `-StateDir` (default
`%ProgramData%\protectorate-node`, the binary's own). It stages the binary, links
`releases\current`, runs `join`, then `protectorate-node install-service`, which registers and
starts the LocalSystem service `protectorate-node`. Only dry runs of the script have happened. A
Windows node cannot take updates from the hub yet; updating one is a reinstall.

### Join and approve

`join` shows a QR code, the registration as text, and the exact approve command, then waits up to
15 minutes (headscale keeps a registration that long):

```
protectorate node approve <auth_id> --name build-arm --fingerprint sha256:<hex>
```

Run that on the hub, or paste the registration text into `protectorate node add '<text>'`, which
records it and checks its control key against the hub's. `protectorate node add` with no argument
prints the join command, the hub's control-key fingerprint and any pending registrations. The app
cannot scan the QR code yet.

The node pins the control key it saw at join and refuses to start against a different one. Once
approved, `protectorate node ls` shows it with its presence.

### Uninstall

`protectorate-node uninstall [--state-dir DIR]` stops the service and removes exactly what
`install-service` listed in `<state>/install-manifest.json`. The state directory and its staged
releases stay. It has run on Windows; on Linux it has never run.
