# Kovac Conversation: Sleeve-Requested Features

Kovac was an early sleeve with minimal context. When asked what tools it would want, these were the insights:

## Memory (.cstack) Improvements

**Structured format over raw logs:**
- `decisions/` - choices made and WHY (rationale is critical for future sleeves)
- `blockers/` - what stopped progress, what was tried (prevents retry loops)
- `context/` - project understanding, patterns learned
- `handoff.md` - always-current state for resleeving

**Key insight:** Memories are useless without search/tagging. Write triggers should auto-capture on:
- Task completion
- Errors encountered
- Before resleeve

## Communication (Needlecast) Needs

- **Async message queue** - don't block waiting for responses
- **Broadcast channel** - "found a bug in shared lib" announcements
- **Query endpoint** - "who's working on auth?" without interrupting others
- **Request-for-help protocol** - structured asks with context included

## Envoy Interface Gaps

- **Signal stuck** - explicit way to request resleeve or escalate (sleeve shouldn't have to guess when to give up)
- **Task clarity** - structured goal format, not prose (parseable objectives)
- **Resource manifest** - what sleeves exist, what repos they own, who to ask about what

## Resleeving Protocol

Critical requirements:
1. **Mandatory handoff write** before swap (no silent deaths)
2. **Incoming briefing read** required at boot (don't start blind)
3. **Hypothesis preservation** - next sleeve should know what previous sleeve was ABOUT TO TRY (not just what was done)

---

## Takeaways for Implementation

The core theme: **continuity matters more than capability**. A sleeve that can pick up exactly where another left off is more valuable than a sleeve with better tools but no context.

Second theme: **async over sync**. Sleeves should be able to work independently and communicate without blocking each other.

---

## beads_rust as .cstack Foundation

Evaluated https://github.com/Dicklesworthstone/beads_rust as a fork candidate for .cstack long-term memory.

### What beads_rust Provides

**Storage model:**
- SQLite primary (fast local queries) + JSONL export (git-friendly sync)
- Issue schema: title, description, design, acceptance_criteria, notes, status, priority, type
- Soft deletes (tombstones) - nothing ever truly lost
- Content hashing for deduplication and merge detection

**Structured data that maps to Kovac's requests:**

| Kovac Wanted | beads_rust Field/Feature |
|--------------|--------------------------|
| decisions + WHY | `close_reason`, comments with rationale |
| blockers / what was tried | `status: blocked`, dependencies, comments |
| context / patterns | `notes`, `design` fields, labels |
| handoff state | `br ready --json`, `br show <id> --json` |
| tagging | Labels (many-to-many strings) |
| search | Full-text over title/description/notes + filters |

**Query capabilities:**
```
br ready --json        # Unblocked, actionable work
br blocked --json      # What's stuck and why
br search "keyword"    # Full-text search
br list --label X      # Filter by tag
br show <id> --json    # Full issue details with deps/comments
```

**Dependency tracking:**
- `br dep add/remove/list/tree` - model what blocks what
- Cycle detection prevents circular dependencies
- `br ready` automatically excludes blocked issues

**Audit trail:**
- Events table tracks every change with actor + timestamp
- Comments preserve context and reasoning
- History backups in `.br_history/`

### Gaps to Address in Fork

1. **Hypothesis preservation** - beads tracks completed work, not "about to try X"
   - Solution: Convention to add comment before resleeve with next hypothesis
   - Or: Add `hypothesis` field to issue schema

2. **Auto-capture triggers** - No hooks for automatic writes
   - Solution: Sidecar wraps `br` calls, auto-comments on errors/completion
   - Or: Add hook system to forked version

3. **Prose context** - beads is task-focused, not narrative
   - Solution: Keep MEMORY.md alongside .beads/ for long-form context
   - beads handles tasks/state, MEMORY.md handles learnings/patterns

### Proposed .cstack Structure

```
.cstack/
  .beads/
    beads.db          # SQLite - fast local queries
    issues.jsonl      # Git-sync format
    config.yaml       # Sleeve-specific defaults
    .br_history/      # Timestamped backups
  MEMORY.md           # Long-form context, patterns, learnings
```

### Why Fork vs Use As-Is

beads_rust is designed for human developers. Sleeves need:
- Tighter integration with resleeve lifecycle
- Auto-capture hooks (errors, completion, resleeve events)
- Possibly simplified schema (sleeves don't need all 40+ commands)
- Hypothesis field as first-class citizen

The core storage model (SQLite + JSONL + content hashing) is solid. Fork to adapt the interface and add sleeve-specific conventions.

---

## beads_viewer as Analysis Layer

Evaluated https://github.com/Dicklesworthstone/beads_viewer as companion to beads_rust.

### What It Is

A TUI (terminal UI) written in Go that provides read-only analysis of beads data. While `br` handles CRUD, `bv` handles intelligence:

```
beads_rust (br)    - WRITE: create, update, close issues
beads_viewer (bv)  - READ: analyze, search, plan, triage
```

### Robot Protocol (Agent-First Design)

All `--robot-*` commands output deterministic JSON for agent consumption:

| Command | Purpose |
|---------|---------|
| `--robot-triage` | Combined analysis: quick wins, blockers, recommendations with reasoning |
| `--robot-plan` | Parallel work tracks respecting dependencies |
| `--robot-insights` | All 9 graph metrics as JSON |
| `--robot-search` | Semantic vector search results |
| `--robot-history` | Bead-to-commit correlations with confidence scores |
| `--robot-file-beads <path>` | Find all beads touching specific files |

### Graph Intelligence

Computes 9 metrics automatically:
1. **PageRank** - Recursive dependency importance
2. **Betweenness Centrality** - Bottleneck detection
3. **HITS (Hubs/Authorities)** - Distinguishes epics from utilities
4. **Critical Path** - Longest dependency chain, zero-slack keystones
5. **Eigenvector Centrality** - Influence via connected neighbors
6. **Degree Centrality** - Connection counts
7. **Density** - Edge-to-node ratio (project coupling)
8. **Cycle Detection** - Circular dependency errors
9. **Topological Sort** - Valid execution order

### How It Fills Kovac's Gaps

| Gap | beads_viewer Solution |
|-----|----------------------|
| Search/discovery of past decisions | `--robot-search` semantic search |
| Hypothesis preservation | History View correlates beads to commits |
| Pattern recognition | 9 graph metrics computed automatically |
| "What should I work on next?" | `--robot-triage` returns prioritized recommendations |
| Dependency visualization | `--robot-plan` generates parallel execution tracks |
| Bottleneck detection | Betweenness centrality identifies blocking issues |

### Sleeve Workflow with Both Tools

```
Boot:    bv --robot-triage     # Get prioritized work
Work:    br update/close       # Modify state
Stuck:   bv --robot-insights   # Find bottlenecks
Search:  bv --robot-search     # Discover past decisions
Handoff: bv --robot-history    # Correlate decisions with commits
```

---

## Unified Fork: cortical-stack

Rather than running two separate tools, fork both into a single unified CLI for sleeves.

### Merge Strategy

```
cortical-stack (cs)
  |
  +-- From beads_rust (Rust)
  |     - SQLite + JSONL storage model
  |     - Issue schema (simplified for sleeves)
  |     - CRUD operations
  |     - Dependency tracking
  |
  +-- From beads_viewer (Go)
  |     - Graph intelligence (PageRank, betweenness, etc.)
  |     - Robot protocol (JSON output)
  |     - Semantic search
  |     - History correlation
  |
  +-- New for Sleeves
        - hypothesis field (first-class)
        - resleeve hooks (auto-capture before swap)
        - sidecar integration (error capture)
        - simplified command set (sleeves don't need 40+ commands)
```

### Language Decision

Two options:
1. **Rust** - Rewrite viewer features in Rust, single binary
2. **Go** - Rewrite storage in Go, single binary

Go may be pragmatic: Protectorate is already Go, and viewer's graph algorithms are already implemented.

### Proposed Commands

```
cs init                    # Initialize .cstack/
cs task create "title"     # Create task
cs task close <id>         # Close with reason
cs task hypothesis <id>    # Record "about to try X"
cs triage                  # What should I work on? (JSON)
cs insights                # Graph metrics (JSON)
cs search "query"          # Semantic search (JSON)
cs history                 # Bead-to-commit correlation (JSON)
cs handoff                 # Pre-resleeve state dump
cs boot                    # Post-resleeve briefing
```

### .cstack Structure (Revised)

```
.cstack/
  stack.db            # SQLite - tasks, hypotheses, history
  stack.jsonl         # Git-sync format
  config.yaml         # Sleeve-specific defaults
  .history/           # Timestamped backups
  MEMORY.md           # Long-form prose context (optional)
```

The unified tool eliminates the read/write split and provides a sleeve-native interface.
