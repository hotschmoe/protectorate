#!/usr/bin/env bash
# Variant driver library. Each variant implements: v_start NAME, v_exec NAME CMD..., v_stop NAME, v_wrap NAME.
# Sandbox layout: $RUN/NAME/{ws,ctl}; $RUN/auth is shared. tmux socket lives in ctl/ so the hub can reach it.

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
UIDN=$(id -u)
GIDN=$(id -g)
RUN=${SPIKE_RUN:-/run/user/$UIDN/spike-iso}
MEM=${SPIKE_MEM:-256M}
CPUS=${SPIKE_CPUS:-1}
IMG=spike-iso-img:2
TOOLS_RO="$HOME/.local $HOME/.nvm $HOME/.grok"
TOOL_PATH="$HOME/.local/bin:$HOME/.nvm/versions/node/v22.23.1/bin:/usr/local/bin:/usr/bin:/bin"
SB_ENV=(HOME="$HOME" PATH="$TOOL_PATH" SHELL=/bin/bash TERM=xterm USER="${USER:-hotschmoe}" LANG=C.UTF-8)

now_ms() { echo $(( $(date +%s%N) / 1000000 )); }

sb_dirs() { mkdir -p "$RUN/$1/ws" "$RUN/$1/ctl" "$RUN/auth"; }

sock() { echo "$RUN/$1/ctl/tmux.sock"; }
sb_tmux() { local n=$1; shift; env -u TMUX tmux -S "$(sock "$n")" "$@"; }

# Paths as seen from inside the sandbox.
case "${V:-}" in
  docker|bwrap|bwrap-cg) WS_IN=/workspace; AUTH_IN=/auth; CTL_IN=/ctl ;;
  *) WS_IN=; AUTH_IN=; CTL_IN= ;;
esac
ws_in() { if [ -n "$WS_IN" ]; then echo "$WS_IN"; else echo "$RUN/$1/ws"; fi; }
auth_in() { if [ -n "$AUTH_IN" ]; then echo "$AUTH_IN"; else echo "$RUN/auth"; fi; }
ctl_in() { if [ -n "$CTL_IN" ]; then echo "$CTL_IN"; else echo "$RUN/$1/ctl"; fi; }

wait_ready() {
  local n=$1 i
  for i in $(seq 1 400); do
    sb_tmux "$n" has-session -t main 2>/dev/null && return 0
    sleep 0.01
  done
  return 1
}

tmux_pid() { local p; for p in $(tree_pids "$(wrap_pid "$1")"); do case "$(cat /proc/$p/comm 2>/dev/null)" in tmux*) echo "$p"; return ;; esac; done; }

tree_pids() {
  local p=$1 c
  echo "$p"
  for c in $(pgrep -P "$p" 2>/dev/null); do tree_pids "$c"; done
}

wrap_pid_file() { echo "$RUN/$1/wrap.pid"; }
wrap_pid() { cat "$(wrap_pid_file "$1")"; }

cpu_ticks() {
  local p t=0 f
  for p in "$@"; do
    f=$(sed 's/^.*) //' "/proc/$p/stat" 2>/dev/null) || continue
    set -- $f
    t=$(( t + ${12} + ${13} ))
  done
  echo "$t"
}

tree_rss_kb() {
  local pids=$(tree_pids "$1") tot=0 r p
  for p in $pids; do
    r=$(ps -o rss= -p "$p" 2>/dev/null | tr -d ' ')
    tot=$(( tot + ${r:-0} ))
  done
  echo "$tot"
}

tree_listing() {
  local p
  for p in $(tree_pids "$1"); do ps -o rss=,comm= -p "$p" 2>/dev/null; done | awk '{printf "%s(%dk) ", $2, $1}'
  echo
}

shell_cmd_env() { printf 'env'; local e; for e in "${SB_ENV[@]}"; do printf ' %q' "$e"; done; }

# ---------------- none (baseline: no isolation) ----------------
v_none_start() {
  local n=$1; sb_dirs "$n"
  env "${SB_ENV[@]}" sh -c "tmux -f /dev/null -S $(sock "$n") new-session -d -s main -c $RUN/$n/ws; while tmux -S $(sock "$n") has-session 2>/dev/null; do sleep 1; done" >/dev/null 2>&1 &
  echo $! > "$(wrap_pid_file "$n")"
}
v_none_exec() { tmux_exec "$@"; }
v_none_stop() { sb_tmux "$1" kill-server 2>/dev/null; }

# ---------------- docker ----------------
v_docker_start() {
  local n=$1; sb_dirs "$n"
  local tools=() t
  for t in $TOOLS_RO; do tools+=(-v "$t:$t:ro"); done
  docker run -d --rm --init --name "spike-iso-$n" --user "$UIDN:$GIDN" \
    --memory "$MEM" --memory-swap "$MEM" --cpus "$CPUS" --pids-limit 512 \
    --tmpfs "$HOME:uid=$UIDN,gid=$GIDN,mode=755" \
    -e HOME="$HOME" -e PATH="$TOOL_PATH" -e SHELL=/bin/bash -e TERM=xterm \
    -v "$RUN/$n/ws:/workspace" -v "$RUN/auth:/auth" -v "$RUN/$n/ctl:/ctl" "${tools[@]}" -w /workspace \
    "$IMG" sh -c 'tmux -f /dev/null -S /ctl/tmux.sock new-session -d -s main; while tmux -S /ctl/tmux.sock has-session 2>/dev/null; do sleep 1; done' >/dev/null
  local pid; pid=$(docker inspect -f '{{.State.Pid}}' "spike-iso-$n")
  ps -o ppid= -p "$pid" | tr -d ' ' > "$(wrap_pid_file "$n")"
}
v_docker_exec() { local n=$1; shift; docker exec -u "$UIDN:$GIDN" "spike-iso-$n" "$@"; }
v_docker_stop() { docker stop -t 1 "spike-iso-$1" >/dev/null 2>&1; }

# ---------------- bwrap ----------------
bwrap_cmd() {
  local n=$1 t
  local args=(bwrap --unshare-pid --unshare-ipc --unshare-uts --die-with-parent
    --tmpfs / --ro-bind /usr /usr --symlink usr/bin /bin --symlink usr/lib /lib --symlink usr/sbin /sbin --ro-bind /etc /etc --dev /dev --proc /proc --tmpfs /tmp --tmpfs "$HOME")
  for t in $TOOLS_RO; do args+=(--ro-bind "$t" "$t"); done
  args+=(--bind "$RUN/$n/ws" /workspace --bind "$RUN/auth" /auth --bind "$RUN/$n/ctl" /ctl --chdir /workspace)
  args+=(--clearenv)
  local e; for e in "${SB_ENV[@]}"; do args+=(--setenv "${e%%=*}" "${e#*=}"); done
  args+=(sh -c 'tmux -f /dev/null -S /ctl/tmux.sock new-session -d -s main; while tmux -S /ctl/tmux.sock has-session 2>/dev/null; do sleep 1; done')
  printf '%s\0' "${args[@]}"
}
v_bwrap_start() {
  local n=$1; sb_dirs "$n"
  local a=(); mapfile -d '' a < <(bwrap_cmd "$n")
  "${a[@]}" >/dev/null 2>&1 &
  echo $! > "$(wrap_pid_file "$n")"
}
tmux_exec() {
  local n=$1; shift
  local c; c=$(printf '%q ' "$@")
  : > "$RUN/$n/ctl/exec.rc"
  sb_tmux "$n" new-window -d "sh -c $(printf '%q' "$c") > $(ctl_in "$n")/exec.out 2>&1; echo \$? > $(ctl_in "$n")/exec.rc"
  local i
  for i in $(seq 1 1500); do [ -s "$RUN/$n/ctl/exec.rc" ] && break; sleep 0.02; done
  cat "$RUN/$n/ctl/exec.out"
  return "$(cat "$RUN/$n/ctl/exec.rc")"
}
v_bwrap_exec() { tmux_exec "$@"; }
kill_wrap() { local p; p=$(wrap_pid "$1"); kill -KILL "$p" 2>/dev/null; }
v_bwrap_stop() { kill_wrap "$1"; }

# ---------------- bwrap + cgroup scope ----------------
v_bwrap-cg_start() {
  local n=$1; sb_dirs "$n"
  local a=(); mapfile -d '' a < <(bwrap_cmd "$n")
  systemd-run --user --scope --quiet --collect --unit="spike-iso-$n" \
    -p MemoryMax="$MEM" -p MemorySwapMax=0 -p CPUQuota=$((CPUS * 100))% -p TasksMax=512 -p OOMPolicy=continue -- "${a[@]}" >/dev/null 2>&1 &
  echo $! > "$(wrap_pid_file "$n")"
}
v_bwrap-cg_exec() { tmux_exec "$@"; }
v_bwrap-cg_stop() { systemctl --user stop "spike-iso-$1.scope" 2>/dev/null; }

# ---------------- plain unshare (pid+mount ns for /proc, nested userns to keep uid) ----------------
unshare_inner() {
  local n=$1
  echo "exec unshare -U --map-user=$UIDN --map-group=$GIDN -- $(shell_cmd_env) sh -c \"tmux -f /dev/null -S $RUN/$n/ctl/tmux.sock new-session -d -s main -c $RUN/$n/ws; while tmux -S $RUN/$n/ctl/tmux.sock has-session 2>/dev/null; do sleep 1; done\""
}
v_unshare_start() {
  local n=$1; sb_dirs "$n"
  unshare -Urpf --kill-child --mount-proc -- sh -c "$(unshare_inner "$n")" >/dev/null 2>&1 &
  echo $! > "$(wrap_pid_file "$n")"
}
v_unshare_exec() { tmux_exec "$@"; }
v_unshare_stop() { kill_wrap "$1"; }

# ---------------- unshare inside a systemd scope with cgroup limits ----------------
v_unshare-cg_start() {
  local n=$1; sb_dirs "$n"
  systemd-run --user --scope --quiet --collect --unit="spike-iso-$n" \
    -p MemoryMax="$MEM" -p MemorySwapMax=0 -p CPUQuota=$((CPUS * 100))% -p TasksMax=512 -p OOMPolicy=continue -- \
    unshare -Urpf --kill-child --mount-proc -- sh -c "$(unshare_inner "$n")" >/dev/null 2>&1 &
  echo $! > "$(wrap_pid_file "$n")"
}
v_unshare-cg_exec() { tmux_exec "$@"; }
v_unshare-cg_stop() { systemctl --user stop "spike-iso-$1.scope" 2>/dev/null; }

sb_start() { "v_${V}_start" "$@"; }
sb_exec() { "v_${V}_exec" "$@"; }
sb_stop() { "v_${V}_stop" "$@"; }

has_limits() { case "$V" in docker|bwrap-cg|unshare-cg) return 0 ;; *) return 1 ;; esac; }

emit() { echo "RESULT|$V|$1|$2"; }
