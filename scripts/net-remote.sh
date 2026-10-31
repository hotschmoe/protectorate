#!/usr/bin/env bash
# Real-machine check: headscale and the hub member here on the LAN address, node
# members on agent-world (Linux) and officewin (Windows), each with the echo
# agent, in a scratch dir under the remote home. Nothing is installed; every
# remote process stops when this script ends or after 10 minutes at most.
export BIND=${BIND:-192.168.10.44} HS_PORT=${HS_PORT:-17580} PX_PORT=${PX_PORT:-17543} HUB_UDP=${HUB_UDP:-17651}
source "$(dirname "$0")/net-lib.sh"
NAME=$(basename "$RUN")

AW=(ssh -o BatchMode=yes -o ConnectTimeout=10 -i "$HOME/.ssh/agent_world_ed25519" -o IdentitiesOnly=yes agentadmin@192.168.10.56)
AW_SCP=(scp -q -o BatchMode=yes -i "$HOME/.ssh/agent_world_ed25519" -o IdentitiesOnly=yes)
AW_DIR=pn-net-e2e/$NAME
OW=(ssh -o BatchMode=yes -o ConnectTimeout=10 officewin)
OW_DIR='C:\Users\hotschmoe\pn-net-e2e\'$NAME

ps_run() { # PowerShell script text -> officewin, with no profile and no window
	"${OW[@]}" "powershell -NoProfile -NonInteractive -EncodedCommand $(printf '%s' "\$ProgressPreference='SilentlyContinue'; $1" | iconv -f utf-8 -t utf-16le | base64 -w0)"
}

build_bins
for target in "linux amd64" "windows amd64"; do
	"$ROOT/scripts/net-build.sh" member $target
	nettest_for $target
done
start_control
enroll_local hub hub "$HS_URL" tag:hub
start_hub a

say "push to agent-world:~/$AW_DIR"
"${AW[@]}" "mkdir -p -m 0700 $AW_DIR"
"${AW_SCP[@]}" "$BIN/member-linux-amd64" "$BIN/nettest-linux-amd64" "$RUN/cert.pem" "agentadmin@192.168.10.56:$AW_DIR/"
for f in member-linux-amd64 nettest-linux-amd64; do
	[ "$("${AW[@]}" "sha256sum $AW_DIR/$f" | cut -d' ' -f1)" = "$(sha256sum "$BIN/$f" | cut -d' ' -f1)" ] ||
		fail "agent-world $f sha256 mismatch"
done

say "push to officewin:$OW_DIR"
ps_run "New-Item -ItemType Directory -Force -Path '$OW_DIR' | Out-Null"
scp -q -o BatchMode=yes "$BIN/member-windows-amd64.exe" "$BIN/nettest-windows-amd64.exe" \
	"officewin:pn-net-e2e/$NAME/"
for f in member-windows-amd64.exe nettest-windows-amd64.exe; do
	remote=$(ps_run "(Get-FileHash -Algorithm SHA256 '$OW_DIR\\$f').Hash" | tr -d '\r\n' | tr A-F a-f)
	[ "$remote" = "$(sha256sum "$BIN/$f" | cut -d' ' -f1)" ] || fail "officewin $f sha256 mismatch: $remote"
done

say "enroll agent-world (control through the proxy, cert pinned for DERP, trusted via SSL_CERT_FILE)"
"${AW[@]}" "cd $AW_DIR && SSL_CERT_FILE=\$PWD/cert.pem timeout 120 ./member-linux-amd64 --mode node --state-dir \$PWD/state \
	--control-url $PX_URL --hostname agent-world --print-registration" >"$RUN/agent-world.reg" 2>"$RUN/agent-world-enroll.log" &
pid=$!
approve agent-world tag:node cat "$RUN/agent-world.reg"
wait "$pid" || fail "agent-world enrollment exited nonzero"
tag_node agent-world "$NODE_ID" tag:node

say "enroll officewin (control direct over http; DERP only, so member binds no UDP socket)"
ps_run "\$env:TS_DEBUG_ALWAYS_USE_DERP='1'
\$p = Start-Process -FilePath '$OW_DIR\\member-windows-amd64.exe' -ArgumentList '--mode','node','--state-dir','$OW_DIR\\state','--control-url','$HS_URL','--hostname','officewin','--print-registration' -NoNewWindow -PassThru -RedirectStandardOutput '$OW_DIR\\reg.txt' -RedirectStandardError '$OW_DIR\\enroll.log'
if (-not \$p.WaitForExit(120000)) { Stop-Process -Id \$p.Id -Force; exit 1 }" >"$RUN/officewin-enroll.log" 2>&1 &
pid=$!
ow_reg() { ps_run "Get-Content -Raw -ErrorAction SilentlyContinue '$OW_DIR\\reg.txt'" | tr -d '\r'; }
approve officewin tag:node ow_reg
wait "$pid" || fail "officewin enrollment timed out: $(cat "$RUN/officewin-enroll.log")"
ow_reg | grep -q '"event":"registered"' || fail "officewin not registered"
tag_node officewin "$NODE_ID" tag:node

say "start node members"
# agent-world: both processes die when this ssh session's stdin (fd 7 here) closes.
mkfifo "$RUN/agent-world.in"
"${AW[@]}" "cd $AW_DIR && (timeout 600 ./nettest-linux-amd64 echo --listen \$PWD/agent.sock 2>agent.log & \
	SSL_CERT_FILE=\$PWD/cert.pem timeout 600 ./member-linux-amd64 --mode node --state-dir \$PWD/state \
	--control-url $PX_URL --hostname agent-world --forward \$PWD/agent.sock --udp-port 17642 2>member.log & \
	cat >/dev/null; kill \$(jobs -p); wait)" <"$RUN/agent-world.in" >"$RUN/agent-world-session.log" 2>&1 &
AW_SESSION=$!
exec 7>"$RUN/agent-world.in"
# officewin: stops on a stop file, or by itself after 10 minutes.
ps_run "\$d='$OW_DIR'; \$env:TS_DEBUG_ALWAYS_USE_DERP='1'
\$e = Start-Process -FilePath \"\$d\\nettest-windows-amd64.exe\" -ArgumentList 'echo','--listen',\"\$d\\agent.sock\" -NoNewWindow -PassThru -RedirectStandardError \"\$d\\agent.log\"
\$m = Start-Process -FilePath \"\$d\\member-windows-amd64.exe\" -ArgumentList '--mode','node','--state-dir',\"\$d\\state\",'--control-url','$HS_URL','--hostname','officewin','--forward',\"\$d\\agent.sock\" -NoNewWindow -PassThru -RedirectStandardError \"\$d\\member.log\"
\"\$(\$e.Id) \$(\$m.Id)\" | Set-Content \"\$d\\pids\"
\$deadline = (Get-Date).AddMinutes(10)
while (-not (Test-Path \"\$d\\stop\") -and (Get-Date) -lt \$deadline) { Start-Sleep -Seconds 1 }
Stop-Process -Id \$m.Id, \$e.Id -Force -ErrorAction SilentlyContinue" >"$RUN/officewin-session.log" 2>&1 &
OW_SESSION=$!
stop_remote() {
	exec 7>&-
	ps_run "New-Item -ItemType File -Force -Path '$OW_DIR\\stop' | Out-Null" || true
	wait "$AW_SESSION" "$OW_SESSION" 2>/dev/null || true
}
trap 'stop_remote; cleanup' EXIT

wait_for 90 "agent-world online in the hub's netmap" peer_online "$RUN/hub.sock" agent-world
wait_for 90 "officewin online in the hub's netmap" peer_online "$RUN/hub.sock" officewin
echo_check agent-world:7475
echo_check officewin:7475
wait_for 30 "direct path to agent-world" peer_direct "$RUN/hub.sock" agent-world
say "agent-world path $(peer_path agent-world)"
say "officewin path $(peer_path officewin)"
echo_check agent-world:7475
echo_check officewin:7475

say "stop remote processes"
stop_remote
sleep 2
left_aw=$("${AW[@]}" "pgrep -af '[m]ember-linux-amd64|[n]ettest-linux-amd64' || true")
left_ow=$(ps_run "Get-Process | Where-Object { \$_.Path -like '*pn-net-e2e*' } | ForEach-Object { \$_.Id }" | tr -d '\r')
[ -z "$left_aw" ] || fail "agent-world still runs: $left_aw"
[ -z "$left_ow" ] || fail "officewin still runs: $left_ow"
say "no remote processes left"
say "PASS ($RUN)"
