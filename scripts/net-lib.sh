# Shared by net-e2e.sh and net-remote.sh (sourced, bash). Sets up headscale
# (embedded DERP, STUN off, go/policy.hujson) behind nettest's TLS proxy, which
# stands in for Traefik. Each run gets a fresh .e2e/run-* directory; nothing is
# ever deleted.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
RUN=$ROOT/.e2e/run-$(date +%Y%m%d-%H%M%S)-$$
BIND=${BIND:-127.0.0.1}
HS_PORT=${HS_PORT:-17480} PX_PORT=${PX_PORT:-17443}
GRPC_PORT=$((HS_PORT + 2)) METRICS_PORT=$((HS_PORT + 1)) STUN_PORT=$((HS_PORT + 3))
HUB_UDP=${HUB_UDP:-17641}
HS_URL=http://$BIND:$HS_PORT PX_URL=https://$BIND:$PX_PORT

say() { printf '== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

PIDS=()
cleanup() {
	for pid in "${PIDS[@]}"; do kill -TERM "$pid" 2>/dev/null || true; done
	for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
}
trap cleanup EXIT

# wait_for <seconds> <description> <command...>
wait_for() {
	local deadline=$(($(date +%s) + $1)) what=$2
	shift 2
	until "$@" >/dev/null 2>&1; do
		[ "$(date +%s)" -lt "$deadline" ] || fail "timed out waiting for $what"
		sleep 0.5
	done
}

build_bins() {
	say "build"
	"$ROOT/scripts/net-build.sh" host
	OS=$(cd "$ROOT/go" && go env GOHOSTOS) ARCH=$(cd "$ROOT/go" && go env GOHOSTARCH)
	BIN=$ROOT/go/bin
	nettest_for "$OS" "$ARCH"
	HEADSCALE=$BIN/headscale-$OS-$ARCH MEMBER=$BIN/member-$OS-$ARCH NETTEST=$BIN/nettest-$OS-$ARCH
}

nettest_for() { # goos goarch
	local ext=""
	[ "$1" = windows ] && ext=.exe
	(cd "$ROOT/go" && CGO_ENABLED=0 GOOS=$1 GOARCH=$2 GOFLAGS=-p=4 go build -trimpath -ldflags "-s -w" \
		-o "$ROOT/go/bin/nettest-$1-$2$ext" ./e2e/nettest)
}

hs() { "$HEADSCALE" -c "$CFG" "$@"; }

start_control() {
	for port in $HS_PORT $PX_PORT $GRPC_PORT $METRICS_PORT; do
		if ss -Hltn "sport = :$port" | grep -q .; then fail "TCP port $port is in use"; fi
	done
	mkdir -p "$RUN/hs"
	chmod 0700 "$RUN" "$RUN/hs"
	say "run dir $RUN"
	openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 2 -subj "/CN=$BIND" \
		-addext "subjectAltName=IP:$BIND" -keyout "$RUN/key.pem" -out "$RUN/cert.pem" 2>/dev/null
	local cert_sha
	cert_sha=$(openssl x509 -in "$RUN/cert.pem" -outform der | sha256sum | cut -d' ' -f1)
	# STUN off: stunport -1. Headscale still wants a STUN listen address while
	# its DERP is on, so it binds loopback and nothing uses it. DERP pins the
	# proxy's self-signed certificate by hash; production uses Traefik's.
	cat >"$RUN/hs/derp.yaml" <<-EOF
	regions:
	  900:
	    regionid: 900
	    regioncode: e2e
	    regionname: Protectorate e2e
	    nodes:
	      - name: 900a
	        regionid: 900
	        hostname: $BIND
	        ipv4: $BIND
	        derpport: $PX_PORT
	        stunport: -1
	        certname: sha256-raw:$cert_sha
	EOF
	CFG=$RUN/hs/headscale.yaml
	jq -n --arg run "$RUN/hs" --arg root "$ROOT" --arg hs "$BIND:$HS_PORT" --arg px "$PX_URL" \
		--arg grpc "127.0.0.1:$GRPC_PORT" --arg metrics "127.0.0.1:$METRICS_PORT" \
		--arg stun "127.0.0.1:$STUN_PORT" --arg proxy "$BIND/32" '{
		server_url: $px,
		listen_addr: $hs, metrics_listen_addr: $metrics, grpc_listen_addr: $grpc, grpc_allow_insecure: false,
		trusted_proxies: [$proxy],
		noise: {private_key_path: ($run + "/noise_private.key")},
		prefixes: {v4: "100.64.0.0/10", v6: "fd7a:115c:a1e0::/48", allocation: "sequential"},
		derp: {server: {enabled: true, verify_clients: true, stun_listen_addr: $stun,
				private_key_path: ($run + "/derp_server_private.key"), automatically_add_embedded_derp_region: false},
			urls: [], paths: [($run + "/derp.yaml")], auto_update_enabled: false},
		disable_check_updates: true,
		database: {type: "sqlite", sqlite: {path: ($run + "/db.sqlite"), write_ahead_log: true}},
		policy: {mode: "file", path: ($root + "/go/policy.hujson")},
		dns: {magic_dns: false, override_local_dns: false, nameservers: {global: []}},
		unix_socket: ($run + "/headscale.sock"), unix_socket_permission: "0700",
		logtail: {enabled: false}, log: {level: "info", format: "text"}
	}' >"$CFG"

	say "headscale on $HS_URL, proxy on $PX_URL"
	"$HEADSCALE" -c "$CFG" serve >"$RUN/headscale.log" 2>&1 &
	PIDS+=($!)
	wait_for 30 "headscale /health" curl -q -fsS "$HS_URL/health"
	wait_for 10 "headscale socket" test -S "$RUN/hs/headscale.sock"
	hs policy check -f "$ROOT/go/policy.hujson" || fail "policy check"
	"$NETTEST" proxy --listen "$BIND:$PX_PORT" --upstream "$HS_URL" \
		--cert "$RUN/cert.pem" --key "$RUN/key.pem" >"$RUN/proxy.log" 2>&1 &
	PIDS+=($!)
	wait_for 10 "proxy" curl -q -fsS --cacert "$RUN/cert.pem" "$PX_URL/health"
	[ "$(curl -q -s -o /dev/null -w '%{http_code}' --cacert "$RUN/cert.pem" "$PX_URL/register/x")" = 404 ] ||
		fail "proxy routes /register"
	USER_ID=$(hs users create e2e -o json | jq -r .id)
	# The hub's own view of the control key, computed as member computes it.
	CONTROL_KEY=sha256:$(curl -q -fsS "$HS_URL/key?v=148" | jq -r .publicKey |
		sed 's/^mkey://' | tr a-f A-F | basenc --base16 -d | sha256sum | cut -d' ' -f1)
}

has_registration() { "$@" | grep -q '"event":"registration"'; }

# approve <name> <tag> <reader...>: reader prints the member's
# --print-registration output. Checks the control-key fingerprint, then
# registers the auth id; the caller waits for "registered" and calls tag_node.
approve() {
	local name=$1
	shift 2
	wait_for 60 "$name registration" has_registration "$@"
	local reg auth_id fp
	reg=$("$@")
	auth_id=$(jq -r 'select(.event=="registration").auth_id' <<<"$reg")
	fp=$(jq -r 'select(.event=="registration").control_key' <<<"$reg")
	[ "$fp" = "$CONTROL_KEY" ] || fail "$name control key $fp, headscale serves $CONTROL_KEY"
	NODE_ID=$(hs auth register --auth-id "$auth_id" --user e2e -o json | jq -r .id)
}

tag_node() { # name id tag
	hs nodes tag -i "$2" -t "$3" -o json | jq -e --arg t "$3" '.tags == [$t]' >/dev/null || fail "tag $1 $3"
	say "enrolled $1 as node $2, $3"
}

# enroll_local <name> <mode> <control-url> <tag>
enroll_local() {
	local reg="$RUN/$1.reg"
	"$MEMBER" --mode "$2" --state-dir "$RUN/$1" --control-url "$3" --hostname "$1" \
		--print-registration >"$reg" 2>"$RUN/$1-enroll.log" &
	local pid=$!
	approve "$1" "$4" cat "$reg"
	wait "$pid" || fail "$1 enrollment exited nonzero"
	grep -q '"event":"registered"' "$reg" || fail "$1 not registered"
	tag_node "$1" "$NODE_ID" "$4"
}

status() { "$NETTEST" status --socket "$1"; }
peer_online() { status "$1" | jq -e --arg n "$2" '.peers[] | select(.name==$n and .online)'; }
peer_direct() { status "$1" | jq -e --arg n "$2" '.peers[] | select(.name==$n and (.cur_addr // "") != "")'; }
peer_path() { status "$RUN/hub.sock" | jq -c --arg n "$1" '.peers[] | select(.name==$n) | {cur_addr, relay, last_handshake}'; }

start_hub() { # log suffix
	"$MEMBER" --mode hub --state-dir "$RUN/hub" --control-url "$HS_URL" --hostname hub \
		--socket "$RUN/hub.sock" --udp-port "$HUB_UDP" 2>"$RUN/hub-$1.log" &
	HUB_PID=$!
	PIDS+=("$HUB_PID")
	wait_for 30 "hub socket" test -S "$RUN/hub.sock"
}

# echo_check <target>: DIAL through the hub member; the echo agent sends back
# the member's identity header, then the payload.
echo_check() {
	local out
	out=$("$NETTEST" dial --socket "$RUN/hub.sock" --target "$1" --send "ping $1") || fail "DIAL $1: $out"
	[ "$(sed -n 2p <<<"$out")" = "ping $1" ] || fail "DIAL $1 echoed: $out"
	sed -n 1p <<<"$out" | jq -e '.tags == ["tag:hub"] and .node != "" and (.ip | startswith("100."))' >/dev/null ||
		fail "DIAL $1 header: $(sed -n 1p <<<"$out")"
	say "DIAL $1 echoed, header $(sed -n 1p <<<"$out")"
}
