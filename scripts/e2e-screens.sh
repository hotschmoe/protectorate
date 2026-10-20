#!/usr/bin/env bash
# Builds the web app, runs a throwaway hub with demo data, and screenshots
# every main screen with headless Chromium at narrow and desktop sizes.
# E2E_PHONE=1 captures the Android shell in a debug web build at 360/412 px.
# Output: e2e-out/ (gitignored). Needs go, flutter, tmux, chromium, Node 22+.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${E2E_OUT:-$root/e2e-out}
port=${E2E_PORT:-$((47900 + RANDOM % 100))}
chromium=${CHROMIUM:-chromium}
mkdir -p "$out"
work=$(mktemp -d "$out/work.XXXXXX")
sock=prot-e2e-$RANDOM$RANDOM
while IFS= read -r name; do
  unset "$name"
done < <(compgen -v PROTECTORATE_)
export PROTECTORATE_HOME=$work/home PROTECTORATE_TMUX_SOCKET=$sock PROTECTORATE_HUB=http://127.0.0.1:$port
mkdir -p "$PROTECTORATE_HOME" "$work/user"

hub_pid=
login_pid=
envoy_started=0
cleanup() {
  [ -n "$login_pid" ] && kill "$login_pid" 2>/dev/null || true
  if [ "$envoy_started" = 1 ]; then
    "$bin" api call SleeveService/KillSleeve '{"name":"envoy"}' >/dev/null 2>&1 || true
  fi
  [ -n "$hub_pid" ] && kill "$hub_pid" 2>/dev/null || true
  tmux -L "$sock" kill-server 2>/dev/null || true
}
trap cleanup EXIT

app_flags=
if [ "${E2E_PHONE:-0}" = 1 ]; then
  app_flags='--debug --dart-define=DEBUG_PHONE_SHELL=true'
fi
(make -C "$root" app-build-web APP_FLAGS="$app_flags" >"$work/flutter.log" 2>&1) \
  || { tail -30 "$work/flutter.log"; exit 1; }
make -C "$root" build >/dev/null
bin=$root/bin/protectorate

hub_args=()
if [ "${E2E_PHONE:-0}" = 1 ]; then
  mkdir -p "$work/bin"
  cp "$root/scripts/testdata/claude-screenshot-peer.sh" "$work/bin/claude"
  chmod 700 "$work/bin/claude"
  export PATH="$work/bin:$PATH"
  hub_args=(--sleeve-tool-dir "$work/bin")
fi

mkdir -p "$work/cache"
head -c 3000000 /dev/zero >"$work/cache/blob"
HOME="$work/user" "$bin" hub serve --listen "127.0.0.1:$port" --web "$root/app/build/web" --node-interval 5s \
  --watch "$work/cache" "${hub_args[@]}" >"$work/hub.log" 2>&1 &
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
  local name=$1 size=$2 url=$3 theme=${4:-light} connected=${5:-false} interaction=${6:-} profile
  [ -z "${E2E_ONLY:-}" ] || [[ $name == $E2E_ONLY ]] || return 0
  profile=$(mktemp -d "$work/chrome.XXXXXX")
  node "$root/scripts/e2e-screens.mjs" "$chromium" "$profile" "$size" "$url" "$theme" \
    "$out/$name-${size%%,*}${theme/light/}.png" "$PROTECTORATE_HUB" "$credential" "$connected" "$interaction"
}

wait_envoy() {
  local expected=$1 status
  for _ in $(seq 100); do
    if status=$("$bin" api call SleeveService/GetSleeve '{"name":"envoy"}' 2>"$work/envoy-status.log") &&
        [[ $status == *"$expected"* ]]; then
      return 0
    fi
    sleep 0.2
  done
  echo "Envoy did not reach $expected" >&2
  cat "$work/envoy-status.log" >&2
  tail -30 "$work/hub.log" >&2
  return 1
}

if [ "${E2E_PHONE:-0}" = 1 ]; then
  for size in 360,800 412,860; do
    for theme in light dark; do
      shot phone-envoy-start "$size" "$web&tab=orchestrator" "$theme" true
    done
  done
  "$bin" api call SleeveService/SpawnEnvoy '{"harness":"HARNESS_CLAUDE"}' >"$work/envoy-spawn.json"
  envoy_started=1
  wait_envoy CHAT_STATE_IDLE
  "$bin" api call ChatService/Send '{"sleeve":"envoy","text":"How is the hardware doing? Show the check and ask before restarting anything.","requestId":"phone-screenshot-health"}' >"$work/envoy-send.json"
  wait_envoy CHAT_STATE_AWAITING_APPROVAL
  "$bin" api call DeviceService/StartDeviceAuth '{"name":"new phone","platform":"android"}' >"$work/pending-device.json"
  for size in 360,800 412,860; do
    for theme in light dark; do
      for tab in orchestrator quotas nodes devices; do
        if [ "$tab" = nodes ]; then
          shot "phone-$tab" "$size" "$web&tab=orchestrator" "$theme" true nodes
        elif [ "$tab" = orchestrator ]; then
          shot "phone-$tab" "$size" "$web&tab=$tab" "$theme" true draft
        else
          shot "phone-$tab" "$size" "$web&tab=$tab" "$theme" true
        fi
      done
      shot phone-node-detail "$size" "$web&tab=nodes&node=$node_name" "$theme" true
      shot phone-nodes-updates "$size" "$web&tab=orchestrator" "$theme" true updates
    done
  done
else
  for size in 412,860 1280,800; do
    for theme in light dark; do
      shot hubs-empty "$size" "http://127.0.0.1:$port/" "$theme"
      shot pair-confirm "$size" "$web" "$theme"
      for tab in overview sleeves nodes notes mail devices; do
        shot "$tab" "$size" "$web&tab=$tab" "$theme" true
      done
      shot node-detail "$size" "$web&tab=nodes&node=$node_name" "$theme" true
      shot node-ghost "$size" "$web&tab=nodes&node=ghost" "$theme" true
      shot terminal "$size" "$web&tab=sleeves&session=demo" "$theme" true
    done
  done
fi
echo "screenshots in $out"
