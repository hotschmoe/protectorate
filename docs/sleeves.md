# Sleeve development environments

Discover available workspaces and reference directories with
`protectorate sleeve workspaces --json`. Entries with `readOnly: true` are
reference directories suitable for `--ro`:

```sh
protectorate sleeve spawn worker --workspace /absolute/checkout --harness codex \
  --ro /home/owner/github/protectorate/references \
  --ro /absolute/research:/references --cpu 200 --mem 5G --exit-after-idle 5m
```

Paths refer to the hub. The default sandbox destination is the host path.
Mounts must be directories, cannot expose credentials or hub state, and are
checked after resolving symlinks. A read-only mount can cover a workspace's
subdirectory. Reference discovery checks known workspaces' `references`
directories and every `hub serve --sleeve-reference-dir`, without walking the home.
Other safe absolute directories can be mounted explicitly.

The hub discovers Flutter from `PROTECTORATE_FLUTTER_SDK`, `flutter` on PATH,
or `~/development/flutter`, in that order. Set the variable in the hub service's
environment for another installation and restart the hub. Custom hub tool-dir
lists replace automatic discovery; include the SDK and Pub tool directories.
Install and prewarm the SDK and global code generators on the hub before
starting workers. Sleeves expose Flutter, Dart, and `~/.pub-cache/bin` on PATH.
The SDK remains read-only except for a sleeve-local copy of `bin/cache`.
Pub's hosted/git package caches and global package metadata are copied when
global Pub tools are available.
Copies use reflinks where supported and otherwise consume additional disk.

`PUB_CACHE=$HOME/.cache/pub` and `GOMODCACHE=$HOME/.cache/go-mod` are writable
per sleeve. Go first tries the host's existing module download cache through a
file proxy, then the public proxy. New dependencies still require downloads,
which stay in the sleeve home. `make gen`, `make app-check`, and Go commands
use these paths; no SDK download into `/tmp` is needed.

Each sleeve seeds its own `$HOME/.gitconfig` with the hub owner's global git
name and email. Missing fields fall back to the literal settings in
`~/github/protectorate/.git/config`, with includes disabled. Configure the
owner's identity before spawning if neither source supplies it:

```sh
git config --global user.name 'Owner Name'
git config --global user.email 'owner@example.com'
```

Existing sleeve-local identity files are preserved. The hub never changes
global or workspace git config. Attribute assisted commits with an explicit
trailer, for example
`Co-Authored-By: Codex (gpt-6.1-sol) <noreply@openai.com>`.
Caller-provided `GIT_AUTHOR_*` and `GIT_COMMITTER_*` can override identity.

CPU limits also set build concurrency: `--cpu 200` defaults to two Go workers,
`GOFLAGS=-p=2`, and two Flutter test workers through `make app-check`. Fractional
cores round down, with a minimum of one; unlimited CPU defaults to one worker.
Override with repeatable `--env GOMAXPROCS=1`, `--env GOFLAGS=-p=1`, or
`--env FLUTTER_TEST_CONCURRENCY=1`. For direct Flutter/Dart test invocations,
pass `--concurrency="$FLUTTER_TEST_CONCURRENCY"`. These limits apply per command;
running several heavy builds at once still shares the sleeve's memory cap.

Idle removal applies only to chat sleeves and defaults off. After continuous
IDLE with no pending or queued mail for `--exit-after-idle`, the hub closes the
harness, kills its scope, revokes its identity and publishes the normal sleeve
exit event. Working turns, approvals, compaction, hub takeovers and pending
mail prevent removal. The home and workspace remain on disk. The timeout and
mounts survive hub restarts and resleeves; the idle clock restarts after a hub
restart. No persisted-state conversion is required for this release. Restart
the updated hub and respawn existing sleeves to opt into mounts or idle removal.

```sh
systemctl --user restart protectorate-hub
```
