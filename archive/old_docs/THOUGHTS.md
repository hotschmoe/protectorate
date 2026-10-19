codesimplifier plugin (container? per agent?)

ralph wiggum loops

---

## Persistence Layer Evolution

Current: `internal/shared/stack/` - markdown files in `.stack/`

Future options:
- jsonl for high-frequency writes (message logs, detailed history)
- sqlite for queryable state (agent registry, cross-agent search)
- hybrid: keep markdown for human-readable (CURRENT.md) + structured storage for machine use

Abstraction path when needed:
```go
type Backend interface {
    ReadCurrent(workspace string) (*CurrentState, error)
    WriteCurrent(workspace string, state *CurrentState) error
    // etc.
}

type MarkdownBackend struct{}
type JSONLBackend struct{}
type SQLiteBackend struct{}
```

Leave as-is until we hit a real limitation.

---

## Sleeve Terminology

Consider renaming "container" to "sleeve" in user-facing places:
- "spin up a claude sleeve"
- "resleeve the agent" (restart with memory intact)
- API: POST /sleeves instead of POST /agents?

Keeps the Altered Carbon theme consistent. Internal code can still say "container" for Docker stuff.

---

## MCP vs Sidecar HTTP vs Hooks vs Skills

### What each thing actually is:

| Thing | What it does | When it runs | You configure it? |
|-------|--------------|--------------|-------------------|
| MCP Server | Exposes tools Claude can call | On-demand during conversation | Yes |
| Hooks | Scripts triggered by events | Session start/stop/etc. | Yes |
| Subagents | Parallel workers | Internally by CC | No |
| Skills | Prompt instructions | Read at conversation start | Yes (in claude.ai) |

### Sidecar HTTP vs MCP Server:

| Sidecar HTTP | MCP Server |
|--------------|------------|
| Any client can call it (curl, manager, other agents) | Only MCP-compatible clients |
| You define the API | Follows MCP spec |
| Simple, boring, works | Standardized, ecosystem benefits |

### Where MCP makes sense:

```
Agent Container
  Claude Code -----> MCP Servers
                       - GitHub (PRs, issues)
                       - Slack (send messages)
                       - PostgreSQL (shared DB)
                       - Cortical (agent status?)
```

### Recommendation:

Phase 1-4: Skip MCP. Sidecar HTTP is simpler, works with any AI CLI.

Phase 6+: Consider MCP for external integrations (GitHub, Slack, databases).
Well-maintained MCP servers exist for these - saves writing custom integrations.

Optional: Cortical MCP server to let agents query each other natively.
But OUTBOX.md -> manager routing works fine, just less elegant.

### External Communication Architecture Decision:

Option A - Agents have direct MCP access:
```
Agent --MCP--> GitHub (creates PR)
Agent --MCP--> Slack (sends message)
```

Option B - Manager as gateway:
```
Agent --OUTBOX--> Manager --API--> GitHub
Agent --OUTBOX--> Manager --API--> Slack
```

Option B is simpler, single audit point, easier to control.
Option A is more autonomous but harder to monitor/limit.

Leaning toward Option B for now - manager as the external gateway.

---

## Resleeving - CLI Hot-Swap

The killer feature: `cortical resleeve agent-xyz --cli codex-cli`

What happens:
1. Stop current container (claude-code)
2. Preserve workspace + `.stack/` entirely
3. Spawn new container with different CLI (codex-cli)
4. New CLI reads same CURRENT.md, PLAN.md, continues where old one left off

```
POST /agents/{id}/resleeve
{
  "cli": "codex-cli",
  "reason": "claude-code stuck in loop, trying fresh perspective"
}
```

Use cases:
- Stuck/looping agent -> resleeve with different model for fresh approach
- Cost optimization -> start cheap (haiku), escalate to expensive (opus) when blocked
- Capability matching -> Claude for reasoning, Codex for raw coding speed
- A/B testing -> same task, different CLIs, compare results
- Failover -> primary CLI down, resleeve to backup

The `.stack/` persistence makes this trivial - it's just container swap, state stays.

Could even auto-resleeve on detection of:
- Ralph Wiggum loops (agent repeating itself)
- Prolonged "blocked" status
- Token burn rate anomalies

---

## Warm Agent Pool + Alpine Optimization

### Alpine Images

Switch to Alpine-based containers to reduce overhead:
- Base image: ~5MB vs ~100MB+ for Debian/Ubuntu
- Faster pull times, lower memory footprint
- Trade-off: musl libc instead of glibc (usually fine for Go binaries + CLI tools)

```dockerfile
FROM alpine:3.19
RUN apk add --no-cache ca-certificates
COPY sidecar /usr/local/bin/
# ... install AI CLI
```

### Base Image Choice: Alpine vs Debian

**Recommendation: Start with Debian bookworm-slim**

| Factor | Alpine + musl | Debian slim + glibc |
|--------|---------------|---------------------|
| Base size | ~5MB | ~80MB |
| With Node.js added | ~150MB | ~180MB |
| Native module compat | Hit or miss | Just works |
| Debug time wasted | Higher | Lower |

Why Debian wins for this use case:
- Claude Code is Node.js - Anthropic tests on glibc, not musl
- npm packages often ship prebuilt glibc-only binaries
- The 30MB delta is noise once Node.js is in the image
- Zero time debugging "works on CI but not in container" issues

Alpine fallback path (if size becomes critical):
- `alpine:edge` + `gcompat` package for glibc compatibility shim
- Or accept musl and pin known-working versions of everything
- Only worth it if you're running dozens of containers and RAM is tight

```dockerfile
# Start here - boring and reliable
FROM debian:bookworm-slim
```

### Custom Base Image (cortical-base)

Build a shared base image with common dependencies pre-installed:

```
cortical-base:latest  (debian:bookworm-slim)
    |
    +-- cortical-claude:latest
    +-- cortical-opencode:latest
    +-- cortical-gemini:latest
```

What goes in cortical-base:
- Debian slim + common packages (git, curl, jq, bash)
- Sidecar binary (pre-compiled)
- Node.js runtime (needed by Claude Code, useful for others)
- Python 3 + uv (for tools that need it)
- Go toolchain (optional - for build-service or agent self-modification)
- Common config files, entrypoint scripts
- .stack/ directory structure

```dockerfile
# cortical-base.Dockerfile
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git curl jq bash \
    nodejs npm \
    python3 python3-pip python3-venv \
    && rm -rf /var/lib/apt/lists/*
COPY sidecar /usr/local/bin/sidecar
COPY entrypoint.sh /entrypoint.sh
RUN mkdir -p /workspace/.stack
ENTRYPOINT ["/entrypoint.sh"]
```

CLI-specific images just add their CLI:
```dockerfile
# cortical-claude.Dockerfile
FROM cortical-base:latest
RUN npm install -g @anthropic-ai/claude-code
```

Benefits:
- Faster CI builds (base layer cached)
- Consistent environment across all agents
- Single place to update shared deps
- Smaller per-CLI image deltas

### Warm Pool Strategy

Keep N agents pre-spawned and idle, ready for instant assignment:

```
Pool Configuration:
  - 1x Claude Code (claude-warm-1)
  - 1x OpenCode (opencode-warm-1)
  - 1x Gemini CLI (gemini-warm-1)
```

Benefits:
- Near-instant task assignment (skip container startup)
- Container startup is ~1-2s, warm pool makes it ~0ms
- Good for burst workloads or interactive use

Implementation:
```go
type WarmPool struct {
    mu       sync.Mutex
    ready    map[string][]*Agent  // cli -> available agents
    minReady map[string]int       // cli -> minimum pool size
}

// Claim takes an agent from pool, returns it for use
func (p *WarmPool) Claim(cli string) (*Agent, error)

// Release returns agent to pool (or kills if pool full)
func (p *WarmPool) Release(agent *Agent) error

// Replenish spawns new agents to maintain minimums
func (p *WarmPool) Replenish(ctx context.Context) error
```

Manager config addition:
```yaml
warm_pool:
  enabled: true
  agents:
    - cli: claude-code
      count: 1
    - cli: opencode
      count: 1
    - cli: gemini-cli
      count: 1
  replenish_interval: 30s
```

Open questions:
- How long before idle warm agents get recycled? (memory cost vs startup savings)
- Should warm agents have a generic workspace or workspace-per-task?
- Cost of keeping warm agents vs on-demand spawn

---

## Random Ideas

- "cortical-dashboard" could show real-time agent status, CURRENT.md previews
- agent-to-agent direct messaging (skip manager for speed?)
- cost tracking per agent (API usage)
- agent "personality" configs (aggressive vs conservative, verbose vs terse)
- "genetic algorithms" for prompts - spawn variations, keep winners
- agent "dreams" - background processing during idle time

---

## Arena - Agent Benchmarking System

Moved from ARENA.md design doc.

**Objective**: Standardized environment to benchmark and compare different AI CLI agents (Claude Code, OpenCode, Aider, etc.) on identical tasks.

### Core Concept

Arena spawns multiple "contestant" containers, each running a different agent config, all starting from the exact same state (file system + stack). Enables A/B testing, cost analysis, capability benchmarking.

### Use Cases

1. **Model Comparison**: Claude 3.7 Sonnet vs GPT-4o on same refactor task
2. **Tool Comparison**: Aider vs Claude Code for specific bug fix
3. **Cost Analysis**: Agent A solved in 5 mins ($0.50), Agent B in 2 mins ($1.20)
4. **Resilience Testing**: Start from "crashed" state, see which agent recovers best

### Challenge Definition

A "Challenge" = frozen workspace state (starting line):
- **Repo**: Git repository at specific commit
- **Stack**: `.stack/` directory with CURRENT.md, PLAN.md, INBOX.md
- **Scenario Types**:
  - Fresh Start: Empty stack (except INBOX.md with goal)
  - Mid-Flight Recovery: Stack populated from previous session (crashed/interrupted)

### Configuration Example

```json
{
  "challenge_id": "refactor-auth-module",
  "challenge_source": {
    "repo": "https://github.com/example/repo.git",
    "commit": "a1b2c3d4",
    "stack_snapshot": "./snapshots/auth-refactor-crash.tar.gz"
  },
  "contestants": [
    { "name": "claude-sonnet", "cli": "claude-code", "model": "sonnet-3.7" },
    { "name": "claude-opus", "cli": "claude-code", "model": "opus-3" },
    { "name": "aider-gpt4", "cli": "aider", "model": "gpt-4o" }
  ],
  "success_criteria": {
    "test_command": "go test ./auth/...",
    "timeout": "30m"
  }
}
```

### Execution Flow

1. **Preparation**: Create N isolated workspaces
2. **Hydration**: Clone repo, checkout commit, extract stack snapshot
3. **Spawn**: Container per contestant with mounted workspace
4. **Start**: Clock starts

### Metrics

- Time to success (wall-clock)
- Cost (API costs - tricky without proxy)
- Steps/interactions (LLM turns)
- Resource usage (CPU/memory)

### Verification

- Passive: Agent marks done in OUTBOX.md, manager runs success_criteria
- Timeout: DNF (Did Not Finish)
- Quality check: Optional linter/complexity analysis

### Implementation Notes

- `cortical snapshot <agent-id> <output-file>`: Zip `.stack/` for Mid-Flight scenarios
- Cost tracking hard without proxy - Phase 1 uses time + rough token estimates
- Phase 2: OpenAI-compatible proxy in Manager for exact cost calculation
- CLI: `cortical arena run <config.json>`
- Dashboard: "Arena" tab with progress bars and live logs

------

should stack journal per day and we back those up (or just live in git?) so we have a deep history and make a plugin or container that reviews those? 

