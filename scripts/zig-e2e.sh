#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PT="$ROOT_DIR/zig-out/bin/protectorate"
PT_NODE="${PT_E2E_NODE_BIN:-$ROOT_DIR/zig-out/bin/protectorate-node}"

if [[ ! -x "$PT" || ! -x "$PT_NODE" ]]; then
  echo "missing binaries under zig-out/bin; run zig build -Doptimize=safe first" >&2
  exit 1
fi

TMP_ROOT="$(mktemp -d /tmp/pt-zig-e2e.XXXXXX)"
export PROTECTORATE_NO_HOST_HISTORY=1
PIDS=()

cleanup() {
  local rc=$?
  for pid in "${PIDS[@]}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  sleep 0.3
  for pid in "${PIDS[@]}"; do
    kill -9 "$pid" >/dev/null 2>&1 || true
    wait "$pid" >/dev/null 2>&1 || true
  done
  if [[ $rc -eq 0 ]] && ! check_crashes "$TMP_ROOT"/*.log; then
    rc=1
  fi
  rm -rf "$TMP_ROOT"
  exit $rc
}
trap cleanup EXIT

check_crashes() {
  if grep -iH -E 'Segmentation fault|panic' "$@"; then
    echo "hub or node crash detected" >&2
    return 1
  fi
}

wait_for_listen() {
  local log_file="$1"
  local i=0
  while [[ $i -lt 100 ]]; do
    if grep -q "protectorate listening " "$log_file" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
    i=$((i + 1))
  done
  echo "hub failed to start listening; log:" >&2
  cat "$log_file" >&2
  return 1
}

extract_port() {
  local line="$1"
  local key="$2"
  sed -n "s/.*${key}=[^:]*:\([0-9]*\).*/\1/p" <<<"$line"
}

rpc_post() {
  local port="$1"
  local token="$2"
  local method="$3"
  local body="$4"
  if [[ -n "$token" ]]; then
    curl -sf -X POST -H "Content-Type: application/json" -H "Authorization: Bearer $token" -d "$body" "http://127.0.0.1:${port}/api/${method}"
  else
    curl -sf -X POST -H "Content-Type: application/json" -d "$body" "http://127.0.0.1:${port}/api/${method}"
  fi
}

echo "=== E2E 1: Hub Web / REST / WS / Pairing / Envoy MCP ==="
HUB1="$TMP_ROOT/hub1"
LOG1="$TMP_ROOT/hub1.log"
mkdir "$TMP_ROOT/empty-path"
PATH="$TMP_ROOT/empty-path" "$PT" serve --home "$HUB1" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox >"$LOG1" 2>&1 &
PID1=$!
PIDS+=("$PID1")
wait_for_listen "$LOG1"
LINE1="$(grep 'protectorate listening ' "$LOG1" | head -n1)"
HTTP1="$(extract_port "$LINE1" http)"
TOK1="$("$PT" token --home "$HUB1" | sed -n 's/^owner=//p')"

curl -sf "http://127.0.0.1:${HTTP1}/health" | grep -q '"ok":true'
curl -sf "http://127.0.0.1:${HTTP1}/" | grep -qiE '<!doctype html>|<html'
HDR1="$TMP_ROOT/web_hdr.txt"
curl -sf -D "$HDR1" -H "Accept-Encoding: gzip" "http://127.0.0.1:${HTTP1}/" | gzip -dc | grep -qiE '<!doctype html>|<html'
grep -qi '^content-encoding: *gzip' "$HDR1"

UNAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -X POST -d '{}' "http://127.0.0.1:${HTTP1}/api/GetState")"
if [[ "$UNAUTH_CODE" != "401" ]]; then
  echo "expected 401 unauthenticated GetState, got $UNAUTH_CODE" >&2
  exit 1
fi

PAIR_RESP="$(rpc_post "$HTTP1" "$TOK1" AddDevice '{"name":"e2e-tablet"}')"
PAIR_CODE="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["code"])' <<<"$PAIR_RESP")"
CLAIMED_JSON="$(rpc_post "$HTTP1" "" PairDevice "{\"code\":\"${PAIR_CODE}\"}")"
DEV_TOK="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])' <<<"$CLAIMED_JSON")"
rpc_post "$HTTP1" "$DEV_TOK" GetState '{}' | grep -q '"hub"'

python3 - "$HTTP1" "$TOK1" <<'PY'
import base64, hashlib, json, os, socket, struct, sys

port = int(sys.argv[1])
tok = sys.argv[2]

key = base64.b64encode(os.urandom(16)).decode()
sock = socket.create_connection(("127.0.0.1", port), timeout=5)
req = (
    f"GET /api/events?access_token={tok} HTTP/1.1\r\n"
    f"Host: 127.0.0.1:{port}\r\n"
    f"Upgrade: websocket\r\n"
    f"Connection: Upgrade\r\n"
    f"Sec-WebSocket-Key: {key}\r\n"
    f"Sec-WebSocket-Version: 13\r\n\r\n"
)
sock.sendall(req.encode())

buf = b""
while b"\r\n\r\n" not in buf:
    buf += sock.recv(4096)
head, rest = buf.split(b"\r\n\r\n", 1)
assert b"101 " in head, f"ws upgrade failed: {head}"

def recv_ws_frame():
    global rest
    while len(rest) < 2:
        rest += sock.recv(4096)
    b0, b1 = rest[0], rest[1]
    pos = 2
    ln = b1 & 0x7f
    if ln == 126:
        while len(rest) < pos + 2:
            rest += sock.recv(4096)
        ln = struct.unpack("!H", rest[pos:pos+2])[0]
        pos += 2
    elif ln == 127:
        while len(rest) < pos + 8:
            rest += sock.recv(4096)
        ln = struct.unpack("!Q", rest[pos:pos+8])[0]
        pos += 8
    while len(rest) < pos + ln:
        rest += sock.recv(4096)
    payload = rest[pos:pos+ln]
    rest = rest[pos+ln:]
    return b0 & 0x0f, payload

op1, p1 = recv_ws_frame()
op2, p2 = recv_ws_frame()
m1 = json.loads(p1)
m2 = json.loads(p2)
assert m1["type"] == "snapshot", m1
assert m2["type"] == "live", m2
sock.close()
PY

MCP_OUT="$("$PT" --envoy --url "http://127.0.0.1:${HTTP1}" --token "$TOK1" <<'JSON'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}}
JSON
)"
grep -q 'protectorate-envoy' <<<"$MCP_OUT"
grep -q 'spawn_sleeve' <<<"$MCP_OUT"
grep -q '"isError":false' <<<"$MCP_OUT"
rpc_post "$HTTP1" "$TOK1" GetDoctor '{}' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert all(not h["present"] for h in d["harnesses"]) and "git binary not found" in d["problems"], d'
ZOMBIES1="$(ps -eo pid,stat,ppid,comm | awk -v hub="$PID1" '$3 == hub && $2 ~ /^Z/')"
if [[ -n "$ZOMBIES1" ]]; then
  echo "hub retained children after failed process launches:" >&2
  echo "$ZOMBIES1" >&2
  exit 1
fi
kill "$PID1" >/dev/null 2>&1 || true
wait "$PID1" >/dev/null 2>&1 || true
check_crashes "$LOG1"
echo "E2E 1 PASSED"

echo "=== E2E 2 & 4: Real pi + hotschmoe-dd (bwrap), Crash/Replay & History BM25 ==="
if [[ "${SKIP_PI_E2E:-0}" == "1" ]]; then
  echo "=== E2E 2 & 4 SKIPPED (SKIP_PI_E2E=1) ==="
else
if ! command -v pi >/dev/null 2>&1; then
  echo "pi CLI not found in PATH" >&2
  exit 1
fi
HUB2="$TMP_ROOT/hub2"
REPO2="$TMP_ROOT/repo2"
mkdir -p "$REPO2"
git -C "$REPO2" init -q
git -C "$REPO2" config user.email "e2e@example.com"
git -C "$REPO2" config user.name "E2E"
echo "hello protectorate" > "$REPO2/README.md"
git -C "$REPO2" add README.md
git -C "$REPO2" commit -qm "init"

LOG2="$TMP_ROOT/hub2.log"
"$PT" serve --home "$HUB2" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 >"$LOG2" 2>&1 &
PID2=$!
PIDS+=("$PID2")
wait_for_listen "$LOG2"
LINE2="$(grep 'protectorate listening ' "$LOG2" | head -n1)"
HTTP2="$(extract_port "$LINE2" http)"
TOK2="$("$PT" token --home "$HUB2" | sed -n 's/^owner=//p')"

SPAWN_REQ="$(python3 - "$REPO2" <<'PY'
import json, sys
print(json.dumps({
    "name": "e2e-pi",
    "harness": "pi_local",
    "provider": "hotschmoe-local",
    "model": "hotschmoe-dd",
    "path": sys.argv[1],
    "message": "Reply with exactly the token E2E_PI_MARKER_7391 and nothing else."
}))
PY
)"
SPAWN_RESP="$(rpc_post "$HTTP2" "$TOK2" SpawnSleeve "$SPAWN_REQ")"
SLV2="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$SPAWN_RESP")"

MARKER_FOUND=0
for _ in $(seq 1 90); do
  TX_RESP="$(rpc_post "$HTTP2" "$TOK2" GetTranscript "{\"sleeve_id\":\"${SLV2}\"}")"
  if grep -q 'E2E_PI_MARKER_7391' <<<"$TX_RESP" && grep -q 'turn_settled' <<<"$TX_RESP"; then
    MARKER_FOUND=1
    break
  fi
  sleep 1
done
if [[ "$MARKER_FOUND" -ne 1 ]]; then
  echo "pi + hotschmoe-dd turn did not settle with marker in 90s; transcript:" >&2
  rpc_post "$HTTP2" "$TOK2" GetTranscript "{\"sleeve_id\":\"${SLV2}\"}" >&2 || true
  exit 1
fi

# Test SIGKILL + Journal Crash Replay
kill -9 "$PID2" >/dev/null 2>&1 || true
wait "$PID2" >/dev/null 2>&1 || true
check_crashes "$LOG2"

LOG2B="$TMP_ROOT/hub2b.log"
"$PT" serve --home "$HUB2" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 >"$LOG2B" 2>&1 &
PID2B=$!
PIDS+=("$PID2B")
wait_for_listen "$LOG2B"
LINE2B="$(grep 'protectorate listening ' "$LOG2B" | head -n1)"
HTTP2B="$(extract_port "$LINE2B" http)"

STATE2B="$(rpc_post "$HTTP2B" "$TOK2" GetState '{}')"
python3 -c '
import json, sys
slv_id = sys.argv[1]
st = json.load(sys.stdin)
assert st["hub"]["restore_error"] is None, st["hub"]
found = [s for s in st["sleeves"] if s["id"] == slv_id]
assert len(found) == 1, st["sleeves"]
assert found[0]["lifecycle"] == "stopped", found[0]
' "$SLV2" <<<"$STATE2B"
rpc_post "$HTTP2B" "$TOK2" GetTranscript "{\"sleeve_id\":\"${SLV2}\"}" | grep -q 'E2E_PI_MARKER_7391'
echo "E2E 2 PASSED"

# E2E 4: BM25 History search on restored transcript
HIST_RESP="$(rpc_post "$HTTP2B" "$TOK2" SearchHistory '{"query":"E2E_PI_MARKER_7391"}')"
python3 -c '
import json, sys
h = json.load(sys.stdin)
assert h["indexed_docs"] > 0, h
assert len(h["hits"]) >= 1, h
assert "E2E_PI_MARKER_7391" in h["hits"][0]["snippet"], h["hits"][0]
' <<<"$HIST_RESP"
kill "$PID2B" >/dev/null 2>&1 || true
wait "$PID2B" >/dev/null 2>&1 || true
check_crashes "$LOG2B"
echo "E2E 4 PASSED"
fi

echo "=== E2E 3: Noise_IK Node Enrollment, Jobs, SHA-256 Artifacts, Rejects, Cancel, Reconnect ==="
HUB3="$TMP_ROOT/hub3"
NODE3="$TMP_ROOT/node3"
NODE_BAD="$TMP_ROOT/node_bad"
LOG3="$TMP_ROOT/hub3.log"
"$PT" serve --home "$HUB3" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox >"$LOG3" 2>&1 &
PID3=$!
PIDS+=("$PID3")
wait_for_listen "$LOG3"
LINE3="$(grep 'protectorate listening ' "$LOG3" | head -n1)"
HTTP3="$(extract_port "$LINE3" http)"
FLEET3="$(extract_port "$LINE3" fleet)"
TOK3="$("$PT" token --home "$HUB3" | sed -n 's/^owner=//p')"

ENROLL="$(rpc_post "$HTTP3" "$TOK3" GetEnrollment '{}')"
JOIN_TOK="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["control_key"])' <<<"$ENROLL")"

JOIN_OUT="$("$PT_NODE" --home "$NODE3" join "$JOIN_TOK" --name e2e-node-1)"
python3 - "$NODE3/config.json" "$FLEET3" <<'PY'
import json, sys
p = sys.argv[1]
port = sys.argv[2]
with open(p) as f:
    cfg = json.load(f)
cfg["addrs"] = ["127.0.0.1:1", f"127.0.0.1:{port}"]
with open(p, "w") as f:
    json.dump(cfg, f, indent=2)
PY
NODE_KEY="$(sed -n 's/^  node_key: *//p' <<<"$JOIN_OUT")"
if [[ -z "$NODE_KEY" ]]; then
  echo "failed to parse node_key" >&2
  exit 1
fi

# Unapproved node MUST be rejected
if "$PT_NODE" --home "$NODE3" authorize >/dev/null 2>&1; then
  echo "expected unapproved node authorize to fail" >&2
  exit 1
fi

ADD_RESP="$(rpc_post "$HTTP3" "$TOK3" AddNode "{\"registration\":\"e2e-node-1:${NODE_KEY}\"}")"
AUTH_ID="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["auth_id"])' <<<"$ADD_RESP")"
rpc_post "$HTTP3" "$TOK3" ApproveNode "{\"auth_id\":\"${AUTH_ID}\",\"name\":\"e2e-node-1\"}" >/dev/null

# Wrong key node MUST ALSO be rejected
"$PT_NODE" --home "$NODE_BAD" join "$JOIN_TOK" --name intruder >/dev/null
if "$PT_NODE" --home "$NODE_BAD" authorize >/dev/null 2>&1; then
  echo "expected wrong-key node authorize to fail" >&2
  exit 1
fi

"$PT_NODE" --home "$NODE3" authorize >/dev/null
"$PT_NODE" --home "$NODE3" serve >"$TMP_ROOT/node3.log" 2>&1 &
NPID3=$!
PIDS+=("$NPID3")

ONLINE=0
for _ in $(seq 1 50); do
  NODES_JSON="$(rpc_post "$HTTP3" "$TOK3" ListNodes '{}')"
  if python3 -c '
import json, sys
nid = sys.argv[1]
ns = json.load(sys.stdin)["nodes"]
m = [n for n in ns if n["id"] == nid and n["online"] and n.get("inventory")]
sys.exit(0 if m else 1)
' "$AUTH_ID" <<<"$NODES_JSON"
  then
    ONLINE=1
    break
  fi
  sleep 0.2
done
if [[ "$ONLINE" -ne 1 ]]; then
  echo "node did not come online with inventory" >&2
  exit 1
fi

# A second connection with the same key replaces the still-live session.
"$PT_NODE" --home "$NODE3" authorize >/dev/null
REPLACED=0
for _ in $(seq 1 100); do
  NODES_JSON="$(rpc_post "$HTTP3" "$TOK3" ListNodes '{}')"
  if python3 -c '
import json, sys
nid = sys.argv[1]
ns = json.load(sys.stdin)["nodes"]
m = [n for n in ns if n["id"] == nid and n["online"] and n.get("inventory")]
sys.exit(0 if m else 1)
' "$AUTH_ID" <<<"$NODES_JSON"
  then
    REPLACED=1
    break
  fi
  sleep 0.2
done
if [[ "$REPLACED" -ne 1 ]]; then
  echo "node did not reconnect after session replacement" >&2
  exit 1
fi

# RunJob with artifact
JOB1_REQ="{\"target\":{\"type\":\"node\",\"node_id\":\"${AUTH_ID}\"},\"argv\":[\"sh\",\"-c\",\"echo E2E_JOB_STDOUT; printf 'artifact-payload-99' > out.bin; head -c 600000 /dev/zero > multi.bin\"],\"artifacts\":[\"out.bin\",\"multi.bin\"]}"
JOB1_RESP="$(rpc_post "$HTTP3" "$TOK3" RunJob "$JOB1_REQ")"
JID1="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$JOB1_RESP")"

JOB1_DONE=0
for _ in $(seq 1 60); do
  J_ST="$(rpc_post "$HTTP3" "$TOK3" GetJob "{\"job_id\":\"${JID1}\"}")"
  if python3 -c '
import json, sys
j = json.load(sys.stdin)
sys.exit(0 if j["state"] == "succeeded" and len(j.get("artifacts", [])) == 2 else 1)
' <<<"$J_ST"
  then
    JOB1_DONE=1
    break
  fi
  sleep 0.2
done
if [[ "$JOB1_DONE" -ne 1 ]]; then
  echo "job 1 did not succeed with artifact" >&2
  rpc_post "$HTTP3" "$TOK3" GetJob "{\"job_id\":\"${JID1}\"}" >&2 || true
  exit 1
fi

rpc_post "$HTTP3" "$TOK3" GetJobLog "{\"job_id\":\"${JID1}\"}" | grep -q 'E2E_JOB_STDOUT'
ART_JSON="$(rpc_post "$HTTP3" "$TOK3" GetJobArtifact "{\"job_id\":\"${JID1}\",\"path\":\"out.bin\"}")"
python3 -c '
import hashlib, json, sys
a = json.load(sys.stdin)
raw = bytes.fromhex(a["hex"])
assert raw == b"artifact-payload-99", raw
want_sha = hashlib.sha256(raw).hexdigest()
assert a["sha256"] == want_sha, (a["sha256"], want_sha)
assert a["size_bytes"] == len(raw), a
' <<<"$ART_JSON"
MULTI_JSON="$(rpc_post "$HTTP3" "$TOK3" GetJobArtifact "{\"job_id\":\"${JID1}\",\"path\":\"multi.bin\"}")"
python3 -c '
import hashlib, json, sys
a = json.load(sys.stdin)
raw = bytes.fromhex(a["hex"])
assert raw == bytes(600000), len(raw)
assert a["sha256"] == hashlib.sha256(raw).hexdigest(), a["sha256"]
' <<<"$MULTI_JSON"

# CancelJob
JOB2_RESP="$(rpc_post "$HTTP3" "$TOK3" RunJob "{\"target\":{\"type\":\"node\",\"node_id\":\"${AUTH_ID}\"},\"argv\":[\"sleep\",\"30\"]}")"
JID2="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$JOB2_RESP")"
sleep 0.4
rpc_post "$HTTP3" "$TOK3" CancelJob "{\"job_id\":\"${JID2}\"}" >/dev/null
CANC_DONE=0
for _ in $(seq 1 40); do
  J2_ST="$(rpc_post "$HTTP3" "$TOK3" GetJob "{\"job_id\":\"${JID2}\"}")"
  if grep -q '"state":"cancelled"' <<<"$J2_ST"; then
    CANC_DONE=1
    break
  fi
  sleep 0.2
done
if [[ "$CANC_DONE" -ne 1 ]]; then
  echo "job 2 did not cancel" >&2
  exit 1
fi

# Reconnect after hub restart on same fleet port
kill "$PID3" >/dev/null 2>&1 || true
wait "$PID3" >/dev/null 2>&1 || true
check_crashes "$LOG3"

LOG3B="$TMP_ROOT/hub3b.log"
"$PT" serve --home "$HUB3" --listen 127.0.0.1:0 --fleet-listen "127.0.0.1:${FLEET3}" --no-sandbox >"$LOG3B" 2>&1 &
PID3B=$!
PIDS+=("$PID3B")
wait_for_listen "$LOG3B"
LINE3B="$(grep 'protectorate listening ' "$LOG3B" | head -n1)"
HTTP3B="$(extract_port "$LINE3B" http)"

RECONN=0
for _ in $(seq 1 80); do
  NODES_JSON="$(rpc_post "$HTTP3B" "$TOK3" ListNodes '{}')"
  if python3 -c '
import json, sys
nid = sys.argv[1]
ns = json.load(sys.stdin)["nodes"]
m = [n for n in ns if n["id"] == nid and n["online"]]
sys.exit(0 if m else 1)
' "$AUTH_ID" <<<"$NODES_JSON"
  then
    RECONN=1
    break
  fi
  sleep 0.25
done
if [[ "$RECONN" -ne 1 ]]; then
  echo "node did not reconnect automatically" >&2
  exit 1
fi

JOB3_RESP="$(rpc_post "$HTTP3B" "$TOK3" RunJob "{\"target\":{\"type\":\"node\",\"node_id\":\"${AUTH_ID}\"},\"argv\":[\"echo\",\"E2E_RECONNECTED\"]}")"
JID3="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$JOB3_RESP")"
J3_OK=0
for _ in $(seq 1 40); do
  J3_ST="$(rpc_post "$HTTP3B" "$TOK3" GetJob "{\"job_id\":\"${JID3}\"}")"
  if grep -q '"state":"succeeded"' <<<"$J3_ST"; then
    J3_OK=1
    break
  fi
  sleep 0.2
done
if [[ "$J3_OK" -ne 1 ]]; then
  echo "post-reconnect job failed: $J3_ST" >&2
  tail -n 20 "$TMP_ROOT/node3.log" "$LOG3B" >&2 || true
  exit 1
fi
rpc_post "$HTTP3B" "$TOK3" GetJobLog "{\"job_id\":\"${JID3}\"}" | grep -q 'E2E_RECONNECTED'

# An artifact declared above the 64 MiB cap is refused before transfer with the cap in the error
# (last, because a failed job excludes its node for a while)
JOBBIG_RESP="$(rpc_post "$HTTP3B" "$TOK3" RunJob "{\"target\":{\"type\":\"node\",\"node_id\":\"${AUTH_ID}\"},\"argv\":[\"sh\",\"-c\",\"head -c 70000000 /dev/zero > big.bin\"],\"artifacts\":[\"big.bin\"]}")"
JIDBIG="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$JOBBIG_RESP")"
JOBBIG_DONE=0
for _ in $(seq 1 100); do
  JB_ST="$(rpc_post "$HTTP3B" "$TOK3" GetJob "{\"job_id\":\"${JIDBIG}\"}")"
  if python3 -c '
import json, sys
j = json.load(sys.stdin)
sys.exit(0 if j["state"] == "failed" and "exceeds the 67108864 byte artifact cap" in (j.get("error") or "") else 1)
' <<<"$JB_ST"
  then
    JOBBIG_DONE=1
    break
  fi
  sleep 0.2
done
if [[ "$JOBBIG_DONE" -ne 1 ]]; then
  echo "oversized artifact was not refused with the cap in the error" >&2
  rpc_post "$HTTP3B" "$TOK3" GetJob "{\"job_id\":\"${JIDBIG}\"}" >&2 || true
  exit 1
fi
if [[ -n "$(find "$HUB3" \( -name 'big.bin*' -o -name '*.ptpart' \) -print -quit)" ]]; then
  echo "refused artifact left files on the hub" >&2
  exit 1
fi

kill "$NPID3" "$PID3B" >/dev/null 2>&1 || true
wait "$NPID3" "$PID3B" >/dev/null 2>&1 || true
check_crashes "$LOG3B" "$TMP_ROOT/node3.log"
echo "E2E 3 PASSED"

echo "=== E2E 4b: __guard Landlock Write Containment ==="
GUARD_ALLOW="$TMP_ROOT/guard_allow"
GUARD_DENY="$TMP_ROOT/guard_deny"
mkdir -p "$GUARD_ALLOW" "$GUARD_DENY"
"$PT" __guard --cwd "$GUARD_ALLOW" --rw "$GUARD_ALLOW" -- /bin/sh -c 'touch ok.txt'
if [[ ! -f "$GUARD_ALLOW/ok.txt" ]]; then
  echo "__guard failed to allow write inside workspace" >&2
  exit 1
fi
if "$PT" __guard --cwd "$GUARD_ALLOW" --rw "$GUARD_ALLOW" -- /bin/sh -c "touch '$GUARD_DENY/nope.txt'" 2>/dev/null; then
  echo "__guard failed to block write outside --rw" >&2
  exit 1
fi
echo "E2E 4b PASSED"

echo "=== E2E 5: Cross-Compiled release-all Smoke Checks ==="
REL="$ROOT_DIR/zig-out/release"
for spec in \
  "x86_64-linux-musl/protectorate:ELF 64-bit.*x86-64" \
  "x86_64-linux-musl/protectorate-node:ELF 64-bit.*x86-64" \
  "aarch64-linux-musl/protectorate:ELF 64-bit.*ARM aarch64" \
  "aarch64-linux-musl/protectorate-node:ELF 64-bit.*ARM aarch64" \
  "x86_64-windows-gnu/protectorate.exe:PE32\+.*x86-64" \
  "x86_64-windows-gnu/protectorate-node.exe:PE32\+.*x86-64" \
  "aarch64-macos/protectorate:Mach-O 64-bit.*arm64" \
  "aarch64-macos/protectorate-node:Mach-O 64-bit.*arm64"
do
  path="${spec%%:*}"
  pat="${spec#*:}"
  full="$REL/$path"
  if [[ ! -f "$full" ]]; then
    echo "missing release binary: $full" >&2
    exit 1
  fi
  fdesc="$(file "$full")"
  if ! grep -qE "$pat" <<<"$fdesc"; then
    echo "unexpected file format for $full: $fdesc" >&2
    exit 1
  fi
done

"$REL/x86_64-linux-musl/protectorate" --version
"$REL/x86_64-linux-musl/protectorate-node" --version
echo "E2E 5 PASSED"

echo "=== E2E 6: Parity Checks (Presence, Disks, UpdateNodes, Worktrees, Takeover, CLI Tables) ==="
HUB6="$TMP_ROOT/hub6"
NODE6="$TMP_ROOT/node6"
LOG6="$TMP_ROOT/hub6.log"
PRES_FILE="$TMP_ROOT/pres6.txt"
echo "AWAY" > "$PRES_FILE"

"$PT" serve --home "$HUB6" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox >"$LOG6" 2>&1 &
PID6=$!
PIDS+=("$PID6")
wait_for_listen "$LOG6"
LINE6="$(grep 'protectorate listening ' "$LOG6" | head -n1)"
HTTP6="$(extract_port "$LINE6" http)"
FLEET6="$(extract_port "$LINE6" fleet)"
TOK6="$("$PT" token --home "$HUB6" | sed -n 's/^owner=//p')"

ENROLL6="$(rpc_post "$HTTP6" "$TOK6" GetEnrollment '{}')"
JTOK6="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["control_key"])' <<<"$ENROLL6")"
JOUT6="$("$PT_NODE" --home "$NODE6" join "$JTOK6" --name e2e-node-6)"
NKEY6="$(sed -n 's/^  node_key: *//p' <<<"$JOUT6")"
python3 - "$NODE6/config.json" "$FLEET6" <<'PY'
import json, sys
p, port = sys.argv[1], sys.argv[2]
with open(p) as f:
    cfg = json.load(f)
cfg["addrs"] = [f"127.0.0.1:{port}"]
with open(p, "w") as f:
    json.dump(cfg, f, indent=2)
PY

ADD6="$(rpc_post "$HTTP6" "$TOK6" AddNode "{\"registration\":\"e2e-node-6:${NKEY6}\"}")"
AID6="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["auth_id"])' <<<"$ADD6")"
rpc_post "$HTTP6" "$TOK6" ApproveNode "{\"auth_id\":\"${AID6}\",\"name\":\"e2e-node-6\"}" >/dev/null

PROTECTORATE_NODE_TEST_PRESENCE="$PRES_FILE" "$PT_NODE" --home "$NODE6" authorize >/dev/null
PROTECTORATE_NODE_TEST_PRESENCE="$PRES_FILE" "$PT_NODE" --home "$NODE6" serve >"$TMP_ROOT/node6.log" 2>&1 &
NPID6=$!
PIDS+=("$NPID6")

TELE6_OK=0
for _ in $(seq 1 50); do
  NJ6="$(rpc_post "$HTTP6" "$TOK6" ListNodes '{}')"
  if python3 -c '
import json, sys
nid = sys.argv[1]
ns = [n for n in json.load(sys.stdin)["nodes"] if n["id"] == nid and n["online"] and n.get("inventory")]
if not ns:
    sys.exit(1)
n = ns[0]
inv = n["inventory"]
tcs = {t["name"] for t in inv.get("toolchains", [])}
if len(inv.get("disks", [])) < 1 or "git" not in tcs:
    sys.exit(1)
if n.get("presence", {}).get("state") != "away":
    sys.exit(1)
sys.exit(0)
' "$AID6" <<<"$NJ6"
  then
    TELE6_OK=1
    break
  fi
  sleep 0.2
done
if [[ "$TELE6_OK" -ne 1 ]]; then
  echo "node 6 did not report disks/git toolchain/away presence" >&2
  exit 1
fi

echo "PRESENT" > "$PRES_FILE"
PRES_OK=0
for _ in $(seq 1 40); do
  NJ6B="$(rpc_post "$HTTP6" "$TOK6" ListNodes '{}')"
  if python3 -c '
import json, sys
nid = sys.argv[1]
ns = [n for n in json.load(sys.stdin)["nodes"] if n["id"] == nid]
sys.exit(0 if ns and ns[0].get("presence", {}).get("state") == "present" else 1)
' "$AID6" <<<"$NJ6B"
  then
    PRES_OK=1
    break
  fi
  sleep 0.25
done
if [[ "$PRES_OK" -ne 1 ]]; then
  echo "node 6 presence override did not flip to present" >&2
  exit 1
fi

RO6="$(rpc_post "$HTTP6" "$TOK6" UpdateNodes '{"nodes":["e2e-node-6"],"version":"v0.7.0"}')"
python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d.get("rollout_id"), d
assert len(d.get("nodes", [])) == 1 and d["nodes"][0]["node_id"] == sys.argv[1], d
' "$AID6" <<<"$RO6"

REPO6="$TMP_ROOT/repo6"
mkdir -p "$REPO6"
git -C "$REPO6" init -b main >/dev/null
git -C "$REPO6" config user.email "e2e@protectorate.local"
git -C "$REPO6" config user.name "Protectorate E2E"
echo "hi" > "$REPO6/README.md"
git -C "$REPO6" add README.md
git -C "$REPO6" commit -m init >/dev/null

WT_RESP="$(rpc_post "$HTTP6" "$TOK6" SpawnSleeve "{\"name\":\"wt6\",\"harness\":\"pi_local\",\"path\":\"${REPO6}\",\"view\":\"worktree\"}")"
read -r WT_ID WT_PATH <<<"$(python3 -c '
import json, sys
s = json.load(sys.stdin)
assert s["path"] != sys.argv[1], s
assert s.get("branch", "").startswith("pt/"), s
print(s["id"], s["path"])
' "$REPO6" <<<"$WT_RESP")"

WT_OK=0
for _ in $(seq 1 30); do
  if git -C "$WT_PATH" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    WT_OK=1
    break
  fi
  sleep 0.1
done
if [[ "$WT_OK" -ne 1 ]]; then
  echo "worktree was not provisioned at $WT_PATH" >&2
  exit 1
fi

TO_RESP="$(rpc_post "$HTTP6" "$TOK6" TakeoverTerminal "{\"sleeve_id\":\"${WT_ID}\"}")"
if [[ "$TO_RESP" != "{}" ]]; then
  echo "expected empty JSON object from TakeoverTerminal, got $TO_RESP" >&2
  exit 1
fi
rpc_post "$HTTP6" "$TOK6" GetTranscript "{\"sleeve_id\":\"${WT_ID}\"}" | grep -q 'takeover'
rpc_post "$HTTP6" "$TOK6" RemoveSleeve "{\"sleeve_id\":\"${WT_ID}\"}" >/dev/null
for _ in $(seq 1 30); do
  if [[ ! -d "$WT_PATH" ]]; then break; fi
  sleep 0.1
done
if [[ -d "$WT_PATH" ]]; then
  echo "worktree $WT_PATH was not removed" >&2
  exit 1
fi

"$PT" doctor | grep -q 'HARNESS'
"$PT" doctor --json | python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d["harnesses"]) >= 5'
"$PT" --url "http://127.0.0.1:${HTTP6}" --token "$TOK6" status | grep -q 'hub='
"$PT" --url "http://127.0.0.1:${HTTP6}" --token "$TOK6" status --json | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "sleeves" in d and "nodes" in d'
"$PT" --url "http://127.0.0.1:${HTTP6}" --token "$TOK6" node list | grep -q 'e2e-node-6'

kill "$NPID6" "$PID6" >/dev/null 2>&1 || true
wait "$NPID6" "$PID6" >/dev/null 2>&1 || true
check_crashes "$LOG6" "$TMP_ROOT/node6.log"
echo "E2E 6 PASSED"

echo "=== E2E 7: Node Admission Authority, Fencing, Child Borrows, Local Socket, & CLI ==="
HUB7="$TMP_ROOT/hub7"
NODE7="$TMP_ROOT/node7"
LOG7="$TMP_ROOT/hub7.log"
"$PT" serve --home "$HUB7" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox >"$LOG7" 2>&1 &
PID7=$!
PIDS+=("$PID7")
wait_for_listen "$LOG7"
LINE7="$(grep 'protectorate listening ' "$LOG7" | head -n1)"
HTTP7="$(extract_port "$LINE7" http)"
FLEET7="$(extract_port "$LINE7" fleet)"
TOK7="$("$PT" token --home "$HUB7" | sed -n 's/^owner=//p')"

ENROLL7="$(rpc_post "$HTTP7" "$TOK7" GetEnrollment '{}')"
JTOK7="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["control_key"])' <<<"$ENROLL7")"
JOUT7="$("$PT_NODE" --home "$NODE7" join "$JTOK7" --name e2e-node-7)"
NKEY7="$(sed -n 's/^  node_key: *//p' <<<"$JOUT7")"
python3 - "$NODE7/config.json" "$FLEET7" <<'PY'
import json, sys
p, port = sys.argv[1], sys.argv[2]
with open(p) as f:
    cfg = json.load(f)
cfg["addrs"] = [f"127.0.0.1:{port}"]
cfg["slots"] = 4
cfg["long_slots"] = 1
with open(p, "w") as f:
    json.dump(cfg, f, indent=2)
PY

ADD7="$(rpc_post "$HTTP7" "$TOK7" AddNode "{\"registration\":\"e2e-node-7:${NKEY7}\"}")"
AID7="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["auth_id"])' <<<"$ADD7")"
rpc_post "$HTTP7" "$TOK7" ApproveNode "{\"auth_id\":\"${AID7}\",\"name\":\"e2e-node-7\"}" >/dev/null
"$PT_NODE" --home "$NODE7" authorize >/dev/null
"$PT_NODE" --home "$NODE7" serve >"$TMP_ROOT/node7.log" 2>&1 &
NPID7=$!
PIDS+=("$NPID7")

BOOT7=""
for _ in $(seq 1 50); do
  NJ7="$(rpc_post "$HTTP7" "$TOK7" ListNodes '{}')"
  BOOT7="$(python3 -c '
import json, sys
nid = sys.argv[1]
ns = [n for n in json.load(sys.stdin)["nodes"] if n["id"] == nid and n["online"] and n.get("boot_id")]
print(ns[0]["boot_id"] if ns else "")
' "$AID7" <<<"$NJ7")"
  if [[ -n "$BOOT7" ]]; then
    break
  fi
  sleep 0.2
done
if [[ -z "$BOOT7" ]]; then
  echo "node 7 did not report online + boot_id" >&2
  exit 1
fi

ADM7="$("$PT" --url "http://127.0.0.1:${HTTP7}" --token "$TOK7" --json lease list e2e-node-7)"
python3 -c '
import json, sys
st = json.load(sys.stdin)
assert st["boot_id"] == sys.argv[1], st
assert st["slots_total"] == 4 and st["long_slots_total"] == 1, st
assert st["next_epoch"] >= 1, st
assert st.get("quarantined") is None, st
' "$BOOT7" <<<"$ADM7"

# Acquire parent shared lease, verify via both hub RPC and local admission.sock CLI.
PARENT_LEASE="$(rpc_post "$HTTP7" "$TOK7" AcquireLease '{"node_id":"e2e-node-7","holder_id":"parent-1","slots":1,"ttl_ms":15000}')"
read -r LID1 EP1 <<<"$(python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["ok"], d
assert d["boot_id"], d
assert d["epoch"] >= 1, d
print(d["lease_id"], d["epoch"])
' <<<"$PARENT_LEASE")"

rpc_post "$HTTP7" "$TOK7" VerifyEpoch "{\"node_id\":\"e2e-node-7\",\"boot_id\":\"${BOOT7}\",\"epoch\":${EP1}}" | python3 -c '
import json, sys
v = json.load(sys.stdin)
assert v["valid"] is True, v
'
"$PT_NODE" --sock "$NODE7/admission.sock" lease verify "$BOOT7" "$EP1" | python3 -c '
import json, sys
v = json.load(sys.stdin)
assert v["valid"] is True, v
'
if "$PT_NODE" --sock "$NODE7/admission.sock" lease verify "wrong-boot" "$EP1" >/dev/null 2>&1; then
  echo "expected wrong-boot verify to exit non-zero" >&2
  exit 1
fi
BAD_EP=$((EP1 + 99))
rpc_post "$HTTP7" "$TOK7" VerifyEpoch "{\"node_id\":\"e2e-node-7\",\"boot_id\":\"${BOOT7}\",\"epoch\":${BAD_EP}}" | python3 -c '
import json, sys
v = json.load(sys.stdin)
assert v["valid"] is False, v
'

# Queue exclusive waiter in background while parent lease is held.
EXCL_OUT="$TMP_ROOT/excl7.json"
(
  rpc_post "$HTTP7" "$TOK7" AcquireLease '{"node_id":"e2e-node-7","holder_id":"excl-waiter","exclusive":true,"slots":4,"ttl_ms":10000,"wait_ms":10000}' >"$EXCL_OUT"
) &
EXCL_BG=$!

Q_VISIBLE=0
for _ in $(seq 1 40); do
  ST_Q="$(rpc_post "$HTTP7" "$TOK7" GetNodeAdmission '{"node_id":"e2e-node-7"}')"
  if python3 -c 'import json,sys; st=json.load(sys.stdin); sys.exit(0 if len(st.get("queue",[])) >= 1 else 1)' <<<"$ST_Q"; then
    Q_VISIBLE=1
    break
  fi
  sleep 0.1
done
if [[ "$Q_VISIBLE" -ne 1 ]]; then
  echo "exclusive waiter did not appear in admission queue" >&2
  exit 1
fi

# Child borrow under parent_epoch MUST succeed without deadlocking behind queued exclusive waiter.
CHILD_LEASE="$(rpc_post "$HTTP7" "$TOK7" AcquireLease "{\"node_id\":\"e2e-node-7\",\"holder_id\":\"child-1\",\"slots\":1,\"ttl_ms\":10000,\"parent_epoch\":${EP1}}")"
read -r LID2 EP2 <<<"$(python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["ok"], d
assert d["epoch"] > int(sys.argv[1]), d
print(d["lease_id"], d["epoch"])
' "$EP1" <<<"$CHILD_LEASE")"

# Releasing parent revokes child immediately and unblocks queued exclusive waiter with strictly higher epoch.
rpc_post "$HTTP7" "$TOK7" ReleaseLease "{\"node_id\":\"e2e-node-7\",\"lease_id\":\"${LID1}\",\"epoch\":${EP1}}" >/dev/null
rpc_post "$HTTP7" "$TOK7" VerifyEpoch "{\"node_id\":\"e2e-node-7\",\"boot_id\":\"${BOOT7}\",\"epoch\":${EP1}}" | python3 -c 'import json,sys; assert json.load(sys.stdin)["valid"] is False'
rpc_post "$HTTP7" "$TOK7" VerifyEpoch "{\"node_id\":\"e2e-node-7\",\"boot_id\":\"${BOOT7}\",\"epoch\":${EP2}}" | python3 -c 'import json,sys; assert json.load(sys.stdin)["valid"] is False'

wait "$EXCL_BG"
read -r LID3 EP3 <<<"$(python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["ok"], d
assert d["epoch"] > int(sys.argv[1]), d
print(d["lease_id"], d["epoch"])
' "$EP2" <"$EXCL_OUT")"
rpc_post "$HTTP7" "$TOK7" ReleaseLease "{\"node_id\":\"e2e-node-7\",\"lease_id\":\"${LID3}\",\"epoch\":${EP3}}" >/dev/null

# Job with detached setsid child must be fenced completely on CancelJob.
FENCE_JOB="$(rpc_post "$HTTP7" "$TOK7" RunJob '{"target":{"type":"node","node_id":"e2e-node-7"},"even_if_present":true,"argv":["python3","-c","import subprocess,sys; p=subprocess.Popen([\"sleep\",\"60\"],start_new_session=True,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); print(\"DETACHED_PID=\"+str(p.pid),flush=True); p.wait()"]}')"
FJID="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$FENCE_JOB")"

DETACHED_PID=""
for _ in $(seq 1 60); do
  FLOG="$(rpc_post "$HTTP7" "$TOK7" GetJobLog "{\"job_id\":\"${FJID}\"}")"
  DETACHED_PID="$(python3 -c '
import json, re, sys
txt = json.load(sys.stdin).get("text", "")
m = re.search(r"DETACHED_PID=(\d+)", txt)
print(m.group(1) if m else "")
' <<<"$FLOG")"
  if [[ -n "$DETACHED_PID" ]]; then
    break
  fi
  sleep 0.2
done
if [[ -z "$DETACHED_PID" ]] || ! kill -0 "$DETACHED_PID" 2>/dev/null; then
  echo "detached descendant process did not start: $DETACHED_PID" >&2
  exit 1
fi

rpc_post "$HTTP7" "$TOK7" GetJob "{\"job_id\":\"${FJID}\"}" | python3 -c '
import json, sys
j = json.load(sys.stdin)
assert j.get("boot_id") == sys.argv[1], j
assert (j.get("epoch") or 0) > int(sys.argv[2]), j
assert j.get("lease_id"), j
' "$BOOT7" "$EP3"

rpc_post "$HTTP7" "$TOK7" CancelJob "{\"job_id\":\"${FJID}\"}" >/dev/null
FJ_DONE=0
for _ in $(seq 1 60); do
  FJST="$(rpc_post "$HTTP7" "$TOK7" GetJob "{\"job_id\":\"${FJID}\"}")"
  if python3 -c 'import json,sys; j=json.load(sys.stdin); sys.exit(0 if j["state"]=="cancelled" else 1)' <<<"$FJST"; then
    FJ_DONE=1
    break
  fi
  sleep 0.2
done
if [[ "$FJ_DONE" -ne 1 ]]; then
  echo "fencing test job did not transition to cancelled" >&2
  exit 1
fi
for _ in $(seq 1 30); do
  if ! kill -0 "$DETACHED_PID" 2>/dev/null; then break; fi
  sleep 0.1
done
if kill -0 "$DETACHED_PID" 2>/dev/null; then
  echo "detached setsid child $DETACHED_PID survived job cancel/fence!" >&2
  kill -9 "$DETACHED_PID" 2>/dev/null || true
  exit 1
fi

# Verify zero leaked leases and no quarantine after fence.
rpc_post "$HTTP7" "$TOK7" GetNodeAdmission '{"node_id":"e2e-node-7"}' | python3 -c '
import json, sys
st = json.load(sys.stdin)
assert st.get("quarantined") is None, st
assert st["slots_used"] == 0 and len(st.get("grants", [])) == 0, st
'

# CLI job run --wait + artifact + AckJob lease release check.
CLI_JOB="$("$PT" --url "http://127.0.0.1:${HTTP7}" --token "$TOK7" --json job run --wait --even-if-present --node e2e-node-7 --artifact report.txt -- sh -c 'printf "adm-ok-7" > report.txt')"
JID_CLI="$(python3 -c '
import json, sys
j = json.load(sys.stdin)
assert j["state"] == "succeeded", j
assert (j.get("epoch") or 0) > 0 and j.get("boot_id"), j
print(j["id"])
' <<<"$CLI_JOB")"

ART_OUT="$("$PT" --url "http://127.0.0.1:${HTTP7}" --token "$TOK7" job artifact "$JID_CLI" report.txt)"
if [[ "$ART_OUT" != "adm-ok-7" ]]; then
  echo "unexpected CLI artifact content: $ART_OUT" >&2
  exit 1
fi

for _ in $(seq 1 30); do
  ST_AFTER="$(rpc_post "$HTTP7" "$TOK7" GetNodeAdmission '{"node_id":"e2e-node-7"}')"
  if python3 -c 'import json,sys; st=json.load(sys.stdin); sys.exit(0 if st["slots_used"]==0 and len(st.get("grants",[]))==0 else 1)' <<<"$ST_AFTER"; then
    break
  fi
  sleep 0.1
done
rpc_post "$HTTP7" "$TOK7" GetNodeAdmission '{"node_id":"e2e-node-7"}' | python3 -c '
import json, sys
st = json.load(sys.stdin)
assert st["slots_used"] == 0 and len(st.get("grants", [])) == 0, st
'

# CLI lease acquire / renew / release roundtrip.
CLI_L="$("$PT" --url "http://127.0.0.1:${HTTP7}" --token "$TOK7" --json lease acquire e2e-node-7 --holder cli-test --slots 2 --ttl-ms 10000)"
read -r CLID CEP <<<"$(python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"], d; print(d["lease_id"], d["epoch"])' <<<"$CLI_L")"
"$PT" --url "http://127.0.0.1:${HTTP7}" --token "$TOK7" --json lease renew e2e-node-7 "$CLID" "$CEP" | python3 -c 'import json,sys; assert json.load(sys.stdin)["ok"]'
"$PT" --url "http://127.0.0.1:${HTTP7}" --token "$TOK7" --json lease release e2e-node-7 "$CLID" "$CEP" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"] and d["fenced"]'

kill "$NPID7" "$PID7" >/dev/null 2>&1 || true
wait "$NPID7" "$PID7" >/dev/null 2>&1 || true
check_crashes "$LOG7" "$TMP_ROOT/node7.log"
echo "E2E 7 PASSED"

echo "=== E2E 8: Full Hub & Node CLI Parity ==="
HUB8="$TMP_ROOT/hub8"
NODE8="$TMP_ROOT/node8"
REPO8="$TMP_ROOT/repo8"
LOG8="$TMP_ROOT/hub8.log"
mkdir -p "$REPO8"

"$PT" serve --home "$HUB8" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox >"$LOG8" 2>&1 &
PID8=$!
PIDS+=("$PID8")
wait_for_listen "$LOG8"
LINE8="$(grep 'protectorate listening ' "$LOG8" | head -n1)"
HTTP8="$(extract_port "$LINE8" http)"
FLEET8="$(extract_port "$LINE8" fleet)"
TOK8="$("$PT" token --home "$HUB8" | sed -n 's/^owner=//p')"
U8="--url http://127.0.0.1:${HTTP8} --token ${TOK8}"

ENROLL8="$(rpc_post "$HTTP8" "$TOK8" GetEnrollment '{}')"
JTOK8="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["control_key"])' <<<"$ENROLL8")"
JOUT8="$("$PT_NODE" --home "$NODE8" join "$JTOK8" --name e2e-node-8)"
NKEY8="$(sed -n 's/^  node_key: *//p' <<<"$JOUT8")"
python3 - "$NODE8/config.json" "$FLEET8" <<'PY'
import json, sys
p, port = sys.argv[1], sys.argv[2]
with open(p) as f:
    cfg = json.load(f)
cfg["addrs"] = [f"127.0.0.1:{port}"]
with open(p, "w") as f:
    json.dump(cfg, f, indent=2)
PY
ADD8="$(rpc_post "$HTTP8" "$TOK8" AddNode "{\"registration\":\"e2e-node-8:${NKEY8}\"}")"
AID8="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["auth_id"])' <<<"$ADD8")"
rpc_post "$HTTP8" "$TOK8" ApproveNode "{\"auth_id\":\"${AID8}\",\"name\":\"e2e-node-8\"}" >/dev/null
"$PT_NODE" --home "$NODE8" authorize >/dev/null
"$PT_NODE" --home "$NODE8" serve >"$TMP_ROOT/node8.log" 2>&1 &
NPID8=$!
PIDS+=("$NPID8")

ONLINE8=0
for _ in $(seq 1 50); do
  if "$PT" $U8 --json node show e2e-node-8 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if d.get("online") else 1)'; then
    ONLINE8=1
    break
  fi
  sleep 0.2
done
if [[ "$ONLINE8" -ne 1 ]]; then
  echo "node 8 did not come online" >&2
  exit 1
fi

"$PT" version | grep -q 'protectorate '
"$PT" $U8 hub info | grep -q 'hub='

VER8="$("$PT" $U8 --json hub info | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')"
mkdir -p "$HUB8/clients/${VER8}"
printf 'APK_PAYLOAD_E2E8' > "$HUB8/clients/${VER8}/protectorate-android.apk"
"$PT" $U8 --json hub info | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "protectorate-android.apk" in d.get("clients", []), d'
UNAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:${HTTP8}/api/client/protectorate-android.apk")"
test "$UNAUTH_CODE" = "401"
TRAV_CODE="$(curl --path-as-is -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer ${TOK8}" "http://127.0.0.1:${HTTP8}/api/client/../auth.json")"
test "$TRAV_CODE" = "404"
DL_CLIENT_HDR="$(curl -sSD "$TMP_ROOT/client8.hdr" -H "Authorization: Bearer ${TOK8}" -o "$TMP_ROOT/client8.apk" "http://127.0.0.1:${HTTP8}/api/client/protectorate-android.apk")"
test "$(cat "$TMP_ROOT/client8.apk")" = "APK_PAYLOAD_E2E8"
grep -qi '^content-length: *16' "$TMP_ROOT/client8.hdr"

PROXY_DENY_LOG="$TMP_ROOT/hub_proxy_deny.log"
"$PT" serve --home "$TMP_ROOT/hub_pdeny" --listen 127.0.0.1:0 --proxy-listen 127.0.0.1:0 --trusted-proxy 192.0.2.1 --fleet-listen 127.0.0.1:0 --no-sandbox >"$PROXY_DENY_LOG" 2>&1 &
PID_PD=$!
PIDS+=("$PID_PD")
wait_for_listen "$PROXY_DENY_LOG"
LINE_PD="$(grep 'protectorate listening ' "$PROXY_DENY_LOG" | head -n1)"
HTTP_PD="$(extract_port "$LINE_PD" http)"
PROXY_PD="$(extract_port "$LINE_PD" proxy)"
curl -sf "http://127.0.0.1:${HTTP_PD}/health" | grep -q '"ok":true'
if curl -s --max-time 2 "http://127.0.0.1:${PROXY_PD}/health" >/dev/null 2>&1; then
  echo "expected untrusted direct connection to proxy port $PROXY_PD to be closed" >&2
  exit 1
fi
kill "$PID_PD" >/dev/null 2>&1 || true
wait "$PID_PD" >/dev/null 2>&1 || true
check_crashes "$PROXY_DENY_LOG"

PROXY_ALLOW_LOG="$TMP_ROOT/hub_proxy_allow.log"
"$PT" serve --home "$TMP_ROOT/hub_pallow" --listen 127.0.0.1:0 --proxy-listen 127.0.0.1:0 --trusted-proxy 127.0.0.1 --fleet-listen 127.0.0.1:0 --no-sandbox >"$PROXY_ALLOW_LOG" 2>&1 &
PID_PA=$!
PIDS+=("$PID_PA")
wait_for_listen "$PROXY_ALLOW_LOG"
LINE_PA="$(grep 'protectorate listening ' "$PROXY_ALLOW_LOG" | head -n1)"
PROXY_PA="$(extract_port "$LINE_PA" proxy)"
curl -sf "http://127.0.0.1:${PROXY_PA}/health" | grep -q '"ok":true'
kill "$PID_PA" >/dev/null 2>&1 || true
wait "$PID_PA" >/dev/null 2>&1 || true
check_crashes "$PROXY_ALLOW_LOG"
"$PT" $U8 hub state | grep -q 'sleeves='
"$PT" --home "$HUB8" hub probation | grep -q 'probation\|healthy'
"$PT" $U8 doctor --refresh | grep -q 'HARNESS'
"$PT" $U8 quota list >/dev/null

"$PT" $U8 node set e2e-node-8 --jobs owner --note maint | grep -q 'owner'
"$PT" $U8 node rename e2e-node-8 e2e-node-8b | grep -q 'e2e-node-8b'
"$PT" $U8 node show e2e-node-8b | grep -q 'e2e-node-8b'
NUPD8="$("$PT" $U8 --json node update e2e-node-8b --version "$VER8")"
python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("rollout_id") and len(d.get("nodes", [])) == 1 and d["nodes"][0]["ok"], d' <<<"$NUPD8"

set +e
JOB8_JSON="$("$PT" $U8 --json job run --wait --even-if-present --node e2e-node-8b -- sh -c 'echo CLI_LOG_8; exit 7')"
JOB8_RC=$?
set -e
if [[ "$JOB8_RC" -ne 7 ]]; then
  echo "expected job run --wait exit code 7, got $JOB8_RC ($JOB8_JSON)" >&2
  exit 1
fi
JID8="$(python3 -c 'import json,sys; j=json.load(sys.stdin); assert j["state"]=="failed"; print(j["id"])' <<<"$JOB8_JSON")"
"$PT" $U8 job list --node e2e-node-8b --limit 5 | grep -q "$JID8"
"$PT" $U8 job logs "$JID8" --offset 0 --limit 9 | grep -q 'CLI_LOG_8'

"$PT" $U8 --json sleeve spawn s8 --path "$REPO8" --harness pi_local --message "hi" | python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["name"]=="s8"'
"$PT" $U8 sleeve show s8 | grep -q 's8'
"$PT" $U8 --json sleeve inbox s8 | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "messages" in d'
"$PT" $U8 sleeve send s8 "follow up" --steer | python3 -c 'import json,sys; assert json.load(sys.stdin)["queued"]'
"$PT" $U8 sleeve inbox s8 --drop >/dev/null
"$PT" $U8 sleeve takeover s8 >/dev/null
"$PT" $U8 sleeve stop s8 >/dev/null
"$PT" $U8 sleeve rm s8 >/dev/null

"$PT" $U8 --json approval list --pending | python3 -c 'import json,sys; assert "approvals" in json.load(sys.stdin)'
"$PT" $U8 --json event list --limit 5 | python3 -c 'import json,sys; assert len(json.load(sys.stdin)["events"]) >= 1'
"$PT" $U8 event read 1 >/dev/null

DADD8="$("$PT" $U8 --json device add cli-dev)"
DCODE8="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["code"])' <<<"$DADD8")"
DPAIR8="$("$PT" $U8 --json device pair "$DCODE8")"
DID8="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["device"]["id"])' <<<"$DPAIR8")"
"$PT" $U8 --json device list | grep -q "$DID8"
"$PT" $U8 device revoke "$DID8" >/dev/null

ETOK8="$("$PT" $U8 envoy token)"
test -n "$ETOK8"
"$PT" $U8 --json envoy read --from 0 --limit 5 | python3 -c 'import json,sys; assert "entries" in json.load(sys.stdin)'

MCP8_OUT="$("$PT" $U8 --as test-agent mcp <<JSON
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"e2e-cli"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"node_admission","arguments":{"node_id":"e2e-node-8b"}}}
JSON
)"
grep -q 'envoy_send' <<<"$MCP8_OUT"
grep -q 'envoy_wait' <<<"$MCP8_OUT"
grep -q 'envoy_read' <<<"$MCP8_OUT"
grep -q 'acquire_lease' <<<"$MCP8_OUT"
grep -q '"isError":false' <<<"$MCP8_OUT"

"$PT_NODE" inventory | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["cpu_count"] >= 1'
"$PT_NODE" sample | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "sampled_at_ms" in d'
"$PT_NODE" presence | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "state" in d'
"$PT_NODE" watch-presence --interval 1 --limit 1 | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "state" in d'
"$PT_NODE" --home "$NODE8" probation | grep -q 'probation\|healthy'

PROB_HOME="$TMP_ROOT/prob-test"
mkdir -p "$PROB_HOME/releases/v1" "$PROB_HOME/releases/v2"
printf 'v1' > "$PROB_HOME/releases/previous.txt"
printf 'v2' > "$PROB_HOME/releases/current.txt"
printf '{"active":"v2","previous":"v1","healthy":false,"attempts":0}\n' > "$PROB_HOME/releases/probation.json"
"$PT_NODE" --home "$PROB_HOME" probation | grep -q 'probation=pending'
test "$(cat "$PROB_HOME/releases/current.txt")" = "v2"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["attempts"] == 1 and not d["healthy"], d' "$PROB_HOME/releases/probation.json"
"$PT_NODE" --home "$PROB_HOME" probation | grep -q 'rolled back from v2 to v1'
test "$(cat "$PROB_HOME/releases/current.txt")" = "v1"

kill "$NPID8" "$PID8" >/dev/null 2>&1 || true
wait "$NPID8" "$PID8" >/dev/null 2>&1 || true
check_crashes "$LOG8" "$TMP_ROOT/node8.log"
echo "E2E 8 PASSED"

if [[ "${SKIP_PI_E2E:-0}" == "1" ]]; then
  echo "ZIG E2E SCENARIOS 1, 3, 5, 6, 7, 8 PASSED (E2E 2 & 4 skipped via SKIP_PI_E2E=1)"
else
  echo "ALL 8 ZIG E2E SCENARIOS PASSED"
fi
