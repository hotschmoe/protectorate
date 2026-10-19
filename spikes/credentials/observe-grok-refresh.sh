#!/usr/bin/env bash
# Usage: observe-grok-refresh.sh <tmpdir from observe-grok-write.sh>
# Re-runs grok in the same FAKE GROK_HOME after the fake token expired, to see the
# refresh path (lock + write) when auth.json is a symlink into a shared dir.
set -eu
T=$1
sleep 1
env -i GROK_AUTH_PROVIDER_COMMAND="$PWD/fake-grok-provider.sh" HOME="$T/home" GROK_HOME="$T/grok" PATH=/usr/bin:/bin TERM=dumb \
  timeout 60 strace -f -o "$T/st2.txt" \
  -e trace=file,rename,renameat2,openat,flock,fcntl,mkdir \
  "${GROK_BIN:-grok}" -p hi </dev/null >"$T/out2.txt" 2>&1 || true
head -c 600 "$T/out2.txt"
ls -la "$T/grok" "$T/shared"
grep -E "auth\.json|auth.*lock" "$T/st2.txt" | grep -v -E "O_RDONLY\)|newfstatat" | cut -c1-240 | head -30
