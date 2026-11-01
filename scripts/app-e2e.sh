#!/bin/sh
# Drives the embedded web app against a real hub in headless Chromium and saves screenshots:
# pairing with a code, a message to Envoy and its reply, the sleeve tree after a CLI spawn, and
# an approval the D9 rule raised. Temp home and ephemeral ports only (never the real hub's).
#
#   scripts/app-e2e.sh [OUT_DIR]
#
# Needs a release binary with the web app embedded (make web release), chromium, node >= 22
# (global WebSocket) and Pi with a local model for pi_local:
#   PT_E2E_PI_PROVIDER  a provider in Pi's ~/.pi/agent/models.json (an OpenAI-compatible endpoint)
#   PT_E2E_PI_MODEL     the model to ask it for
# Both are read from the environment, else from the gitignored .env.e2e at the repo root.
# PROTECTORATE_BIN and CHROMIUM override the binary and the browser.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
if [ -f "$root/.env.e2e" ]; then set -a; . "$root/.env.e2e"; set +a; fi
out=${1:-${TMPDIR:-/tmp}/pt-app-e2e}
bin=${PROTECTORATE_BIN:-${CARGO_TARGET_DIR:-target}/release/protectorate}
chromium=${CHROMIUM:-chromium}
provider=${PT_E2E_PI_PROVIDER:-}
model=${PT_E2E_PI_MODEL:-}
if [ -z "$provider" ] || [ -z "$model" ]; then
    echo "app-e2e: set PT_E2E_PI_PROVIDER and PT_E2E_PI_MODEL (or put them in .env.e2e)" >&2
    exit 2
fi

mkdir -p "$out"
work=$(mktemp -d "${TMPDIR:-/tmp}/pt-app-e2e.XXXXXX")
home=$work/home
mkdir -p "$home"
hub_pid=
chrome_pid=
rm_pid=
finish() {
    for p in $rm_pid $chrome_pid $hub_pid; do kill "$p" 2>/dev/null || true; done
    echo "app-e2e: work dir $work (hub log $work/serve.log)"
}
trap finish EXIT INT TERM

free_port() {
    node -e 'const s=require("net").createServer().listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})'
}
pt() { "$bin" --home "$home" "$@"; }
step() { echo "app-e2e: $*"; }

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
# Types a command into the app command line (Escape opens it).
appcmd() {
    cdp key Escape
    sleep 0.5
    cdp type "$1"
    cdp key Enter
    sleep 1.5
}

cat >"$home/config.json" <<EOF
{"envoy": {"harness": "pi_local"}, "pi_local_provider": "$provider", "pi_local_model": "$model",
 "d9_approval_timeout_secs": 300, "harness_update_hours": 0}
EOF
port=$(free_port)
"$bin" --home "$home" serve --listen "127.0.0.1:$port" --log-level debug 2>"$work/serve.log" &
hub_pid=$!
url=http://127.0.0.1:$port
for _ in $(seq 50); do pt hub info >/dev/null 2>&1 && break; sleep 0.2; done
curl -q -fsS "$url/" | grep -q flutter_bootstrap ||
    { echo "app-e2e: $bin does not serve the web app (make web release)"; exit 1; }
step "hub on $url; waiting for Envoy"
pt sleeve wait envoy --until idle --timeout 180

code=$(pt device add e2e-browser | sed -n 's/^pairing code for e2e-browser: //p')
cdp_port=$(free_port)
"$chromium" --headless=new --no-first-run --no-default-browser-check \
    --user-data-dir="$work/chrome" --remote-debugging-port="$cdp_port" \
    --window-size=1280,860 --use-angle=swiftshader --enable-unsafe-swiftshader \
    about:blank 2>"$work/chrome.log" &
chrome_pid=$!
for _ in $(seq 50); do curl -q -fsS "http://127.0.0.1:$cdp_port/json/version" >/dev/null 2>&1 && break; sleep 0.2; done

step "pairing the browser with $code"
cdp open "$url/?pair=$code"
for _ in $(seq 60); do pt --json device list | grep -q '"last_seen_ms": *[1-9]' && break; sleep 0.5; done
sleep 3
appcmd settings
cdp shot "$out/01-paired-settings.png"

step "sending to Envoy from the app"
appcmd envoy
appcmd "> Reply with exactly the word PONG and nothing else."
for _ in $(seq 20); do pt envoy read | grep -q 'owner: Reply with exactly' && break; sleep 0.5; done
pt envoy wait --timeout 180 | grep -q PONG || { echo "app-e2e: Envoy did not answer PONG"; exit 1; }
sleep 2
cdp shot "$out/02-envoy-chat.png"

step "spawning a sleeve from the CLI"
repo=$work/repo
git init -q "$repo"
git -C "$repo" -c user.name=e2e -c user.email=e2e@localhost commit -q --allow-empty -m init
pt sleeve spawn demo --path "$repo" --harness pi_local --provider "$provider" --model "$model" >/dev/null
pt sleeve wait demo --until idle --timeout 180
appcmd sleeves
cdp shot "$out/03-sleeves-tree.png"

step "raising a D9 approval: Envoy removes a sleeve during an agent's turn"
agent=$(sed -n 's/^envoy-only token: //p' "$work/serve.log" | head -n1)
PROTECTORATE_TOKEN=$agent pt envoy send --as duet \
    "Write the numbers from 1 to 1500 as words, one per line, with no other text." >/dev/null
for _ in $(seq 120); do pt sleeve show envoy | grep -q working && break; sleep 0.5; done
scope=$(sed -n 's/.*sleeve envoy: launched g[0-9]* ([a-z_]*) in \([^ ]*\),.*/\1/p' "$work/serve.log" | tail -n1)
cg=$(systemctl --user show -p ControlGroup --value "$scope")
envoy_token=
for p in $(cat "/sys/fs/cgroup$cg/cgroup.procs"); do
    t=$(tr '\0' '\n' <"/proc/$p/environ" 2>/dev/null | sed -n 's/^PROTECTORATE_TOKEN=//p')
    [ -n "$t" ] && envoy_token=$t && break
done
[ -n "$envoy_token" ] || { echo "app-e2e: no token in $scope"; exit 1; }
PROTECTORATE_TOKEN=$envoy_token pt sleeve rm demo >"$work/rm.out" 2>&1 &
rm_pid=$!
for _ in $(seq 40); do pt approval list --pending | grep -q 'remove sleeve demo' && break; sleep 0.5; done
appcmd approvals
cdp shot "$out/04-d9-approval.png"
appcmd envoy
cdp shot "$out/05-envoy-d9-request.png"

step "denying it from the app"
id=$(pt --json approval list --pending | sed -n 's/.*"id": *"\(action:[^"]*\)".*/\1/p' | head -n1)
appcmd "/deny $id"
wait "$rm_pid" || true
rm_pid=
grep -q 'did not allow remove sleeve demo' "$work/rm.out" ||
    { echo "app-e2e: sleeve rm was not refused: $(cat "$work/rm.out")"; exit 1; }
step "sleeve rm after the deny: $(cat "$work/rm.out")"
sleep 1
appcmd approvals
cdp shot "$out/06-d9-denied.png"
step "screenshots in $out"
