#!/bin/sh
# Drives the embedded web app against a real hub in headless Chrome and saves screenshots: pairing
# with a code, a message to Envoy and its reply, the sleeve tree after a CLI spawn, and an approval
# the D9 rule raised. Temp home and ephemeral ports only (never the real hub's).
#
#   scripts/app-e2e.sh [OUT_DIR]
#
# Needs a hub binary with the web app embedded (make web, then make build), Chrome or Chromium,
# node >= 22 (global WebSocket), git and Pi with a local model for pi_local:
#   PT_E2E_PI_PROVIDER  a provider in Pi's ~/.pi/agent/models.json (an OpenAI-compatible endpoint)
#   PT_E2E_PI_MODEL     the model to ask it for
# Both are read from the environment, else from the gitignored .env.e2e at the repo root.
# PROTECTORATE_BIN and CHROME override the binary and the browser.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
if [ -f "$root/.env.e2e" ]; then set -a; . "$root/.env.e2e"; set +a; fi
out=${1:-${TMPDIR:-/tmp}/pt-app-e2e}
bin=${PROTECTORATE_BIN:-$root/zig-out/bin/protectorate}
chrome=${CHROME:-}
if [ -z "$chrome" ]; then
    for c in google-chrome chromium chromium-browser; do
        command -v "$c" >/dev/null 2>&1 && chrome=$c && break
    done
fi
provider=${PT_E2E_PI_PROVIDER:-}
model=${PT_E2E_PI_MODEL:-}
if [ -z "$provider" ] || [ -z "$model" ]; then
    echo "app-e2e: set PT_E2E_PI_PROVIDER and PT_E2E_PI_MODEL (or put them in .env.e2e)" >&2
    exit 2
fi
[ -x "$bin" ] || { echo "app-e2e: $bin is missing (make build)" >&2; exit 2; }
[ -n "$chrome" ] || { echo "app-e2e: no Chrome or Chromium found (set CHROME)" >&2; exit 2; }

export PROTECTORATE_NO_HOST_HISTORY=1
mkdir -p "$out"
work=$(mktemp -d "${TMPDIR:-/tmp}/pt-app-e2e.XXXXXX")
home=$work/home
hub_pid=
chrome_pid=
rm_pid=
ok=0
finish() {
    for p in $rm_pid $chrome_pid $hub_pid; do kill "$p" 2>/dev/null || true; done
    sleep 1
    for p in $rm_pid $chrome_pid $hub_pid; do kill -9 "$p" 2>/dev/null || true; done
    if [ "$ok" = 1 ]; then
        rm -rf "$work"
    else
        echo "app-e2e: FAILED; work dir kept at $work (hub log $work/serve.log)" >&2
    fi
}
trap finish EXIT INT TERM

free_port() {
    node -e 'const s=require("net").createServer().listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})'
}
step() { echo "app-e2e: $*"; }
die() { echo "app-e2e: $*" >&2; exit 1; }

# One CDP action per call against the single page of the running browser.
cdp() {
    node --input-type=module -e '
const [port, cmd, ...args] = process.argv.slice(1);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((ok, fail) => { ws.onopen = ok; ws.onerror = fail; });
let id = 0;
const waiting = new Map();
ws.onmessage = (m) => {
  const r = JSON.parse(m.data);
  if (r.id && waiting.has(r.id)) { waiting.get(r.id)(r); waiting.delete(r.id); }
};
const call = (method, params = {}) => new Promise((ok, fail) => {
  const n = ++id;
  waiting.set(n, (r) => (r.error ? fail(new Error(`${method}: ${r.error.message}`)) : ok(r.result)));
  ws.send(JSON.stringify({ id: n, method, params }));
});
const key = (k) => ({
  Enter: { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 },
  Escape: { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 },
}[k]);
switch (cmd) {
  case "open":
    await call("Page.navigate", { url: args[0] });
    break;
  case "shot": {
    const { data } = await call("Page.captureScreenshot", { format: "png" });
    (await import("node:fs")).writeFileSync(args[0], Buffer.from(data, "base64"));
    break;
  }
  case "click":
    for (const type of ["mousePressed", "mouseReleased"]) {
      await call("Input.dispatchMouseEvent",
        { type, x: +args[0], y: +args[1], button: "left", clickCount: 1 });
    }
    break;
  case "type":
    await call("Input.insertText", { text: args[0] });
    break;
  case "key":
    await call("Input.dispatchKeyEvent", { type: "rawKeyDown", ...key(args[0]) });
    await call("Input.dispatchKeyEvent", { type: "keyUp", ...key(args[0]) });
    break;
  default:
    throw new Error(`unknown cdp command ${cmd}`);
}
ws.close();
' "$cdp_port" "$@"
}
# Types a command into the app command line (Escape opens it, Enter runs it and closes it).
appcmd() {
    cdp key Escape
    sleep 0.5
    cdp type "$1"
    cdp key Enter
    sleep 1.5
}

"$bin" serve --home "$home" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox \
    >"$work/serve.log" 2>&1 &
hub_pid=$!
for _ in $(seq 100); do grep -q 'protectorate listening ' "$work/serve.log" 2>/dev/null && break; sleep 0.1; done
line=$(grep 'protectorate listening ' "$work/serve.log" | head -n1)
port=$(echo "$line" | sed -n 's/.*http=[^:]*:\([0-9]*\).*/\1/p')
[ -n "$port" ] || die "hub did not start: $(cat "$work/serve.log")"
url=http://127.0.0.1:$port
pt() { "$bin" --home "$home" --url "$url" "$@"; }
curl -q -fsS "$url/" | grep -q flutter_bootstrap ||
    die "$bin does not serve the web app (build with -Dweb-dir pointing at a built app/build/web)"
agent_token=$("$bin" token --home "$home" | sed -n 's/^envoy=//p')
[ -n "$agent_token" ] || die "no envoy token"

step "hub on $url; starting Envoy"
pt sleeve spawn envoy --harness pi_local --provider "$provider" --model "$model" >/dev/null
pt sleeve wait envoy --until idle --timeout 180 >/dev/null

code=$(pt device add e2e-browser | sed -n 's/.*code=\([A-Z0-9]*\).*/\1/p' | head -n1)
[ -n "$code" ] || die "no pairing code"
cdp_port=$(free_port)
"$chrome" --headless=new --no-first-run --no-default-browser-check --no-sandbox \
    --user-data-dir="$work/chrome" --remote-debugging-port="$cdp_port" \
    --window-size=1280,860 --use-angle=swiftshader --enable-unsafe-swiftshader \
    about:blank 2>"$work/chrome.log" &
chrome_pid=$!
for _ in $(seq 50); do curl -q -fsS "http://127.0.0.1:$cdp_port/json/version" >/dev/null 2>&1 && break; sleep 0.2; done

step "pairing the browser with $code"
cdp open "$url/?pair=$code"
for _ in $(seq 60); do pt --json device list | grep -q '"last_seen_ms": *[1-9]' && break; sleep 0.5; done
pt --json device list | grep -q '"last_seen_ms": *[1-9]' || die "the browser did not pair"
sleep 3
appcmd settings
cdp shot "$out/01-paired-settings.png"

step "sending to Envoy from the app"
appcmd envoy
appcmd "> Reply with exactly the word PONG and nothing else."
for _ in $(seq 20); do pt envoy read | grep -q 'owner: Reply with exactly' && break; sleep 0.5; done
pt envoy read | grep -q 'owner: Reply with exactly' || die "the app's message did not reach Envoy"
pt envoy wait --timeout 60 | grep -q PONG || die "Envoy did not answer PONG"
sleep 2
cdp shot "$out/02-envoy-chat.png"

step "spawning a sleeve from the CLI"
repo=$work/repo
git init -q "$repo"
git -C "$repo" -c user.name=e2e -c user.email=e2e@localhost commit -q --allow-empty -m init
pt sleeve spawn demo --path "$repo" --harness pi_local --provider "$provider" --model "$model" >/dev/null
pt sleeve wait demo --until idle --timeout 180 >/dev/null
appcmd sleeves
cdp shot "$out/03-sleeves-tree.png"

step "raising a D9 approval: Envoy removes a sleeve during an agent's turn"
PROTECTORATE_TOKEN=$agent_token pt sleeve send envoy \
    "Write the numbers from 1 to 1500 as words, one per line, with no other text." >/dev/null
for _ in $(seq 120); do pt sleeve list | grep '^envoy ' | grep -q working && break; sleep 0.5; done
PROTECTORATE_TOKEN=$agent_token pt sleeve rm demo >"$work/rm.out" 2>&1 &
rm_pid=$!
for _ in $(seq 40); do pt approval list --pending | grep -q 'D9' && break; sleep 0.5; done
pt approval list --pending | grep -q 'D9' || die "no D9 approval was raised"
appcmd approvals
cdp shot "$out/04-d9-approval.png"
appcmd envoy
cdp shot "$out/05-envoy-d9-request.png"

step "denying it from the app"
id=$(pt --json approval list --pending |
    node -e 'const a=JSON.parse(require("fs").readFileSync(0,"utf8")).approvals;console.log(a.length?a[0].id:"")')
[ -n "$id" ] || die "no pending approval id"
appcmd "/deny $id"
wait "$rm_pid" || true
rm_pid=
grep -q 'denied' "$work/rm.out" || die "sleeve rm was not refused: $(cat "$work/rm.out")"
step "sleeve rm after the deny: $(cat "$work/rm.out")"
pt sleeve show demo >/dev/null || die "demo is gone after the deny"
sleep 1
appcmd approvals
cdp shot "$out/06-d9-denied.png"
ok=1
step "passed; screenshots in $out"
