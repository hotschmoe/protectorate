# Sleeves as tasks

Status: draft, 2026-11-03. Owner decisions taken the same day. Section 5 rests on a measurement
of Envoy's own session logs and a survey of the reference projects (2026-11-03).

This replaces the sleeve tree of `sleeves-state-machines.md` 1.2 (rules R1 to R6) and the
parent/child parts of SPEC's mail rules. The state machines in that document otherwise stand.

## 1. What changes and why

Today a directory gets a root sleeve, and parallel work hangs off it as child sleeves (depth 1,
at most 8) in worktrees. In practice:

- The root often exists only to hold the directory's write claim, so it is a placeholder.
- Envoy did the coordinating anyway: 26 spawns, 100 sends, 89 waits in its session logs.
- No sleeve has ever mailed a parent or a child. One sleeve mail exists in the whole journal (a
  test sleeve to Envoy).
- Envoy learned what sleeves did by reading their transcripts: 196 reads in 128 commands, each
  piped through tail, grep or awk to find the final message.

Idle sleeves cost no vendor tokens: subscriptions charge for tokens, and an idle headless
process holds no paid session. What costs is context reuse. A provider prompt cache lasts
minutes to an hour whether the process lives or not. A sleeve that idles past that and takes an
unrelated task re-sends all its earlier context, carries it on every later turn and compacts
sooner. Stopping the process saves memory, not tokens.

## 2. The model

- **Workspace** is a hub object: a directory with an exclusive write claim (R1's rule, unchanged:
  a claim may not equal, contain or lie inside another live claim). It is no longer a sleeve.
  Kinds: the directory itself, a git worktree of a git workspace (`$PROTECTORATE_HOME/worktrees/
  <workspace>/<task>`, branch `<workspace>/<task>`), or a read-only view (no claim).
- **Sleeve** is one task: a workspace, a harness and a brief. There is no parent and no child.
  Envoy, the owner and external general agents spawn sleeves; sleeves do not.
- **Inside a sleeve** the harness organizes its own help: native subagents, and other harness
  CLIs, which every sleeve already has with their logins (`logins::prepare_all` links all of
  them). Envoy may tell a grok sleeve to use codex subagents. The hub does not see those inner
  tokens in the sleeve's budget; they show only in that provider's quota row.
- **Envoy-dispatched tasks always get a worktree.** The workspace directory itself stays the
  owner's: the owner may run a sleeve there directly, one writer at a time. This keeps the
  owner's checkout clean and makes parallel tasks the default, not a special case.

## 3. Lifecycle

1. Spawn: workspace (worktree created from the workspace's HEAD), harness, brief.
2. Work, then the turn ends. The hub records the task's report (section 5) and Envoy's wait
   returns it.
3. Follow-ups go to the same sleeve while it is warm. After 20 minutes idle the hub stops its
   process; the sleeve stays resumable through the harness's native resume.
4. Merge (section 4), then removal of the sleeve and its worktree.

## 4. Merging a worktree task

Code does the mechanics; Envoy makes the call.

- `protectorate workspace merge <sleeve>` (hub method): refuses unless the sleeve is idle and its
  worktree is clean; rebases or fast-forwards the task branch onto the workspace's branch,
  refuses on conflict with the conflicting files listed, runs the workspace's check command when
  one is configured, and merges only if it passes. The result is a short fixed-format report
  (commits, diffstat, check result), not a transcript.
- Envoy decides whether to merge now, send the sleeve back with the failure, or spawn a review
  sleeve first. A conflict goes back to the task sleeve to rebase, since its context knows the
  change.
- Pushing stays outside this: protected-branch pushes keep their D9 approval rule.

## 5. Envoy's context

### 5.1 What Envoy spends

From Envoy's Claude Code session logs (408 requests over its life):

- Its context reached 584k tokens and never compacted. Cache reads were 136M tokens against 1.07M
  of cache writes and 248k of output, so cost is context size times the number of requests,
  not the size of any one result.
- 34% of requests only observed (a transcript, show or background-wait notification) and cost a
  third of all cache reads. 84 of 89 waits ran as background tasks because a foreground shell
  call stops at 10 minutes, so each wait costs a notification turn.
- Context growth: Envoy's own output 44% (thinking, briefs, and about 10% re-typing sleeve reports
  for whoever asked), transcript results 27.5%, other sleeve commands 5%, git, files, jobs and
  events 10%.
- 68% of transcript reads were "get the final report", often with `git log --stat`. In a sample,
  half the bytes read were the final message, 8% git output, 41% not needed. Test status always
  came from the sleeve's own claim.
- Restarts were hub updates while idle, two resleeves and one expired login, not context
  pressure. Each long-gap resume rewrote the cache (92k and 383k tokens, 44% of all cache writes).
- Envoy's instructions (crates/pt-hub/src/envoy.md: "wait, then read its transcript") drive the
  pattern.

### 5.2 Changes, in order

1. **The hub delivers a task report when a sleeve's turn ends.** Deterministic, no model:
   the final assistant message (capped at about 2 KB, with a pointer to the rest), stop reason,
   turn tokens, the turn's notices, commits since the recorded base, diffstat, the last test
   command the hub saw and its exit status (labelled as sleeve-claimed when the hub did not see
   it), and pending approvals. It arrives in Envoy as one queued message, not as a wait that
   needs a background task. `brief.rs` already builds most of these facts for resleeve briefs.
   The hub records each task's base commit at spawn (it does not today, which once gave Envoy a
   wrong `git log` range). An explicit sleeve report, when the sleeve sends one, takes the final
   message's place. Estimated: about 40% fewer cache reads.
2. **Envoy's context is kept small.** When Envoy is idle and its context passes a threshold
   (150k to 250k; simulated savings 60 to 73%), the hub starts a fresh Envoy session with a brief
   built from hub state: open tasks with their latest reports, pending approvals, the owner's
   recent asks. This is the resleeve path Envoy already has, aimed at itself; it uses the
   harness's own session start, so it stays within the hands-off rule. It needs item 1 first, so
   task state lives in the hub and not only in Envoy's memory.
3. **Forward without re-typing.** A hub method sends a sleeve's report (or a transcript range)
   to another sleeve or the owner verbatim, so Envoy relays by id instead of paraphrasing.
4. **Bounded transcript reads.** `sleeve transcript` gains `--last-turn`, `--kind`, a small
   default limit and a byte cap with a truncation marker and the next cursor. Full history stays
   reachable for investigation.
5. **`sleeve wait --any`** over several sleeves, returning the first report.
6. **Default caps** on `job ls` and `event list` (one `job ls` added 13k tokens).
7. **Envoy's instructions** change to "act on the report; read the transcript only when the report
   is not enough", shipped with items 1 and 4.

### 5.3 Mail

Mail stays for questions and blockers: a sleeve asks Envoy, or a sleeve Envoy names, and the
existing rate limit holds. Routine progress is not mailed, and a sleeve does not mail a copy of
its final report, since item 1 delivers it. Sibling mail is allowed when Envoy introduces the
pair (it names the peer in the brief), because each such message saves Envoy a relay; it counts
against the sender's limit and carries the D9 mark like any mail.

### 5.4 Measuring

The same measurement runs again after items 1 and 2, on comparable tasks: Envoy's input, cache
and output tokens per dispatched task, transcript reads, restarts. A smaller result does not
always mean a smaller bill (the rea project saw selected summaries cost more input tokens than
full results in its Codex trials), so the measure is tokens per task, not bytes per call.

## 6. What goes away (full migration, one change)

- Child spawn (`--parent`, `--readonly` children), the 8-child cap, depth rules, the stop and
  remove cascade, `Stopping(ParentStopped)`, child budgets counted against the root.
- Parent mail and its per-sender bound, `$PROTECTORATE_PARENT`, the "itself, its children, its
  parent" send rule. The sleeve-to-Envoy rate limit stays (per sleeve instead of per tree), and
  the D9 mark keeps crossing relays.
- Envoy's prompt sections about trees (crates/pt-hub/src/envoy.md).

## 7. Migration prerequisite: the journal

The hub replays its whole journal at start, and the live journal holds child spawns and their
events. Removing those from the reducer without decoding them is a replay failure, and keeping
decoders for them is the compat code we do not write. So the migration needs a cut first: a
journal snapshot (the reduced state written once, new events after it), which also bounds replay
time in general, or a planned fresh journal on this hub. Recommendation: build the snapshot,
since replay already freezes every reducer change.

## 8. Order of work

1. Task reports and the recorded base commit (5.2 item 1), with Envoy's instructions (item 7).
   They work with today's tree, so the biggest saving does not wait on the migration. Done
   2026-11-03 (SPEC "Turn reports"): reports are pushed as mail from the sleeve, not returned by
   `wait`.
2. Envoy's context ceiling (item 2), then measure (5.4). Owner decision 2026-11-03: 500k tokens
   at most. The harness may compact earlier on its own; when Envoy reaches 500k and its harness
   has not compacted, the hub sends the harness's own `/compact` (the existing Compact command),
   not a fresh session.
3. The journal snapshot (section 7).
4. Workspaces, task sleeves and the tree removal (sections 2, 3, 6), with `workspace merge`
   (section 4) and the 20-minute idle stop.
5. Forwarding, bounded transcript reads, `wait --any` and the default caps (items 3 to 6), in
   whatever order the second measurement points to.

## 9. Open questions

- The workspace check command: per workspace config, or found by convention (`make check`)?
- How long a finished, unmerged sleeve stays resumable before the hub asks Envoy to merge or drop
  it.
- Per-sleeve token budgets are on hold (owner, 2026-11-03): a sleeve is not sized or budgeted at
  spawn, and one that hits its quota resleeves as today.
