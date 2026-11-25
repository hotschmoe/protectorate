# Design documents: what is current

[SPEC.md](../../SPEC.md) is the design and wins over everything here. Most of these files were
written during the Rust and headscale era (2026-11-01 to 2026-11-18); the program is now one Zig
program with its own Noise IK fleet link. Read a file marked historical for its reasoning, not
for how the system works today. For the code, read [docs/srcmd/](../srcmd/README.md).

Status words: **current** (still the guidance), **partly current** (read with SPEC.md's
"Superseded" lines), **historical** (describes a design or code that no longer exists),
**proposal** (not adopted or not built).

## docs/design/

| File | Status |
|---|---|
| `tigerstyle-network.md` | current: the accepted direction for the fleet and HTTP network internals (its header still says "proposal, not adopted") |
| `sessions-chat-first.md` | partly current: chat first, terminal last; harness details are dated |
| `sleeves-state-machines.md` | partly current: state machines and guardrails; the Rust, tmux and pane details are historical |
| `sleeves-as-tasks.md` | partly current: the sleeve tree and mail rules, a 2026-11-03 draft that SPEC.md has not fully taken in |
| `history-continuation.md` | partly current: history tiers and the continuation brief; crate names and SQLite plans are historical |
| `nodes-ci-deliverables-verus.md` | partly current: presence, jobs, doctor and UI; the tailnet transport, deliverables and Verus are historical |
| `ci-and-release.md` | proposal (2026-11-18): not implemented, written for the Rust workspace |
| `network-trust-language.md` | historical: embedded headscale, tsnet and the Rust-and-Go language choice, all superseded by the Noise IK link |
| `network-without-go.md` | historical: a proposal and experiment that led away from Go |
| `network-rust-only.md` | historical: the Rust Noise proposal; the Zig program built the same idea |
| `architecture-cleanup.md` | historical: Rust crate cleanup; its linear-import idea became the L0 to L5 tiers |
| `survey-concurrency.md` | historical: a survey of the Rust workspace at one commit |
| `survey-data-layout.md` | historical: a data-layout survey of the Rust workspace |
| `survey-state-machines.md` | historical: a survey of the Rust workspace at one commit |
| `review-codex-2026-11-01.md` | historical: review of the first design; accepted fixes are in SPEC.md section 12a |
| `review-grok-2026-11-01.md` | historical: review of the first design; accepted fixes are in SPEC.md section 12b |

## docs/audit/

All three audit the Rust crates (`pt-history`, `pt-quota`, `pt-update`) and describe code that no
longer exists. Their findings are historical; check the Zig code (`src/history/`, `src/quota/`,
`src/update/`) before assuming one still applies.

| File | Status |
|---|---|
| `pt-history.md` | historical |
| `pt-quota.md` | historical |
| `pt-update.md` | historical |

## docs/spec-page/

| File | Status |
|---|---|
| `2026-11-01-next-spec.html` | historical: a snapshot of the claude.ai spec artifact, version 4, taken 2026-11-01 |

## docs/sessions/

| File | Status |
|---|---|
| `2026-11-01-redesign.md` | current as a record: the owner's own words, kept verbatim to check the design against |
| `2026-11-01-decisions.md` | current as a record: the owner's answers, which became SPEC.md section 13 |
| `2026-11-01-devlog.md` | historical: the log of the first Rust build |
