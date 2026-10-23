# Install the Rust hub and node

The Linux release contains `protectorate-rs`, `protectorate-cli`, `protectorate-mcpbridge`, the sleeve runner and sandbox helper, and the node sandbox helper for amd64 and arm64. The hub also uses the Flutter web archive. The installers verify every required file against the release's `SHA256SUMS`, check the embedded server version, and stage all companions beside the server. No Protectorate Go executable or bwrap installation is required; the hub still runs the separately pinned Headscale server.

Run as the user who will own the service. A working `systemctl --user` manager, `systemd-run`, Git, OpenSSL, Python 3, and curl are required. The scripts install no system packages and need no sudo.

```sh
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-hub.sh
bash install-hub.sh --dry-run
bash install-hub.sh
```

The default hub listener is `127.0.0.1:7470`. To listen on a LAN address, pass `--listen IP:PORT --tls-cert /absolute/cert.pem --tls-key /absolute/key.pem`; the Rust server refuses non-loopback plaintext. Certificates must cover the address the hub uses for its internal listener. After installation, configure the tailnet with the server's printed `hub tailnet init` command. The installer downloads the pinned Headscale binary on a normal hub install; `--no-start` stages only the Rust bundle and service unit.

For a node, transfer its one-use enrollment key into a private file and install with a reachable HTTPS control URL:

```sh
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-hub.sh
curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/master/install-node.sh
chmod 600 /absolute/path/enroll.key
bash install-node.sh --dry-run
bash install-node.sh --control https://HUB:7480 --name officepc --key-file /absolute/path/enroll.key --ca-cert /absolute/path/hub-ca.pem
```

`--ca-cert` is only needed for a private HTTPS CA. The script registers a new node before enabling its service. An already registered node keeps its identity. Add `--allow-plain-jobs` only for a care node whose plain jobs must run as the agent user. The node agent service listens at `127.0.0.1:7475` by default; use TLS certificate and key flags if you expose it on a LAN address. Remove the enrollment key file after registration. The Rust hub now dials enrolled nodes through an authenticated Headscale and private DERP path and checks the current peer identity. Registration alone does not make a node ready: its service must run, appear in the live map, and pass a fresh metrics poll. This path has passed a disposable two-member HTTP test; an installed, long-running service still needs validation. Run `protectorate node doctor --hub https://HUB:7470` on the node to inspect its local enrollment and user service; add `--ca-cert /absolute/path/hub-ca.pem` for a private HTTPS CA or `--json` for machine-readable results.

Once a Rust release is active and the hub tailnet is configured, the hub also serves a public Linux bootstrap at `/install.sh` and a Windows PowerShell installer at `/install.ps1`. The Linux bootstrap downloads the Rust server and node sandbox helper from the hub's verified `/dl` assets, checks each SHA-256 again, and runs `node install`. For example, after receiving a one-use invite:

```sh
curl --cacert /absolute/path/hub-ca.pem -fsSL https://HUB:7470/install.sh | CURL_CA_BUNDLE=/absolute/path/hub-ca.pem PROTECTORATE_ENROLL_KEY='INVITE_KEY' sh -s -- --name officepc --ca-cert /absolute/path/hub-ca.pem
```

The Linux bootstrap accepts `--print-target`, `--allow-plain-jobs`, `--dry-run`, and `--no-start`. It leaves its verified download cache under `${XDG_CACHE_HOME:-$HOME/.cache}/protectorate-rust`. For Windows, save `/install.ps1` and pass `-Name` and `-KeyFile`, or set `PROTECTORATE_NODE_NAME` and `PROTECTORATE_ENROLL_KEY_FILE` before running it. The hub fills in the active build version, release download tag and repository, manifest hash, and control URL; the installer still verifies the manifest and downloaded assets before registration. The Windows installer currently packages amd64 assets only. These public routes return 503 until the hub has both a current Rust release and a configured tailnet.

The Rust hub exposes owner-token diagnostics at `/debug/metrics` and `/debug/pprof/`. Its heap and thread reports are bounded process snapshots, and its CPU report samples at most five seconds; these are Rust reports, not Go pprof files. `/debug/nodes/NAME/metrics` returns the hub's cached node monitor status. Live remote node profiles are not yet available from the Rust node agent.

Both scripts accept `--channel stable|dev`, `--version TAG`, `--release-repo OWNER/REPO`, `--release-dir ABSOLUTE_DIR`, `--home ABSOLUTE_DIR`, `--token-file ABSOLUTE_FILE`, `--listen IP:PORT`, `--tls-cert FILE --tls-key FILE`, `--dry-run`, and `--no-start`. The node wrapper needs `install-hub.sh` beside it because both roles share one verifier and service implementation. `--base-url HTTPS_URL` selects a release mirror. `--bundle-dir ABSOLUTE_DIR` installs a local release bundle with `VERSION`, `SHA256SUMS`, and matching Rust binaries; a hub bundle also needs `protectorate-web-VERSION.tar.gz`. Keep the checksums from the trusted release. `--dry-run` downloads and verifies into a temporary directory without writing a token, unit, or release pointer.

For a hub behind a reverse proxy, pass `--public-url https://PUBLIC_HOST` so generated invites and install links use its public address. Repeatable `--trusted-proxy IP`, `--local-llm-url URL`, and `--local-model-host HOST` options carry the corresponding hub settings into the service unit. `--dead-retention 24h` controls when exited tmux sessions are reaped; use `0` to keep them. The installer refuses to replace a service unit it did not create, so a Go service cutover requires a separately reviewed unit transition.

### Existing Go hub service

`--stage-for-cutover --no-start` installs the checked Rust bundle and writes a separate `protectorate-rust-hub.service` candidate. It leaves `protectorate-hub.service` and the running Go process untouched. Use an explicit Rust `--home` and `--listen`; a spare loopback port lets the candidate run for a trial without competing for the Go listener. The Rust state is separate from Go state.

After validating the candidate and deciding on the listener and TLS arrangement, record the SHA-256 of the exact Go unit file. Stop that service before the transition. `--adopt-go-unit --expect-go-unit-sha256 HASH` then checks that the loaded unit has no drop-ins, the unit file still matches the supplied hash, and the service is inactive. It requires explicit `--home` and `--listen`, verifies the release, saves the Go unit under `~/.config/systemd/user/protectorate-hub.go-HASH.service.backup`, and starts the Rust unit. A non-loopback Rust listener requires `--tls-cert` and `--tls-key`; the current Go listener configuration cannot simply be copied without a TLS or reverse-proxy plan. If activation fails, the installer restores the Go unit. It does not import Go state.

The successful adoption output prints a rollback command containing both unit fingerprints. Stop the Rust service before running that explicit `--rollback-go-unit` command. It checks both hashes and the loaded unit, retains the saved Go unit and a copy of the Rust unit, then reloads and starts Go. Keep the printed hashes and backup path with the cutover record.

Releases are stored under `${XDG_DATA_HOME:-$HOME/.local/share}/protectorate-rust/releases/VERSION`. The `current` symlink selects the active bundle; `previous` records the prior version. The hub's verified web files are extracted under `VERSION/web`. A hub install can add the verified web archive to a matching node-first bundle in the same release root. User services are `~/.config/systemd/user/protectorate-hub.service` and `protectorate-node.service`. The installer replaces only its own marked unit and refuses an existing unmanaged unit. Rust state defaults to `$HOME/.protectorate-rust`, separate from existing Go state; pass `--home` for an intentional migration. Tokens live under that state directory as `token` for the hub and `agent-token` for a standalone node; the installer creates them with mode 0600 and preserves existing tokens. Check the current service with `systemctl --user status protectorate-hub` or `systemctl --user status protectorate-node`. To keep a user service running while logged out, run `sudo loginctl enable-linger "$USER"` once.

## Build locally

```sh
PROTECTORATE_BUILD_VERSION=vX.Y.Z cargo build --release -p protectorate-server -p protectorate-sleeve-runner -p protectorate-node -p protectorate-cli -p protectorate-mcpbridge --bins
```

Place the six resulting binaries in one directory with the published platform names (`*-linux-amd64` or `*-linux-arm64`), create `VERSION`, and generate `SHA256SUMS` with `sha256sum`. For a hub, include the Flutter web archive named `protectorate-web-VERSION.tar.gz` in the checksums. Then pass `--bundle-dir /absolute/path`. Installing from local source without a checksum manifest is deliberately unsupported.

## Windows node (current user)

The Windows installer selects the native Rust `protectorate-rs`, `protectorate-cli`, and `protectorate-mcpbridge` release assets for the machine's x64 or ARM64 OS. Each asset is named `COMPONENT-windows-amd64.exe` or `COMPONENT-windows-arm64.exe`. It requires a trusted SHA256 digest of the release's `SHA256SUMS` manifest, then checks all three selected assets against that manifest before writing state or registering a task. Get the digest through a trusted release channel; a manifest downloaded from an untrusted mirror is not its own proof. Windows CI builds both architectures and gates publication on all six assets.

Run PowerShell as the user whose interactive desktop the node should drive. Put the one-use enrollment key in a private file and use a reachable HTTPS control URL:

```powershell
$version = "vX.Y.Z"
$trustedManifestSha256 = "PASTE_64_HEX_DIGIT_SHA256_FROM_TRUSTED_RELEASE_CHANNEL"
./install-node-windows.ps1 -Version $version -ManifestSha256 $trustedManifestSha256 -Control "https://HUB:7480" -Name "officepc" -KeyFile "C:\private\enroll.key"
```

For a private HTTPS CA, add `-CaCert C:\private\hub-ca.pem`. Add `-AllowPlainJobs` only when enrolling a care node. To install from files already downloaded through a trusted channel, place `VERSION`, `SHA256SUMS`, and all three assets for the machine's architecture in one directory and pass `-BundleDir C:\private\bundle`. `-VerifyOnly` checks the pinned manifest and asset hashes without changing the machine. Remove the enrollment key after a successful install.

The installer refuses existing Rust node state, install directory, or `Protectorate-Rust-Node` task. It creates private state and an agent token under `%LOCALAPPDATA%\Protectorate-Rust-Node`, installs the three release executables under `%LOCALAPPDATA%\Programs\Protectorate-Rust-Node`, and copies the verified server executable to `protectorate-rs-guardian.exe`. The limited-privilege logon task runs this guardian through `conhost.exe --headless` in the interactive session required for desktop actions. The guardian keeps the task alive while the hub updates `protectorate-rs.exe`; a failed trial restores the previous executable. If enrollment or task setup fails, the installer removes the newly created local task, files, and state; if it cannot prove the task stopped and was removed, it preserves files and state. A successful remote enrollment may still need revocation at the hub. Run `& "$env:LOCALAPPDATA\Programs\Protectorate-Rust-Node\protectorate-rs.exe" node doctor --home "$env:LOCALAPPDATA\Protectorate-Rust-Node" --json` to inspect the installed node. An isolated native officewin trial verified install, task startup, reinstall replacement, doctor, and clean uninstall. A later disposable Windows node passed hub-mediated update confirmation and rollback. Desktop input remains untested on that workstation.

For shared-node jobs, provision a separate local runner account once from an
elevated PowerShell prompt **as the same user that runs the node task**. Choose
a new absolute job root outside the agent state and user profile, then restart
the node task:

```powershell
& "$env:LOCALAPPDATA\Programs\Protectorate-Rust-Node\protectorate-rs.exe" node install-runner --home "$env:LOCALAPPDATA\Protectorate-Rust-Node" --job-root "C:\ProgramData\Protectorate-Rust\runner-jobs"
```

Without provisioning, the node reports no sandbox and rejects sandboxed jobs.
An isolated officewin trial provisioned the account from an elevated interactive
task, ran a sandboxed job under `protectorate-runner` in the job workspace,
confirmed that it could not read the node token, and canceled a process tree
including its child. User-scoped DPAPI sealing failed from an elevated SSH
session on that host, so run provisioning from the user's interactive session.
Care nodes need the explicit installer option above to accept their plain jobs.

The release workflow requires the Rust Linux amd64 and arm64 bundles, the native Windows backend, CLI and MCP bridge trio, the Termux arm64 node cross-build, and Flutter desktop and web builds before publishing. The release contains the checked Rust assets, Flutter UI assets, `VERSION`, and `SHA256SUMS`; it does not build Go executables or an Android APK. Windows node updates use the verified server asset; Windows hub self-update still lacks a matched companion bundle. The Termux binary is published as `protectorate-rs-android-arm64`; a hosted build and real-device installation remain to be verified. The broader rewrite status is in [rust-migration.md](rust-migration.md).
