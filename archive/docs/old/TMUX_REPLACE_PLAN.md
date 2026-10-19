# Terminal Architecture Replacement Plan

Replace **tmux + ttyd** with **abduco + Envoy-as-gateway** for simpler architecture and better orchestration support.

**Goals:**
- Native mouse wheel scrollback without hotkeys
- Read-only observation mode for Envoy monitoring
- Simplified architecture (fewer components per sleeve)
- Direct programmatic control from Envoy

---

## Current Architecture

```
Browser (xterm.js)
    |
    v [WebSocket ws://localhost:7470/sleeves/{name}/terminal]
Envoy WS Proxy (:7470)
    |
    v [WebSocket ws://sleeve-{name}:7681/ws, subprotocol "tty"]
ttyd (:7681 per sleeve)
    |
    v [PTY]
tmux session "main"
    |
    v
Claude CLI / Shell
```

**Problems:**
- tmux requires copy-mode for scroll (even with custom bindings)
- ttyd is redundant - Envoy already handles WebSocket, Docker already handles exec
- Heavy for our use case (0% of multiplexing features used)
- No native read-only observation mode
- Complex configuration across multiple layers

---

## Target Architecture

```
Browser (xterm.js)
    |
    v [WebSocket ws://localhost:7470/sleeves/{name}/terminal]
Envoy Terminal Gateway (:7470)
    |
    v [Docker Exec API - AttachExec with PTY]
abduco session (per sleeve)
    |
    v
Claude CLI / Shell
```

**Improvements:**
- Envoy directly execs into containers (no ttyd intermediary)
- abduco provides session persistence + read-only mode
- Single WebSocket endpoint handles all terminal access
- Envoy controls access (auth, read-only, logging)
- Simpler sleeve image (no ttyd binary)

---

## Component Comparison

### Session Manager: tmux vs abduco

| Feature | tmux | abduco |
|---------|------|--------|
| Session persistence | Yes | Yes |
| Mouse scroll | Copy-mode required | Terminal handles natively |
| Read-only attach | No | Yes (`-r` flag) |
| Session listing | `tmux ls` | `abduco` (no args) |
| Exit status tracking | No | Yes (built-in) |
| Binary size | ~1MB | ~50KB |
| Config complexity | High (.tmux.conf) | None |
| Multiple observers | Yes | Yes |

### Terminal Gateway: ttyd vs Envoy Direct Exec

| Feature | ttyd | Envoy Direct Exec |
|---------|------|-------------------|
| Port per sleeve | Yes (7681 each) | No (single 7470) |
| Auth handling | Per-instance | Centralized |
| Read-only support | No | Yes (abduco -r) |
| Logging/audit | Per-instance | Centralized |
| Protocol | Custom "tty" subprotocol | Standard binary WS |
| Container footprint | +ttyd binary | None |
| Attack surface | ttyd + network port | Docker socket only |

---

## Future Protectorate Requirements

### 1. Envoy Observation Mode (HIGH PRIORITY)

Envoy needs to observe sleeve activity without interfering.

**Use cases:**
- Health monitoring (is CLI responsive?)
- Activity detection (is sleeve working or idle?)
- Log streaming to webui
- Debugging stuck sleeves

**abduco approach:**
```bash
# Read-only attach - can see output, cannot send input
abduco -r /tmp/claude.sock
```

**Implementation:** Envoy adds `/sleeves/{name}/terminal?mode=observe` endpoint.

### 2. Command Injection (MEDIUM PRIORITY)

Envoy may need to send commands to sleeves.

**Use cases:**
- Trigger soft resleeve (send exit command)
- Inject context updates
- Emergency abort

**abduco approach:**
```bash
# Attach and send command
abduco -a /tmp/claude.sock
# Then write to stdin
```

**Implementation:** Envoy's exec handler can write to attached session.

### 3. Session State Detection (MEDIUM PRIORITY)

Detect if CLI has crashed vs is waiting for input.

**Use cases:**
- Auto-restart crashed CLIs
- Distinguish "thinking" from "crashed"
- Report accurate status to webui

**abduco approach:**
```bash
# Check if session exists and get exit status
abduco  # Lists sessions with status
```

**Implementation:** Sidecar parses abduco output for health reporting.

### 4. Multi-Tenant Observation (MEDIUM PRIORITY)

Multiple users watching same sleeve.

**Use cases:**
- Team debugging
- Demo/presentation mode
- Audit logging

**Both abduco and Envoy support:** Multiple read-only connections simultaneously.

### 5. Sidecar Integration (FUTURE)

Per Q_TODO_NEXT.md, sidecar will eventually manage CLI as child process.

```
Future architecture:
Sidecar
    |
    +-- abduco session
    |       |
    |       +-- Claude CLI
    |
    +-- HTTP API (:8080)
```

**Requirements:**
- Sidecar spawns session programmatically
- Sidecar can query session state
- Sidecar can inject commands
- Sidecar can kill/restart CLI without killing session

---

## Implementation Plan

### Phase 1: Add abduco to Base Image

**File: `containers/base/Dockerfile`**

```dockerfile
# Remove tmux, add abduco
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    git \
    abduco \
    && rm -rf /var/lib/apt/lists/*

# Remove: tmux installation
# Remove: .tmux.conf creation (lines 29-37)
```

**Changes:**
- Replace `tmux` with `abduco` in apt-get
- Remove entire `.tmux.conf` heredoc block
- Remove `EXPOSE 7681` (ttyd port no longer needed)
- Remove ttyd download/install block

### Phase 2: Update Sleeve Entrypoint

**File: `containers/sleeve/entrypoint.sh`**

Replace tmux-session.sh with abduco-session.sh:

```bash
#!/bin/bash
set -e

# Fix ownership of mounted volumes
chown -R claude:claude /home/claude/workspace
chown -R claude:claude /home/claude/.claude 2>/dev/null || true

# Copy settings if provided
if [ -f /etc/claude/settings.json ]; then
    cp /etc/claude/settings.json /home/claude/.claude/settings.json
    chown claude:claude /home/claude/.claude/settings.json
fi

# Session socket location
SOCKET_DIR="/home/claude/.abduco"
SOCKET_PATH="${SOCKET_DIR}/claude.sock"

# Create socket directory
mkdir -p "$SOCKET_DIR"
chown claude:claude "$SOCKET_DIR"

# Create session manager script
cat > /usr/local/bin/abduco-session.sh << 'SCRIPT'
#!/bin/bash
cd /home/claude/workspace
exec claude --dangerously-skip-permissions
SCRIPT
chmod +x /usr/local/bin/abduco-session.sh

# Start abduco session (detached)
# -c creates new session, runs command
# -e sets detach key (we use a rare combo to avoid conflicts)
su - claude -c "abduco -c $SOCKET_PATH -e '^\\' /usr/local/bin/abduco-session.sh" &

# Keep container running (sidecar or sleep)
if [ -x /usr/local/bin/sidecar ]; then
    exec /usr/local/bin/sidecar
else
    # Fallback: keep container alive
    exec sleep infinity
fi
```

**Key changes:**
- No ttyd startup
- abduco creates session at known socket path
- Container stays alive via sidecar (not ttyd)
- Socket path is predictable: `/home/claude/.abduco/claude.sock`

### Phase 3: Update Envoy Entrypoint

**File: `containers/envoy/entrypoint.sh`**

```bash
#!/bin/bash
set -e

# Fix ownership
chown -R claude:claude /home/claude/workspaces
chown -R claude:claude /home/claude/.claude 2>/dev/null || true

# SSH agent setup (unchanged)
if [ -S /ssh-agent ]; then
    mkdir -p /home/claude/.ssh
    ssh-keyscan github.com gitlab.com >> /home/claude/.ssh/known_hosts 2>/dev/null || true
    chown -R claude:claude /home/claude/.ssh
fi

# Copy settings (unchanged)
if [ -f /etc/claude/settings.json ]; then
    cp /etc/claude/settings.json /home/claude/.claude/settings.json
    chown claude:claude /home/claude/.claude/settings.json
fi

# Envoy's own terminal session (for Poe)
SOCKET_DIR="/home/claude/.abduco"
SOCKET_PATH="${SOCKET_DIR}/envoy.sock"

mkdir -p "$SOCKET_DIR"
chown claude:claude "$SOCKET_DIR"

cat > /usr/local/bin/abduco-session.sh << 'SCRIPT'
#!/bin/bash
cd /home/claude/workspaces
exec bash
SCRIPT
chmod +x /usr/local/bin/abduco-session.sh

# Start envoy's own abduco session
su - claude -c "abduco -c $SOCKET_PATH -e '^\\' /usr/local/bin/abduco-session.sh" &

# Start envoy (no ttyd)
exec envoy
```

**Key changes:**
- Remove ttyd startup entirely
- Envoy has its own abduco session for Poe access

### Phase 4: Implement Docker Exec Terminal Gateway

**File: `internal/envoy/docker.go`** (additions)

```go
import (
    "io"
    "github.com/docker/docker/api/types"
)

// ExecAttachOptions configures terminal exec
type ExecAttachOptions struct {
    Container string
    Command   []string
    ReadOnly  bool // Use abduco -r instead of -a
}

// ExecAttach creates an interactive exec session
func (d *DockerClient) ExecAttach(ctx context.Context, opts ExecAttachOptions) (types.HijackedResponse, error) {
    // Build abduco command based on mode
    var cmd []string
    socketPath := "/home/claude/.abduco/claude.sock"

    if opts.ReadOnly {
        cmd = []string{"abduco", "-r", socketPath}
    } else {
        cmd = []string{"abduco", "-a", socketPath}
    }

    // Create exec instance
    execConfig := types.ExecConfig{
        AttachStdin:  !opts.ReadOnly,
        AttachStdout: true,
        AttachStderr: true,
        Tty:          true,
        Cmd:          cmd,
    }

    execResp, err := d.cli.ContainerExecCreate(ctx, opts.Container, execConfig)
    if err != nil {
        return types.HijackedResponse{}, fmt.Errorf("exec create: %w", err)
    }

    // Attach to exec instance
    attachResp, err := d.cli.ContainerExecAttach(ctx, execResp.ID, types.ExecStartCheck{
        Tty: true,
    })
    if err != nil {
        return types.HijackedResponse{}, fmt.Errorf("exec attach: %w", err)
    }

    return attachResp, nil
}

// ExecResize resizes the exec TTY
func (d *DockerClient) ExecResize(ctx context.Context, execID string, height, width uint) error {
    return d.cli.ContainerExecResize(ctx, execID, container.ResizeOptions{
        Height: height,
        Width:  width,
    })
}
```

### Phase 5: Replace WebSocket Proxy with Exec Gateway

**File: `internal/envoy/terminal_gateway.go`** (new file)

```go
package envoy

import (
    "context"
    "encoding/json"
    "io"
    "log"
    "net/http"
    "sync"

    "github.com/gorilla/websocket"
)

// Terminal message types (simplified from ttyd protocol)
const (
    MsgInput  = '0' // Client -> Server: keyboard input
    MsgOutput = '0' // Server -> Client: terminal output
    MsgResize = '1' // Client -> Server: terminal resize
)

// ResizeMessage from client
type ResizeMessage struct {
    Cols uint `json:"cols"`
    Rows uint `json:"rows"`
}

// InitMessage from client (first message after connect)
type InitMessage struct {
    Columns int `json:"columns"`
    Rows    int `json:"rows"`
}

var terminalUpgrader = websocket.Upgrader{
    ReadBufferSize:  4096,
    WriteBufferSize: 4096,
    CheckOrigin:     func(r *http.Request) bool { return true },
}

// handleTerminalGateway handles WebSocket terminal connections via Docker exec
func (s *Server) handleTerminalGateway(w http.ResponseWriter, r *http.Request, containerName string, readOnly bool) {
    // Upgrade to WebSocket
    ws, err := terminalUpgrader.Upgrade(w, r, nil)
    if err != nil {
        log.Printf("websocket upgrade failed: %v", err)
        return
    }
    defer ws.Close()

    ctx, cancel := context.WithCancel(r.Context())
    defer cancel()

    // Read init message to get terminal size
    _, initData, err := ws.ReadMessage()
    if err != nil {
        log.Printf("failed to read init message: %v", err)
        return
    }

    var initMsg InitMessage
    if err := json.Unmarshal(initData, &initMsg); err != nil {
        log.Printf("failed to parse init message: %v", err)
        return
    }

    // Create exec session
    execResp, err := s.docker.ExecAttach(ctx, ExecAttachOptions{
        Container: containerName,
        ReadOnly:  readOnly,
    })
    if err != nil {
        log.Printf("failed to attach to container %s: %v", containerName, err)
        ws.WriteMessage(websocket.TextMessage, []byte("Failed to attach: "+err.Error()))
        return
    }
    defer execResp.Close()

    var wg sync.WaitGroup

    // Relay: Docker -> WebSocket
    wg.Add(1)
    go func() {
        defer wg.Done()
        defer cancel()

        buf := make([]byte, 4096)
        for {
            n, err := execResp.Reader.Read(buf)
            if err != nil {
                if err != io.EOF {
                    log.Printf("exec read error: %v", err)
                }
                return
            }

            // Send as binary message with output type prefix
            msg := append([]byte{MsgOutput}, buf[:n]...)
            if err := ws.WriteMessage(websocket.BinaryMessage, msg); err != nil {
                log.Printf("websocket write error: %v", err)
                return
            }
        }
    }()

    // Relay: WebSocket -> Docker (skip if read-only)
    if !readOnly {
        wg.Add(1)
        go func() {
            defer wg.Done()
            defer cancel()

            for {
                _, data, err := ws.ReadMessage()
                if err != nil {
                    if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseNormalClosure) {
                        log.Printf("websocket read error: %v", err)
                    }
                    return
                }

                if len(data) == 0 {
                    continue
                }

                msgType := data[0]
                payload := data[1:]

                switch msgType {
                case MsgInput:
                    if _, err := execResp.Conn.Write(payload); err != nil {
                        log.Printf("exec write error: %v", err)
                        return
                    }
                case MsgResize:
                    var resize ResizeMessage
                    if err := json.Unmarshal(payload, &resize); err == nil {
                        // TODO: Implement resize via exec ID tracking
                        log.Printf("resize requested: %dx%d", resize.Cols, resize.Rows)
                    }
                }
            }
        }()
    }

    wg.Wait()
}
```

### Phase 6: Update HTTP Handlers

**File: `internal/envoy/handlers.go`** (modifications)

```go
// handleSleeveTerminal - updated to use direct exec
func (s *Server) handleSleeveTerminal(w http.ResponseWriter, r *http.Request) {
    name := r.PathValue("name")
    if name == "" {
        http.Error(w, "sleeve name required", http.StatusBadRequest)
        return
    }

    // Check for read-only mode
    readOnly := r.URL.Query().Get("mode") == "observe"

    // Get container name
    containerName := "sleeve-" + name

    // Verify sleeve exists
    container, err := s.docker.GetContainerByName(containerName)
    if err != nil || container == nil {
        http.Error(w, "sleeve not found", http.StatusNotFound)
        return
    }

    // Handle terminal via exec gateway
    s.handleTerminalGateway(w, r, containerName, readOnly)
}

// handleEnvoyTerminal - updated for envoy's own session
func (s *Server) handleEnvoyTerminal(w http.ResponseWriter, r *http.Request) {
    // Envoy container is "protectorate-envoy" or use hostname
    containerName := os.Getenv("HOSTNAME")
    if containerName == "" {
        containerName = "protectorate-envoy"
    }

    // Envoy terminal is always read-write
    s.handleTerminalGateway(w, r, containerName, false)
}
```

### Phase 7: Update Frontend Protocol

**File: `internal/envoy/web/templates/index.html`**

Minimal changes needed - the message format is similar:

```javascript
// Update constants (line ~697)
const MSG_INPUT = 48;   // '0' - unchanged
const MSG_OUTPUT = 48;  // '0' - unchanged
const MSG_RESIZE = 49;  // '1' - unchanged

// Update connect() method - remove "tty" subprotocol requirement
connect() {
    // ... existing code ...

    // Change: Remove subprotocol (no longer ttyd)
    this.ws = new WebSocket(wsUrl);  // Was: new WebSocket(wsUrl, ['tty'])
    this.ws.binaryType = 'arraybuffer';

    // Rest unchanged - message format is compatible
}

// Add observation mode support
connectObserve() {
    const wsUrl = `ws://${window.location.host}/sleeves/${this.sleeveName}/terminal?mode=observe`;
    // ... same as connect but read-only indicator in UI
}
```

### Phase 8: Update Docker Compose

**File: `docker-compose.dev.yaml`** (modifications)

```yaml
services:
  envoy:
    # ... existing config ...
    ports:
      - "7470:7470"
      # Remove: - "7681:7681"  # No longer needed
    # ... rest unchanged ...
```

### Phase 9: Update SleeveInfo Structure

**File: `internal/envoy/sleeve_manager.go`** (modifications)

```go
type SleeveInfo struct {
    Name          string    `json:"name"`
    ContainerID   string    `json:"container_id"`   // Keep
    ContainerName string    `json:"container_name"` // Add explicit field
    Status        string    `json:"status"`
    Health        string    `json:"health"`
    SidecarAddr   string    `json:"sidecar_addr"`
    // Remove: TTYDPort    int       `json:"ttyd_port"`
    // Remove: TTYDAddress string    `json:"ttyd_address"`
    CreatedAt     time.Time `json:"created_at"`
    LastCheckin   time.Time `json:"last_checkin"`
}
```

---

## File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `containers/base/Dockerfile` | Modify | Replace tmux with abduco, remove ttyd |
| `containers/sleeve/Dockerfile` | Modify | Remove EXPOSE 7681 |
| `containers/sleeve/entrypoint.sh` | Rewrite | Use abduco instead of tmux+ttyd |
| `containers/envoy/entrypoint.sh` | Rewrite | Remove ttyd, add abduco for Poe |
| `internal/envoy/docker.go` | Extend | Add ExecAttach, ExecResize methods |
| `internal/envoy/terminal_gateway.go` | New | WebSocket-to-exec bridge |
| `internal/envoy/handlers.go` | Modify | Update terminal handlers |
| `internal/envoy/ws_proxy.go` | Remove | No longer needed |
| `internal/envoy/sleeve_manager.go` | Modify | Remove TTYDPort/TTYDAddress fields |
| `internal/envoy/web/templates/index.html` | Modify | Remove "tty" subprotocol, add observe mode |
| `docker-compose.dev.yaml` | Modify | Remove port 7681 mapping |
| `docker-compose.yaml` | Modify | Remove port 7681 mapping |
| `.env.example` | Modify | Remove TTYD-related vars if any |

---

## Testing Plan

### Phase 1: Unit Tests

| Test | Description |
|------|-------------|
| ExecAttach read-write | Verify bidirectional I/O via docker exec |
| ExecAttach read-only | Verify output-only mode works |
| Terminal resize | Verify SIGWINCH propagation |
| Session persistence | Verify abduco survives disconnect |
| Multiple observers | Verify concurrent read-only connections |

### Phase 2: Integration Tests

| Test | Expected |
|------|----------|
| Mouse wheel scroll in xterm.js | Scrolls buffer natively |
| Scroll in vim/less | Scrolls within application |
| Disconnect and reconnect | Returns to same session state |
| Container restart | Session persists (if volume-mounted) |
| Multiple browser tabs | All show same content |
| Read-only observation | Can see output, input ignored |
| Inject command via API | Text appears in session |
| Detect CLI exit status | Sidecar reports correctly |
| Input latency | Subjective: feels responsive |
| Copy/paste | Works via browser |

### Phase 3: Load Tests

| Test | Target |
|------|--------|
| 10 concurrent terminals | All responsive |
| 50 observers on one sleeve | No degradation |
| Rapid connect/disconnect | No resource leaks |

---

## Rollback Plan

If issues arise, revert in this order:

1. **Immediate:** Revert entrypoint changes, keep ttyd
2. **Short-term:** Revert Dockerfile changes
3. **Full rollback:** Git revert to pre-migration commit

Keep tmux+ttyd binaries in base image during testing phase for quick rollback.

---

## Zellij Reference (Future Consideration)

Keeping this section for potential future use cases.

### When Zellij Might Be Better

| Use Case | Why Zellij |
|----------|-----------|
| Plugin system for custom features | WASM plugins can extend functionality |
| Session resurrection with layout | Survives container restarts with full state |
| Built-in web client | Could replace xterm.js entirely |
| Multi-pane workflows | If sleeves need split terminals |

### Zellij Installation (for reference)

```dockerfile
ARG ZELLIJ_VERSION=0.41.2
RUN curl -fsSL \
    "https://github.com/zellij-org/zellij/releases/download/v${ZELLIJ_VERSION}/zellij-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xzf - -C /usr/local/bin \
    && chmod +x /usr/local/bin/zellij

# Zellij config
RUN mkdir -p /home/claude/.config/zellij \
    && cat > /home/claude/.config/zellij/config.kdl << 'EOF'
mouse_mode true
scroll_buffer_size 50000
copy_on_select true
pane_frames false
EOF
```

### Hybrid Option (Not Recommended Currently)

- Use abduco for sleeves (read-only observation)
- Use Zellij for envoy terminal (nicer UX for operators)

Decision: Start with abduco everywhere for consistency. Revisit Zellij if plugin use cases emerge.

---

## Open Questions

1. **abduco socket location:** Use `/home/claude/.abduco/` (user dir) or `/run/abduco/` (system)?
   - Recommendation: User dir for proper permissions

2. **Multi-sleeve observation:** Can Envoy observe all sleeves simultaneously?
   - Yes: Open one exec per sleeve, multiplex in Envoy

3. **Terminal resize:** How to propagate SIGWINCH through docker exec?
   - Use `ContainerExecResize` API with stored exec ID

4. **Session recovery:** What happens if abduco session dies?
   - Sidecar can detect and restart; add health check endpoint

5. **Auth for observe mode:** Should read-only require different permissions?
   - Future: Add role-based access (admin=write, viewer=read-only)

---

## Next Steps

1. [ ] Update base Dockerfile (remove tmux/ttyd, add abduco)
2. [ ] Update sleeve entrypoint (abduco session manager)
3. [ ] Update envoy entrypoint (remove ttyd)
4. [ ] Implement ExecAttach in docker.go
5. [ ] Create terminal_gateway.go
6. [ ] Update handlers.go
7. [ ] Update frontend (remove "tty" subprotocol)
8. [ ] Test mouse scroll
9. [ ] Test read-only observation
10. [ ] Test session persistence
11. [ ] Remove ws_proxy.go
12. [ ] Update documentation
