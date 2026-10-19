#!/usr/bin/env bash
# Docker proof of the recommended credential-sharing scheme, on FAKE files only.
# Two containers (spike-cred-a "refreshes", spike-cred-b observes) share one host dir.
# Containers are stopped (and auto-removed via --rm) on exit.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
IMG=python@sha256:ae52c5bef62a6bdd42cd1e8dffef86b9cd284bde9427da79839de7a4b983e7ca # python:3.11-slim arm64; pin your arch's digest
R=$(mktemp -d)
mkdir -p "$R"/shared/{claude,codex,grok,pi}
echo "workdir (fake data only): $R"
cleanup() { docker stop spike-cred-a spike-cred-b spike-cred-c >/dev/null 2>&1; }
trap cleanup EXIT
for n in a b; do
  docker run -d --rm --name spike-cred-$n --user "$(id -u):$(id -g)" \
    -v "$R/shared:/creds" -v "$HERE:/spike:ro" "$IMG" sleep 600 >/dev/null
done
A() { docker exec spike-cred-a "$@"; }
B() { docker exec spike-cred-b "$@"; }
PY=(python3 /spike/replay.py)
fail=0
expect() { # name expected actual
  if [ "$2" = "$3" ]; then echo "  PASS $1 (got $3)"; else echo "  FAIL $1 (want $2, got $3)"; fail=1; fi
}

# Per-sleeve layout, created inside each container's own writable layer.
for c in A B; do
  $c sh -c '
    mkdir -p /tmp/sleeve/codex /tmp/sleeve/grok /tmp/sleeve/pi-sessions
    ln -sfn /creds/codex/auth.json     /tmp/sleeve/codex/auth.json
    ln -sfn /creds/grok/auth.json      /tmp/sleeve/grok/auth.json
    ln -sfn /creds/grok/auth.json.lock /tmp/sleeve/grok/auth.json.lock
  '
done

echo "== claude: shared dir via CLAUDE_SECURESTORAGE_CONFIG_DIR=/creds/claude"
A "${PY[@]}" write claude /creds/claude/.credentials.json T1
expect "B sees initial" T1 "$(B "${PY[@]}" read /creds/claude/.credentials.json)"
A "${PY[@]}" write claude /creds/claude/.credentials.json T2   # temp+rename refresh
expect "B sees refreshed value" T2 "$(B "${PY[@]}" read /creds/claude/.credentials.json)"
( A "${PY[@]}" hold-lock claude /creds/claude/.credentials.json 4 >/dev/null & ) ; sleep 1
expect "lock held by A is visible to B (mkdir lock dir)" LOCKED "$(B "${PY[@]}" try-lock claude /creds/claude/.credentials.json)"
sleep 4

echo "== codex: per-sleeve CODEX_HOME, auth.json symlink -> /creds/codex/auth.json"
A "${PY[@]}" write codex /tmp/sleeve/codex/auth.json T1
expect "B sees initial" T1 "$(B "${PY[@]}" read /tmp/sleeve/codex/auth.json)"
A "${PY[@]}" write codex /tmp/sleeve/codex/auth.json T2            # in-place O_TRUNC through the link
expect "B sees refreshed value" T2 "$(B "${PY[@]}" read /tmp/sleeve/codex/auth.json)"
expect "A's symlink survived" symlink "$(A sh -c '[ -L /tmp/sleeve/codex/auth.json ] && echo symlink || echo replaced')"

echo "== grok: per-sleeve GROK_HOME, auth.json and auth.json.lock symlinked into /creds/grok"
A "${PY[@]}" write grok /tmp/sleeve/grok/auth.json T1
expect "B sees initial" T1 "$(B "${PY[@]}" read /tmp/sleeve/grok/auth.json)"
A "${PY[@]}" write grok /tmp/sleeve/grok/auth.json T2              # lock, temp in shared dir, rename
expect "B sees refreshed value" T2 "$(B "${PY[@]}" read /tmp/sleeve/grok/auth.json)"
expect "A's symlink survived rename" symlink "$(A sh -c '[ -L /tmp/sleeve/grok/auth.json ] && echo symlink || echo replaced')"
( A "${PY[@]}" hold-lock grok /tmp/sleeve/grok/auth.json 4 >/dev/null & ) ; sleep 1
expect "flock held by A blocks B (shared lock inode)" LOCKED "$(B "${PY[@]}" try-lock grok /tmp/sleeve/grok/auth.json)"
sleep 4

echo "== pi: shared PI_CODING_AGENT_DIR=/creds/pi, per-sleeve PI_CODING_AGENT_SESSION_DIR=/tmp/sleeve/pi-sessions"
A "${PY[@]}" write pi /creds/pi/auth.json T1
expect "B sees initial" T1 "$(B "${PY[@]}" read /creds/pi/auth.json)"
A "${PY[@]}" write pi /creds/pi/auth.json T2
expect "B sees refreshed value" T2 "$(B "${PY[@]}" read /creds/pi/auth.json)"
( A "${PY[@]}" hold-lock pi /creds/pi/auth.json 4 >/dev/null & ) ; sleep 1
expect "lock dir held by A is visible to B" LOCKED "$(B "${PY[@]}" try-lock pi /creds/pi/auth.json)"
sleep 4

echo "== negatives"
echo "  (a) single-file bind mount + rename-over (what Claude/Grok do):"
echo '{}' > "$R/single.json"
docker run --rm --name spike-cred-c -v "$R/single.json:/x/auth.json" \
  -v "$HERE:/spike:ro" "$IMG" python3 /spike/replay.py rename-over /x/auth.json
echo "  (b) copy-at-start goes stale:"
cp "$R/shared/claude/.credentials.json" "$R/copy.json"
A "${PY[@]}" write claude /creds/claude/.credentials.json T3
echo "      live=$(B "${PY[@]}" read /creds/claude/.credentials.json) copy=$(python3 -I -c "import json,sys;print(json.load(open(sys.argv[1]))['claudeAiOauth']['accessToken'])" "$R/copy.json")"

[ $fail = 0 ] && echo "ALL PASS" || echo "SOME FAILED"
exit $fail
