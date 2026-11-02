# pt-quota audit

Date: 2026-11-02. Branch: `pt/audit-quota`. Crate: `crates/pt-quota`, plus the hub paths that feed it (`pt-hub` `quota.rs`, `select.rs`, `exec.rs`, `logins.rs`, `guard` in `pt-core`).

This is a read of the code. No product code was changed. The only test run was the local `pt-quota` probe e2e, with `live_logins_each_return_a_reading` skipped so nothing called a real vendor or the live hub.

Severity: P0 is a default-config compromise or an unprompted spend. P1 is a wrong resleeve, park, or spend on the normal hub path, or a sandbox boundary break. P2 needs a less common body or candidate list. P3 is hardening.

How a reading becomes a decision: the hub's quota thread calls `Cache::refresh` and publishes `Quota` rows. G1 does not read them. Spawn names the harness itself. On resleeve, `pt-core` `Core::select` (`guard.rs`) excludes the current harness when the cause is not manual, and `pt-hub` `select::select_target` calls `pt_quota::select_with_fallback` at task size Medium. `None` parks (`ParkReason::NoTarget`). A parse that returns `Err` becomes `QuotaReading::Error`. `Error::Unavailable` (no login file) becomes `Unknown`. Both, and `Stale`, fail `fresh_for`, so `availability` reports `Unknown` and the fallback may still pick that harness.

## Findings

### P1. A stale reading still carries its windows, and the fallback spends as if it had none

`crates/pt-quota/src/select.rs`: `availability`, `select_with_fallback`. Called from `crates/pt-hub/src/select.rs`: `select_target`.

`fresh_for` keeps a row only when `state` is `Fresh`. `Cache::quota` and `Quotas::list` leave the last windows in place when they flip the row to `Stale` (10 minutes after the last good fetch, or sooner under `PT_TEST_QUOTA_STALE_MS`). `availability` never looks at those windows. A row whose tightest window is at 99% used becomes `Unknown` once it is stale, and the fallback returns the first such candidate.

Failure: Claude's last good read is 99% on the 5h window. The next polls fail (network, or the timeout in the next finding). Ten minutes later the row is `Stale` and still says 99%. A G1 on Grok excludes Grok. `resleeve_to` is `[grok, claude]`. `select_target` skips the stale row. `select_with_fallback` returns Claude. The new incarnation spends an account the last real reading said was exhausted, trips G1 again, and burns a resleeve. The same path parks when every remaining candidate is a fresh reading under 15%: a stale 0% does not count as spare, so a subscription that still has room is skipped and the sleeve can park.

`select_target` itself refuses stale rows. Its fuzz target checks that. The hub does not call `select_target`; it calls `select_with_fallback`, which the fuzz target never calls. REVISIT's note that an unavailable reading may win describes a missing row. A stale row is a row the cache still holds.

Suggested fix: in `availability`, score the last windows of a `Stale` row. Below `min_usable_pct`, return `Exhausted`. Use `Unknown` only when the row has no scored window. Add that case to the select fuzz target.

Reproduction, local only:

1. `CARGO_TARGET_DIR=/tmp/ptq-audit CARGO_BUILD_JOBS=4 cargo test -p pt-quota --test probe -- --exact cache_uses_monotonic_time_for_ttl_backoff_and_age`
2. That test already shows a fresh Claude row aging to `stale` while the process stays up (`aged claude stale` in `pt-quota-probe` when `--test-cache-ms` is set). The probe prints the windows on that aged line and runs selection only on the pre-age rows, so it does not show the spend itself.
3. The spend is the hub call in `crates/pt-hub/src/select.rs`. Feed `select_with_fallback` a `Quota` with `reading: Stale`, one window `5h` at `99.0`, `resets_at_ms` in the future, and candidates `[Claude]`. The function returns Claude and the reason `without a fresh quota reading`.

### P1. A read that finishes after the deadline is thrown away

`crates/pt-quota/src/cache.rs`: `Cache::refresh`.

`refresh` waits `read_timeout` (45s) on a detached thread. On timeout it stores the receiver, counts a failure, and returns `Error::Timeout`. The next `refresh` does `try_recv` and drops the value:

```rust
Ok(_) | Err(mpsc::TryRecvError::Disconnected) => slot.pending = None,
```

A body that arrived at 46s never becomes `slot.good`. The timeout stays, `failures` grows, and `backoff` climbs to `backoff_max` (1 hour). `Grok::read` can make this likely: when `subscriptionTier` is empty it issues a second curl, and each curl is given the full timeout, while the cache deadline covers both.

Failure: the vendor is slow and the body says `used_percent: 100`. The hub keeps the previous fresh row (often "plenty left") until `stale_after`, then finding 1 treats it as unknown and resleeves onto that account. Or the previous row said exhausted, the new body says the window reset and usage is 0, and the hub keeps parking or avoiding that subscription for the backoff. A reader that is always slower than 45s never updates, even though every request succeeds.

The detached thread also cannot be cancelled. REVISIT already says a stuck file read holds one worker. This is the other half: a read that does finish is discarded.

Suggested fix: on `Ok(result)`, apply it the same way as a read that beat the deadline (replace `good` on `Ok`, count a failure on `Err`) and clear `pending`. Give every curl a deadline that is the time left in `read_timeout`, so Grok's settings call cannot eat the billing result.

Reproduction, local only:

1. Build `pt-quota-probe` with `CARGO_TARGET_DIR=/tmp/ptq-audit`.
2. Serve `http://127.0.0.1:<port>/api/oauth/usage` with a 50s delay and a small valid body (`five_hour.utilization` 100, `resets_at` tomorrow).
3. Write a fixture `.claude/.credentials.json` with a future `expiresAt` and run the probe with `--home` and `--claude-url http://127.0.0.1:<port>`.
4. The line is `claude error` and the note contains `timed out`. The server still completes. A second probe process does not see that body; the first process exits at the deadline and drops the worker with it. The discard of a late `Ok` is the `try_recv` arm above, which runs only when the `Cache` outlives the worker (the hub thread does).

### P1. A sleeve can aim the hub's quota request at a host it chooses, and attach a hub environment variable

`crates/pt-quota/src/local.rs`: `parse_provider`, `Local::read`. `crates/pt-quota/src/http.rs`: `send`. The writable path is `crates/pt-hub/src/logins.rs` (`models.json` is linked into every sleeve) and `crates/pt-hub/src/exec.rs` (`sb.rw_paths.extend(logins.dirs)`).

`apiKey` in Pi's `models.json` is either a `!command` (refused, and the e2e checks that no command runs), a literal, or the name of an environment variable. The lookup is `std::env::var` in the hub process. The sandbox is `--clearenv` plus a short allowlist (`pt-sandbox` `default_env_allow`), and `--proc /proc` with `--unshare-pid`, so a sleeve cannot read the hub's environ. The quota reader can.

When `local_url` is empty, the request URL is that provider's `baseUrl`. That is the intended setup (REVISIT: drop `local_url` once hubs leave it empty). Plain `http` is limited to loopback, and `localhost` is pinned to `127.0.0.1`. A sleeve already has the host network, so it binds `127.0.0.1:<port>`, writes `baseUrl` to that port and `apiKey` to the name of a hub variable, and the next poll (60s) GETs `/v1/models` with `Authorization: Bearer <value>`. `https` base URLs are not limited to loopback, so the same write can send the value off the machine. Curl is not passed `-L`, and tokens are not placed on argv. The leak is the request the hub makes on purpose.

The same body decides dispatch. Any non-empty model id list becomes a fresh `Local` row. `select_target` picks a fresh local row whenever no metered candidate has fresh spare. A sleeve-run server that returns `{"data":[{"id":"x"}]}` makes `pi_local` win over a paid harness whose reading is missing. The sleeve's own Pi process would use that `baseUrl` too; the extra effect is the hub preferring it, and the hub attaching a variable the sleeve cannot see.

`pi_local_provider` empty skips the file. The hole is the configured hub, which is when this reader runs.

Suggested fix: resolve `apiKey` only for names the owner listed, or only for a `PT_LOCAL_` prefix, and ignore `baseUrl` unless it is `local_url` or an owner-set allowlist. Keep treating a `!command` key as unavailable. Do not let a fresh local row outrank an `Unknown` paid candidate that is earlier in `resleeve_to` unless the owner asked for that order explicitly.

Reproduction, local only (this is what the e2e already does):

```
CARGO_TARGET_DIR=/tmp/ptq-audit CARGO_BUILD_JOBS=4 \
  cargo test -p pt-quota --test probe -- --exact local_reader_sends_the_pi_provider_key
```

The test writes `models.json` with `"apiKey":"PT_E2E_LOCAL_KEY"`, runs the probe with that variable set to `local-secret-key`, and the loopback server records `Bearer local-secret-key`. The output does not contain the key. The same test shows a wrong key becomes HTTP 401 and a `!cat ...` key stays `unknown` and does not run. Pointing `baseUrl` at a second listener is the same reader with the sleeve's path: the login store directory is the rw bind above.

### P2. A body that fails to parse is selected anyway

`crates/pt-quota/src/select.rs`: `availability`, `select_with_fallback`. `crates/pt-quota/src/cache.rs`: `Slot::quota`.

`parse_usage`, `parse_billing`, and `parse_models` fail the whole read on a missing window, a non-numeric percent, a negative percent, or an empty model list. That `Err` is stored as `Error`, which `availability` treats like a missing login. The fallback then picks it ahead of candidates that are fresh and exhausted.

The probe e2e locks this in. With Claude at 99% and Grok at 100%, a Codex body of `"used_percent":"n/a"` selects Codex:

```
target Codex: first eligible in resleeve_to without a fresh quota reading
```

REVISIT says an unavailable reading may win, and that known-exhausted candidates may not. A hostile or broken body is not an absent login. After a restart, or while `quota_poll_secs` is 0, every row is `Unknown` (`not read yet`) and the same fallback spends on the first eligible candidate. Readings are not journaled, so every hub start does this until the first refresh returns.

Suggested fix: let `Unknown` (no login, reader off, not read yet) take the fallback, and refuse `Error` the same way as `Exhausted`. Update `partial_responses_and_exhausted_selection` to match. Persisting the last good windows would also stop a restart from spending blind; REVISIT already lists that as optional.

Reproduction:

```
CARGO_TARGET_DIR=/tmp/ptq-audit CARGO_BUILD_JOBS=4 \
  cargo test -p pt-quota --test probe -- --exact partial_responses_and_exhausted_selection
```

Passed on 2026-11-02. It uses fixture logins and `127.0.0.1` only.

### P2. A reset time that has already passed hides the used percent

`crates/pt-quota/src/select.rs`: `score`. `crates/pt-quota/src/codex.rs`: `parse_usage`. `crates/pt-quota/src/time.rs`: `secs_to_ms`.

`score` skips a window with `resets_at_ms <= now_ms`. If every window is skipped, `score` returns `None` and `availability` returns `Unknown` even when `reading` is `Ok` and `used_pct` is 100. Codex stores `reset_at` of `0` as epoch ms `0` (`secs_to_ms(0)` is `Some(0)`), which is always in the past. A window that just elapsed is the same shape: the vendor can still be enforcing the old counter while the timestamp has passed.

Failure: Codex returns `used_percent: 100` and `reset_at: 0` (or a timestamp a few seconds behind the hub clock). The row is fresh, the window is dropped, and the fallback launches Codex. Claude is safer on a bad timestamp: an unparseable `resets_at` becomes "no reset" and the percent still counts. Grok's single credit window has the Codex shape, because `parse_billing` always sets the period end as the reset.

Suggested fix: a skipped past window with `used_pct` above the task minimum should still yield `Exhausted` until a later read shows the new period. Treat `reset_at <= 0` as "no reset", which already keeps the percent.

Reproduction: the probe's `--allow claude,codex,grok` path, with a fixture Codex login and a loopback `/wham/usage` of `used_percent` 100, `limit_window_seconds` 18000, `reset_at` 0. The Codex line is `fresh` and contains `[5h 100% reset passed]`. Selection returns Codex, because `score` dropped the only window. Do not point `--codex-url` at a real host.

### P2. One model-scoped Claude window exhausts the whole subscription

`crates/pt-quota/src/claude.rs`: `parse_usage`. `crates/pt-quota/src/select.rs`: `score`.

`usable` is the minimum remaining percent across every window, including `seven_day_sonnet`, `seven_day_opus`, and each `limits` entry of kind `weekly_scoped`. `select_target` rejects the subscription when that minimum is under the task size (15% for Medium, which is the only size the hub passes).

Failure: the 5h window is at 0% and the weekly Sonnet window is at 100%. The sleeve's work is not on Sonnet. Claude is `Exhausted`. If it is the only fresh candidate, the sleeve parks. If another subscription has any fresh spare, the hub resleeves onto it. The label is also taken from the vendor `display_name` (`wk <name>`). `window_length_ms` stops at the first token `wk`, so the name does not change the length, but a full scoped window still gates dispatch.

Suggested fix: gate on the unscoped windows (`5h`, `wk`, and for the others the single credit or rate-limit window). Keep scoped windows on the quota row for the UI.

Reproduction: fixture Claude login, loopback `/api/oauth/usage` with `five_hour.utilization` 0 and `seven_day_sonnet.utilization` 100, both `resets_at` in the future, `--allow claude,pi_local` and a local `/v1/models` that returns one id. The target line names `pi_local`. With `--allow claude` only, the target line is `none` and the reason is insufficient fresh quota.

### P2. PiVendor is chosen when its reading is missing, which it always is

`crates/pt-quota/src/select.rs`: `availability`. `crates/pt-hub/src/wire.rs`: `api_harness` maps `PiVendor` to `Harness::PiVendor`, and `subscription` maps it to `Subscription::Api`. There is no `Api` reader.

`SUBSCRIBED` is Claude, Codex, Grok. Anything else except `PiLocal` is `Unknown`. `select_target` never returns PiVendor. REVISIT says PiVendor and Antigravity are never selected. The hub calls `select_with_fallback`, so the first `Unknown` candidate wins once the metered ones are fresh and under 15%.

Failure: `resleeve_to` is `[claude, pi_vendor]`, Claude's 5h window is at 99%, PiVendor has no row that can ever be fresh. The fallback returns PiVendor and the hub launches it. That spends the API key with no quota window. Antigravity does not come through this hub path: `select::select_target` keeps headless targets only, and Antigravity has no headless mode. `PiDurable` is mapped to `Harness::PiLocal` before the call, so it shares the local row; that part matches the "shows as pi_local" note.

Suggested fix: `availability` should return a fourth state, or `Exhausted`-like "no meter", for a harness that is not `PiLocal` and not in `SUBSCRIBED`, and the fallback should skip it. If an API key is meant to be a backup, give it a reader or an owner-set budget and a fresh row.

Reproduction: the same `--allow` probe cannot name `pi_vendor` (the flag only accepts claude, codex, grok, pi_local). The hub function is `crates/pt-hub/src/select.rs` `select_target` with candidates whose harnesses are Claude then PiVendor, quotas a fresh Claude window at 99%, and `eligible` true for both. `pt_quota::select_with_fallback` returns `Harness::PiVendor`.

### P3. Login files are followed if they are symlinks

`crates/pt-quota/src/lib.rs`: `read_login`.

The open uses `O_NONBLOCK` and then `metadata().is_file()` on that fd, so a fifo does not block and a later replace of the path does not change what is read. The probe test `fifo_login_does_not_block_other_readers` covers the fifo. `O_NOFOLLOW` is not set. The login store is writable by every sleeve, so a sleeve can replace `auth.json`, `.credentials.json`, or `models.json` with a symlink to a path the hub can read and the sandbox has masked (`default_masks`: `.ssh`, `.config/gh`, the hub home, and the rest). The bytes are parsed, not copied into the error: `pt-json` errors are an offset plus a static message, and curl's status errors do not include the bearer. A file that happens to have the login shape is sent as the bearer to the fixed vendor URL, or, for `models.json`, used as in the P1 above.

Suggested fix: open with `O_NOFOLLOW` and reject a symlink. The sleeve can still write a regular file; the symlink is the extra reach into masked paths.

Reproduction: under a scratch `--home`, `ln -s /etc/hosts .claude/.credentials.json`, run `pt-quota-probe --home` that directory. The Claude line is `error` and the note is a JSON decode error. It returns in well under the 45s deadline. Do not point the symlink at a real credential file.

### P3. Codex window length is thrown away and parsed back from the label

`crates/pt-quota/src/codex.rs`: `parse_usage` (`window_label(secs / 60)`). `crates/pt-quota/src/select.rs`: `window_length_ms`, `projected_unused`.

`18000` seconds becomes `5h` and `604800` becomes `wk`, and those parse back. A length that is not a whole hour, day, or week becomes `Nm` or `window`, and `window_length_ms` has no minute unit, so it returns `None`. `projected_unused` then skips. `expiring` stays at the current remainder, which is higher than a pace projection would be, so a short window that is burning fast outranks a slower one with a similar remainder. `spare_trigger` is not called by the hub (D29 is deferred), so today this only affects `select_target` ranking. Projection also cannot raise spare above the current remainder (`p.min(remaining)`), so this does not invent quota that the percent does not show.

Suggested fix: store the vendor's window length on `QuotaWindow` and use that in `projected_unused`. Keep the label for display.

Reproduction: fixture Codex body with `limit_window_seconds: 300` and `used_percent: 50`. The probe line shows `[5m 50% ...]`. `window_length_ms("5m")` is `None`, so a call to `spare_quota` returns the current remainder (50) rather than a pace.

## Checked and sound

- Vendor URLs in the hub are the compiled defaults. `quota.rs` passes `base_url: None` for Claude, Codex, and Grok. A sleeve cannot retarget those curls by editing a file. It can replace the token in the shared login file; the next read then shows that token's account, which is the same account the sleeve would spend. The mismatch window is the TTL (5 min) plus the discard bug above.
- Curl is `Command` with a fixed argv: `-q` first, `--proto =http,https`, `--max-filesize` 1 MiB, no `-L`, no shell. URL and headers go on stdin. `config_quote` rejects control characters and escapes `\` and `"`. A login token or `account_id` with a newline or quote fails the read. `http://` is rejected unless the host is `localhost` or a loopback `IpAddr`, userinfo and `%` and `\` are rejected, and `localhost` is forced to `127.0.0.1` with `--resolve`. The e2e `local_endpoint_rejects_non_loopback_http_and_notes_an_unlisted_model` checks `http://192.0.2.1:1234`.
- An unlisted configured model is a note on a fresh row, not a failure. That matches the comment in `Local::read`. It does mean any model list at all is "reachable", which is the dispatch half of the P1.
- Percent parsers require a finite number `>= 0`. A string, a negative, or a non-numeric JSON value fails the whole snapshot (the e2e covers Claude `"n/a"`, Codex `-1`, and an empty Grok config). Values above 100 are stored and then clamped to 0% remaining in `score`. `pt-json` refuses nesting past 128 and does not echo the input. Login reads stop at 1 MiB. No panic path showed up in the parsers, `parse_rfc3339` (checked add and mul; years are four digits), the base64 walk (the bit count stays below 8 after each emit), or backoff (`shift` is at most 16, multiply saturates).
- `read_login` on a fifo returns `not a regular file` and the other readers still finish. Covered by `fifo_login_does_not_block_other_readers` (under 5s in the test).
- Cache age, TTL, and backoff use `Instant`, not the wall clock the vendor timestamps use. `cache_uses_monotonic_time_for_ttl_backoff_and_age` forces a second read with a wall clock a day earlier and still gets one fresh Claude row, one error for a bad Grok body, and an aged `stale` line. A wall-clock jump can still move `resets_at` across `now_ms` (finding on past resets).
- `select_target` (the pure one) only returns a harness that is allowed, not excluded, healthy if listed, and backed by a fresh row; local wins only when no subscription scored; ties keep Claude then Codex then Grok (`<=` does not replace). The fuzz target in `crates/pt-quota/fuzz/fuzz_targets/select.rs` checks those. It does not check `select_with_fallback`.
- One worker per slot. `refresh` returns when the scoped threads hit the deadline, so a stuck read does not freeze the quota thread. The hub publishes under the quotas mutex and does not hold it across curl.
- Secrets in probe output: the fixture and local-key tests assert the token and the key are absent from stdout. 401 and expiry are fixed strings.
- `!command` API keys are not executed.
- G1, spawn, and `spare_trigger` do not read these rows. A bad percent cannot by itself start a resleeve or start backlog work. It changes the target of a resleeve that G1 (or a manual resleeve) already started, or it parks when the fallback finds only fresh exhausted rows. `spare_trigger` is dead code until D29. `task_size::derive` is dead on the hub path: `pt-hub` always passes `TaskSize::Medium`, and the health slice is always empty. REVISIT already records those three, the 5 min / 10 min cache, the single stuck worker, and doctor checking that a login file is non-empty rather than calling these readers.
- `PiDurable` and `PiLocal` collapse to one API harness before selection, so they share the local row. That matches the REVISIT note that the experiment has no API harness of its own.

## Tests

```
CARGO_TARGET_DIR=/tmp/ptq-audit CARGO_BUILD_JOBS=4 \
  cargo test -p pt-quota --test probe -- --skip live_logins --test-threads=1
```

Result: 6 passed, 1 filtered out (`live_logins_each_return_a_reading`), 1.07s, debug, 2026-11-02. The six are the curl fixture, the Pi key and `!command` case, partial bodies and exhausted selection, non-loopback HTTP, the fifo login, and monotonic cache aging. No full `make test`, no fuzz run, no `protectorate` hub or node command.

## Open questions

- Is spending on an `Error` or `Unknown` row an accepted policy for every failure, or only for "no login file"? The e2e and REVISIT currently say every failure. Findings P2 "body that fails to parse" and P1 "stale windows" assume the owner wants the last real percent to keep blocking.
- Which variables are actually in the hub service environment? The models.json finding needs one that is not on the sandbox allowlist. A hub started with a clean environment has little to leak; a user unit that inherited `GH_TOKEN` or a provider key does.
- Should a model-scoped Claude cap block every Claude resleeve, or only work that names that model? The code does the first. Nothing in SPEC 8 says so.
