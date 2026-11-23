#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PT="$ROOT_DIR/zig-out/bin/protectorate"
PT_NODE="$ROOT_DIR/zig-out/bin/protectorate-node"

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
  rm -rf "$TMP_ROOT"
  exit $rc
}
trap cleanup EXIT

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
"$PT" serve --home "$HUB1" --listen 127.0.0.1:0 --fleet-listen 127.0.0.1:0 --no-sandbox >"$LOG1" 2>&1 &
PID1=$!
PIDS+=("$PID1")
wait_for_listen "$LOG1"
LINE1="$(grep 'protectorate listening ' "$LOG1" | head -n1)"
HTTP1="$(extract_port "$LINE1" http)"
TOK1="$("$PT" token --home "$HUB1" | sed -n 's/^owner=//p')"

curl -sf "http://127.0.0.1:${HTTP1}/health" | grep -q '"ok":true'
curl -sf "http://127.0.0.1:${HTTP1}/" | grep -qiE '<!doctype html>|<html'

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
kill "$PID1" >/dev/null 2>&1 || true
wait "$PID1" >/dev/null 2>&1 || true
echo "E2E 1 PASSED"

echo "=== E2E 2 & 4: Real pi + hotschmoe-dd (bwrap), Crash/Replay & History BM25 ==="
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
echo "E2E 4 PASSED"

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

# RunJob with artifact
JOB1_REQ="{\"target\":{\"type\":\"node\",\"node_id\":\"${AUTH_ID}\"},\"argv\":[\"sh\",\"-c\",\"echo E2E_JOB_STDOUT; printf 'artifact-payload-99' > out.bin\"],\"artifacts\":[\"out.bin\"]}"
JOB1_RESP="$(rpc_post "$HTTP3" "$TOK3" RunJob "$JOB1_REQ")"
JID1="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$JOB1_RESP")"

JOB1_DONE=0
for _ in $(seq 1 60); do
  J_ST="$(rpc_post "$HTTP3" "$TOK3" GetJob "{\"job_id\":\"${JID1}\"}")"
  if python3 -c '
import json, sys
j = json.load(sys.stdin)
sys.exit(0 if j["state"] == "succeeded" and len(j.get("artifacts", [])) == 1 else 1)
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
  echo "post-reconnect job failed" >&2
  exit 1
fi
rpc_post "$HTTP3B" "$TOK3" GetJobLog "{\"job_id\":\"${JID3}\"}" | grep -q 'E2E_RECONNECTED'
kill "$NPID3" "$PID3B" >/dev/null 2>&1 || true
wait "$NPID3" "$PID3B" >/dev/null 2>&1 || true
echo "E2E 3 PASSED"

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
echo "ALL 5 ZIG E2E SCENARIOS PASSED"
