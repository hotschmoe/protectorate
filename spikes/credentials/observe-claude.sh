#!/usr/bin/env bash
# Observe Claude Code's credential read/write pattern against FAKE credentials.
# Never touches the real home: HOME, CLAUDE_CONFIG_DIR and
# CLAUDE_SECURESTORAGE_CONFIG_DIR all point into a fresh temp dir.
set -eu
T=$(mktemp -d)
mkdir -p "$T/home" "$T/cfg" "$T/sec"
cat > "$T/sec/.credentials.json" <<'EOF'
{"claudeAiOauth":{"accessToken":"FAKE-AT-1","refreshToken":"FAKE-RT-1","expiresAt":1000,"refreshTokenExpiresAt":1000,"scopes":["user:inference"],"subscriptionType":"max","rateLimitTier":"x"}}
EOF
chmod 600 "$T/sec/.credentials.json"
echo '{"hasCompletedOnboarding":true}' > "$T/home/.claude.json"
echo "tmp: $T"
env -i HOME="$T/home" PATH=/usr/bin:/bin \
  CLAUDE_CONFIG_DIR="$T/cfg" CLAUDE_SECURESTORAGE_CONFIG_DIR="$T/sec" \
  timeout 60 strace -f -o "$T/st.txt" \
  -e trace=file,rename,renameat2,openat,flock,fcntl,mkdir \
  "${CLAUDE_BIN:-claude}" -p hi --max-turns 1 </dev/null >"$T/out.txt" 2>&1 || true
echo "--- fs after:"; ls -la "$T/sec" "$T/cfg"
echo "--- credential/lock related syscalls:"
grep -E "credentials|oauth_refresh" "$T/st.txt" | grep -v ENOENT.*stat | head -60
echo "--- output:"; head -c 400 "$T/out.txt"
