# V1 Specification Conversation - 2026-01-21

Running document of design decisions and discussions for Protectorate V1.

---

## Session Start

**Context**: Created SPEC_V1_ENVOY.md and SPEC_V1_SLEEVE.md. Updated SPEC_V1.md with needlecast terminology.

---

## Topic: Installation Strategy

**Question**: Install cstack/needlecast per container in repo OR on host with inheritance?

**Decision**: Hybrid approach
- **Data files** (CURRENT.md, MEMORY.md, inbox.md, outbox.md) live in project repo, git tracked
- **Hooks, skills, commands** baked into sleeve image OR mounted from host
- install.sh NEVER overwrites existing markdown files
- Format migrations handled separately with explicit migration scripts

**Rationale**: Similar to how git works - binary is system-wide, data is per-repo.

---

## Topic: Needlecast Structure (Resolved)

**V1 Structure** (single file):
```
/workspace/
  .needlecast/
    inbox.md      # Messages TO this sleeve
    outbox.md     # Messages FROM this sleeve
  .cstack/
    CURRENT.md
    PLAN.md
    MEMORY.md
```

**V2 Consideration** (file-per-message):
```
.needlecast/
  inbox/
    {timestamp}_message.md
  outbox/
    {timestamp}_message.md
  arena/
    {timestamp}_message.md
```

**Access patterns**:
- Envoy sees ALL sleeves' .cstack/ and .needlecast/
- Sleeves see only their own .cstack/.needlecast/
- Sleeves CANNOT see other sleeves' inbox/outbox
- (V2) Arena will be shared across sleeves

**Decisions**:
- Single file for V1 (simpler)
- Arena shelved for V2
- File-per-message shelved for V2

---

## Topic: Go Package for cstack/needlecast Parsing

**Proposed code**: User provided a `cstack` package with:
- `ParseCurrent()` - parse CURRENT.md
- `ParsePlan()` - parse PLAN.md
- `WriteInbox/WriteOutbox` - append messages
- `ReadInbox/ReadOutbox` - read messages
- `ClearOutbox` - truncate after routing
- `InitStack` - create directory and templates

**Analysis**:

### Issue 1: Mixed Concerns

The code puts INBOX/OUTBOX in `.cstack/` but we decided:
- `.cstack/` = memory (CURRENT.md, PLAN.md, MEMORY.md)
- `.needlecast/` = communication (inbox/, outbox/, arena/)

### Issue 2: Package Location

Options:
```
A) Separate repos (cortical-stack, needlecast)
   - Each has its own Go module
   - protectorate imports them
   - Pro: Reusable outside protectorate
   - Con: More repos to manage

B) Internal to protectorate
   - internal/cstack/ (memory parsing)
   - internal/needlecast/ (message parsing)
   - Pro: Simpler, co-evolves with protectorate
   - Con: Not reusable

C) Hybrid
   - Separate repos for SPEC + install scripts
   - Go code lives in protectorate
   - Pro: Spec is reusable, code is internal
```

### Recommendation: Option C (Hybrid)

```
cortical-stack repo:
  - SPEC.md (format specification)
  - install.sh (creates .cstack/ scaffolding)
  - migrate.sh (format migrations)
  - hooks/, skills/, commands/ (Claude Code integration)
  - NO Go code

needlecast repo:
  - SPEC.md (format specification)
  - install.sh (creates .needlecast/ scaffolding)
  - NO Go code

protectorate repo:
  - internal/cstack/parser.go (parse CURRENT.md, PLAN.md)
  - internal/needlecast/messages.go (read/write/route messages)
  - These are implementation details, not reusable libs
```

### Suggested Refactor of Proposed Code

Split into two packages:

```go
// internal/cstack/parser.go
package cstack

const StackDir = ".cstack"

type CurrentState struct {
    Status       string    // idle, working, blocked, done
    Task         string
    Progress     Progress
    Blockers     []string
    LastModified time.Time
}

type Progress struct {
    Total     int
    Completed int
}

func ParseCurrent(workspacePath string) (*CurrentState, error)
func ParsePlan(workspacePath string) (*PlanState, error)
// NO message functions here
```

```go
// internal/needlecast/messages.go
package needlecast

const NeedlecastDir = ".needlecast"

type Message struct {
    ID        string
    From      string
    To        string
    Thread    string
    Type      string
    Content   string
    Timestamp time.Time
}

func ReadInbox(workspacePath string) ([]Message, error)
func ReadOutbox(workspacePath string) ([]Message, error)
func WriteInbox(workspacePath string, msg Message) error
func WriteOutbox(workspacePath string, msg Message) error
func ClearOutbox(workspacePath string) error
// NO cstack parsing here
```

---

## Topic: Message Format

**Current format** (YAML-like frontmatter):
```markdown
---
ID: msg-123
From: alice
To: bob
Thread: auth-help
Type: question
Time: 2026-01-21T10:00:00Z
---
Message content here.
Can be multiple lines.

```

**Pros**:
- Human readable
- Git-friendly (diffable)
- Familiar frontmatter pattern

**Cons**:
- Parsing is more complex than JSON
- Easy to corrupt with bad formatting

**Alternative**: JSON Lines (one message per line)
```json
{"id":"msg-123","from":"alice","to":"bob","type":"question","content":"...","ts":"2026-01-21T10:00:00Z"}
```

**Pros**:
- Trivial to parse
- Append-only friendly
- No corruption risk

**Cons**:
- Not human readable
- Git diffs are ugly

**Decision**: **YAML frontmatter in markdown** for V1. Keep JSON lines as fallback if parsing becomes problematic.

---

## Topic: File-per-message vs Single File

**Option A**: Single file (current)
```
.needlecast/
  inbox.md      # All messages appended
  outbox.md     # All messages appended
```

**Option B**: File per message
```
.needlecast/
  inbox/
    2026-01-21T10-00-00_msg-123.md
    2026-01-21T10-05-00_msg-124.md
  outbox/
    2026-01-21T10-02-00_msg-125.md
```

**Tradeoffs**:

| Aspect | Single File | File Per Message |
|--------|-------------|------------------|
| Atomicity | Risk of corruption | Each file atomic |
| Git history | One file changes | Many small files |
| Clearing | Truncate | Delete files |
| Ordering | Parse order | Filename sort |
| Complexity | Simpler | More filesystem ops |

**Decision**: **Single file for V1** (inbox.md, outbox.md). File-per-message deferred to V2 if atomicity becomes an issue.

---

## Topic: Go Package Structure (Resolved)

**Decision**: Split into two internal packages:

```
protectorate/
  internal/
    cstack/
      types.go      # CurrentState, PlanState, Task, SleeveStatus
      parser.go     # ParseCurrent, ParsePlan, ExtractStatus
    needlecast/
      types.go      # Message
      messages.go   # ReadInbox, ReadOutbox, WriteInbox, ClearOutbox
```

**Rationale**:
- Separation of concerns (memory vs communication)
- Internal to protectorate (not reusable lib, co-evolves with system)
- Specs live in separate repos (cortical-stack, needlecast), Go code lives here

---

## Open Questions

1. ~~**Arena in V1?**~~ - RESOLVED: Shelved for V2

2. **Message persistence in git?** - Should cleared messages be preserved in git history or truly deleted?

3. **Sidecar polling vs inotify?** - Should sidecar watch files or poll on interval?

4. **Who clears outbox?** - Envoy after routing, or sidecar on command?

---

## Next Steps

1. ~~Finalize message format~~ - DONE: YAML frontmatter in markdown
2. ~~Finalize file structure~~ - DONE: Single file for V1
3. Implement internal/cstack and internal/needlecast packages
4. ~~Update specs with final needlecast paths~~ - DONE

---

## Decisions Log

| Decision | Choice | Date | Rationale |
|----------|--------|------|-----------|
| Sleeve terminal | tmux + ttyd | 2026-01-21 | Industry standard, web accessible |
| Envoy-Sleeve comm | Sidecar HTTP API | 2026-01-21 | Clean separation, language agnostic |
| User-Envoy comm | CLI-first | 2026-01-21 | Automatable by humans and LLMs |
| Memory format | cortical-stack (.cstack/) | 2026-01-21 | Separate repo, git tracked |
| Communication | needlecast (.needlecast/) | 2026-01-21 | Separate from memory |
| V1 CLI support | Claude Code only | 2026-01-21 | Simplify initial scope |
| Go code location | internal/ in protectorate | 2026-01-21 | Implementation detail, not lib |
| Message format | YAML frontmatter | 2026-01-21 | Human readable, git-friendly |
| File structure | Single file (V1) | 2026-01-21 | Simpler, file-per-message in V2 |
| Arena | Shelved for V2 | 2026-01-21 | Not needed for initial implementation |
| .needlecast location | In workspace (git tracked) | 2026-01-21 | Persists through resleeve, git history |
