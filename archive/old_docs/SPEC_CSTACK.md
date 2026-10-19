# SPEC: Cortical Stack (Memory System)

## Summary

| Aspect | Decision |
|--------|----------|
| **Base** | Fork `beads_rust` (Rust) |
| **Intelligence** | Port from `beads_viewer` (Go -> Rust) |
| **Binary** | Single unified tool (~8-10 MB) |
| **Name** | `cortical-stack` (CLI: `cstack`) |
| **Standalone** | Yes - works without Protectorate |
| **Directory** | `.cstack/` |

## Overview

The Cortical Stack is the memory and task management system for Protectorate sleeves. Rather than building from scratch, we fork **beads_rust** and port graph intelligence from **beads_viewer** into a single unified Rust binary.

**Decision**: Fork beads_rust, port beads_viewer intelligence, single binary.

## Naming

| What | Name | Reason |
|------|------|--------|
| Repository | `cortical-stack` | Altered Carbon reference, our identity |
| CLI command | `cstack` | Unique, no conflicts, memorable |
| Data directory | `.cstack/` | Our identity, clean break from upstream |
| Issue IDs | `cs-xxxx` | Our prefix (configurable) |

We use `.cstack/` because:
- Clean identity separation from upstream beads
- Signals this is cortical-stack, not beads
- Avoids confusion if user has both tools installed
- We're forking and customizing heavily anyway


## Design Principles

### Standalone First

```
cortical-stack is a GENERAL-PURPOSE tool.
Anyone can use it for their projects.
It has ZERO Protectorate dependencies.

Protectorate-specific behavior (hypothesis auto-capture,
resleeve triggers, envoy polling) lives in the SIDECAR,
not in cortical-stack itself.
```

### Single Binary

```
ONE tool to install, ONE tool to learn.
No separate viewer, no daemon, no services.
~8-10 MB static binary with everything included.
```

### Non-Invasive

```
cortical-stack NEVER runs git automatically.
User controls when to sync, when to commit.
Explicit operations only.
```

---

## Decisions (Resolved)

### Q1: Go-based beads or Rust-based beads?

**Answer: beads_rust**

| Factor | beads_rust wins |
|--------|-----------------|
| Binary size | 5-8 MB vs 30+ MB |
| Startup | <50ms vs 200+ms |
| Memory | <30 MB vs 80+ MB |
| Philosophy | Non-invasive (explicit sync) |
| Complexity | 26K lines vs 206K lines |
| Extensibility | Easier to modify |

### Q2: Include beads_viewer intelligence?

**Answer: Yes, port to Rust and merge into single binary**

Rather than shipping two binaries (br + bv), we port the essential graph intelligence algorithms from beads_viewer (Go) into our beads_rust fork (Rust).

**Benefits**:
- Single binary (~8-10 MB vs 8 + 15 = 23 MB)
- One tool to learn
- Consistent UX
- Fewer dependencies in sleeve containers
- Full control over the codebase

**What we port**:
- Graph metrics (PageRank, betweenness, critical path, cycles)
- Triage scoring algorithm
- Parallel execution planning

**What we skip (V1)**:
- Semantic search (use basic text search)
- Git correlation (add in V2)
- Burndown/forecast (add in V2)
- TUI (not needed)

---

## User Workflow Vision

The cortical stack enables an autonomous agent workflow:

```
INTERVIEW --> SPEC --> BEADS --> AUTONOMOUS WORK --> COMPLETION
```

### Phase 1: Interview (Human + AI)

User starts with an idea, ethos, vision. AI conducts structured interview:
- 70-100 questions covering requirements, constraints, preferences
- Captures edge cases, error handling expectations
- Documents non-functional requirements (performance, security, etc.)
- Records user's mental model and terminology

Output: `INTERVIEW.md` - comprehensive requirements capture

### Phase 2: Specification (AI generates)

AI synthesizes interview into formal specification:
- Architecture overview
- Component breakdown
- API contracts
- Data models
- Integration points

Output: `SPEC.md` - detailed technical specification

### Phase 3: Beads Generation (AI decomposes)

AI breaks specification into atomic tasks:
- Each task is a single bead with clear acceptance criteria
- Dependencies explicitly modeled (blocks/blocked-by)
- Milestones defined as parent beads
- Testing milestones interspersed throughout
- Estimate: **300-600 beads** for a substantial project

Output: `.cstack/` populated with full task graph

### Phase 4: Autonomous Execution (Sleeves)

Envoy orchestrates sleeve work:
- Sleeve queries `cstack triage --robot` for prioritized recommendations
- Works task, updates bead status via `cstack update`
- Envoy monitors progress via sidecar `/status` endpoint
- Resleeve on stuck/error conditions
- Route questions to other sleeves or escalate to human

### Phase 5: Completion & Notification

Envoy notifies user when:
- Major milestone completed
- Critical blocker requiring human input
- Agent appears stuck (no progress on ready tasks)
- All beads completed

---

## Sleeve Integration

### Boot Sequence

```
1. Sidecar starts
2. Calls: cstack triage --robot
3. Presents top recommendation to AI CLI with reasoning
4. AI accepts task
5. Sidecar calls: cstack update <id> --status in_progress
```

### Work Loop

```
1. AI works on current task
2. On completion:
   - Sidecar calls: cstack close <id> --reason "summary"
   - Sidecar calls: cstack triage --robot
   - Presents next task
3. On blocker:
   - Sidecar calls: cstack update <id> --status blocked
   - Sidecar calls: cstack comment <id> "blocked by X"
   - Sidecar calls: cstack triage --robot (skip blocked, get next)
4. On error:
   - Sidecar captures context
   - May trigger resleeve
```

### Pre-Resleeve

```
1. Sidecar detects resleeve trigger (stuck, error, manual)
2. Auto-captures:
   - cstack hypothesis <id> "was attempting X approach"
   - cstack comment <id> "error: <context>"
3. Resleeve proceeds
```

### Post-Resleeve

```
1. New sleeve boots, sidecar reads .cstack/
2. Calls: cstack show <in_progress_id> --robot
3. Reads hypothesis field
4. Presents to AI CLI:
   "Previous sleeve was attempting: X
    Error encountered: Y
    Suggested next step: Z"
5. New sleeve continues or pivots
```

---

## Schema Extensions

### Hypothesis Field (New)

Added to beads_rust Issue struct:

```rust
pub struct Issue {
    // ... existing fields ...

    /// What the agent is ABOUT TO TRY, not just what was done.
    /// Critical for resleeve continuity.
    /// Set via: cstack hypothesis <id> "trying X approach"
    /// Cleared on close or via: cstack hypothesis <id> --clear
    pub hypothesis: Option<String>,
}
```

**SQLite schema addition**:
```sql
ALTER TABLE issues ADD COLUMN hypothesis TEXT;
```

**JSONL serialization**:
```json
{
  "id": "cs-a1b2",
  "title": "Implement auth",
  "status": "in_progress",
  "hypothesis": "Trying JWT approach with refresh tokens"
}
```

### Why Hypothesis Matters

```
Without hypothesis:
  Sleeve A works on auth, gets stuck, resleeved.
  Sleeve B sees: "auth task in_progress"
  Sleeve B has no idea what A was trying.
  Sleeve B may retry the same failed approach.

With hypothesis:
  Sleeve A records: "trying JWT with RS256"
  Sleeve A gets stuck, auto-captures: "error: key generation failed"
  Sleeve B sees: "was trying JWT with RS256, failed on key gen"
  Sleeve B tries different approach: "trying JWT with HS256"
```

---

## Needlecast Integration

cortical-stack handles TASKS. Needlecast handles MESSAGES.

```
.cstack/             # What to do (task graph)
.needlecast/        # What to say (inter-sleeve communication)
```

These are orthogonal systems:
- Task blocked on auth module -> `cstack update cs-a1b2 --status blocked`
- "Hey alice, I pushed auth changes" -> needlecast message

Needlecast is a separate spec (SPEC_NEEDLECAST.md).

---

## Research Results

### beads (Go) vs beads_rust Analysis

#### beads (Go) - Steve Yegge's Original

| Aspect | Details |
|--------|---------|
| **Size** | 206K lines of code, 461 Go files, 360 test files |
| **Commits** | 5,679 total, 4,603 in last 3 months (very active) |
| **Latest** | v0.49.0 (January 22, 2026) |
| **Binary** | ~30+ MB |
| **Architecture** | SQLite + JSONL + optional Dolt backend |
| **Schema** | 125+ issue fields, complex workflow states |
| **Features** | Daemon mode, Linear/Jira sync, Dolt federation, MCP server |
| **Philosophy** | Full-featured, enterprise-ready, automatic operations |

**Strengths**:
- Extremely active development
- Production-proven at scale
- Rich integrations (Linear, Jira, GitHub)
- Daemon for background sync
- MCP server integration

**Weaknesses**:
- Complex (125+ fields, 50+ commands)
- Large binary size
- Automatic git operations (may conflict with sleeve workflow)
- Heavy dependency chain (173 direct deps)

#### beads_rust - Dicklesworthstone's Port

| Aspect | Details |
|--------|---------|
| **Size** | 26K lines Rust, 37 command modules |
| **Stars** | 366 (stable niche adoption) |
| **Latest** | 0.1.8 (January 18, 2026), commits today |
| **Binary** | 5-8 MB (static, no GC) |
| **Architecture** | SQLite + JSONL (frozen on "classic" beads) |
| **Schema** | 30 core fields, clean subset |
| **Features** | All core CRUD, deps, labels, comments, sync |
| **Philosophy** | Non-invasive - NEVER runs git automatically |

**Strengths**:
- Small, fast binary (5-8 MB vs 30+ MB)
- Non-invasive philosophy (explicit sync, no hooks)
- No daemon to manage
- Simpler schema (30 fields vs 125)
- Comprehensive tests with CI/CD
- `#![forbid(unsafe_code)]` - memory safe
- Frozen on stable architecture (no Dolt/federation complexity)

**Weaknesses**:
- No Linear/Jira sync (out of scope)
- No daemon mode
- Smaller community
- Missing some advanced features (molecules, gates, slots)

#### Head-to-Head Comparison

| Criteria | beads (Go) | beads_rust | Winner |
|----------|-----------|------------|--------|
| Binary size | 30+ MB | 5-8 MB | **beads_rust** |
| Startup time | 200+ ms | <50 ms | **beads_rust** |
| Memory usage | 80+ MB | <30 MB | **beads_rust** |
| Feature count | 50+ commands | 37 commands | beads (Go) |
| Schema complexity | 125+ fields | 30 fields | **beads_rust** (simpler) |
| Git behavior | Auto-commits, hooks | Explicit sync only | **beads_rust** (non-invasive) |
| Maintenance | Very active | Active | Tie |
| Container-friendliness | Good | **Excellent** | **beads_rust** |
| Extension/fork effort | High (complex) | Lower (simpler) | **beads_rust** |

---

### beads_viewer Analysis

**Purpose**: Graph-aware AI agent interface built on top of beads.

| Aspect | Details |
|--------|---------|
| **Language** | Go 1.25+ |
| **Size** | 23 packages, 358 files |
| **Main entry** | 6,592 lines (monolithic dispatcher) |
| **TUI** | Charmbracelet (Bubble Tea) stack |
| **Dependencies** | gonum (graph), charmbracelet (TUI), zero external APIs |

#### Robot Protocol Commands (44+ commands)

**Core Triage**:
```
bv --robot-triage          # Mega-command: everything AI needs
bv --robot-next            # Single top recommendation
bv --robot-triage-by-track # Group by execution track
```

**Graph Intelligence**:
```
bv --robot-insights        # Full 9 metrics dashboard
bv --robot-graph           # DAG export (JSON/DOT/Mermaid)
bv --robot-plan            # Parallel execution tracks
```

**Search & History**:
```
bv --robot-search          # Semantic + hybrid search
bv --robot-history         # Bead-to-commit correlation
bv --robot-file-beads      # Beads touching a file
```

**Forecasting**:
```
bv --robot-burndown        # Sprint burndown
bv --robot-forecast        # ETA predictions
bv --robot-capacity        # Parallelization simulation
```

#### 9 Graph Metrics Computed

1. **PageRank** - Network importance (prestige)
2. **Betweenness Centrality** - Bottleneck detection
3. **Eigenvector Centrality** - Influencer detection
4. **HITS (Hubs/Authorities)** - Aggregators vs prerequisites
5. **Critical Path Score** - Longest path to terminal
6. **K-Core Decomposition** - Structural cohesion
7. **Articulation Points** - Single points of failure
8. **Slack** - Parallelizable work buffer
9. **Cycles** - Circular dependency detection

**Key Design**: Phase 1 metrics are instant. Phase 2 metrics are async with 500ms timeout. Status reporting tells agents which metrics are trustworthy.

#### Extractability Assessment: 8/10

- `pkg/analysis/` - Graph metrics, triage (fully extractable)
- `pkg/correlation/` - Git history linking (standalone)
- `pkg/search/` - Semantic search (standalone)
- `pkg/ui/` - TUI (can be removed for robot-only mode)

**Could be split into**: Library (metrics) + CLI (robot commands) + TUI (optional)

---

### Final Architecture

```
cortical-stack (cstack) - STANDALONE TOOL
|
|  Works anywhere. No Protectorate required.
|  Single ~8-10 MB static binary.
|
+-- Core Commands (from beads_rust)
|     cstack init
|     cstack create / update / close
|     cstack ready / list / search
|     cstack dep add / remove / tree / cycles
|     cstack sync (SQLite <-> JSONL)
|
+-- Intelligence Commands (ported from beads_viewer)
|     cstack triage [--json]      # Prioritized recommendations
|     cstack insights [--json]    # Graph metrics dashboard
|     cstack plan [--json]        # Parallel execution tracks
|
+-- Protectorate Extensions (optional, for sleeves)
      cstack hypothesis <id>      # Record what you're about to try
      --robot flag            # Machine-readable output


Protectorate Sidecar (SEPARATE, wraps cstack)
|
|  Protectorate-specific hooks live HERE, not in cstack.
|
+-- Auto-capture before resleeve
+-- Error context capture
+-- /status endpoint (calls cstack triage --json)
+-- /health endpoint
```

---

## Graph Intelligence (Ported from beads_viewer)

### Algorithms to Port

| Algorithm | Purpose | Rust Library | Effort |
|-----------|---------|--------------|--------|
| PageRank | Network importance | `petgraph` built-in | Low |
| Betweenness | Bottleneck detection | `petgraph` built-in | Low |
| Articulation Points | Single points of failure | `petgraph` built-in | Low |
| Cycle Detection | Circular deps | `petgraph` built-in | Low |
| Critical Path | Longest path to terminal | Topo sort + DP | Medium |
| HITS (Hubs/Auth) | Aggregators vs prereqs | ~100 lines iterative | Medium |
| Eigenvector | Influencer detection | `nalgebra` or custom | Medium |
| K-Core | Structural cohesion | ~80 lines | Medium |

**petgraph** covers 60% of what we need out of the box.

### Triage Scoring (from beads_viewer)

```
score = (
    pagerank_weight     * pagerank_score     +  # 0.35
    betweenness_weight  * betweenness_score  +  # 0.25
    blocker_weight      * blocker_count      +  # 0.20
    staleness_weight    * days_stale         +  # 0.10
    priority_weight     * priority_score        # 0.10
)
```

Each recommendation includes:
- **Score**: 0.0-1.0 (higher = more urgent)
- **Breakdown**: Component scores
- **Reasons[]**: Human-readable explanations
- **Unblocks[]**: What gets unblocked if this completes

### Output Modes

```
cstack triage              # Human-readable table
cstack triage --json       # Machine-readable JSON
cstack triage --robot      # Alias for --json (agent-friendly)
```

### Phased Metrics (from beads_viewer design)

**Phase 1 (instant, always available)**:
- Degree (in/out)
- Topological order
- Density
- Ready work count

**Phase 2 (async, 500ms timeout)**:
- PageRank
- Betweenness
- HITS
- Critical path

Status flags tell consumers which metrics are trustworthy:
```json
{
  "metrics": {
    "pagerank": { "status": "computed", "elapsed_ms": 45 },
    "betweenness": { "status": "timeout", "elapsed_ms": 500 }
  }
}
```

---

## Command Reference

### Core Commands (from beads_rust)

```bash
# Initialization
cstack init                          # Create .cstack/ in current directory

# Issue Lifecycle
cstack create "title" [--type bug|feature|task|epic]
cstack update <id> --status in_progress
cstack close <id> --reason "completed"
cstack delete <id>                   # Soft delete (tombstone)

# Querying
cstack list [--status open] [--priority 0-1] [--label X]
cstack ready                         # Unblocked, actionable work
cstack show <id>                     # Full issue details
cstack search "query"                # Text search

# Dependencies
cstack dep add <child> <parent>      # Child blocked by parent
cstack dep remove <child> <parent>
cstack dep tree <id>                 # Visualize dependency tree
cstack dep cycles                    # Detect circular dependencies

# Labels & Comments
cstack label add <id> <label>
cstack comment <id> "text"

# Sync
cstack sync                          # Bidirectional SQLite <-> JSONL
cstack sync --flush-only             # Export to JSONL
cstack sync --import-only            # Import from JSONL
```

### Intelligence Commands (ported from beads_viewer)

```bash
# Triage - "What should I work on?"
cstack triage [--json]               # Ranked recommendations with reasoning
cstack triage --top 5                # Limit to top N
cstack triage --label backend        # Filter by label

# Insights - "What's the state of the project?"
cstack insights [--json]             # Full metrics dashboard
cstack insights --metric pagerank    # Single metric

# Plan - "How can work be parallelized?"
cstack plan [--json]                 # Parallel execution tracks
cstack plan --agents 3               # Optimize for N parallel workers
```

### Protectorate Extensions (optional)

```bash
# Hypothesis - "What am I about to try?"
cstack hypothesis <id> "trying X approach"
cstack hypothesis <id> --clear

# Robot mode (for sidecar/agent consumption)
cstack ready --robot                 # JSON output, no colors
cstack triage --robot
```

---

## File Structure

```
project/
  .cstack/
    stack.db          # SQLite - fast local queries
    issues.jsonl      # Git-sync format
    config.yaml       # Project-specific settings
    .history/         # Timestamped backups (optional)
```

---

## Implementation Checklist

### Phase 1: Fork & Baseline (Week 1)

- [ ] Fork beads_rust to hotschmoe/cortical-stack
- [ ] Verify builds and passes all tests
- [ ] Document upstream commit hash
- [ ] Set up CI/CD (GitHub Actions)

### Phase 2: Schema Extensions (Week 1)

- [ ] Add `hypothesis` field to Issue struct
- [ ] Add `hypothesis` column to SQLite schema
- [ ] Add `cstack hypothesis <id> "text"` command
- [ ] Update JSONL serialization
- [ ] Write tests for hypothesis CRUD

### Phase 3: Graph Foundation (Week 2)

- [ ] Add `petgraph` dependency
- [ ] Create `src/graph/` module
- [ ] Implement dependency graph builder from issues
- [ ] Port cycle detection (verify against beads_viewer)
- [ ] Port articulation points
- [ ] Write graph tests

### Phase 4: Core Metrics (Week 2-3)

- [ ] Implement PageRank (use petgraph)
- [ ] Implement betweenness centrality (use petgraph)
- [ ] Implement critical path (topo sort + longest path)
- [ ] Add timeout handling for expensive metrics
- [ ] Write metric tests with known graphs

### Phase 5: Advanced Metrics (Week 3)

- [ ] Implement HITS (hubs/authorities)
- [ ] Implement eigenvector centrality
- [ ] Implement k-core decomposition
- [ ] Implement slack calculation
- [ ] Validate against beads_viewer output

### Phase 6: Triage & Intelligence Commands (Week 4)

- [ ] Implement triage scoring algorithm
- [ ] Add `cstack triage` command with --json output
- [ ] Add `cstack insights` command
- [ ] Add `cstack plan` command (parallel tracks)
- [ ] Add --robot flag for machine-readable output
- [ ] Write integration tests

### Phase 7: Polish & Documentation (Week 4)

- [ ] Update README for standalone use
- [ ] Document all commands
- [ ] Add examples for common workflows
- [ ] Performance benchmarks
- [ ] Release v0.1.0

### Phase 8: Protectorate Integration (Post-release)

- [ ] Sidecar calls `cstack triage --robot`
- [ ] Auto-capture: `cstack hypothesis` before resleeve
- [ ] Auto-capture: `cstack comment` on errors
- [ ] Envoy queries sleeve sidecars
- [ ] Envoy detects stuck sleeves

---

## Standalone vs Protectorate Usage

### Standalone Usage (Anyone)

```bash
# Initialize in any project
cd my-project
cstack init

# Create tasks from spec
cstack create "Implement auth" --type feature --priority 1
cstack create "Add login endpoint" --type task
cstack dep add cs-002 cs-001  # Login depends on auth

# Work on tasks
cstack triage                 # What should I work on?
cstack update cs-002 --status in_progress
# ... do work ...
cstack close cs-002 --reason "Implemented"

# Sync with git
cstack sync --flush-only
git add .cstack/
git commit -m "Complete login endpoint"
```

### Protectorate Usage (Sleeves)

The sidecar wraps cortical-stack with Protectorate-specific behavior:

```
Sidecar Hooks (not in cstack itself):

1. Boot:
   - Read .cstack/, call cstack triage --robot
   - Present top task to AI CLI

2. Work Loop:
   - Monitor AI CLI activity
   - On task claim: cstack update --status in_progress
   - On completion: cstack close, cstack triage for next

3. Pre-Resleeve:
   - Auto-call: cstack hypothesis <current> "was attempting X"
   - Auto-call: cstack comment <current> "error context..."

4. Post-Resleeve:
   - Read hypothesis from in_progress task
   - Present to new AI CLI: "Previous sleeve was trying X"
```

**Key Point**: cortical-stack knows nothing about sleeves, sidecars, or envoy. It's just a task tracker with graph intelligence. Protectorate wraps it.

---

## Comparison: Before vs After

### Before (Two Binaries)

```
Sleeve Container (~23 MB of task tools)
  +-- br (beads_rust): 8 MB
  +-- bv (beads_viewer): 15 MB
  +-- Different languages (Rust + Go)
  +-- Different maintainers
  +-- Schema compatibility issues
```

### After (Single Binary)

```
Sleeve Container (~10 MB of task tools)
  +-- cstack (cortical-stack): 8-10 MB
  +-- Pure Rust, single codebase
  +-- We control everything
  +-- Consistent UX and schemas
```

---

## Dependencies

### Rust Crates

| Crate | Purpose | Size Impact |
|-------|---------|-------------|
| `petgraph` | Graph algorithms | ~200 KB |
| `rusqlite` | SQLite (already in beads_rust) | - |
| `serde` | JSON serialization (already in beads_rust) | - |
| `clap` | CLI parsing (already in beads_rust) | - |
| `nalgebra` | Linear algebra (eigenvector, optional) | ~500 KB |

Estimated binary size: **8-10 MB** (vs 5-8 MB for beads_rust alone).

---

## References

- [beads_rust](https://github.com/Dicklesworthstone/beads_rust) - Our fork base
- [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) - Algorithm reference
- [beads (Go)](https://github.com/steveyegge/beads) - Original inspiration
- [petgraph](https://docs.rs/petgraph) - Rust graph library
- [Kovac Conversation](./kovac_convo.md) - Early sleeve feedback on memory needs
