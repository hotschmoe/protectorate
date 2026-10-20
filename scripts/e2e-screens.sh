#!/usr/bin/env bash
# Builds the web app, runs a throwaway hub with demo data, and screenshots
# every main screen with headless Chromium at phone and desktop sizes.
# Output: e2e-out/ (gitignored). Needs go, flutter, tmux, chromium, Node 22+.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${E2E_OUT:-$root/e2e-out}
port=${E2E_PORT:-$((47900 + RANDOM % 100))}
chromium=${CHROMIUM:-chromium}
work=$(mktemp -d)
sock=prot-e2e-$RANDOM$RANDOM
export PROTECTORATE_HOME=$work/home PROTECTORATE_TMUX_SOCKET=$sock PROTECTORATE_HUB=http://127.0.0.1:$port
unset PROTECTORATE_TOKEN PROTECTORATE_TOKEN_FILE PROTECTORATE_NODE_TOKEN
mkdir -p "$out" "$PROTECTORATE_HOME"

hub_pid=
login_pid=
cleanup() {
  [ -n "$login_pid" ] && kill "$login_pid" 2>/dev/null || true
  [ -n "$hub_pid" ] && kill "$hub_pid" 2>/dev/null || true
  tmux -L "$sock" kill-server 2>/dev/null || true
}
trap cleanup EXIT

(make -C "$root" app-build-web >"$work/flutter.log" 2>&1) \
  || { tail -30 "$work/flutter.log"; exit 1; }
make -C "$root" build >/dev/null
bin=$root/bin/protectorate

mkdir -p "$work/cache"
head -c 3000000 /dev/zero >"$work/cache/blob"
"$bin" hub serve --listen "127.0.0.1:$port" --web "$root/app/build/web" --node-interval 5s \
  --watch "$work/cache" --watch "$HOME/.cache" >"$work/hub.log" 2>&1 &
hub_pid=$!
for _ in $(seq 50); do
  curl -sf "http://127.0.0.1:$port/" >/dev/null && break
  sleep 0.2
done

"$bin" session new demo --cwd "$work" -- bash -c 'echo demo; echo hello from the demo session; exec bash' >/dev/null
"$bin" backlog add "Wire the pairing flow" --project hotschmoe/protectorate --body "Scan the QR, confirm, land on the overview." --pin >/dev/null
"$bin" backlog add "Screenshot every screen" --project hotschmoe/protectorate --body "Phone and desktop, dark and light." >/dev/null
"$bin" backlog add "Fix flaky parser test" --project octo/widgets --body "https://github.com/octo/widgets/issues/7" >/dev/null
"$bin" node add ghost --addr 127.0.0.1:1 >/dev/null
node_name=$("$bin" node ls 2>/dev/null | awk 'NR>1 && $1 != "ghost" {gsub(/\*/, "", $1); print $1; exit}')
sleep 8

web=$("$bin" pair --url "http://127.0.0.1:$port" | sed -n 's/^web: *//p')
PROTECTORATE_HOME="$work/client" "$bin" login --hub "$PROTECTORATE_HUB" --name screenshot-browser --platform web --json >"$work/login.jsonl" 2>"$work/login.log" &
login_pid=$!
code=
for _ in $(seq 100); do
  code=$(node -e 'try { process.stdout.write(JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8").split("\n")[0]).userCode || ""); } catch {}' "$work/login.jsonl")
  [ -z "$code" ] || break
  sleep 0.1
done
[ -n "$code" ] || { cat "$work/login.log"; exit 1; }
"$bin" devices approve "$code" >/dev/null
wait "$login_pid"
login_pid=
credential=$(node -e 'const path = require("node:path"); const hash = require("node:crypto").createHash("sha256").update(process.argv[2]).digest("hex"); process.stdout.write(path.join(process.argv[1], "clients", hash + ".token"));' "$work/client" "$PROTECTORATE_HUB")

shot() {
  local name=$1 size=$2 url=$3 theme=${4:-light} connected=${5:-false} profile
  [ -z "${E2E_ONLY:-}" ] || [[ $name == $E2E_ONLY ]] || return 0
  profile=$(mktemp -d "$work/chrome.XXXXXX")
  node "$root/scripts/e2e-screens.mjs" "$chromium" "$profile" "$size" "$url" "$theme" \
    "$out/$name-${size%%,*}${theme/light/}.png" "$PROTECTORATE_HUB" "$credential" "$connected"
}

for size in 412,860 1280,800; do
  for theme in light dark; do
    shot hubs-empty "$size" "http://127.0.0.1:$port/" "$theme"
    shot pair-confirm "$size" "$web" "$theme"
    for tab in overview sleeves nodes notes mail; do
      shot "$tab" "$size" "$web&tab=$tab" "$theme" true
    done
    shot node-detail "$size" "$web&tab=nodes&node=$node_name" "$theme" true
    shot node-ghost "$size" "$web&tab=nodes&node=ghost" "$theme" true
    shot terminal "$size" "$web&tab=sleeves&session=demo" "$theme" true
  done
done
echo "screenshots in $out"
