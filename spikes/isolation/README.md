# Spike: workspace isolation for sleeves

Question (PROBLEMS P7/P8): what is the best per-workspace isolation so agents stop killing each
other's processes, with CPU/RAM limits, shared auth, and a persistent tmux session the hub can
attach to? Constraint from the owner: "lightweight" applies only to how we treat the harness CLIs
(never wrapped, patched or proxied). The isolation layer may be as complex as needed to be correct,
robust and fast.

Host: Debian 12, aarch64 (12 cores, 30 GB), kernel 6.6, cgroup v2 + systemd, Docker 20.10,
bubblewrap 0.8, tmux 3.3a, /dev/kvm. All runs as an unprivileged user (uid 1001). Measured 2026-10-19.

## Results matrix

| | none (baseline) | A. Docker | B1. bwrap | B2. bwrap + systemd scope | C1. unshare | C2. unshare + scope | D. Firecracker | E. gVisor (runsc do) |
|---|---|---|---|---|---|---|---|---|
| 1. Cold start to running tmux (median of 5) | 40 ms | 530-700 ms | 49 ms | 111 ms | 66 ms | 94 ms | 930-1080 ms to systemd login; ~50 ms to first userspace | 310-470 ms (shell); 1.2 s incl. claude+codex+tmux |
| 2. Idle memory | ~4 MB | cgroup 2.3 MiB; procs RSS 24 MB (shim 14.5 MB); dockerd+containerd fixed ~120 MB RSS | RSS sum 13.5 MB (double counts libs) | cgroup 2.7 MB; RSS 13.5 MB | RSS 19.9 MB (nested unshare + bash) | cgroup 10.5 MB | host RSS 45 MB (kernel+init) to 100 MB (full systemd), 256 MiB guest | not measured |
| 3. PID isolation (A cannot see/kill host or B) | NO: killed host dummy and B | yes | yes | yes | yes | yes | yes (separate kernel) | yes |
| 4. CPU cap enforced (4 busy loops, 1 CPU cap) | no (4.4 cores) | 1.2 cores | no (4.35 cores) | 1.14 cores | no (4.5 cores) | 1.10 cores | vcpu count only (not measured) | `--ignore-cgroups` needed rootless: no cap |
| 4. Memory cap enforced (`tail /dev/zero`, 256M) | no | yes: hog killed (137), sandbox survives | no | yes with `OOMPolicy=continue` | no | yes | guest RAM size is the cap | no (rootless) |
| 5a. Workspace rw, host sees it | yes | yes | yes | yes | yes (no mount ns, same path) | yes | no: needs block device or network FS (no virtiofs in Firecracker) | yes |
| 5b. Shared auth dir, write-temp-rename in A seen by B | yes | yes | yes | yes | yes | yes | not natively (separate disks) | not tested |
| 5c. Single-file bind + rename-over | n/a | EBUSY (confirmed) | EBUSY (confirmed) | EBUSY | n/a | n/a | n/a | not tested |
| 5d. Symlink file -> shared dir, CLI renames over link | link replaced, B sees stale value (confirmed failure) | same | same | same | same | same | n/a | n/a |
| 5e. Symlink to the whole shared dir, rename inside | works | works | works | works | works | works | n/a | n/a |
| 6. Harness CLIs from host (`claude`, `codex`, `pi`, `grok`, node, gcc, git) without reinstall | host | yes via ro mounts of `~/.local ~/.nvm ~/.grok`; gcc/git/tmux from image | yes, same ro mounts; toolchain from host `/usr` | yes | yes (host fs fully visible) | yes | no: rootfs image only (Ubuntu squashfs has tmux, no claude/node/gcc); must bake or add disks | yes: claude, codex, tmux ran under `runsc do` |
| 7. Hub attaches to tmux inside | yes | yes: `tmux -S` on bind-mounted `ctl/` dir, or `docker exec tmux` | yes: `tmux -S` on bind-mounted `ctl/` dir (PTY attach tested) | yes | yes (socket on host path) | yes | needs serial/vsock/ssh; no socket sharing | not tested |
| 8. Rootless | n/a | no: docker group is effectively root | yes | yes (systemd user manager, cpu+memory+pids delegated) | yes | yes | yes (needs only /dev/kvm; TAP networking would need root) | yes |
| Host fs exposed to sandbox | all | only mounts | only mounts (`--tmpfs /` + ro `/usr /etc`) | same | all (home listing visible) | all | none | host root as in-memory overlay |
| Network isolation | none | yes (bridge) | none (shares host net) | none | none | none | needs TAP (root) | `--network=none` here |
| Security boundary | none | shared kernel; default seccomp + caps dropped | shared kernel; can add seccomp | shared kernel | shared kernel | shared kernel | hardware (KVM) | userspace kernel (syscall filter) |

Raw numbers: `results-reference.log` (one `RESULT|variant|key|value` per line).

## Reproduce

```
cd spikes/isolation
docker build -t spike-iso-img:2 image          # arm64 debian:12.11-slim + tmux, git, build-essential
./fc-setup.sh                                  # firecracker v1.17.0 (sha256 verified) + CI kernel/squashfs into .cache/
./run-all.sh                                   # all variants -> results.log
./run.sh bwrap-cg pid limits                   # one variant, selected tests
./fc.sh 256 1                                  # firecracker boot + memory (mem MiB, vcpus)
./gv.sh                                        # gVisor release 20260112.0 rootless probe
```

Variants for `run.sh`: `none docker bwrap bwrap-cg unshare unshare-cg`. Tests: `start mem pid limits
mounts tools tmux`. Scratch state lives in `/run/user/$UID/spike-iso/`; docker containers are named
`spike-iso-*` and run with `--rm`. Downloads go to the gitignored `.cache/`.

How the harness works: `lib.sh` implements start/exec/stop per variant. Every sandbox runs a tmux
server on a socket in a bind-mounted `ctl/` dir, which is how the hub reaches it and how `sb_exec`
runs commands (tmux windows), so hogs and test commands live inside the sandbox's pid namespace and
cgroup. PID test: a dummy `sleep` on the host and one in sandbox B; sandbox A runs `kill -9` on their
real host PIDs and `pkill -f` by name; then we check both survive.

## Findings and gotchas (all hit while measuring)

- Plain PID-namespace isolation is enough to fix P8. In every isolated variant sandbox A saw 8-9
  processes, 0 of the host dummy or B's process; `kill -9 <host pid>` returned "No such process".
  The baseline killed both.
- Limits need a cgroup. bwrap and unshare alone cap nothing. `systemd-run --user --scope` works
  rootless because the user manager delegates `cpu memory pids`. Enforced: CPU 4 loops -> 1.1 cores;
  256M memory hog gets OOM-killed. By default systemd kills the whole scope on any OOM kill; set
  `-p OOMPolicy=continue` so only the hog dies (Docker does that natively).
- Shared auth: bind-mounted directories behave correctly with write-temp-rename. Single-file bind
  mount + rename gives `EBUSY` (confirmed for Docker and bwrap). A symlink to a file is replaced by
  the rename, so the other sandbox keeps the stale token. A symlink to the directory works.
- Entering a bwrap sandbox from outside with `nsenter` fails unprivileged (capabilities dropped,
  `setgroups` denied, pid ns owned by a parent userns). Use the tmux socket (or `bwrap` started by a
  per-sandbox supervisor) as the control channel. Docker has `docker exec`.
- `unshare --pid` ignores SIGTERM while waiting for its child; stop it with SIGKILL (`--kill-child`
  then tears the namespace down). Rootless `unshare -r` gives uid 0 inside; harnesses such as Claude
  refuse some modes as root, so we nest `unshare -U --map-user=$UID` to keep a non-root uid. bwrap keeps
  the uid natively.
- tmux 3.3a has no `-D`; keep the sandbox's pid 1 alive with a `has-session` loop. Unset `$TMUX` when
  the hub itself runs in tmux.
- Docker: the cached `debian:12` here was amd64 (qemu binfmt). Host aarch64 binaries then failed
  ("not found") and cold start doubled. Always pin a platform-specific tag.
- Landlock is not enabled in this kernel's LSM list, and it would not hide processes anyway.
- Firecracker works rootless with KVM on this aarch64 host (kernel 6.6): full Ubuntu 24.04 guest to
  autologin shell in ~1 s, ~100 MB host RSS for a 256 MiB guest. Gaps for our use: no virtiofs, so no
  live shared auth/workspace mount; no host toolchain reuse; TAP networking needs root (not tested).
- gVisor `runsc do` works rootless with the static release binary (systrap, ptrace, kvm platforms all
  boot in ~0.3-0.5 s) and ran the real `claude` and `codex` binaries plus tmux unmodified. Rootless
  means no cgroups (`--ignore-cgroups`). Syscall overhead and compatibility under heavy builds not
  measured.
- Elixir/OTP: would help supervise sleeves (restart policies, monitors, per-sleeve processes, hot
  upgrades) in the hub, but adds nothing to isolation, which is a kernel feature. Language choice for
  the engine is independent of this decision.

## Untouched-harness check

The harness binaries are never modified or wrapped. What changes per runtime is only the environment
they start in. bwrap/Docker/unshare run `claude 2.1.303`, `codex`, `pi`, `grok` straight from the host
install paths mounted read-only at the same absolute paths. Their config/auth directories must be
mounted at their usual locations (for example `~/.claude`, `~/.codex`) from the shared auth store plus
a per-sleeve state dir; sandbox `$HOME` is a tmpfs otherwise. Firecracker is the only variant that
forces a change (separate OS image, harnesses reinstalled or baked in, auth copied or served over a
network filesystem).

## Recommendation

Default sleeve runtime on Linux: **bubblewrap inside a `systemd-run --user --scope`** (variant B2).
- Fastest correct option: 111 ms cold start, ~3 MB cgroup memory, no daemon, rootless.
- Real PID isolation, enforced CPU/RAM/pids caps (set `OOMPolicy=continue`), explicit mount
  list so the sandbox sees only workspace + shared auth + read-only toolchains.
- Harnesses run untouched from host installs; hub controls them through a tmux socket in a
  bind-mounted dir; stale-token problems avoided by mounting auth as a directory.
- Needs: unprivileged user namespaces (default on Debian; Ubuntu 24.04 needs an AppArmor profile) and a
  systemd user manager.
- Gap to close: bwrap shares the host network namespace, so two sleeves can fight over a dev-server port
  and agents can see each other's listening sockets. Fixing this (`--unshare-net` plus pasta/slirp4netns,
  or a network namespace per sleeve) needs a helper not installed here. If per-sleeve networking becomes a
  hard requirement before that, use Docker.

Runtime interface alongside it:
- **Docker** as the portable runtime: macOS/Windows hosts, per-sleeve bridge network, built-in OOM
  semantics and `docker exec`. Cost is 0.5-0.7 s starts, a ~120 MB resident daemon, an image to
  maintain for toolchains, and docker-group = root on Linux. Acceptable, not the default on Linux.
- **Untrusted work (donated work on public repos, P6)**: **Firecracker** microVM. It is the only
  variant that is a hardware boundary, boots in about 1 s and costs about 100 MB per VM. Budget the
  engineering for workspace/auth transport (block device or vsock agent), baked toolchains, and root
  or a helper for TAP networking. If a VM is not available, gVisor rootless is the intermediate option
  (user-space kernel, no cgroup limits rootless, harnesses ran unmodified). Do not treat bwrap or Docker
  as a boundary against hostile code: they share the host kernel.
- Do not use plain `unshare` (C1/C2): it exposes the whole host filesystem (including `$HOME`), has no
  mount isolation for auth, and is strictly less capable than bwrap for similar startup cost.
