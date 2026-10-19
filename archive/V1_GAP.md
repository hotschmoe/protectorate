# V1 Implementation Gap Analysis

**Generated**: 2026-01-28
**Overall Status**: ~70% Complete (but architecturally inverted)

---

## Core Architecture Principle: CLI-First

**The current implementation is architecturally inverted.**

```
INTENDED (CLI-First):                CURRENT (Inverted):

  Human / LLM                          Human
      |                                  |
      v                                  v
  ENVOY CLI  <---->  HTTP API        HTTP API  <---->  Web UI
      |                  ^                ^
      v                  |                |
  Web UI ----------------+            (no CLI)
  (optional, for humans)
```

### Why CLI-First Matters

The envoy container runs an AI orchestrator called **Poe** (Claude Code or similar). Poe needs to:

1. **Spawn and manage sleeves** - delegate tasks to worker agents
2. **Route messages** - coordinate inter-sleeve communication
3. **Monitor health** - detect and fix problems
4. **Observe the entire stack** - full visibility into all sleeves

**Poe controls Protectorate via CLI commands.** If functionality only exists in the web UI, Poe can't use it.

### Interface Purposes

| Interface | Primary User | Purpose |
|-----------|--------------|---------|
| **CLI** | Poe (AI orchestrator) | Full control of Protectorate stack |
| **HTTP API** | Internal | Backend for both CLI and Web UI |
| **Web UI** | Humans | Observe sleeves, interact with Poe |

The web UI is for humans to watch what Poe is doing and occasionally intervene. It is NOT the primary control plane.

---

## Critical Gaps (Must Fix for V1)

| Gap | Status | Est. LOC | Priority |
|-----|--------|----------|----------|
| **CLI Interface** | 0% - No CLI exists | ~400-500 | **P0** |
| **Sidecar Binary** | 0% - `cmd/sidecar` missing | ~400-500 | P0 |
| **Needlecast Routing** | 5% - No backend | ~200-300 | P0 |
| **Polling Loop** | 0% - No background tasks | ~150-200 | P1 |
| **State Persistence** | 0% - In-memory only | ~100-150 | P1 |

**Total estimated**: ~1,250-1,650 LOC for minimum V1 compliance.

---

## CLI Commands Required for V1

Poe needs these commands to orchestrate sleeves:

```bash
# Sleeve lifecycle
envoy spawn --workspace foo --goal "implement feature X"
envoy status                    # List all sleeves
envoy status --json             # Machine-readable for Poe
envoy info <sleeve>             # Detailed sleeve info
envoy kill <sleeve>             # Terminate sleeve
envoy attach <sleeve>           # Connect to terminal

# Messaging (Needlecast)
envoy send <sleeve> "message"   # Direct message to sleeve
envoy inbox <sleeve>            # Read sleeve's inbox
envoy outbox <sleeve>           # Read sleeve's outbox
envoy broadcast "message"       # Message all sleeves (V2?)

# Observation
envoy logs <sleeve>             # View sleeve logs
envoy doctor                    # System health check

# Configuration
envoy config show               # Display current config
envoy workspaces                # List workspaces
envoy workspaces --json         # Machine-readable
```

### CLI Implementation Approach

The CLI should be a thin wrapper over the HTTP API:

```go
// cmd/envoy/main.go
func main() {
    app := &cli.App{
        Commands: []*cli.Command{
            {
                Name:  "spawn",
                Action: func(c *cli.Context) error {
                    // POST to http://localhost:7470/api/sleeves
                },
            },
            {
                Name:  "status",
                Action: func(c *cli.Context) error {
                    // GET http://localhost:7470/api/sleeves
                    // Format as table or JSON based on --json flag
                },
            },
            // ...
        },
    }
}
```

This keeps the HTTP API as the single source of truth while giving Poe (and humans) a proper CLI.

---

## Component Status

### Envoy Manager (75% Complete)

#### Implemented
- HTTP API on port 7470 (good foundation for CLI)
- Docker integration (container lifecycle, networks, exec)
- Sleeve management (spawn, kill, resource limits)
- Workspace management (git operations, cstack stats)
- Terminal WebSocket proxy
- Web UI (sleeves, workspaces, doctor tabs)

#### Missing
- [ ] **CLI layer** - wrap HTTP API with CLI commands
- [ ] Needlecast message routing backend
- [ ] Background polling loop
- [ ] State persistence (`~/.envoy/state.json`)
- [ ] Bootstrap mode / setup wizard
- [ ] Soft/hard resleeve endpoints

### Sidecar (0% Complete)

Entire implementation missing. Required:
- [ ] `cmd/sidecar/main.go`
- [ ] `internal/sidecar/` package
- [ ] HTTP API: `/health`, `/status`, `/outbox`, `/resleeve`

### Internal Packages

| Package | Status | Notes |
|---------|--------|-------|
| `internal/envoy/` | 75% | Missing CLI, polling, messaging |
| `internal/sidecar/` | 0% | Does not exist |
| `internal/cstack/` | 0% | Parsing helpers needed |
| `internal/needlecast/` | 0% | Message types and routing |

### Web UI (60% Complete)

The web UI exists but should be secondary to CLI. Current state:

| Tab | Status | CLI Equivalent Needed |
|-----|--------|----------------------|
| Sleeves | Functional | `envoy status`, `envoy spawn` |
| Workspaces | Functional | `envoy workspaces` |
| Doctor | Functional | `envoy doctor` |
| Needlecast | Placeholder | `envoy send`, `envoy inbox` |
| Logs | Placeholder | `envoy logs` |
| Config | Placeholder | `envoy config` |

---

## Architecture Notes

### Envoy Container Structure

```
ENVOY CONTAINER
+------------------------------------------+
|  Poe (Claude Code)                       |
|    |                                     |
|    | uses CLI                            |
|    v                                     |
|  envoy CLI  ---->  HTTP API (:7470)      |
|                         |                |
|                    Web UI (for humans)   |
+------------------------------------------+
         |
         | Docker socket
         v
    [sleeve containers]
```

Poe runs inside the envoy container and uses the `envoy` CLI to manage sleeves. Humans connect to the web UI to observe.

### Current Inversion Problem

Right now:
- Web UI exists and is functional
- No CLI exists
- Poe has no way to control the stack programmatically

This blocks the core use case of AI-driven orchestration.

---

## Priority Order for V1 Completion

### P0 - Architectural (Blocking)
1. **CLI Interface** - Poe needs this to control anything
2. **Sidecar** - Sleeves need to report status
3. **Needlecast routing** - Inter-sleeve communication

### P1 - Required
4. **Polling loop** - Background health checks
5. **State persistence** - Survive restarts

### P2 - Can Defer
6. **Bootstrap wizard** - Manual setup works
7. **Gitea integration** - External git works
8. **GitHub mirror** - Optional feature

---

## Restructuring Notes

The current code structure may need adjustment:

```
Current:
  cmd/envoy/main.go        -> starts HTTP server only

Needed:
  cmd/envoy/main.go        -> CLI with subcommands
  cmd/envoy/serve.go       -> "envoy serve" starts HTTP server
  cmd/envoy/spawn.go       -> "envoy spawn" calls API
  cmd/envoy/status.go      -> "envoy status" calls API
  ...
```

Or use a CLI framework like `urfave/cli` or `cobra` to handle subcommands cleanly.

The HTTP server becomes one mode (`envoy serve`) rather than the default.

---

## File Reference

Active V1 specs:
- `SPEC_V1.md` - Main specification (see "CLI-First Design" section)
- `SPEC_V1_ENVOY.md` - Envoy manager details
- `SPEC_V1_SLEEVE.md` - Sleeve/sidecar details
- `docs/SPEC_SIDECAR.md` - Sidecar API spec

Archived (see `old_docs/`):
- `SPEC_V1_convo_2026-01-21.md` - Design discussion
- `Q_TODO_NEXT.md` - CLI/container decision notes
