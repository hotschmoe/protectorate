# Workspace Branch Management

This document explores how git branches should work with workspaces and sleeves in Protectorate.

## Current State

When a workspace is cloned:
- Default branch is checked out (usually `main` or `master`)
- No special branch handling occurs
- Sleeve mounts the workspace directory as-is
- Multiple sleeves could theoretically mount the same workspace (currently blocked by `in_use` check)

## Key Questions

1. Should each sleeve work on its own branch?
2. Should we use git worktrees instead of branches?
3. How do we handle branch switching while a sleeve is running?
4. What branch info should the WebUI display?
5. How do we handle uncommitted changes?

---

## Option 1: Simple Branch Display (Read-Only)

Show branch info in WebUI without management capabilities.

**WebUI Shows:**
- Current branch name
- Uncommitted changes indicator (dirty/clean)
- Ahead/behind origin count

**Implementation:**
```go
type WorkspaceGitInfo struct {
    Branch       string `json:"branch"`
    IsDirty      bool   `json:"is_dirty"`
    AheadOrigin  int    `json:"ahead_origin"`
    BehindOrigin int    `json:"behind_origin"`
}
```

**Pros:**
- Simple to implement
- No risk of disrupting running sleeves
- Informational only

**Cons:**
- Users must SSH into sleeve to manage branches
- No workflow integration

---

## Option 2: Branch Switching via WebUI

Allow branch switching from the Workspaces tab.

**Behavior:**
- Only allow switching when workspace is NOT in use
- Require clean working tree (no uncommitted changes)
- Fetch from origin before showing branch list

**WebUI Features:**
- Dropdown to select branch
- "Fetch" button to update remote refs
- Warning if uncommitted changes exist

**Pros:**
- Convenient for users
- Prevents conflicts (only switch when not in use)

**Cons:**
- Could lose work if user switches with stashed changes
- Branch list could be large for repos with many branches

---

## Option 3: Sleeve-Specific Branches (Auto-Created)

Automatically create a branch when spawning a sleeve.

**Behavior:**
```
Spawn sleeve "quell" on workspace "my-project"
  -> Creates branch: sleeve/quell/TIMESTAMP
  -> Checks out that branch
  -> Sleeve works in isolation
```

**Naming Convention:**
```
sleeve/{sleeve-name}/{timestamp}
sleeve/quell/20240115-103045
```

**Pros:**
- Complete isolation between sleeves
- Easy to track what each sleeve did
- Can merge sleeve work back to main

**Cons:**
- Branch proliferation
- Need cleanup strategy
- Merging becomes user's responsibility
- Loses context if user wanted to work on existing branch

---

## Option 4: Git Worktrees

Use git worktrees instead of separate clones.

**Concept:**
```
/workspaces/my-project/           <- bare repo or main worktree
/workspaces/my-project-quell/     <- worktree for sleeve quell
/workspaces/my-project-virginia/  <- worktree for sleeve virginia
```

**Behavior:**
- Clone creates main workspace
- Each sleeve spawn creates a worktree
- Worktrees share git objects (space efficient)
- Each worktree can be on different branch

**Pros:**
- Multiple sleeves can work on same repo simultaneously
- Space efficient (shared .git objects)
- Built-in git feature
- Each sleeve gets its own branch naturally

**Cons:**
- More complex to implement
- Worktree management overhead
- Users might not understand worktrees
- Naming/organization becomes important

---

## Option 5: Hybrid Approach

Combine simple display with optional sleeve branching.

**Default Mode:**
- Workspace stays on whatever branch user chose
- WebUI shows branch info (read-only)
- Single sleeve per workspace (current behavior)

**Isolation Mode (opt-in):**
- User enables "isolation mode" for workspace
- Spawning sleeve creates worktree + branch
- Multiple sleeves can work simultaneously
- Worktrees cleaned up when sleeve killed

**WebUI:**
```
Workspace: my-project
Branch: main
Status: Clean
[x] Enable isolation mode (creates worktree per sleeve)
```

---

## Option 6: Task-Based Branching

Integrate with task/issue tracking for branch naming.

**Behavior:**
- User provides task ID when spawning sleeve
- Branch created: `task/{task-id}/{sleeve-name}`
- Example: `task/PROJ-123/quell`

**Pros:**
- Meaningful branch names
- Easy to correlate work with tasks
- Good for PR workflows

**Cons:**
- Requires task ID input
- Assumes external task system

---

## WebUI Display Recommendations

### Workspaces Table Columns

| Name | Branch | Status | Sleeve | Actions |
|------|--------|--------|--------|---------|
| my-project | `main` | Clean | quell | [...] |
| cortical-stack | `feat/memory` | 2 uncommitted | - | [...] |
| protectorate | `dev-hotschmoe` | 3 ahead | virginia | [...] |

### Status Indicators

```
Clean           - No uncommitted changes
X uncommitted   - Has uncommitted changes (X = count)
X ahead         - Ahead of origin by X commits
X behind        - Behind origin by X commits
X ahead, Y behind - Diverged from origin
```

### Expanded Workspace View (click to expand)

```
my-project
+-- Branch: main
+-- Remote: origin/main
+-- Status: 2 files modified, 1 untracked
+-- Last commit: abc123 "feat: Add feature" (2 hours ago)
+-- Origin sync: Up to date
```

---

## Recommendation

**Phase 1: Read-Only Display (Implement First)**

Add git info to workspace listing without any management features.

```go
type WorkspaceInfo struct {
    Name       string            `json:"name"`
    Path       string            `json:"path"`
    InUse      bool              `json:"in_use"`
    SleeveName string            `json:"sleeve_name,omitempty"`
    Git        *WorkspaceGitInfo `json:"git,omitempty"`
}

type WorkspaceGitInfo struct {
    Branch         string `json:"branch"`
    RemoteBranch   string `json:"remote_branch,omitempty"`
    IsDirty        bool   `json:"is_dirty"`
    UncommittedCount int  `json:"uncommitted_count"`
    AheadCount     int    `json:"ahead_count"`
    BehindCount    int    `json:"behind_count"`
    LastCommit     string `json:"last_commit,omitempty"`
    LastCommitTime string `json:"last_commit_time,omitempty"`
}
```

**Reasoning:**
- Low risk, high value
- Users get visibility without complexity
- Foundation for future features
- No changes to sleeve workflow

**Phase 2: Branch Switching (When Not In Use)**

Add dropdown to switch branches when workspace is available.

**Reasoning:**
- Natural next step after display
- Safe because we only allow when not in use
- Covers 80% of use cases

**Phase 3: Worktree Mode (Future)**

Add opt-in worktree support for advanced users who need parallel work.

**Reasoning:**
- Enables powerful workflows
- Optional, doesn't complicate default experience
- Solves multi-sleeve collaboration

---

## What's Best For Each Actor

### Best for User
- **Visibility**: See branch, status, sync state at a glance
- **Safety**: Prevent operations that could lose work
- **Simplicity**: Don't require git expertise for basic operations
- **Power**: Advanced features available but not required

### Best for Envoy
- **Stateless**: Don't track git state in envoy, query on demand
- **Simple API**: Minimal git operations (status, checkout, fetch)
- **Error handling**: Clear errors when git operations fail
- **No magic**: Don't auto-commit, auto-push, or make assumptions

### Best for Sleeve
- **Predictability**: Workspace state is what user expects
- **Isolation**: Work doesn't interfere with other sleeves
- **Freedom**: Sleeve can make any git operations it needs
- **No surprises**: Envoy doesn't modify workspace while sleeve runs

---

## Git Commands for Implementation

```bash
# Get current branch
git -C /path/to/workspace rev-parse --abbrev-ref HEAD

# Check if dirty (has uncommitted changes)
git -C /path/to/workspace status --porcelain

# Count uncommitted files
git -C /path/to/workspace status --porcelain | wc -l

# Get ahead/behind counts
git -C /path/to/workspace rev-list --left-right --count origin/main...HEAD

# Get last commit info
git -C /path/to/workspace log -1 --format="%h %s (%cr)"

# List remote branches
git -C /path/to/workspace branch -r

# Fetch from origin
git -C /path/to/workspace fetch origin

# Switch branch (only when clean)
git -C /path/to/workspace checkout branch-name

# Create worktree
git -C /path/to/workspace worktree add ../workspace-sleeve-name branch-name
```

---

## Questions to Resolve

1. **Should we auto-fetch on workspace list?**
   - Pro: Always accurate sync status
   - Con: Slow, network dependent

2. **What if workspace isn't a git repo?**
   - Empty workspace created via "Create new"
   - User manually copied files
   - Solution: `git` field is null/omitted

3. **Should sleeve be able to request branch info via sidecar?**
   - Sleeve might want to know what branch it's on
   - Add `/git/status` endpoint to sidecar?

4. **How to handle detached HEAD state?**
   - User checked out specific commit
   - Show commit hash instead of branch name

5. **Should we support multiple remotes?**
   - Most repos only have `origin`
   - Start with origin-only, expand later if needed
