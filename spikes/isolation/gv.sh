#!/usr/bin/env bash
# gVisor (runsc) rootless probe: download static binary (sha512 verified) and try `runsc do`.
set -u
C=$(cd "$(dirname "$0")" && pwd)/.cache/gv
mkdir -p "$C"
cd "$C"
B=https://storage.googleapis.com/gvisor/releases/release/20260112.0/aarch64
if [ ! -x runsc ]; then
  curl -fsSLO "$B/runsc" && curl -fsSLO "$B/runsc.sha512" && sha512sum -c runsc.sha512 && chmod +x runsc
fi
./runsc --version | head -2
now_ms() { echo $(( $(date +%s%N) / 1000000 )); }
for plat in systrap ptrace kvm; do
  t0=$(now_ms)
  out=$(./runsc --rootless --network=none --ignore-cgroups --platform=$plat --root "$C/state-$plat" --debug-log=/dev/null do sh -c 'echo ok; ps -e | wc -l; kill -0 1 && echo pid1-visible' 2>&1 | tail -4 | tr '\n' ' ')
  echo "RESULT|gvisor-$plat|do_ms|$(( $(now_ms) - t0 ))|$out"
done
export PATH="$HOME/.local/bin:$HOME/.nvm/versions/node/v22.23.1/bin:$PATH"
t0=$(now_ms)
out=$(./runsc --rootless --network=none --ignore-cgroups --platform=systrap --root "$C/state-tools" --debug-log=/dev/null do sh -c 'claude --version; codex --version; tmux -V; tmux -f /dev/null new -d -s m && sleep 0.3 && tmux ls' 2>&1 | tail -6 | tr '\n' ' ')
echo "RESULT|gvisor-systrap|tools_ms|$(( $(now_ms) - t0 ))|$out"
