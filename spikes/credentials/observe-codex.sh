#!/usr/bin/env bash
# Observe Codex's auth.json write using `codex login --with-api-key` with a FAKE key,
# in a FAKE CODEX_HOME whose auth.json is a symlink into a "shared" dir.
set -eu
T=$(mktemp -d)
mkdir -p "$T/home" "$T/codex" "$T/shared"
echo '{}' > "$T/shared/auth.json"; chmod 600 "$T/shared/auth.json"
ln -s "$T/shared/auth.json" "$T/codex/auth.json"
echo "tmp: $T"
echo "sk-FAKE-KEY-FOR-SPIKE" | env -i HOME="$T/home" CODEX_HOME="$T/codex" PATH=/usr/bin:/bin \
  timeout 40 strace -f -o "$T/st.txt" \
  -e trace=file,rename,renameat2,openat,flock,fcntl,mkdir \
  "${CODEX_BIN:-codex}" login --with-api-key >"$T/out.txt" 2>&1 || true
head -c 400 "$T/out.txt"
ls -la "$T/codex" "$T/shared"
grep -E "auth\.json" "$T/st.txt" | grep -v -E "O_RDONLY|newfstatat|statx" | cut -c1-240 | head -20
