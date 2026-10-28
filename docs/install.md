# Install the Rust hub

## Network model

Protectorate never builds a network and never listens on the internet. The routers own connectivity: the home and office UniFi Dream Machines join their LANs with site-to-site WireGuard, and phones are WireGuard road-warrior clients of the office router. Protectorate runs on those private addresses and authenticates every peer itself; being on the network is never proof of identity.

- Every listener binds an explicit private address: loopback, RFC 1918, 100.64.0.0/10, fc00::/7 or fe80::/10. `0.0.0.0`, `::` and public addresses are refused, with no override.
- The hub creates its own CA on first start (`HOME/ca/ca.pem`, key `HOME/ca/ca-key.pem`, mode 0600; the key never leaves the hub and is part of hub backups). Each start issues the hub a fresh certificate for its `--tls-listen` addresses and `--tls-name` names.
- Clients (the app, the CLI, the MCP bridge) reach the hub over HTTPS and trust only that CA; devices sign every request with the key they registered when they paired, and scripts and sleeves hold short-lived capabilities (see [auth.md](auth.md)). The hub also serves plain HTTP on its loopback `--listen` for local tools and sleeves; the owner token works only there.

Recommended router firewall rules:

| From | To | Allow |
|------|----|-------|
| client and VPN zones (laptops, phones) | hub | TCP 7470 only |

## Hub

The Linux release contains `protectorate-rs`, `protectorate` (the CLI), `protectorate-mcpbridge`, and the sleeve runner and sandbox helper, for amd64 and arm64. The hub also uses the Flutter web archive. The installer verifies every required file against the release's `SHA256SUMS`, checks the embedded server version, and stages all companions beside the server.

Run as the user who will own the service. A working `systemctl --user` manager, `systemd-run`, Git, OpenSSL, Python 3, and curl are required. The installer installs no system packages and need no sudo.

```sh
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-hub.sh
bash install-hub.sh --dry-run
bash install-hub.sh --tls-listen 192.168.22.5:7470 --tls-name stronghold.lan
```

`--listen` (default `127.0.0.1:7470`) is the loopback HTTP listener; each `--tls-listen` is a private address served over HTTPS.

For a hub behind a reverse proxy, the proxy must trust the hub CA. Repeatable `--trusted-proxy IP`, `--local-llm-url URL`, and `--local-model-host HOST` options carry the corresponding hub settings into the service unit. `--dead-retention 24h` controls when exited tmux sessions are reaped; use `0` to keep them. `--backup-dir DIR` turns on the hub's scheduled backups (every 6 hours) into that directory; keep it outside the hub home, since a backup holds the CA key and every device identity.

To use the hub from another machine, give the CLI the CA: `PROTECTORATE_TLS_CA_CERT=/path/ca.pem protectorate --hub https://stronghold.lan:7470 status`. Browsers need the CA installed in the system or browser trust store. For the Android app, run `PROTECTORATE_TLS_CA_CERT=~/.protectorate/ca/ca.pem protectorate pair --url https://stronghold.lan:7470 --app` on the hub: the pairing link carries the CA, and the app trusts only it for that hub.

The Rust hub exposes owner-token diagnostics at `/debug/metrics` and `/debug/pprof/` on its loopback listener.

The installer accepts `--channel stable|dev`, `--version TAG`, `--release-repo OWNER/REPO`, `--release-dir ABSOLUTE_DIR`, `--home ABSOLUTE_DIR`, `--listen IP:PORT`, `--dry-run`, and `--no-start`. `--base-url HTTPS_URL` selects a release mirror. `--bundle-dir ABSOLUTE_DIR` installs a local release bundle with `VERSION`, `SHA256SUMS`, matching Rust binaries and `protectorate-web-VERSION.tar.gz`. `--dry-run` downloads and verifies into a temporary directory without writing a token, unit, or release pointer.

Releases are stored under `${XDG_DATA_HOME:-$HOME/.local/share}/protectorate/releases/VERSION`. The `current` symlink selects the active bundle; `previous` records the prior version. The user service is `~/.config/systemd/user/protectorate-hub.service`. The installer replaces only its own marked unit and refuses an existing unmanaged unit. State defaults to `$HOME/.protectorate`: the hub keeps its owner `token` (mode 0600) and `ca/` there. To keep a user service running while logged out, run `sudo loginctl enable-linger "$USER"` once.

## Build locally

```sh
bash scripts/build-rust-dist.sh vX.Y.Z arm64   # or amd64; needs zig and cargo-zigbuild
```

This writes the five binaries with their published platform names into `dist/`. Create `VERSION` and generate `SHA256SUMS` with `sha256sum`. Include the Flutter web archive named `protectorate-web-VERSION.tar.gz` in the checksums. Then pass `--bundle-dir /absolute/path`. Installing from local source without a checksum manifest is deliberately unsupported.

## Connect a phone or another client

`protectorate pair [--url https://HOST:7470] [--app]` on the hub prints a QR code and link containing only the hub URL and CA. Open it in the app, name the device and compare the code with the request on a trusted device before approving, or run `protectorate devices approve CODE` on the hub for the first device. See [auth.md](auth.md#device-approval).
