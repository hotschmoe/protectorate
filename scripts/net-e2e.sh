#!/usr/bin/env bash
# Local network e2e on one machine: headscale behind a TLS proxy, a hub member,
# a node member with an echo agent, and a tag:node probe. Exits nonzero on the
# first failure. See net-lib.sh for the control setup.
source "$(dirname "$0")/net-lib.sh"
NODE_UDP=${NODE_UDP:-17642}

build_bins
start_control
# The node reaches control through the proxy and trusts its certificate.
export SSL_CERT_FILE=$RUN/cert.pem

say "enroll"
enroll_local hub hub "$HS_URL" tag:hub
enroll_local node node "$PX_URL" tag:node

"$NETTEST" echo --listen "$RUN/agent.sock" >"$RUN/agent.log" 2>&1 &
PIDS+=($!)

start_members() { # phase
	start_hub "$1"
	"$MEMBER" --mode node --state-dir "$RUN/node" --control-url "$PX_URL" --hostname node \
		--socket "$RUN/node.sock" --forward "$RUN/agent.sock" --udp-port "$NODE_UDP" 2>"$RUN/node-$1.log" &
	NODE_PID=$!
	PIDS+=("$NODE_PID")
	wait_for 30 "node socket" test -S "$RUN/node.sock"
	wait_for 60 "node online in the hub's netmap" peer_online "$RUN/hub.sock" node
}
stop_members() {
	kill -TERM "$HUB_PID" "$NODE_PID"
	wait "$HUB_PID" "$NODE_PID" || true
}

say "phase A: direct path"
start_members a
NODE_IP=$(status "$RUN/hub.sock" | jq -r '.peers[] | select(.name=="node") | .ips[0]')
HUB_IP=$(status "$RUN/hub.sock" | jq -r '.self.ips[0]')
echo_check node:7475
echo_check "$NODE_IP:7475"
wait_for 30 "direct path to node" peer_direct "$RUN/hub.sock" node
peer_path node
status "$RUN/hub.sock" | jq -e '.self.peerapi == null' >/dev/null || fail "hub member serves peerAPI"
status "$RUN/node.sock" | jq -e '.self.peerapi == null' >/dev/null || fail "node member serves peerAPI"
for bad in "hub:7475" "nosuch:7475" "node"; do
	out=$("$NETTEST" dial --socket "$RUN/hub.sock" --target "$bad" --send x 2>&1) && fail "DIAL $bad succeeded"
	say "DIAL $bad refused: ${out##*: }"
done
out=$("$NETTEST" dial --socket "$RUN/node.sock" --target "$HUB_IP:7475" --send x 2>&1) && fail "node member dialed"
say "node socket DIAL refused: ${out##*: }"

say "policy: a tag:node peer cannot open a flow to the hub or another node"
PROBE_KEY=$(hs preauthkeys create --user "$USER_ID" --tags tag:node --ephemeral -e 10m -o json | jq -r .key)
"$NETTEST" probe --state-dir "$RUN/probe" --control-url "$HS_URL" --auth-key "$PROBE_KEY" \
	--hold 60s --target "$NODE_IP:7475" --target "$HUB_IP:7475" --target "$HUB_IP:22" \
	--target "$HUB_IP:80" --target "$HUB_IP:443" --target "$HUB_IP:7470" --target "$HUB_IP:41641" \
	>"$RUN/probe.out" 2>"$RUN/probe.log" &
PIDS+=($!)
wait_for 90 "probe results" grep -q '"event":"done"' "$RUN/probe.out"
cat "$RUN/probe.out"
jq -e 'select(.event=="probe") | select(.connected)' "$RUN/probe.out" >/dev/null && fail "probe opened a flow"
[ "$(grep -c '"event":"probe"' "$RUN/probe.out")" = 7 ] || fail "probe did not try every target"
wait_for 30 "probe in the hub's netmap" peer_online "$RUN/hub.sock" probe
out=$("$NETTEST" dial --socket "$RUN/hub.sock" --target probe:7475 --send "hub to probe") ||
	fail "hub could not reach the probe: $out"
[ "$out" = "hub to probe" ] || fail "probe echo: $out"
say "hub reaches the probe on :7475, so the probe's failures are the policy"

say "phase B: DERP only, through the proxy"
stop_members
export TS_DEBUG_ALWAYS_USE_DERP=1
start_members b
echo_check node:7475
status "$RUN/hub.sock" | jq -e '.peers[] | select(.name=="node") | .cur_addr == null and .relay != ""' >/dev/null ||
	fail "phase B path is not DERP: $(status "$RUN/hub.sock" | jq -c '.peers[]')"
peer_path node
grep -q 'GET /derp upgrade="DERP" routed=true' "$RUN/proxy.log" || fail "no DERP upgrade through the proxy"
grep -q 'POST /ts2021 upgrade="tailscale-control-protocol" routed=true' "$RUN/proxy.log" ||
	fail "no ts2021 upgrade through the proxy"
say "ts2021 and DERP upgrades passed the proxy: $(grep -c 'routed=true' "$RUN/proxy.log") routed requests"
stop_members
unset TS_DEBUG_ALWAYS_USE_DERP

say "control-key pin"
mkdir -m 0700 "$RUN/badpin"
echo sha256:0000000000000000000000000000000000000000000000000000000000000000 >"$RUN/badpin/control-key.pin"
if "$MEMBER" --mode node --state-dir "$RUN/badpin" --control-url "$HS_URL" --hostname badpin \
	--print-registration >/dev/null 2>"$RUN/badpin.log"; then
	fail "member started with a wrong control-key pin"
fi
grep -q 'control key changed' "$RUN/badpin.log" || fail "wrong pin error: $(cat "$RUN/badpin.log")"
say "wrong pin refused: $(grep -o 'control key changed[^"]*' "$RUN/badpin.log" | head -1)"

say "PASS ($RUN)"
