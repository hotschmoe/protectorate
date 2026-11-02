# Audit: pt-update

Adversarial read of `pt-update` and the hub and node paths that stage, switch, and roll back releases. Tree: `next` at `5af9a497` (`pt/audit-update`). `pt/node-update-windows` (`1622fb2c`) is an ancestor of that commit, so the Windows supervisor in `crates/protectorate-node/src/supervise.rs` is what is on `next`. No product code was changed. No live hub, node, or account was contacted.

Severity: **P0** remote code execution or a fleet-wide compromise from input that does not already control the release account or the service user. **P1** a normal operator action bricks the fleet or throws away the rollback net. **P2** a realistic failure leaves a bad release in place, skips the rest of a rollout, or exhausts memory or disk. **P3** latent or defense in depth.

A previous review (devlog 2026-11-01, merged as `next-updfix`) fixed 13 update-path findings. Those fixes were re-read. Items still listed in `REVISIT.md` are not filed again; they are collected at the end so the gap is visible.

## Findings

### 1. P1 — `hub update --rollback` does not restore the headscale snapshot

`crates/pt-hub/src/update.rs` `update_hub`; restore lives in `restore_headscale` and is called only from `probation_prestart` via `Store::start_attempt_with_restore`.

`hub update` snapshots headscale because its migrations are one way (SPEC 6a), then switches `releases/current` on probation. The automatic path restores that snapshot before it points `current` back. `hub update --rollback` calls `Store::rollback`, which only swaps the `current` and `previous` symlinks. It never reads `probation.headscale_snapshot`.

The new hub's health check (`watch_probation` / `healthy`) is `HubRecovered` plus `GetHubInfo` on its own socket. That can pass after headscale has already migrated. `operate.md` then tells the operator to run `hub update --rollback`. The previous binary comes back against the migrated database. Headscale will not run it. The snapshot directory is still on disk and unused.

`Store::rollback` also refuses an open trial (`refuse_open_trial`), so there is no manual restore during the two-minute window either. The only restore is a failed health check.

Failure: publish a hub build that migrates headscale and still answers `GetHubInfo`. Update the hub. The trial confirms. `hub update --rollback`. The process starts, headscale rejects the database, and the fleet stays down. Nodes enrolled after the snapshot are not the loss the automatic path warns about; the database is simply the new one, which the old binary cannot open.

Local reproduction (do not point this at a live home):

1. Build `protectorate` with `CARGO_TARGET_DIR=/tmp/pt-upd` and `CARGO_BUILD_JOBS=4`.
2. Reuse the fixture in `crates/protectorate/tests/update.rs` `prestart_restores_headscale_snapshot_before_rollback`: a fake fleet dir, `db.sqlite` copied into `snapshots/before-v0.0.2`, then a row inserted into the live database, `current` on `v0.0.2`, `previous` on `v0.0.1`, probation already confirmed (`contacted_ms` set, `headscale_snapshot` set).
3. Run `protectorate --home <that home> hub update --rollback` from a process whose argv0 is `releases/current/protectorate`.
4. Observe `current` now names `v0.0.1` and `select name from nodes` still returns the during-trial row. The automatic prestart path in that same test restores `before` and prints `nodes enrolled after this snapshot are lost`.

Fix: on `--rollback`, if the probation record for the release being left has `headscale_snapshot`, restore it with the same function as prestart, or refuse until the operator passes an explicit flag. Say which database the command will put back. A health check that includes one headscale or fleet round trip (already in `REVISIT.md`) makes a bad migration fail the trial, which is the path that already restores.

### 2. P2 — One poller Presence confirms a node trial for good

`crates/protectorate-node/src/serve.rs` `serve_conn`; `crates/protectorate-node/src/update.rs` `Releases::contact`; `crates/pt-update/src/store.rs` `Store::judge`.

Any successful request other than `Hello` on a `tag:hub` connection calls `contact`. The fleet poller (`crates/pt-fleet/src/poller.rs` `round`) sends `Presence` about every 15 seconds, then inventory and sample. `contact` sets `contacted_ms`. After that, `start_attempt` returns `Confirmed` before it counts starts or looks at the deadline. `MAX_TRIAL_STARTS` (3) never runs.

SPEC 6a treats hub contact as the proof that the new node is reachable, and Hello is correctly excluded. The poller is also the hub. It can confirm the trial before `UpdateNodes`'s own follow-up `Presence`, and a crash loop after that single answer does not roll back. `Restart=always` keeps starting the bad binary. If `update_one` then times out, that OS and arch are added to `failed_groups` and every later node in the group is skipped, while the canary stays on the release that just failed the wait.

Failure: a release that answers `Presence` once and then faults (a timer, the first job, member restart). The poller confirms it. The hub reports that the canary did not stay up and does not update its siblings. The canary crash-loops on the new binary until somebody switches it by hand.

Local reproduction:

1. Run the node update e2e stand-in (`crates/protectorate/tests/update.rs` `Supervisor`) so `current` is on a trial and `PROTECTORATE_NODE_TEST_PAUSE_WATCH` is unset.
2. From another process, `protectorate-node rpc --connect <sock> Presence` (the test header is `tag:hub`). `probation.json` gains `contacted_ms`. The existing test `node_trial_refuses_overlap_retries_confirmation_and_rejects_expiry` already shows a successful `Presence` writes that field.
3. Kill the node three more times. Each prestart prints the confirmed verdict and leaves `current` on the trial tag. `rolled_back_ms` stays null.

Fix: confirm only a deliberate follow-up from the updater (or require the node to still be answering at the end of the window). Keep counting start failures after contact, and roll back when they pass `MAX_TRIAL_STARTS`. Do not let an ordinary poll satisfy the update waiter's proof.

### 3. P2 — The download cap does not apply without Content-Length

`crates/pt-update/src/fetch.rs` `Source::curl`, `get`, `download`.

The cap is curl `--max-filesize`. Curl's own manual says that option does nothing when the size is not known up front. `get` then buffers all of stdout and checks `body.len()`. `download` writes `--output` to completion and then checks the file length. A chunked response is stored first. The listing cap is 16 MiB, `SHA256SUMS` is 1 MiB, and an asset is 1 GiB, all only after the fact. A multi-gigabyte chunked listing OOMs the hub or the node inside `Command::output`. A chunked asset fills the disk; cleanup runs only if `stage` returns and `remove_dir_all` succeeds. A kill during the transfer leaves the staging directory.

Default GitHub asset URLs send `Content-Length`, so this is not a bypass of a normal `github.com` blob. It is reachable for any `browser_download_url` or `release_api` that answers `200` with `Transfer-Encoding: chunked` and no length. The node uses the `api` and `repo` fields of `Update` (`crates/protectorate-node/src/update.rs` `Releases::update`), and asset URLs are taken from the JSON with no host allow-list (`Release::from_json`). `install.sh` and `install-node.sh` stream the same way, with no `--max-filesize` at all.

Confirmed locally, without building the crate: a loopback server sent 200000 bytes, chunked, no `Content-Length`. `curl -q --fail --silent --show-error --location --proto '=http' --max-filesize 1000 --output /tmp/pt-upd-chunk-out http://127.0.0.1:<port>/x` exited 0 and left a 200000-byte file.

Failure: point a scratch hub's `release_api` at that server (or serve the release JSON from it, with `browser_download_url` on the same host). `hub update` or a node `Update` grows the process or the staging directory without bound. The sha256 check never runs if the machine is already out of memory.

Fix: have the parent read curl's stdout (or the output file) and kill curl at `max + 1`. Treat a killed-for-size curl as `ErrorKind::Fetch`. Do not depend on `--max-filesize`. Pin asset URLs to the API host, or to `github.com` and `*.githubusercontent.com`, unless the API is the loopback stand-in.

### 4. P2 — `latest` sees only the first 100 releases

`crates/pt-update/src/fetch.rs` `Source::releases`, `Source::latest`.

The client requests `/releases?per_page=100` and does not follow `Link` or `page`. GitHub lists that page by creation time, newest first, not by semver. `latest` then picks the highest tag on that page that the channel admits. `install.sh` `resolve_tag` and `install-node.sh` do the same `per_page=100` cut.

Failure: more than 100 releases exist and the newest 100 are all `vX.Y.Z-dev.N`. `hub update` on the stable channel reports that the repo has no stable release, even when `vX.Y.Z` is on page 2. A dev tag created earlier than those 100 is also invisible, so a high semver that was not among the 100 most recently created tags is skipped.

Local reproduction: extend the fake release server in `crates/pt-update/tests/fake_release.rs` to return 100 `v0.0.1-dev.N` tags and one older `v1.0.0`. `Source::latest(Channel::Stable)` returns "no stable release". `latest(Channel::Dev)` returns the highest dev tag on that page, not `v1.0.0`.

Fix: walk pages until a short page, or ask the releases API for the tag range the channel needs. Bound the walk. Share one implementation with the installers.

### 5. P2 — The hub gives up 10 minutes before the node stops downloading

`crates/pt-hub/src/update.rs` `update_one` (`NODE_CALL_TIMEOUT`, 20 minutes); `crates/pt-update/src/fetch.rs` `Source::new` (`asset_timeout`, 30 minutes).

`Update` is one RPC that covers the node's whole download, hash, version check, and switch. The hub's fleet call times out at 20 minutes. The node's curl is allowed 30. The node does not watch the socket while curl runs, so a closed hub connection does not cancel the transfer. After the timeout the hub treats the node as failed, records the OS and arch in `failed_groups`, and skips every later node in that group. The node can still rename the staging directory into place, switch on probation, and exit 75.

Failure: a slow link where the asset arrives in minute 21. The canary installs the release. The hub prints a failure and does not update the other machines of that arch.

Local reproduction: run the node update e2e with a fake asset handler that sleeps past 20 minutes (or set a debug timeout if you add one; today the constants are fixed). Hold the `Update` RPC open. Close it from the hub side at 20 minutes. Observe the node log still switch and exit 75, and `update_nodes` skip the next target with `the canary for this OS and arch failed`.

Fix: use one budget for the RPC and the download, and cancel the node's curl when the peer hangs up. On a timeout, reconnect and read `Hello.release` before declaring the canary failed. Do not skip the rest of the group while the canary's `current` is already the new tag.

### 6. P2 — A node probation error leaves the new binary running

`crates/protectorate-node/src/update.rs` `Releases::open`, `watch`.

`Releases::open` turns a `probation()` or `check_probation()` error into a log line (`probation().ok()` and the `Err` arm). It does not set the trial flag and does not spawn `watch`. The process keeps serving. `watch`'s `Err` arm logs and retries forever; it does not exit 75. Rollback fails closed only when the store returns `Err` from `roll_back_trial` (the previous pointer does not match `probation.previous`, or that version directory is gone) and when something else restarts the process.

Linux `ExecStartPre` does fail the next start if `probation` returns that error, so a crash after the error takes the node down instead of back to the old binary. Until that crash, the bad release stays up past the deadline. Windows `supervise` `prestart` logs the same error and starts `current` anyway (`probation: ...; starting releases/current anyway`), including when its own fallback `start_attempt` fails. That Windows choice is already in `REVISIT.md`; the Linux in-process watch has the same fail-open shape and is not.

`Store::judge` also returns `Idle` when `current` does not name `probation.version`, after `repair_locked`. A crash at `probation-written` (probation file synced, `current` not switched yet) is safe: the old binary is what is running. The dangerous case is a trial that is already `current` and then cannot roll back.

Failure: during a 10-minute node trial, remove `releases/<previous-tag>/` (disk cleanup; nothing in `pt-update` prunes, so an operator will). At the deadline `watch` logs `probation of <tag> expects previous <old>, found ...` or `release <old>: ...` once a second. `current` stays on the new tag. The node never exits 75.

Local reproduction: the supervisor stand-in in `crates/protectorate/tests/update.rs`, a trial with a short window, delete the previous version directory, let the deadline pass. `releases/current` still names the trial tag and the process is still the one that was started from it.

Fix: if rollback fails, or `probation.json` does not parse, exit 75 only after switching `current` back when the previous directory is still there; if it is not, exit non-zero and do not keep serving. On Linux, prefer staying down over serving the trial. Surface one permanent error, not a log per second.

### 7. P2 — A snapshot that cannot be restored wedges the hub

`crates/pt-hub/src/update.rs` `restore_headscale`; `crates/pt-update/src/store.rs` `roll_back_trial`.

Rollback writes `probation.reason` and syncs it before it calls the restore callback. `restore_headscale` requires `snapshot/db.sqlite` to be a file. If that fails, `current` is left on the trial and the reason stays. The next `start_attempt` sees `reason` and tries the same restore again. `probation_at_start` then refuses to serve (`needs rollback by hub probation before startup`) because `reason` is set or the deadline has passed. `install.sh` uses `Restart=on-failure`, so systemd retries until the start limit and then leaves the unit failed.

The hub does not come back on the old binary or the new one. Recovery is to edit or delete `releases/probation.json` by hand. The existing test `prestart_restores_headscale_snapshot_before_rollback` asserts this: with `headscale_snapshot` pointing at a missing directory, prestart exits non-zero, `current` stays on the trial, and `serve` fails closed. The test then overwrites the JSON to prove the happy path.

Failure: the snapshot directory is removed, or `db.sqlite` is unreadable, during a trial that should roll back (health check failed, or three bad starts). The hub stays down.

Local reproduction: run that e2e, or stop it after the `missing` fixture and before the test rewrites `probation.json`. `systemctl --user start` of a unit with that `ExecStartPre` does not reach `serve`.

Fix: restore the database to a temporary name and switch `current` only after the copy is synced. If the snapshot is missing, still switch `current` back when the operator (or prestart) asks, and report that the database was left as-is, or refuse with a single documented escape (`hub update --rollback --keep-database`) that does not spin the start limit. Do not persist `reason` until the restore has succeeded or been explicitly skipped.

### 8. P3 — A failed trial collapses the rollback stack

`crates/pt-update/src/store.rs` `switch_locked`, `roll_back_trial`.

`switch_on_probation` points `previous` at the release that was `current`, then points `current` at the trial. That overwrites whatever `previous` named before (the last successful rollback target). `roll_back_trial` points `current` back at the saved release and does not put the old `previous` back. After a failed trial both symlinks name the restored release. `hub update --rollback` swaps them and stays put. The version directory of the older release is still on disk.

The existing restore test asserts `current` and `previous` are both `v0.0.1` after rollback. That avoids a second rollback landing on the failed trial. It also drops one successful generation.

Failure: hub on v1 with `previous` = v0. A trial of v2 fails its health check and returns to v1. `hub update --rollback` does not reach v0.

Local reproduction: three staged versions, `switch` to v0, `switch` to v1, `switch_on_probation` to v2 with a short window, `check_probation` after the window. Both pointers read `v1`. `rollback` returns a switch whose `from` and `to` are `v1`.

Fix: store the previous pointer from before the trial and write it back in `roll_back_trial`. Keep the failed tag out of `previous`, so a later rollback moves to that older release rather than to the trial.

### 9. P3 — The node does not cap `probation_secs`

`crates/protectorate-node/src/update.rs` `Releases::update`; the hub cap is `MAX_NODE_PROBATION` in `crates/pt-hub/src/update.rs` `update_nodes`.

The hub rejects a probation outside 1 second to 1 day. The node does `Duration::from_secs(u.probation_secs.max(1))` and stores `deadline_ms = now.saturating_add(window)`, with `window` clamped only to `u64::MAX` milliseconds. A `Update` frame with `probation_secs` at `u64::MAX` arms a trial that will not expire. The hub is the principal that should send `Update`, and a hostile hub can already name `api` and `repo`. The node should still enforce the same ceiling so a bad or confused hub cannot disable the timer while serving a release the operator trusts.

Local reproduction: `protectorate-node rpc` an `Update` whose `probation_secs` is `18446744073709551615` against a local fake release. After switch, `probation.json` `deadline_ms` is far past any operational window. `check_probation` stays `Waiting`.

Fix: reject values outside `1..=86_400` on the node with `ErrorCode::BadRequest`, using the same constant as the hub.

## Checked and sound

- **Download URL before curl.** `Source::new` allows `https://` or `http://` only to `127.0.0.1`, `::1`, or `localhost`, with a numeric port and no userinfo. `@`, `\`, spaces, and control bytes in the authority are rejected. `repo` must be a single `OWNER/NAME` of plain characters. The tag interpolated into `/releases/tags/{tag}` has to pass `version_name` (no slash, no `..`, no controls). Curl is argv, not a shell; `-q` is first, so `~/.curlrc` is not read; `--proto` and `--proto-redir '=https'` block a downgrade.
- **SHA256SUMS.** `Sums::parse` requires UTF-8, 64 lowercase hex digits (`pt-hash` rejects uppercase), then `  ` or ` *`, then one plain file name. Duplicates, empty files, `.`, `..`, slashes, and controls fail. `stage` checks `VERSION` against the sums and requires the trimmed text to equal the tag, then checks each pick's bytes. A reused directory is hashed again under the store lock against the sums just downloaded. A failed stage deletes its own staging directory. The version directory appears by rename after the files are synced.
- **What runs.** `check_binary` runs only after that hash. On Unix it starts a new process group and the timeout kills the group (`binary_check_times_out_when_descendant_keeps_stdout`). The expected first line is `<program> <tag>`. A Windows node also requires a `supervise: 1` line, so an older binary cannot be switched under the frozen supervisor.
- **Switch and crash order.** Probation is synced before `current` moves. `previous` is updated before `current` on the way forward, and `current` is updated before anything else on a trial rollback, so a crash leaves the last known-good binary named by `current` or leaves the trial recorded while `current` is still the old release. Unix pointer replace is a rename over the old symlink. The Windows three-step rename is finished by `Store::open` (`repair_locked`); `PT_TEST_POINTER_ASIDE` exercises it on Linux. The store lock is held across pointer and `probation.json` updates. Temp files for probation use rename.
- **Who may ask.** `UpdateHub` and `UpdateNodes` are `Owner` in `pt-api`. A sleeve sandbox masks the hub home and bind-mounts `releases/` read-only (`crates/pt-hub/src/exec.rs`), so a sleeve cannot plant a staging symlink. The node accepts `Update` only after a `tag:hub` header. Member writes that header from tailscale WhoIs (`go/cmd/member/node.go`); the TCP listener is loopback-only; the unix socket is `0o600`.
- **Admission.** A node update takes the updating flag before the download and refuses new submits. `running()` is checked while that lock is held. An overlapping `Update` during a trial is refused (`node_trial_refuses_overlap_retries_confirmation_and_rejects_expiry`).
- **Versions.** `Version` matches semver precedence for tags without build metadata, including long numeric pre-release ids compared by length so they do not have to fit in `u64`. The `tags` fuzz target checks that against the `semver` crate. Stable rejects pre-release tags; dev admits every parsed tag, so it is not stuck behind stable. `latest` skips drafts and unparsable tags.
- **Hub trial timer.** `HUB_HEALTH_WINDOW` is 120 s and `NODE_CONTACT_WINDOW` is 600 s, matching SPEC 6a. `confirm` will not confirm a different version, a past deadline, or a record that already has `reason`. A missed confirm on a healthy hub restarts so prestart can roll back.
- **Headscale copy.** `copy_state` sanitizes the snapshot label to alphanumerics, `.`, and `-`, and copies the database only while headscale is paused. The automatic rollback path really does restore `db.sqlite` and drops the trial WAL (`prestart_restores_headscale_snapshot_before_rollback`).
- **Windows on this branch.** `supervise` is the SCM entry (`winservice.rs`): previous release's `probation` in a kill-on-close job, 60 s timeout, then `releases\current\...\serve`. Exit 75 restarts after 1 s, anything else after 5 s. The `%ProgramFiles%` copy is not replaced by `node update`. That matches `docs/windows-node-update.md`. It has not been run on Windows here; the Linux e2e drives the same loop with `PT_TEST_POINTER_ASIDE`.

## Already recorded, still true

These are in `REVISIT.md` or `docs/operate.md`. They were re-checked and left as known.

- No signing key. `SHA256SUMS` and the asset are both taken from the release document over HTTPS. A party who can replace both can ship any binary. Immutable releases are not checked.
- Verified bytes are not bound to an inode. `sha256_file` follows symlinks. A same-uid writer can replace a staged file after the hash and before `check_binary` or the switch. The threat model treats agents on the owner's machines as trusted.
- Probation deadlines are wall-clock milliseconds. A forward step rolls back early; a backward step extends the trial. There is no boot-relative clock and no systemd watchdog.
- `check_binary` and the node supervisor do not retry `ETXTBSY`. One busy exec fails the update.
- The hub exits 75 and the service manager starts `releases/current`. It does not re-exec, and sleeves die with the process. The health check does not include a fleet round trip.
- A node with no outbound network is not offered a hub-streamed body. SPEC 6a still describes that path. `REVISIT.md` records the gap.
- Old `releases/<tag>/` directories are never pruned. A crash during `stage` can also leave a `.staging-*` directory.
- The Windows supervisor does not update itself. A failed or hung `probation` (60 s) is replaced by the supervisor's own `start_attempt`, and `current` is started even when that fails. Real Windows rename and `remove_dir` on a directory symlink have not been run; only the Linux aside knob has.
- `install.sh` / `install-node.sh` are a second staging implementation (grep of the GitHub JSON, `sort -V`, curl). They are not callers of `pt-update`. They share the 100-release page and the uncapped curl download.

## Tests

No crate test was run. A full `protectorate` e2e build is a large compile on a shared 12-core machine, and the findings above are the source paths plus one curl check.

```text
curl -q --fail --silent --show-error --location --proto '=http' --max-filesize 1000 \
  --output /tmp/pt-upd-chunk-out http://127.0.0.1:<port>/x
```

against a loopback chunked 200000-byte body with no `Content-Length`: exit 0, file size 200000. That confirms finding 3. The rollback and probation behaviors are the ones the existing tests in `crates/protectorate/tests/update.rs` and `crates/pt-update/tests/fake_release.rs` already encode; they were read, not re-executed.

## Open questions

- Should `hub update --rollback` after a confirmed trial restore the snapshot (finding 1) or keep the migrated database on purpose? `operate.md` describes a restore only for the automatic path. The unsafe case is a trial that migrated headscale and still passed `GetHubInfo`.
- Should any non-Hello from the poller count as the SPEC 6a "hub contact", or only the update waiter? Finding 2 assumes the crash loop after a single Presence is not an acceptable reading of that sentence.
- GitHub asset CDN responses seen so far include `Content-Length`. Finding 3 is filed because the cap is not actually enforced for the HTTP the code accepts, not because a current `github.com` blob was observed to be chunked.
