#!/usr/bin/env bash
# Real-machine check: headscale and the hub member here on a LAN address, node members on one
# Linux and one Windows machine over ssh, each with the echo agent, in a scratch dir under the
# remote home. Nothing is installed; every remote process stops when this script ends or after
# 10 minutes at most.
#
#   PT_NET_BIND        this machine's LAN address, which both nodes can reach
#   PT_NET_LINUX_HOST  ssh destination of the Linux (amd64) node, such as user@host
#   PT_NET_LINUX_KEY   optional ssh identity file for it
#   PT_NET_WIN_HOST    ssh destination of the Windows (amd64) node; its default shell is cmd.exe
# Read from the environment, else from the gitignored .env.e2e at the repo root. HS_PORT,
# PX_PORT and HUB_UDP override the ports.
ROOT=$(cd "$(dirname "$0")/.." && pwd)
if [ -f "$ROOT/.env.e2e" ]; then set -a; . "$ROOT/.env.e2e"; set +a; fi
for v in PT_NET_BIND PT_NET_LINUX_HOST PT_NET_WIN_HOST; do
	[ -n "${!v:-}" ] || { echo "net-remote: set $v (or put it in .env.e2e); see the header of $0" >&2; exit 2; }
done
export BIND=$PT_NET_BIND HS_PORT=${HS_PORT:-17580} PX_PORT=${PX_PORT:-17543} HUB_UDP=${HUB_UDP:-17651}
source "$ROOT/scripts/net-lib.sh"
NAME=$(basename "$RUN")
# The nodes' tailnet hostnames.
LN=linux-node WN=windows-node

SSH_OPTS=(-o BatchMode=yes -o ConnectTimeout=10)
if [ -n "${PT_NET_LINUX_KEY:-}" ]; then LN_KEY=(-i "$PT_NET_LINUX_KEY" -o IdentitiesOnly=yes); else LN_KEY=(); fi
AW=(ssh "${SSH_OPTS[@]}" "${LN_KEY[@]}" "$PT_NET_LINUX_HOST")
AW_SCP=(scp -q -o BatchMode=yes "${LN_KEY[@]}")
AW_DIR=pn-net-e2e/$NAME
OW=(ssh "${SSH_OPTS[@]}" "$PT_NET_WIN_HOST")

ps_run() { # PowerShell script text -> the Windows node, with no profile and no window
	"${OW[@]}" "powershell -NoProfile -NonInteractive -EncodedCommand $(printf '%s' "\$ProgressPreference='SilentlyContinue'; $1" | iconv -f utf-8 -t utf-16le | base64 -w0)"
}
OW_DIR=$(ps_run '$env:USERPROFILE' | tr -d '\r\n')\\pn-net-e2e\\$NAME
[[ "$OW_DIR" == ?:\\* ]] || fail "no USERPROFILE from $PT_NET_WIN_HOST: $OW_DIR"

build_bins
for target in "linux amd64" "windows amd64"; do
	"$ROOT/scripts/net-build.sh" member $target
	nettest_for $target
done
start_control
enroll_local hub hub "$HS_URL" tag:hub
start_hub a

say "push to $PT_NET_LINUX_HOST:~/$AW_DIR"
"${AW[@]}" "mkdir -p -m 0700 $AW_DIR"
"${AW_SCP[@]}" "$BIN/member-linux-amd64" "$BIN/nettest-linux-amd64" "$RUN/cert.pem" "$PT_NET_LINUX_HOST:$AW_DIR/"
for f in member-linux-amd64 nettest-linux-amd64; do
	[ "$("${AW[@]}" "sha256sum $AW_DIR/$f" | cut -d' ' -f1)" = "$(sha256sum "$BIN/$f" | cut -d' ' -f1)" ] ||
		fail "$PT_NET_LINUX_HOST $f sha256 mismatch"
done

say "push to $PT_NET_WIN_HOST:$OW_DIR"
ps_run "New-Item -ItemType Directory -Force -Path '$OW_DIR' | Out-Null"
scp -q -o BatchMode=yes "$BIN/member-windows-amd64.exe" "$BIN/nettest-windows-amd64.exe" \
	"$PT_NET_WIN_HOST:pn-net-e2e/$NAME/"
for f in member-windows-amd64.exe nettest-windows-amd64.exe; do
	remote=$(ps_run "(Get-FileHash -Algorithm SHA256 '$OW_DIR\\$f').Hash" | tr -d '\r\n' | tr A-F a-f)
	[ "$remote" = "$(sha256sum "$BIN/$f" | cut -d' ' -f1)" ] || fail "$PT_NET_WIN_HOST $f sha256 mismatch: $remote"
done

say "enroll the Linux node (control through the proxy, cert pinned for DERP, trusted via SSL_CERT_FILE)"
"${AW[@]}" "cd $AW_DIR && SSL_CERT_FILE=\$PWD/cert.pem timeout 120 ./member-linux-amd64 --mode node --state-dir \$PWD/state \
	--control-url $PX_URL --hostname $LN --print-registration" >"$RUN/$LN.reg" 2>"$RUN/$LN-enroll.log" &
pid=$!
approve $LN tag:node cat "$RUN/$LN.reg"
wait "$pid" || fail "$LN enrollment exited nonzero"
tag_node $LN "$NODE_ID" tag:node

say "enroll the Windows node (control direct over http; DERP only, so member binds no UDP socket)"
ps_run "\$env:TS_DEBUG_ALWAYS_USE_DERP='1'
\$p = Start-Process -FilePath '$OW_DIR\\member-windows-amd64.exe' -ArgumentList '--mode','node','--state-dir','$OW_DIR\\state','--control-url','$HS_URL','--hostname','$WN','--print-registration' -NoNewWindow -PassThru -RedirectStandardOutput '$OW_DIR\\reg.txt' -RedirectStandardError '$OW_DIR\\enroll.log'
if (-not \$p.WaitForExit(120000)) { Stop-Process -Id \$p.Id -Force; exit 1 }" >"$RUN/$WN-enroll.log" 2>&1 &
pid=$!
ow_reg() { ps_run "Get-Content -Raw -ErrorAction SilentlyContinue '$OW_DIR\\reg.txt'" | tr -d '\r'; }
approve $WN tag:node ow_reg
wait "$pid" || fail "$WN enrollment timed out: $(cat "$RUN/$WN-enroll.log")"
ow_reg | grep -q '"event":"registered"' || fail "$WN not registered"
tag_node $WN "$NODE_ID" tag:node

say "start node members"
# The Linux node: both processes die when this ssh session's stdin (fd 7 here) closes.
mkfifo "$RUN/$LN.in"
"${AW[@]}" "cd $AW_DIR && (timeout 600 ./nettest-linux-amd64 echo --listen \$PWD/agent.sock 2>agent.log & \
	SSL_CERT_FILE=\$PWD/cert.pem timeout 600 ./member-linux-amd64 --mode node --state-dir \$PWD/state \
	--control-url $PX_URL --hostname $LN --forward \$PWD/agent.sock --udp-port 17642 2>member.log & \
	cat >/dev/null; kill \$(jobs -p); wait)" <"$RUN/$LN.in" >"$RUN/$LN-session.log" 2>&1 &
AW_SESSION=$!
exec 7>"$RUN/$LN.in"
# The Windows node: stops on a stop file, or by itself after 10 minutes.
ps_run "\$d='$OW_DIR'; \$env:TS_DEBUG_ALWAYS_USE_DERP='1'
\$e = Start-Process -FilePath \"\$d\\nettest-windows-amd64.exe\" -ArgumentList 'echo','--listen',\"\$d\\agent.sock\" -NoNewWindow -PassThru -RedirectStandardError \"\$d\\agent.log\"
\$m = Start-Process -FilePath \"\$d\\member-windows-amd64.exe\" -ArgumentList '--mode','node','--state-dir',\"\$d\\state\",'--control-url','$HS_URL','--hostname','$WN','--forward',\"\$d\\agent.sock\" -NoNewWindow -PassThru -RedirectStandardError \"\$d\\member.log\"
\"\$(\$e.Id) \$(\$m.Id)\" | Set-Content \"\$d\\pids\"
\$deadline = (Get-Date).AddMinutes(10)
while (-not (Test-Path \"\$d\\stop\") -and (Get-Date) -lt \$deadline) { Start-Sleep -Seconds 1 }
Stop-Process -Id \$m.Id, \$e.Id -Force -ErrorAction SilentlyContinue" >"$RUN/$WN-session.log" 2>&1 &
OW_SESSION=$!
stop_remote() {
	exec 7>&-
	ps_run "New-Item -ItemType File -Force -Path '$OW_DIR\\stop' | Out-Null" || true
	wait "$AW_SESSION" "$OW_SESSION" 2>/dev/null || true
}
trap 'stop_remote; cleanup' EXIT

wait_for 90 "$LN online in the hub's netmap" peer_online "$RUN/hub.sock" $LN
wait_for 90 "$WN online in the hub's netmap" peer_online "$RUN/hub.sock" $WN
echo_check $LN:7475
echo_check $WN:7475
wait_for 30 "direct path to $LN" peer_direct "$RUN/hub.sock" $LN
say "$LN path $(peer_path $LN)"
say "$WN path $(peer_path $WN)"
echo_check $LN:7475
echo_check $WN:7475

say "stop remote processes"
stop_remote
sleep 2
left_aw=$("${AW[@]}" "pgrep -af '[m]ember-linux-amd64|[n]ettest-linux-amd64' || true")
left_ow=$(ps_run "Get-Process | Where-Object { \$_.Path -like '*pn-net-e2e*' } | ForEach-Object { \$_.Id }" | tr -d '\r')
[ -z "$left_aw" ] || fail "$LN still runs: $left_aw"
[ -z "$left_ow" ] || fail "$WN still runs: $left_ow"
say "no remote processes left"
say "PASS ($RUN)"
