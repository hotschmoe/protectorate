# Install the Rust hub and node

## Network model

Protectorate never builds a network and never listens on the internet. The routers own connectivity: the home and office UniFi Dream Machines join their LANs with site-to-site WireGuard, and phones are WireGuard road-warrior clients of the office router. Protectorate runs on those private addresses and authenticates every peer itself; being on the network is never proof of identity.

- Every listener binds an explicit private address: loopback, RFC 1918, 100.64.0.0/10, fc00::/7 or fe80::/10. `0.0.0.0`, `::` and public addresses are refused, with no override.
- The hub creates its own CA on first start (`HOME/ca/ca.pem`, key `HOME/ca/ca-key.pem`, mode 0600; the key never leaves the hub and is part of hub backups). Each start issues the hub a fresh certificate for its `--tls-listen` addresses and `--tls-name` names.
- Clients (the app, the CLI, the MCP bridge) reach the hub over HTTPS and trust only that CA; devices sign every request with the key they registered when they paired, and scripts and sleeves hold short-lived capabilities (see [auth.md](auth.md)). The hub also serves plain HTTP on its loopback `--listen` for local tools and sleeves; the owner token works only there.
- A node joins with a single-use code from `node invite`. It creates its own P-256 key, signs the code with it and sends the public key; the hub CA returns a certificate naming the node. The node trusts only the hub CA.
- The hub dials nodes over mutual TLS on port 7475; the only call a node makes to the hub afterwards is reporting its own address (below). A node accepts only a client certificate from the hub CA that names the hub, so another node or any other CA is refused. The hub accepts only the node certificate issued at the node's current join, at the address the node reported; `node rm` forgets it, which revokes it. Certificates last a year and the hub renews them in place a month before expiry.
- **A node follows its machine's address.** When DHCP gives the machine another IP, the node notices within 30 seconds (it checks that the IP it serves on is still assigned to the machine), rebinds to the address its route to the hub leaves from, keeps the port, stores it in `node.json` and calls `NodeService/ReportNodeAddress` on the hub over mutual TLS with its own certificate. The hub accepts that call from no one else, requires the reported IP to be the source IP of the connection it arrived on (the hub and nodes are not behind NAT, so a node can only point the hub at itself) and a private address, audits the old and new address, and dials the new one from its next poll; no restart is needed on either side. A report that fails (hub down, rejected) is retried until it succeeds, and a good one is repeated every ten minutes, which the hub treats as a no-op. An explicit `node serve --listen IP:PORT` is the exception: it is served exactly as given, never replaced, and a failure to bind it is an error; drop the flag to follow the machine. `node install --listen` only chooses the address recorded at join. Set `PROTECTORATE_NODE_ADDRESS_INTERVAL` (seconds) to change the 30 second check. Reports work only where the source IP of the node's connection to the hub is the IP it serves on, which holds on one LAN or a routed site-to-site link without NAT; a multi-homed node serving on an address other than the one its route to the hub uses keeps working with the address recorded at join but cannot report a move (`node doctor` says so).

Recommended router firewall rules:

| From | To | Allow |
|------|----|-------|
| client and VPN zones (laptops, phones) | hub | TCP 7470 only |
| hub | nodes | TCP 7475 only |
| node | node | nothing |
| care and family machines | | their own VLAN, reachable only from the hub |

## Hub

The Linux release contains `protectorate-rs`, `protectorate` (the CLI), `protectorate-mcpbridge`, the sleeve runner and sandbox helper, and the node sandbox helper for amd64 and arm64. The hub also uses the Flutter web archive. A hub install also keeps every node asset its release lists (the other Linux architecture, the Termux node and the Windows trio), because invite lines download node binaries from the hub's `/dl`, never from GitHub. The installers verify every required file against the release's `SHA256SUMS`, check the embedded server version, and stage all companions beside the server.

Run as the user who will own the service. A working `systemctl --user` manager, `systemd-run`, Git, OpenSSL, Python 3, and curl are required. The scripts install no system packages and need no sudo.

```sh
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-hub.sh
bash install-hub.sh --dry-run
bash install-hub.sh --tls-listen 192.168.22.5:7470 --tls-name stronghold.lan
```

`--listen` (default `127.0.0.1:7470`) is the loopback HTTP listener; each `--tls-listen` is a private address served over HTTPS. `--public-url https://HOST:7470` names the URL invites and installers use; without it the first `--tls-listen` address is used. The installer prints the CA path after it starts the hub. For a hub behind a reverse proxy, the proxy must trust the hub CA. Repeatable `--trusted-proxy IP`, `--local-llm-url URL`, and `--local-model-host HOST` options carry the corresponding hub settings into the service unit. `--dead-retention 24h` controls when exited tmux sessions are reaped; use `0` to keep them.

To use the hub from another machine, give the CLI the CA: `PROTECTORATE_TLS_CA_CERT=/path/ca.pem protectorate --hub https://stronghold.lan:7470 status`. Browsers need the CA installed in the system or browser trust store. For the Android app, run `PROTECTORATE_TLS_CA_CERT=~/.protectorate/ca/ca.pem protectorate pair --url https://stronghold.lan:7470 --app` on the hub: the pairing link carries the CA, and the app trusts only it for that hub.

The Rust hub exposes owner-token diagnostics at `/debug/metrics` and `/debug/pprof/` on its loopback listener. `/debug/nodes/NAME/metrics` returns the hub's cached node monitor status. `/ca.pem` is public: it is the certificate every invite pins by SHA-256.

## Nodes

Invite a node from the app (ADD NODE) or the CLI (`protectorate node invite NAME`). The invite shows one line per platform. Each line downloads `/ca.pem`, checks its SHA-256 against the value in the line, then downloads the installer over TLS pinned to that CA and runs it with the join code. The code works once and expires (one hour by default); treat the line like a password. On Linux:

```sh
ca=$(mktemp) && curl -fsSk --max-redirs 0 'https://HUB:7470/ca.pem' -o "$ca" && [ "$(sha256sum < "$ca" | cut -d' ' -f1)" = CA_SHA256 ] && script=$(curl -fsSL --cacert "$ca" 'https://HUB:7470/install.sh') && ...
```

The Linux bootstrap downloads the Rust server and node sandbox helper from the hub's verified `/dl` assets, checks each SHA-256 again, and runs `node install`, which joins and installs the `protectorate-node` user service. It accepts `--name`, `--listen IP:PORT`, `--print-target`, `--allow-plain-jobs`, `--dry-run`, and `--no-start`, and keeps its download cache under `${XDG_CACHE_HOME:-$HOME/.cache}/protectorate`. Without `--listen` the node listens on the address of the interface it uses to reach the hub, port 7475; the hub records that address at join and updates it by itself when the node reports a new one; `protectorate node add NAME --addr IP:PORT` still sets one by hand (an update keeps the certificate). DHCP reservations are still kinder, since jobs running on a node die with its address.

The manual path, with the release bundle already on the machine:

```sh
bash install-node.sh --hub https://HUB:7470 --name officepc --code-file /absolute/path/join.code --ca-cert /absolute/path/ca.pem
```

Add `--allow-plain-jobs` only for a care node whose plain jobs must run as the agent user. An already joined node keeps its key and certificate. Run `protectorate node doctor` on the node to inspect its join state, certificate, user service and address (the recorded address, whether the machine still has it, the address its route to the hub uses and the last report to the hub), or add `--json` for machine-readable results.

Both scripts accept `--channel stable|dev`, `--version TAG`, `--release-repo OWNER/REPO`, `--release-dir ABSOLUTE_DIR`, `--home ABSOLUTE_DIR`, `--listen IP:PORT`, `--dry-run`, and `--no-start`. The node wrapper needs `install-hub.sh` beside it because both roles share one verifier and service implementation. `--base-url HTTPS_URL` selects a release mirror. `--bundle-dir ABSOLUTE_DIR` installs a local release bundle with `VERSION`, `SHA256SUMS`, and matching Rust binaries; a hub bundle also needs `protectorate-web-VERSION.tar.gz`. `--dry-run` downloads and verifies into a temporary directory without writing a token, unit, or release pointer.

Releases are stored under `${XDG_DATA_HOME:-$HOME/.local/share}/protectorate/releases/VERSION`. The `current` symlink selects the active bundle; `previous` records the prior version. User services are `~/.config/systemd/user/protectorate-hub.service` and `protectorate-node.service`. The installer replaces only its own marked unit and refuses an existing unmanaged unit. State defaults to `$HOME/.protectorate`: the hub keeps its owner `token` (mode 0600) and `ca/` there, a node keeps `node.json` and `tls/`. To keep a user service running while logged out, run `sudo loginctl enable-linger "$USER"` once.

## Build locally

```sh
PROTECTORATE_BUILD_VERSION=vX.Y.Z cargo build --profile dist -p protectorate-server -p protectorate-sleeve-runner -p protectorate-node -p protectorate-cli -p protectorate-mcpbridge --bins
```

Place the six resulting binaries from `target/dist` in one directory with the published platform names (`*-linux-amd64` or `*-linux-arm64`), create `VERSION`, and generate `SHA256SUMS` with `sha256sum`. For a hub, include the Flutter web archive named `protectorate-web-VERSION.tar.gz` in the checksums. Then pass `--bundle-dir /absolute/path`. Installing from local source without a checksum manifest is deliberately unsupported.

## Windows node (current user)

Paste the invite's Windows line into Windows PowerShell 5.1 (`powershell.exe`, not `pwsh`) as the user whose session the node runs in. It downloads `/ca.pem`, checks its SHA-256, trusts only that CA for the rest of the session and runs the hub's `/install.ps1`, which is pinned to the hub's release version and manifest hash and downloads the binaries from the hub's `/dl` over the same pinned TLS.

The installer selects the native Rust `protectorate-rs`, `protectorate` (the CLI), and `protectorate-mcpbridge` release assets for the machine's x64 or ARM64 OS (`COMPONENT-windows-amd64.exe` or `COMPONENT-windows-arm64.exe`) and checks all three against the pinned `SHA256SUMS` before writing state or registering a task. It refuses existing node state, install directory, or `Protectorate-Node` task. It joins the hub with the code from `PROTECTORATE_JOIN_CODE`, keeps the node key and certificate under `%LOCALAPPDATA%\Protectorate-Node`, installs the executables under `%LOCALAPPDATA%\Programs\Protectorate-Node`, and registers a limited-privilege logon task that runs the guardian copy of the server through `conhost.exe --headless`. The guardian keeps the task alive while the hub updates `protectorate-rs.exe`; a failed trial restores the previous executable. If the join or task setup fails, the installer removes the task, files and state it created. Add `-AllowPlainJobs` only for a care node. `-BundleDir C:\private\bundle` installs from trusted local files instead of the hub, and `-VerifyOnly` checks the assets without changing the machine.

Run `& "$env:LOCALAPPDATA\Programs\Protectorate-Node\protectorate-rs.exe" node doctor --home "$env:LOCALAPPDATA\Protectorate-Node" --json` to inspect the installed node.

For shared-node jobs, provision a separate local runner account once from an elevated PowerShell prompt **as the same user that runs the node task**. Choose a new absolute job root outside the agent state and user profile, then restart the node task:

```powershell
& "$env:LOCALAPPDATA\Programs\Protectorate-Node\protectorate-rs.exe" node install-runner --home "$env:LOCALAPPDATA\Protectorate-Node" --job-root "C:\ProgramData\Protectorate\runner-jobs"
```

Without provisioning, the node reports no sandbox and rejects sandboxed jobs. Run provisioning from the user's interactive session; user-scoped DPAPI sealing fails from an elevated SSH session.

The release workflow requires the Rust Linux amd64 and arm64 bundles, the native Windows backend, CLI and MCP bridge trio, the Termux arm64 node cross-build, and Flutter desktop and web builds before publishing. The Termux binary is published as `protectorate-rs-android-arm64` and joins and serves like the Linux node ([rust-termux.md](rust-termux.md)).
