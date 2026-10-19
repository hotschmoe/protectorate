#!/usr/bin/env bash
# Force Grok to WRITE auth.json using a fake API key login, in a FAKE GROK_HOME,
# with auth.json being a symlink to a "shared" location. Shows whether the write
# goes through the link (in place) or replaces the link (temp + rename).
set -eu
T=$(mktemp -d)
mkdir -p "$T/home" "$T/grok" "$T/shared"
echo '{}' > "$T/shared/auth.json"; chmod 600 "$T/shared/auth.json"
ln -s "$T/shared/auth.json" "$T/grok/auth.json"
echo "tmp: $T"
[ -n "${LOCKLINK:-}" ] && { : > "$T/shared/auth.json.lock"; ln -s "$T/shared/auth.json.lock" "$T/grok/auth.json.lock"; }
MODE=${MODE:-symlink}
[ "$MODE" = plain ] && { rm "$T/grok/auth.json"; }
env -i GROK_AUTH_PROVIDER_COMMAND="$PWD/fake-grok-provider.sh" HOME="$T/home" GROK_HOME="$T/grok" PATH=/usr/bin:/bin TERM=dumb \
  timeout 40 strace -f -o "$T/st.txt" \
  -e trace=file,rename,renameat2,openat,flock,fcntl,mkdir \
  "${GROK_BIN:-grok}" ${GROK_ARGS:-login --help} </dev/null >"$T/out.txt" 2>&1 || true
head -c 1500 "$T/out.txt"
echo "--- listing"; ls -la "$T/grok" "$T/shared"
echo "--- syscalls on auth.json"
grep -E "auth\.json" "$T/st.txt" | grep -v -E "O_RDONLY|newfstatat" | cut -c1-240 | head -30
