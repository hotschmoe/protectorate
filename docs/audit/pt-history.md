# Audit: pt-history

Adversarial review of `pt-history` and the hub paths that feed it (`pt-hub` history ingest and `SearchHistory` / `ReadHistory`). No product code was changed. Findings are ordered by severity. Line numbers refer to this worktree (`pt/audit-history`, based on `next` at the time of the review).

Severity: P0 remote or unauthenticated compromise, P1 a sleeve (or other local caller) can read another tenant's session with a realistic step, P2 a real bug that needs a narrower precondition or is availability / spec drift, P3 defense in depth or a small leak.

## Findings

### H1. P1. Re-opening a log follows directory symlinks the sleeve controls

`safe_open::file` / `safe_open::directory` (`crates/pt-history/src/safe_open.rs`), called from `tail::open` and `discover` (`crates/pt-history/src/tail.rs`, `crates/pt-history/src/discover.rs`). The hub reaches this from `History::sleeve_pass` (`crates/pt-hub/src/history.rs`).

`O_NOFOLLOW` is applied only to the last path component. `directory()` does not set it at all (the comment says a configured root may be a symlink). Discovery then walks with `child()`, which does refuse a symlink, and `DirEntry::file_type` does not follow links, so a symlink **below** a real root is skipped. Two holes remain:

1. The log root the hub passes is `sleeve_home/.grok/sessions` (and the Claude, Codex, and Pi roots). That directory is inside the sleeve's private home. The sleeve can replace it with a symlink. `directory()` follows it, `discover` lists the target, and `tail::open` later opens `root/.../file` by path, which follows the same symlink.
2. After a real file has been chosen, the sleeve can replace a **parent directory** of that file with a symlink. The next `tail::open` follows it. `O_NOFOLLOW` only saves the final component, which is what `correctness::replacement_with_a_symlink_is_not_opened` swaps. Swapping the parent is not covered.

Grok logs make (2) reliable. The indexed file is always `chat_history.jsonl`. The session id is the directory name. Once the hub has recorded `.../<id>/chat_history.jsonl` (`Choice::Native` sticks for the process), replacing `<id>` with a symlink to any directory that contains `chat_history.jsonl` makes the next ingest read that file. A changed inode sets `Opened.reset`, so the cursor restarts at byte 0 and the new bytes are stored under the attacker's sleeve id (`Source.sleeve` is the caller's id, not a property of the file). `SearchHistory` / `ReadHistory` then return them, because the scope check looks at that sleeve id.

The hub process is outside the sandbox. `exec` masks the hub home and the owner's `.grok` with a tmpfs inside the sandbox (`crates/pt-hub/src/exec.rs`, `pt_sandbox::default_masks`). Those masks do not apply to the hub's own `open`. A sleeve that can name the target directory reads it anyway.

Claude, Codex, and Pi names include the session id, so a parent-directory swap only hits a file of that exact name. Symlinking the file itself still fails `O_NOFOLLOW`. Grok is the case where the final component is a fixed name.

Confirmed with a debug `pt-history-probe` (the same `discover` + `Index::ingest` the hub uses), `CARGO_TARGET_DIR=/tmp/pthist`:

- Log root is a symlink: `~/.grok/sessions` -> a directory tree holding `sess/chat_history.jsonl` with the text `SECRET-GROK-MARKER`. The probe reported `selected: 1 files`, `docs=1`, and `query "SECRET-GROK-MARKER": top 1` for session `sess`.
- Parent swap between discover and open: a real `.../cwd/mine/chat_history.jsonl` was discovered, then `mine` was replaced with a symlink to the victim session directory before ingest (debug `PT_TEST_HISTORY_DISCOVERY_BARRIER`). The original line was longer than the victim line so the probe's byte cap still covered it. Result: `docs=1`, `query "SECRET-GROK-MARKER": top 1` attributed to session `mine`, `query "minemarker": top 0`. The probe's own stdout withholds snippet text; the hit means the foreign bytes were tokenized into the index.

The hub does not need the barrier. It re-opens the chosen path about once a second (`history::POLL`).

Failure scenario: sleeve A runs Grok. It learns the path of sleeve B's session directory (the brief writes `grok session <native_id>` into the workspace; the cwd encoding is in the fixture as URL-escaped path bytes, and sleeve ids are small integers under `$HOME/../..`). A replaces its session directory with a symlink to B's. Within a second the hub re-indexes B's `chat_history.jsonl` as A's docs. A calls `history search` with its own token and reads B's prompts and tool traffic. The scope filter allows it.

Reproduction (local build only, debug probe; does not start the hub):

```text
export CARGO_TARGET_DIR=/tmp/pthist CARGO_BUILD_JOBS=4
cargo build -p pt-history --bin pt-history-probe
PROBE=/tmp/pthist/debug/pt-history-probe
BASE=$(mktemp -d)
mkdir -p "$BASE/victim/sess" "$BASE/sleeve/.grok/sessions/cwd/mine" "$BASE/index"
printf '%s\n' '{"type":"user","prompt_index":0,"content":"SECRET-GROK-MARKER"}' \
  > "$BASE/victim/sess/chat_history.jsonl"
python3 -c 'open("'"$BASE"'/sleeve/.grok/sessions/cwd/mine/chat_history.jsonl","w").write("{\"type\":\"user\",\"prompt_index\":0,\"content\":\"minemarker"+"x"*80+"\"}\n")'
BARRIER=$BASE/discovered
PT_TEST_HISTORY_DISCOVERY_BARRIER=$BARRIER \
  $PROBE --home "$BASE/sleeve" --index "$BASE/index" --runs 1 --query SECRET-GROK-MARKER &
# when $BARRIER appears:
rm -rf "$BASE/sleeve/.grok/sessions/cwd/mine"
ln -s "$BASE/victim/sess" "$BASE/sleeve/.grok/sessions/cwd/mine"
: > "$BARRIER.continue"
# expect: query "SECRET-GROK-MARKER": top 1, session mine
```

Root-symlink variant: `ln -s "$BASE/victim" "$BASE/sleeve2/.grok/sessions"` and run the probe with `--home "$BASE/sleeve2"`. Expect a hit for session `sess`.

Suggested fix: open every component under the sleeve home with `O_NOFOLLOW` on an `openat` walk from a directory fd, and ingest from that fd. Do not reopen by absolute path. Treat a symlink at the log root as absence (the hub root is not an owner-configured path; `directory()`'s follow rule does not belong here). On any inode change, keep refusing to follow a new parent. `operate.md` already says discovery and reopening ignore symbolic links below the root; reopening should do that for every component, not only the last.

### H2. P2. `history read` loads every matching doc, then applies the cap and the scope check

`Index::read` (`crates/pt-history/src/index.rs`, `Selector::Turns`) and `read_history` (`crates/pt-hub/src/api.rs`).

`ReadHistory` with a session and no `--to` asks for turns `0..=u32::MAX`. `Index::read` builds a `Vec` of every live doc in that range, each with its full text, and returns it. The handler then drops docs the caller cannot reach, and only then truncates to `HISTORY_READ_MAX` (200). A sleeve that can name a large session forces the hub to materialize it. Its own session is enough: the log lives in the sleeve home, lines up to `MAX_LINE` (8 MiB) become docs, and tool paths are not clipped (below).

`State::push_files` and `tool_call` (`crates/pt-history/src/extract/mod.rs`) cap tool **text** at `TOOL_MAX` (512) and other text at `TEXT_MAX` (32 KiB). The `files` vector keeps every path string from the line. One `apply_patch` or `file_path` inside an 8 MiB line is stored whole, returned on search (`HistoryDoc.files`) and on read, and counted into the segment. A few hundred such docs make one `ReadHistory` allocate on the order of a gigabyte. The HTTP body limit (4 MiB) bounds the request, not this response, and not the allocation that happens before the 200-doc cut.

Search is in better shape: it ranks first and loads text only for the capped hit list (`HISTORY_MAX` 100). It still copies the uncapped `files` list into each hit.

Failure scenario: a sleeve appends many Claude or Pi tool-use lines with multi-megabyte `file_path` strings to its session log, waits for the ingest thread, and calls `history read <session>`. The hub process, shared by every sleeve, holds the full vector. Naming another sleeve's session id does the same load and then returns `NotFound`, because the filter runs after `Index::read`.

Reproduction (local hub, or a throwaway that calls `Index::ingest` then `Index::read`; do not point it at a real home): write a few thousand JSONL user lines of a few kilobytes under a fixture session, ingest, `read` with `Selector::Turns { from: 0, to: u32::MAX }`, and observe the returned length before any cap. The existing e2e `history_search_scope_and_rebuild` reads one turn of a real Pi session and does not reach this.

Suggested fix: cap inside `Index::read` (a max doc count and a max total text bytes) and apply the sleeve check before copying text out, ideally as a filter the index can see. Clip or drop `files` entries with the same budget as tool text. Have `read_history` pass a bounded turn window and page with `more`, which the response type already has.

### H3. P2. One global path per native session id, and the cursor is per path

`History::sleeve_pass` (`crates/pt-hub/src/history.rs`) and `Index::ingest` (`crates/pt-history/src/index.rs`).

`Passes::choices` is a `HashMap` from native session id to a single `Choice`, for the whole hub, not per sleeve. The first pass that resolves an id stores the path. Every later sleeve with that same id ingests **that** path (`Choice::Native` is not recomputed). `source_key` is `kind + path`. One cursor, updated by whoever ingests. `rec.sleeve` is overwritten by the caller, and docs created in that call are stored under the caller's sleeve id. Sleeves are walked in `SleeveId` order, so the lower id reads new bytes first.

REVISIT already notes the related case: a session can be indexed twice when a later run finds a native log the transcript already covered, because the choice is not persisted. This is the cross-sleeve form. Isolation of the bytes depends on session ids never colliding and on no sleeve reporting another sleeve's id.

Pi makes the second part softer. `Rpc::State` in `crates/pt-drivers/src/pi.rs` emits `native_id` from `get_state`'s `sessionId`. On a new session it does not require that id to equal the `--session-id` the hub put on argv (the check runs only when `resume` is set, and then only `starts_with`). Claude pins the id in the driver and ignores a mismatched report for the id it records. A Pi that reports an id the sleeve arranged (a session file in the home, if Pi prefers it over the flag) selects the `choices` entry and therefore the file.

Failure scenario: sleeve 2's transcript gains a `session_info` whose `native_id` is sleeve 1's. Sleeve 1 already resolved `choices` to its log. On the next pass sleeve 2's source list uses sleeve 1's path with sleeve 2's sleeve tag. Bytes read in sleeve 2's ingest are searchable as sleeve 2. Sleeve 1's own later ingest sees the cursor at end of file and adds nothing, so those lines never appear under sleeve 1.

Reproduction: against a local hub, two sleeves whose transcripts contain the same `native_id` (a copied session id, or a Pi `get_state` that echoes a planted id) and a log at the first-resolved path. Search as the second sleeve for a phrase that exists only in the first sleeve's log. The stock e2e uses two Pi sleeves with distinct ids and does not hit this. I did not run that e2e.

Suggested fix: key the choice by `(sleeve id, native id)` and the source cursor by `(sleeve id, kind, path)`. Never open a path that `discover` did not return from that sleeve's own log root on that pass. For Pi, refuse a `sessionId` that does not match the id the hub passed.

### H4. P2. A sleeve can read its children's history. D26 says its own run only

`reaches`, `history_sleeves`, `read_history` (`crates/pt-hub/src/api.rs`). Method comment on `SearchHistory` / `ReadHistory` in `crates/pt-api/src/methods.rs`.

SPEC section 12 item 12 and D26: `history.read` covers Envoy, plus a continued sleeve for its own run; wider search is owner-only. `docs/operate.md` says the owner and Envoy can search everything and a sleeve only its own runs. The handler implements `reaches`: a sleeve credential matches itself and any sleeve whose `parent` is itself, including a child whose life is `Removed` (removed sleeves stay in the map). Envoy, owner, and device tokens are `full()` and skip the sleeve filter. That part matches "covers Envoy" and operate.md.

What D26 and operate.md do not allow: a parent sleeve receives its children's prompts, tool arguments, and failed tool output. Those docs are ordinary index rows tagged with the child id. `history_sleeves` fills an empty sleeve list with every reachable id, and a named child passes the `reaches` check. `read_history` allows the same set. The "continued" qualifier is also not enforced; any live sleeve can read at any time. operate.md agrees with "any time" and disagrees about children. The API comment documents the children rule, so this is spec drift with a security consequence, not an accident in one branch.

Vendor general agents are not in this set. `Scope::EnvoyOnly` cannot call a `Sleeve` method (`Scope::allows`), and `mcp --envoy` does not register history. REVISIT's note on that is accurate.

Failure scenario: a parent sleeve and a child that was given a task whose tool output contains a credential or another sleeve's diff. The parent calls `protectorate history search --sleeve <child> <phrase>` with `PROTECTORATE_TOKEN` set to its own capability. The call succeeds. The same call from a sibling returns "outside this capability".

Reproduction: the existing test `history_search_scope_and_rebuild` in `crates/protectorate/tests/history.rs` already shows a sleeve cannot see an unrelated sleeve. Extend that shape locally with a child spawned by alpha and assert alpha's token does, today, see the child's phrase. I did not run it (it needs a local Pi model and a hub build).

Suggested fix: decide which sentence is law. If D26 stands, `history_sleeves` and the read filter should allow only the caller's own sleeve id unless the principal is owner, device, or Envoy. If children are intended, change D26 and operate.md so the three texts match, and still exclude grandchildren (today a root does not reach a grandchild, because `reaches` checks only the direct parent).

### H5. P3. Empty sleeve filter means every sleeve, and names are an existence oracle

`history_sleeves` and `search_history` (`crates/pt-hub/src/api.rs`), `resolve` / `pass` (`crates/pt-history/src/search.rs`).

`Filters` treats an empty `sleeves` list as "any sleeve". The owner path returns `Ok(None)` from `history_sleeves`, and `unwrap_or_default()` turns that into the empty list on purpose. The same empty list is what a non-full principal gets if its reachable set is empty (`Ok(Some(vec![]))`). Search cannot tell the two apart. Today a live sleeve is still in the state map after `remove` (life becomes `Removed`), `from_token` rejects `Removed`, and `reaches` includes the sleeve itself, so the list is not empty. A later change that drops sleeves from the map would make an in-flight sleeve call search the whole index.

Separately, asking for a sleeve by name returns `NotFound` when the name is unknown and `Forbidden` when it exists but is outside the capability. A sleeve can probe which names exist. `indexed_docs` on every search response is the count of docs for all sleeves. `HistoryDoc.path` is the absolute log path. For the caller's own docs that path is under `$HOME`, which the sleeve already has. For a child's docs it reveals the hub layout.

Failure scenario: after a future cleanup that removes `Removed` sleeves from the map, a request that authenticated just before removal runs `search_history` with an empty sleeve filter and receives another sleeve's snippets.

Reproduction: not reachable on current `State::remove`. The oracle is: as a sleeve token, `history search --sleeve no-such-name` versus `--sleeve <a real other name>` and compare `NotFound` with `Forbidden`. Local hub only.

Suggested fix: use a tri-state on the filter (unrestricted / these ids / match nothing) and do not map "match nothing" through an empty `Vec`. Return the same `NotFound` for an unknown name and an unreachable name. Stop returning a global `indexed_docs` to sleeve callers, or count only the rows they can see.

### H6. P3. Segment growth is uncapped once merges no longer fit

`Index::merge` and `Index::load` (`crates/pt-history/src/index.rs`).

Merges stop when every adjacent pair is larger than `MAX_MERGE` (1 GiB), leaving more than `MAX_SEGMENTS` (8) files in place. Each later commit adds another segment. `load` does `fs::read` of the whole segment at open. A sleeve that can write a large session log (the log is in its home) drives that growth on the hub's disk, which is not the sleeve's quota. The measured ratio (1 GiB of logs to 11 MB of index) keeps ordinary logs small; the cap does not. A crash can also leave unreferenced `seg-*` files. REVISIT already records the leftovers and the missing sweep.

Failure scenario: a sleeve writes tens of gigabytes of unique short JSONL lines. The history directory fills the hub disk, or the next start reads several large segments into RAM and the hub is slow or killed.

Reproduction: point a local `Index` at a fixture of many small docs, commit past eight segments that each exceed half of `MAX_MERGE`, and watch `segs.len()` stick above 8 while new `seg-*` files appear. I did not generate a 1 GiB fixture.

Suggested fix: a max bytes for the index and a max bytes ingested per source per pass. Refuse further ingest from a source that exceeds it. Sweep unreferenced segment files at open, as REVISIT says.

## Checked and sound

- **Who may call.** `SearchHistory` and `ReadHistory` are `Scope::Sleeve`. Owner and device tokens pass. `EnvoyOnly` does not (`Scope::allows`). Anonymous does not. `crates/protectorate/src/mcp.rs` has no history methods. The sleeve socket binds a presented token to the caller's cgroup (`http::principal`); a token for another sleeve is refused there. I did not re-test the socket.
- **Sibling and parent reads through the API.** `reaches` is self or direct child. A sleeve cannot name a sibling or its parent and receive that row. `read_history` turns an out-of-scope doc into `NotFound` with the same text as a missing doc. The e2e `history_search_scope_and_rebuild` covers the unrelated-sleeve case for search, `--sleeve`, and `read --doc`. I did not re-run it.
- **Final-component symlinks and symlinks below a real root.** `discover` skips non-regular entries. `correctness::prompts_and_discovery_stay_inside_the_root` expects a file symlink and a directory symlink under `.claude/projects/...` not to be indexed. `replacement_with_a_symlink_is_not_opened` expects replacing the file itself, between discover and open, to fail the ingest. That matches `O_NOFOLLOW` on the last component. H1 is the parent and the root, which those tests do not swap.
- **Session id is not a path.** `discover::session_of` reads a filename component. Ingest does not `join` the id onto a directory. A `../` or absolute id in a log line does not select a file. Native extractors do not take the session id from the line; the hub sets it from `Source`. The hub transcript extractor updates the session only from `session_info` lines in the hub-written transcript (`sleeves/<id>/transcript.jsonl`, outside the home, covered by the hub-home mask).
- **Segment decoder.** `segment::decode` checks the magic, version, CRC-32C, section lengths, UTF-8, ascending ids and terms, string indexes, and posting bounds. `walk` rejects a doc index past `ndocs`. The only `expect` in the crate is in `search::matches` after a `find` that just succeeded, on the same segment. I did not find a panic on a hostile log line in the extractors (`fuzz_extract` asserts the text caps). A crafted segment can still ask for a huge string table; writing one requires write access to the history directory, which is under the hub home (mode `0700`, masked from sleeves).
- **Commit crash window.** A segment is written, fsynced, and renamed, then the manifest. A failure before the manifest rename restores the in-memory pending list and leaves the previous manifest. The index lock is `flock` and a second opener gets `WouldBlock`, which `open_index` does **not** treat as corruption. `PermissionDenied` is likewise left off rather than renamed aside.
- **Query parsing.** Quotes and `Mode::Phrase` stay inside the tokenizer. There is no shell. The body limit on the hub HTTP server bounds the query. BM25 divides by `df + 0.5` and clamps the average length to at least 1. `Snippet::marked` is used by the brief; the CLI uses `get` and skips a bad span. I did not find a slice panic in `snippet` for UTF-8 input (offsets are filled through `window.len()`).
- **Ingest memory.** The tail window is 16 MiB, pending text flushes around 96 MiB, and doc ids use `checked_add`. Overlong lines are skipped rather than stored. `parse_time` rejects out-of-range dates and negative instants.
- **Nodes.** No `pt-node` or `protectorate-node` caller indexes or serves history. Inputs are hub-local files plus the API.
- **Known shortcuts, not re-litigated.** REVISIT: Grok timestamps are the session start; Grok tool results are not indexed; Claude subagents are skipped; merges re-tokenize; corrupt indexes are renamed aside and kept; one source per session is chosen per process and not stored. Those match the code I read.

## Tests

- `CARGO_TARGET_DIR=/tmp/pthist CARGO_BUILD_JOBS=4 cargo build -p pt-history --bin pt-history-probe`  
  Finished debug build in 2.81 s.
- Manual probe runs described under H1, against `/tmp` fixtures, not a hub. Parent-directory swap and symlinked log root both indexed the foreign `chat_history.jsonl`.
- Did not run `cargo test -p protectorate --test history` or `cargo test -p pt-history --test correctness`. The history e2e needs a local Pi model and a hub. The correctness tests cover the final-component symlink, which is the case that already fails closed.

## Open questions

- D26 versus the API comment: should a parent sleeve see its children's history? The code and `methods.rs` say yes. SPEC and operate.md say a sleeve sees only its own runs. H4 should be closed by editing one of them, not by guessing.
- Are Grok session directory names treated as secrets? H1 needs that path. If briefs, logs, or shared worktrees routinely contain it, the precondition is weak.
- Pi: does `pi --session-id` always win over a session file already in the home? The driver accepts whatever `get_state` returns on a new session. I did not run Pi.
