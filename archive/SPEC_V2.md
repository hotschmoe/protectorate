# Protectorate - V2 Specification

## Overview

V2 extends to multi-machine deployment, advanced routing, and agent loop management.

## Multi-Machine Topology

### MASTER/SLAVE Architecture

```
                        +------------------+
                        |  MASTER ENVOY    |
                        |  (Primary host)  |
                        |  + GITEA         |
                        +--------+---------+
                                 |
              +------------------+------------------+
              |                  |                  |
              v                  v                  v
      +-------+------+   +------+-------+   +------+-------+
      | SLAVE ENVOY  |   | SLAVE ENVOY  |   | SLAVE ENVOY  |
      | (Host B)     |   | (Host C)     |   | (Host D)     |
      |              |   |              |   |              |
      | Sleeves 1-5  |   | Sleeves 6-10 |   | Sleeves 11-15|
      +--------------+   +--------------+   +--------------+
```

### Domain-Driven Addressing

- Master: `cortical.hotschmoe.com`
- Gitea: `gitea.hotschmoe.com`
- Slaves: Referenced by master, not directly addressed

### Setup Wizard Options

```
1) Start Cortical (become MASTER)
2) Join Cortical (become SLAVE)
```

### Responsibilities

**Master:**
- Run Gitea (or delegate to dedicated host)
- Coordinate all managers
- Route cross-host messages
- Central configuration
- Mirror to GitHub

**Slave:**
- Spawn sleeves on local host only
- Report to master
- Execute master's commands
- Local Docker socket access only

## Reverse Proxy (Traefik)

### Service Discovery

```yaml
labels:
  - "traefik.enable=true"
  - "traefik.http.routers.sleeve-${NAME}.rule=Host(`${NAME}.cortical.local`)"
  - "traefik.http.services.sleeve-${NAME}.loadbalancer.server.port=8080"
```

### Routing

```
manager.cortical.local -> envoy:7470
gitea.cortical.local -> gitea:3000
sleeve-alice.cortical.local -> sleeve-alice:8080
```

### SSL Termination

- Let's Encrypt for public domains
- Self-signed for local/private deployments

## Ralphing (Agent Loops)

Reference: https://ghuntley.com/loop/

### Loop Safety Model

```
+--------------------------------------------------+
|  AGENT LOOP                                       |
|  +--------------------------------------------+  |
|  |  Iteration 1 -> Iteration 2 -> ...         |  |
|  |       ^              ^                      |  |
|  |       |              |                      |  |
|  +-------|--------------|---------------------+  |
|          |              |                        |
|     OBSERVATION     INTERVENTION                 |
|     (read-only)     (if needed)                  |
|          |              |                        |
|          v              v                        |
|  +--------------------------------------------+  |
|  |  ENVOY MANAGER / USER INTERFACE            |  |
|  |  - View loop progress                      |  |
|  |  - See iteration outputs                   |  |
|  |  - Inject guidance                         |  |
|  |  - Pause/resume loop                       |  |
|  |  - Set guardrails                          |  |
|  |  - KILL SWITCH                             |  |
|  +--------------------------------------------+  |
+--------------------------------------------------+
```

### Guardrails

- Max iterations per loop
- Cost/token budget
- Time limits
- Human checkpoint intervals
- Sandboxed execution environment
- Automatic pause on anomaly detection

### Research Required

Before implementing:
- [ ] Deep dive https://ghuntley.com/loop/
- [ ] Identify patterns applicable to our architecture
- [ ] Define "safe" loop execution model
- [ ] Design loop state tracking in .cstack/
- [ ] Design manager observation/intervention APIs
- [ ] Define cost/resource monitoring approach
- [ ] Document failure modes and recovery

## Shared Arena

```
/shared/arena/
  - Global broadcast messages from manager
  - Shared knowledge base / context
  - Cross-sleeve announcements
  - Manager moderates all writes
```

Mounted in V1 but unused until V2.

## Messaging Integration

Priority: Telegram or self-hosted (easiest first)

Features:
- Notifications on milestones
- Remote commands via chat
- Status updates

## Advanced Memory (Beads Integration)

If cortical-stack moves to V2 with beads:
- Dependency-aware tasks
- Semantic compaction
- Better search

## Container Optimization

### Warm Container Pool

Pre-spawned containers waiting for assignment:

```
Pool: [ready-1, ready-2, ready-3]

Spawn request -> Claim from pool -> Start AI CLI
                       |
                       v
              Pool refills in background
```

Reduces spawn latency significantly.

### Per-CLI Images

If single image becomes unwieldy:

```
protectorate-sleeve-claude:latest
protectorate-sleeve-gemini:latest
protectorate-sleeve-opencode:latest
```

### Resource-Aware Scheduling

Assign sleeves to hosts based on:
- Available CPU/memory
- GPU availability
- Network locality

## Deployment Features

### Agent-Driven Production Deployment

Agents can deploy their work:

```
POST /deploy
{
  "repo": "my-app",
  "environment": "staging",
  "strategy": "canary"
}
```

With appropriate guardrails and approval flows.

### CI/CD Integration

- GitHub Actions triggers
- Webhook receivers
- Status reporting back to PRs

## Remote Build (rch Integration)

Fork: github.com/Dicklesworthstone/remote_compilation_helper

Dedicated build containers for compilation-heavy tasks:
- Cross-platform builds
- Faster compilation
- Build cache sharing

## V2 API Extensions

```
# Multi-machine
GET  /cluster/status          # Cluster health
POST /cluster/join            # Join as slave
GET  /hosts                   # List hosts
GET  /hosts/{id}/sleeves      # Sleeves on host

# Loops
POST /sleeves/{id}/loop/start # Start loop
POST /sleeves/{id}/loop/pause # Pause loop
POST /sleeves/{id}/loop/stop  # Stop loop
GET  /sleeves/{id}/loop/status # Loop status

# Arena
POST /arena/broadcast         # Broadcast message
GET  /arena/messages          # Read arena

# Deployment
POST /deploy                  # Trigger deployment
GET  /deployments             # List deployments
GET  /deployments/{id}        # Deployment status
```

## Migration Path

V1 deployments upgrade to V2:
1. Update envoy binary
2. Optional: Add slave hosts
3. Optional: Configure Traefik
4. Optional: Enable loop features

V1 single-machine continues to work in V2.
