#!/usr/bin/env bash
# Builds the web app, runs a throwaway hub with demo data, and screenshots
# every main screen with headless Chromium at phone and desktop sizes.
# Output: e2e-out/ (gitignored). Needs go, flutter, tmux, chromium.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${E2E_OUT:-$root/e2e-out}
port=${E2E_PORT:-$((47900 + RANDOM % 100))}
chromium=${CHROMIUM:-chromium}
work=$(mktemp -d)
sock=prot-e2e-$RANDOM$RANDOM
export PROTECTORATE_HOME=$work/home PROTECTORATE_TMUX_SOCKET=$sock PROTECTORATE_HUB=http://127.0.0.1:$port
mkdir -p "$out" "$PROTECTORATE_HOME"

hub_pid=
cleanup() {
  [ -n "$hub_pid" ] && kill "$hub_pid" 2>/dev/null || true
  tmux -L "$sock" kill-server 2>/dev/null || true
}
trap cleanup EXIT

(cd "$root/app" && flutter build web --dart-define=E2E_AUTOCONFIRM=true >"$work/flutter.log" 2>&1) \
  || { tail -30 "$work/flutter.log"; exit 1; }
(cd "$root" && go build -o "$work/protectorate" ./cmd/protectorate)
bin=$work/protectorate

"$bin" hub serve --listen "127.0.0.1:$port" --web "$root/app/build/web" >"$work/hub.log" 2>&1 &
hub_pid=$!
for _ in $(seq 50); do
  curl -sf "http://127.0.0.1:$port/" >/dev/null && break
  sleep 0.2
done

"$bin" session new demo --cwd "$work" -- bash -c 'echo demo; echo hello from the demo session; exec bash' >/dev/null
"$bin" backlog add "hotschmoe/protectorate" "Wire the pairing flow" --prio 1 >/dev/null
"$bin" backlog add "hotschmoe/protectorate" "Screenshot every screen" --prio 3 --harness codex >/dev/null
"$bin" backlog add "donate: octo/widgets" "Fix flaky parser test" --issue https://github.com/octo/widgets/issues/7 >/dev/null
sleep 1

web=$("$bin" pair --url "http://127.0.0.1:$port" --name "demo hub" | sed -n 's/^web: *//p')

shot() {
  local name=$1 size=$2 url=$3 profile
  profile=$(mktemp -d "$work/chrome.XXXXXX")
  "$chromium" --headless=new --disable-gpu --no-sandbox --user-data-dir="$profile" \
    --window-size="$size" --virtual-time-budget=15000 \
    --screenshot="$out/$name-${size%%,*}.png" "$url" >/dev/null 2>&1
}

for size in 412,860 1280,800; do
  shot hubs-empty "$size" "http://127.0.0.1:$port/"
  shot pair-confirm "$size" "$web"
  for tab in overview quota sessions backlog; do
    shot "$tab" "$size" "$web&confirm=auto&tab=$tab"
  done
  shot terminal "$size" "$web&confirm=auto&tab=sessions&session=demo"
done
echo "screenshots in $out"
