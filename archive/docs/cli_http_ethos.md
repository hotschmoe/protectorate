# CLI-First Architecture: HTTP Ethos

## Philosophy

Protectorate follows a **CLI-first, HTTP-backed** architecture where:

1. **Poe (the orchestrating LLM on envoy) controls everything via CLI**
2. **Web UI is for human observation and interaction with Poe**

```
+----------------+     CLI     +--------+     HTTP     +--------+
|      Poe       | ----------> | envoy  | <---------- | Web UI |
| (orchestrator) |             | daemon |             | (human)|
+----------------+             +--------+             +--------+
        |                          |                       |
        | envoy spawn alice        | /api/sleeves          | observe
        | envoy status             | /api/sleeves          | interact
        | envoy kill bob           | /api/sleeves/{name}   |
        v                          v                       v
```

## Why CLI over Direct API

This pattern is industry standard, used by Docker, Kubernetes, systemd, and most daemon-based tools:

```
docker ps          -> HTTP to dockerd
kubectl get pods   -> HTTP to kube-apiserver
systemctl status   -> D-Bus to systemd
envoy status       -> HTTP to envoy serve
```

### Benefits

1. **Single owner of mutable state** - The daemon owns all state; no races between CLI invocations
2. **Stateless CLI** - Each command is fresh, no cleanup needed, simple error handling
3. **Uniform API** - Web UI and CLI use identical HTTP endpoints
4. **Testable** - Mock HTTP server for CLI tests, no internal coupling
5. **Debuggable** - `curl` works for troubleshooting, logs show all operations
6. **Scriptable** - Poe can compose commands, pipe JSON output, build workflows

## Roles

### Poe (LLM on Envoy)

Poe is the orchestrating intelligence that runs on the envoy container. Poe:

- Spawns sleeves to work on tasks
- Monitors sleeve health and progress
- Routes work between sleeves
- Manages the workspace lifecycle
- Uses `envoy` CLI for all operations

```bash
# Poe's typical workflow
envoy status --json                    # Check current sleeves
envoy spawn /workspaces/myproject      # Start work
envoy doctor --json                    # Verify system health
envoy info alice --json                # Check sleeve progress
envoy kill alice                       # Clean up when done
```

### Web UI (Human Interface)

The web UI exists for humans to:

- Observe what Poe is doing
- Monitor sleeve health and resource usage
- Interact with Poe through the envoy terminal
- Debug issues when things go wrong
- Manually intervene when necessary

The web UI is **not** the primary control surface. It's a window into Poe's operations.

## Command Categories

### Sleeve Management (P0)
```
envoy serve                    # Start the daemon
envoy status [--json]          # List sleeves
envoy spawn <workspace>        # Spawn new sleeve
envoy kill <name>              # Terminate sleeve
envoy info <name>              # Detailed sleeve info
```

### System Health (P0)
```
envoy doctor [--json]          # System diagnostics
envoy stats [--json]           # Host resources
envoy auth                     # Auth status
```

### Workspace Management (P1)
```
envoy workspaces [--json]      # List workspaces
envoy clone <url>              # Clone repository
envoy branches <workspace>     # List branches
envoy checkout <ws> <branch>   # Switch branch
envoy fetch [workspace]        # Git fetch
envoy pull <workspace>         # Git pull
envoy push <workspace>         # Git push
```

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| CLI Framework | urfave/cli/v2 | Lightweight, good subcommands, Go idiomatic |
| Command Structure | Flat | Matches Docker pattern, fewer keystrokes |
| Output Format | Table + JSON flag | Human-readable default, machine-parseable option |
| Default Command | serve | Backwards compatible, natural for daemon |

## JSON Mode

All commands support `--json` for machine-readable output:

```bash
envoy status --json | jq '.[] | select(.status == "running")'
envoy doctor --json | jq '.[] | select(.status != "pass")'
```

This enables Poe to:
- Parse structured responses
- Build decision trees based on state
- Compose complex workflows

## Container Usage

Inside the envoy container, Poe uses the CLI directly:

```bash
# From inside envoy container
envoy status --json
envoy spawn /home/claude/workspaces/myproject --name alice
```

The `ENVOY_URL` environment variable (default: `http://localhost:7470`) works inside the container because envoy listens on all interfaces.

## Implementation Notes

- CLI is a thin wrapper - all business logic lives in `internal/envoy/`
- HTTP server remains the source of truth
- No state in CLI - each invocation is stateless
- Exit codes: 0 = success, 1 = error
- Errors go to stderr, output to stdout
