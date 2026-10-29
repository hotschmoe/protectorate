# Install the Rust hub

## Network model

Protectorate never builds a network and never listens on the internet. The routers own connectivity: the home and office UniFi Dream Machines join their LANs with site-to-site WireGuard, and phones are WireGuard road-warrior clients of the office router. Protectorate runs on those private addresses and authenticates every peer itself; being on the network is never proof of identity.

- Every listener binds an explicit private address: loopback, RFC 1918, 100.64.0.0/10, fc00::/7 or fe80::/10. `0.0.0.0`, `::` and public addresses are refused, with no override.
- The hub is reachable two ways only. On the hub machine, its loopback `--listen` (default `127.0.0.1:7470`) serves the CLI, sleeves and the MCP bridge; the owner token works only there. From the LAN and the VPN, a reverse proxy terminates TLS with an ordinary public certificate and forwards plain HTTP to the hub's `--proxy-listen` address, which closes every connection that does not come from a `--trusted-proxy` address. Browsers, the app and a remote CLI therefore need no private CA, and no client can send a token to the hub in the clear.
- Clients authenticate with bearer tokens: devices (the app, a remote CLI) hold the device token the owner added them with, and scripts and sleeves hold short-lived capabilities (see [auth.md](auth.md)).

Production: Traefik on stronghold (192.168.10.50) serves `https://protectorate.hotschmoe.com` with its Let's Encrypt certificate behind its lan-only middleware and forwards to the hub on ms-r1, which runs with `--proxy-listen 192.168.10.44:7470 --trusted-proxy 192.168.10.50`.

Recommended router firewall rules:

| From | To | Allow |
|------|----|-------|
| client and VPN zones (laptops, phones) | reverse proxy | TCP 443 only |
| reverse proxy | hub | TCP 7470 only |

## Hub

The Linux release contains `protectorate-rs`, `protectorate` (the CLI), `protectorate-mcpbridge`, and the sleeve runner and sandbox helper, for amd64 and arm64. The hub also uses the Flutter web archive. The installer verifies every required file against the release's `SHA256SUMS`, checks the embedded server version, and stages all companions beside the server.

Run as the user who will own the service. A working `systemctl --user` manager, `systemd-run`, Git, OpenSSL, Python 3, and curl are required. The installer installs no system packages and need no sudo.

```sh
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-hub.sh
bash install-hub.sh --dry-run
bash install-hub.sh --proxy-listen 192.168.10.44:7470 --trusted-proxy 192.168.10.50
```

`--listen` (default `127.0.0.1:7470`) is the loopback HTTP listener and must be loopback. Each repeatable `--proxy-listen IP:PORT` is a private address served over plain HTTP to the reverse proxy; it needs at least one repeatable `--trusted-proxy IP`, and `--trusted-proxy` without `--proxy-listen` is refused. Point the proxy at `http://IP:PORT` and let it pass WebSocket upgrades for `/ws/terminal/`.

Repeatable `--local-llm-url URL` and `--local-model-host HOST` options carry the corresponding hub settings into the service unit. `--dead-retention 24h` controls when exited tmux sessions are reaped; use `0` to keep them. `--backup-dir DIR` turns on the hub's scheduled backups (every 6 hours) into that directory; keep it outside the hub home, since a backup holds the owner token, the capability key and the device token hashes.

To use the hub from another machine, add a device for it on the hub (below) and run the CLI with that token: `PROTECTORATE_TOKEN=pd1.... protectorate --hub https://protectorate.hotschmoe.com status`, or keep the token in a 0600 file passed with `--token-file`. HTTPS uses the public web PKI roots.

The Rust hub exposes owner-token diagnostics at `/debug/metrics` and `/debug/pprof/` on its loopback listener.

The installer accepts `--channel stable|dev`, `--version TAG`, `--release-repo OWNER/REPO`, `--release-dir ABSOLUTE_DIR`, `--home ABSOLUTE_DIR`, `--listen IP:PORT`, `--proxy-listen IP:PORT`, `--trusted-proxy IP`, `--dry-run`, and `--no-start`. `--base-url HTTPS_URL` selects a release mirror. `--bundle-dir ABSOLUTE_DIR` installs a local release bundle with `VERSION`, `SHA256SUMS`, matching Rust binaries and `protectorate-web-VERSION.tar.gz`. `--dry-run` downloads and verifies into a temporary directory without writing a token, unit, or release pointer.

Releases are stored under `${XDG_DATA_HOME:-$HOME/.local/share}/protectorate/releases/VERSION`. The `current` symlink selects the active bundle; `previous` records the prior version. The user service is `~/.config/systemd/user/protectorate-hub.service`. The installer replaces only its own marked unit and refuses an existing unmanaged unit. State defaults to `$HOME/.protectorate`: the hub keeps its owner `token` (mode 0600) and `identity.json` there. To keep a user service running while logged out, run `sudo loginctl enable-linger "$USER"` once.

## Build locally

```sh
bash scripts/build-rust-dist.sh vX.Y.Z arm64   # or amd64; needs zig and cargo-zigbuild
```

This writes the five binaries with their published platform names into `dist/`. Create `VERSION` and generate `SHA256SUMS` with `sha256sum`. Include the Flutter web archive named `protectorate-web-VERSION.tar.gz` in the checksums. Then pass `--bundle-dir /absolute/path`. Installing from local source without a checksum manifest is deliberately unsupported.

## Connect a phone or another client

`protectorate devices add phone --url https://protectorate.hotschmoe.com --app` on the hub adds a device and prints its token once, a web and an app pairing link, and a terminal QR of the app link (of the web link without `--app`). Scan or open the link on the phone; the app saves the hub and the token. The token and both links are bearer secrets: anyone holding one is that device until it is revoked. Later devices can be added from the app's DEVICES view on any device. See [auth.md](auth.md#devices).
