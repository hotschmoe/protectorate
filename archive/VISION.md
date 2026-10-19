# Protectorate - Vision

## What is Protectorate?

In the Altered Carbon universe, the Protectorate is the interstellar governing body that oversees humanity across settled worlds. It maintains order, regulates sleeve technology, and coordinates across vast distances.

This project applies that concept to AI agent orchestration: Protectorate is the system that manages, coordinates, and oversees AI agent containers (sleeves) while letting the AI tools themselves (the consciousness) remain untouched.

## Core Philosophy

### Native CLI Tools, Managed Orchestration

```
WE DO NOT:
  - Create new AI harnesses
  - Wrap or modify how Claude Code, Gemini CLI, etc. work internally
  - Fight against how these tools evolve

WE DO:
  - Let Anthropic, Google, OpenAI update and refine their CLI tools
  - Allow one person to manage DOZENS of these harnesses
  - Provide orchestration, memory persistence, and coordination
  - Defer to native solutions when they ship better alternatives

ANALOGY:
  - CLI tools (Claude Code, etc.) = Consciousness (DHF)
  - Our containers = Sleeves (bodies)
  - Our orchestration = The Protectorate (governing body)
  - We don't modify the consciousness, we manage the sleeves
```

### Fork and Maintain Strategy

All external tools we depend on will be:
1. **Forked** to our organization
2. **Maintained** as stable versions
3. **Updated** by pulling changes from upstream as needed
4. **Replaced** if the original tool ships something better

This protects us from upstream breaking changes, abandoned projects, API drift, and feature removal.

## Terminology

| Term | Meaning |
|------|---------|
| Protectorate | The framework/system (this repo) |
| Envoy | The manager process/CLI tool |
| Sleeve | Agent container (the body) |
| Cortical Stack | Agent memory format (.cstack/) - what the sleeve knows |
| Needlecast | Inter-sleeve communication (/needlecast/) - what sleeves say/hear |
| DHF | The AI consciousness (Claude, Gemini, etc.) |
| Resleeve | Switch CLI or respawn container |

## Architecture

```
                    +------------------------+
                    |        ENVOY           |
                    |    (Manager Process)   |
                    |  - Spawns sleeves      |
                    |  - Routes messages     |
                    |  - Health monitoring   |
                    +----------+-------------+
                               |
         +---------------------+---------------------+
         |                     |                     |
         v                     v                     v
  +------+--------+     +------+--------+     +------+--------+
  | Sleeve Alice  |     | Sleeve Bob    |     | Sleeve Carol  |
  | Claude Code   |     | Gemini CLI    |     | OpenCode      |
  | .cstack/      |     | .cstack/      |     | .cstack/      |
  +---------------+     +---------------+     +---------------+
```

### Key Components

**Envoy Manager**
- Runs as a container with Docker socket access
- Spawns and manages sleeve containers
- Routes messages between sleeves via needlecast (INBOX/OUTBOX)
- Provides CLI and API for human/LLM interaction
- Handles bootstrap (first-run setup) and normal operation

**Sleeves**
- Debian Bookworm Slim base
- Pre-installed: Python, Node, Bun, Rust, Zig
- Pre-installed: All AI CLI tools (Claude Code, Gemini, etc.)
- Sidecar binary for health/status reporting
- Mounted workspace with .cstack/ directory

**Sidecar**
- Lightweight HTTP server in each sleeve
- Exposes /health, /status, /outbox, /resleeve
- Parses .cstack/ files to report sleeve state
- Handles soft resleeve (CLI swap)

## Design Principles

1. **Containers as isolation** - Each sleeve provides security, resource limits, and failure domains

2. **Filesystem as database** - State in plain markdown files for debuggability and crash resilience

3. **Envoy polls sleeves** - Simpler than push, graceful degradation if sleeve unreachable

4. **Protocol over framework** - Simple HTTP APIs and file conventions that any AI CLI can follow

5. **One sleeve per stack** - No concurrent access to same .cstack/ directory (prevents corruption)

## Resleeve Operations

**Soft Resleeve** - Container stays alive, just swap the AI CLI process:
```
tmux session:
  [kill claude-code]
  [start gemini-cli]

Same workspace, same .cstack/, different consciousness
```

**Hard Resleeve** - Container destroyed, fresh container spawned:
```
docker kill agent-foo
docker run ... agent-foo

Clean process state, workspace volume persists
```

## What We Don't Do

- **No AI modifications** - We never modify or wrap AI CLI internals
- **No multi-sleeve per stack** - Concurrent access causes corruption
- **No framework lock-in** - cortical-stack works without protectorate
- **No backwards compatibility hacks** - When we refactor, we fully migrate
