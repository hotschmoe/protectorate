#!/usr/bin/env bash
# Observe Grok CLI's auth.json read/write pattern with a FAKE GROK_HOME.
set -eu
T=$(mktemp -d)
mkdir -p "$T/home" "$T/grok"
cat > "$T/grok/auth.json" <<'J'
{"https://auth.x.ai::fake-client":{"key":"eyJhbGciOiJub25lIn0.eyJleHAiOjEwMDB9.ZmFrZQ","auth_mode":"oidc","create_time":"2026-01-01T00:00:00Z","user_id":"u","email":"fake@example.invalid","first_name":"F","last_name":"L","principal_type":"User","principal_id":"p","team_id":"t","coding_data_retention_opt_out":false,"refresh_token":"FAKE-RT-1","expires_at":"2026-01-01T00:00:00Z","oidc_issuer":"https://auth.x.ai","oidc_client_id":"fake-client"}}
J
chmod 600 "$T/grok/auth.json"
echo "tmp: $T"
env -i HOME="$T/home" GROK_HOME="$T/grok" PATH=/usr/bin:/bin TERM=dumb \
  timeout 40 strace -f -o "$T/st.txt" \
  -e trace=file,rename,renameat2,openat,flock,fcntl,mkdir,inotify_add_watch \
  "${GROK_BIN:-grok}" -p hi </dev/null >"$T/out.txt" 2>&1 || true
ls -la "$T/grok"
grep -E "auth\.json|\.lock|inotify|flock" "$T/st.txt" | cut -c1-230 | head -60
echo "--- output:"; head -c 500 "$T/out.txt"
