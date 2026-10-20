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

(make -C "$root" app-build-web APP_FLAGS=--dart-define=E2E_AUTOCONFIRM=true >"$work/flutter.log" 2>&1) \
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

web=$("$bin" pair --url "http://127.0.0.1:$port" --name "demo hub" | sed -n 's/^web: *//p')

shot() {
  local name=$1 size=$2 url=$3 theme=${4:-light} profile scheme=()
  [ -z "${E2E_ONLY:-}" ] || [[ $name == $E2E_ONLY ]] || return 0
  [ "$theme" = dark ] && scheme=(--blink-settings=preferredColorScheme=0)
  profile=$(mktemp -d "$work/chrome.XXXXXX")
  "$chromium" --headless=new --disable-gpu --no-sandbox --user-data-dir="$profile" \
    "${scheme[@]}" --window-size="$size" --virtual-time-budget=20000 \
    --screenshot="$out/$name-${size%%,*}${theme/light/}.png" "$url" >/dev/null 2>&1
}

for size in 412,860 1280,800; do
  for theme in light dark; do
    shot hubs-empty "$size" "http://127.0.0.1:$port/" "$theme"
    shot pair-confirm "$size" "$web" "$theme"
    for tab in overview sleeves nodes notes mail; do
      shot "$tab" "$size" "$web&confirm=auto&tab=$tab" "$theme"
    done
    shot node-detail "$size" "$web&confirm=auto&tab=nodes&node=$node_name" "$theme"
    shot node-ghost "$size" "$web&confirm=auto&tab=nodes&node=ghost" "$theme"
    shot terminal "$size" "$web&confirm=auto&tab=sleeves&session=demo" "$theme"
  done
done
echo "screenshots in $out"
