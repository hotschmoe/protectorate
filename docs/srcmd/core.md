# Core State Machine (`src/core/`)

All persistent orchestration state lives in `core/state.zig` (`State`) and advances strictly through stamped events (`event.StampedEvent`) applied by a single deterministic pure transition function `core/apply.apply(gpa, state, se, &intents)`.

## TigerStyle Design Rules

1. **Sequence Monotonicity**: Every accepted event must advance `state.event_seq + 1` in order. Any validation or state error restores `event_seq` via `errdefer` so failed applies leave zero partial mutation.
2. **Exclusive Workspace Claim**: No two non-removed, non-stopped, non-parked sleeves with `.worktree` layout may hold overlapping workspace paths. Checked uniformly across `.sleeve_created`, `.start_requested`, `.launched`, and `.resleeve_requested` via `State.active_claim_conflict`.
3. **Generation Fencing (`IncarnationState.g`)**: Every harness launch bumps `next_g`. Stale events (`session_bound`, `prompt_delivered`, `turn_started`, `turn_settled`, `exited`) carrying an outdated generation `g` are ignored safely; pending approvals from prior generations are automatically marked `.fenced`.
4. **Guardrail D9 & Rate Limits**: Agent-mail turns on Envoy set `d9 = true` so gated actions require explicit owner approval. Vendor mail has its own journaled message kind. A queued batch ends at a boundary between vendor mail and other messages, and vendor mail keeps its sender header in each prompt. `d9` clears on `.turn_settled`, `.stopped`, `.exited`, `.parked`, `.removed`, or rate-limit park (`G2` resleeve window, `G3` resume window, `G4` strike window).
5. **Typed Side-Effect Intents (`intent.Intent`)**: `apply()` performs zero I/O. It appends ordered `Intent` commands (`provision_worktree`, `launch_incarnation`, `deliver_prompt`, `interrupt_incarnation`, `kill_incarnation`, `respond_approval`, `write_brief`, `dispatch_job`, `verify_job`, `cancel_job`, `emit_notice`) which `hub/runtime.zig` executes outside the state lock.
