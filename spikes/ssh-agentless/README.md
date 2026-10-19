# Spike: SSH as the hub-to-node transport (no daemon, no tailnet)

Question: what is the lightest possible hub-to-node channel? Can we skip a
resident node daemon and an overlay network entirely, and just run our binary
over SSH on demand?

## What was built

`main.go`: a ~50 line stdin/stdout JSON probe. It reads one JSON request per
line and answers `metrics`. Anything else is rejected
(`method not allowed`). Built with `CGO_ENABLED=0 -ldflags="-s -w"`.

| target | size |
|--------|------|
| linux/arm64 | 2.5 MB |
| windows/amd64 | 2.7 MB |
| windows/arm64 | 2.5 MB |
| android/arm64 | 2.8 MB |

## Test (hub = linux/arm64, node = `officewin`, Windows AMD64, LAN)

```
scp .out/probe-windows-amd64.exe officewin:AppData/Local/Temp/protectorate-spike/probe.exe
echo '{"method":"metrics"}' | ssh officewin '%TEMP%\protectorate-spike\probe.exe'
```

| pattern | latency |
|---------|---------|
| fresh SSH connection per call | 318 ms |
| ControlMaster-multiplexed, new exec per call | 45 ms/call |
| one long-lived SSH session, 200 requests streamed on stdin | 39 ms total (~0.2 ms/request) |

Nothing was installed on the node. The binary sits in `%TEMP%` and nothing
keeps running after the call.

## Findings

- **SSH stdio is a perfectly adequate RPC transport.** A long-lived session
  is effectively a persistent bidirectional pipe, so it can carry node-pushed
  events too (metrics ticks, job logs) without a listener on the node.
- **Gating comes for free from sshd.** Untested here, to avoid changing
  officewin's config: an `authorized_keys` entry like
  `restrict,command="protectorate node rpc" ssh-ed25519 AAAA...` means the hub's
  key can *only* start our binary in RPC mode. Our binary then enforces the
  per-host policy (allowlisted methods, approval for destructive ones). A
  separate, normal key, or a short-lived certificate, would be needed for
  free-form admin shells. On Windows, admin accounts read
  `C:\ProgramData\ssh\administrators_authorized_keys`.
- **OpenSSH is already on all five platforms** (Windows ships it; Termux
  has it), and it's already how the user works. Enrollment is the same single
  step a tailnet needs: put a key on the node.
- **What SSH alone does NOT solve:** reachability. Roaming laptops, NAT and
  machines outside the LAN/office VPN can't be reached. That's the real job of
  a tailnet (or WireGuard). So SSH replaces the *RPC/transport/gating* role,
  not the *reachability* role.

## Implication

Separate the two concerns:

```
reachability : LAN | existing WireGuard | tsnet/Headscale (optional, for roaming)
transport    : SSH session -> `protectorate node rpc` (stdio JSON), forced command
policy       : enforced inside our binary on the node
```

That makes a resident node daemon optional. It's needed only for things that
must happen when the hub isn't connected, which is nothing so far.
