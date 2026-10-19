#!/bin/bash
set -e

chown -R agent:agent /home/agent/workspace
chown -R agent:agent /home/agent/.creds 2>/dev/null || true
chown -R agent:agent /home/agent/.config 2>/dev/null || true

# Setup Claude credentials from volume
# Claude expects: ~/.claude/.credentials.json and ~/.claude.json
if [ -f /home/agent/.creds/claude/.credentials.json ]; then
    mkdir -p /home/agent/.claude
    cp /home/agent/.creds/claude/.credentials.json /home/agent/.claude/.credentials.json
    chown agent:agent /home/agent/.claude/.credentials.json
    chmod 600 /home/agent/.claude/.credentials.json
fi

# Setup Claude settings (includes onboarding flag)
if [ -f /home/agent/.creds/claude/settings.json ]; then
    cp /home/agent/.creds/claude/settings.json /home/agent/.claude.json
    chown agent:agent /home/agent/.claude.json
    chmod 600 /home/agent/.claude.json
fi

# Ensure .claude directory ownership
chown -R agent:agent /home/agent/.claude 2>/dev/null || true

# Setup other CLI tool credentials
su - agent -c "
    ln -sf /home/agent/.creds/gemini /home/agent/.config/gemini 2>/dev/null || true
    ln -sf /home/agent/.creds/codex /home/agent/.codex 2>/dev/null || true
    ln -sf /home/agent/.creds/git /home/agent/.ssh 2>/dev/null || true
"

SOCKET_DIR="/home/agent/.dtach"
SOCKET_PATH="${SOCKET_DIR}/session.sock"

mkdir -p "$SOCKET_DIR"
chown agent:agent "$SOCKET_DIR"

# Session script: bash shell (envoy can send commands to start claude)
cat > /usr/local/bin/sleeve-session.sh << 'SCRIPT'
#!/bin/bash
export TERM=xterm-256color
export PATH="$HOME/.local/bin:$PATH"
cd /home/agent/workspace

while true; do
    bash --login
    exit_code=$?
    echo ""
    echo "[Shell exited with code $exit_code. Restarting in 1 second...]"
    echo "[Press Ctrl+\\ to detach from session]"
    echo ""
    sleep 1
done
SCRIPT
chmod +x /usr/local/bin/sleeve-session.sh

# dtach: -n creates daemon session (no attach), -z disables suspend
su - agent -c "dtach -n $SOCKET_PATH -z /usr/local/bin/sleeve-session.sh"

exec /usr/local/bin/sidecar
