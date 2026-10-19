#!/usr/bin/env bash
# Usage: run.sh VARIANT [test...]   VARIANT: none docker bwrap bwrap-cg unshare unshare-cg
# Tests: start mem pid limits mounts tools tmux (default: all). Prints RESULT|variant|key|value lines.
export V=${1:?variant}; shift
. "$(dirname "$0")/lib.sh"
mkdir -p "$RUN"
TESTS=${*:-start mem pid limits mounts tools tmux}

median() { sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
cleanup() { for n in A B ${CS:-}; do sb_stop "$n" 2>/dev/null; done; pkill -f '^sleep 7002$' 2>/dev/null; }
trap cleanup EXIT

t_start() {
  local ts=() i t0
  CS=""
  for i in 1 2 3 4 5; do
    t0=$(now_ms); sb_start "cs$i"; CS="$CS cs$i"
    wait_ready "cs$i" || { emit start_fail "cs$i"; continue; }
    ts+=($(( $(now_ms) - t0 )))
  done
  emit cold_start_ms_median "$(printf '%s\n' "${ts[@]}" | median)"
  emit cold_start_ms_all "${ts[*]}"
  for i in 1 2 3 4 5; do sb_stop "cs$i"; done
  sleep 1
}

t_mem() {
  sb_start A; wait_ready A; sleep 2
  local w; w=$(wrap_pid A)
  emit idle_rss_kb_sum "$(tree_rss_kb "$w")"
  emit idle_procs "$(tree_listing "$w")"
  case "$V" in
    docker) emit docker_stats_mem "$(docker stats --no-stream --format '{{.MemUsage}}' spike-iso-A)" ;;
    *-cg) emit cgroup_mem_current_kb "$(( $(cat /sys/fs/cgroup/user.slice/user-$UIDN.slice/user@$UIDN.service/app.slice/spike-iso-A.scope/memory.current 2>/dev/null || echo 0) / 1024 ))" ;;
  esac
  sb_stop A; sleep 1
}

t_pid() {
  sb_start A; sb_start B; wait_ready A; wait_ready B
  setsid sleep 7002 & local hostpid=$!
  sb_tmux B send-keys -t main 'sleep 7003' Enter; sleep 0.5
  local bpid; bpid=$(pgrep -f '^sleep 7003$' | head -1)
  emit host_pid_of_B_sleep "${bpid:-none}"
  emit A_sees_host_and_B_sleeps "$(sb_exec A sh -c "ps -eo args | grep -c '^sleep 700[23]\$'")"
  emit A_total_visible_procs "$(sb_exec A sh -c 'ps -e --no-headers | wc -l')"
  emit A_kill_by_hostpid "$(sb_exec A sh -c "kill -9 $hostpid $bpid 2>&1; echo rc=\$?" | tr "\n" " ")"
  sb_exec A pkill -9 -f '^sleep 700[23]$'; emit A_pkill_rc $?
  sleep 0.5
  kill -0 "$hostpid" 2>/dev/null && emit host_dummy_survived yes || emit host_dummy_survived NO
  pgrep -f '^sleep 7003$' >/dev/null && emit B_sleep_survived yes || emit B_sleep_survived NO
  kill "$hostpid" 2>/dev/null
  pkill -f '^sleep 7003$' 2>/dev/null
  sb_stop A; sb_stop B; sleep 1
}

t_limits() {
  sb_start A; wait_ready A
  local w; w=$(wrap_pid A)
  sb_tmux A new-window -d -t main 'for i in 1 2 3 4; do sh -c "while :; do :; done" & done; sleep 8'
  sleep 2
  local t1 t2; t1=$(cpu_ticks $(tree_pids "$w")); sleep 4; t2=$(cpu_ticks $(tree_pids "$w"))
  emit cpu_cores_used_with_4_hogs "$(awk -v d=$((t2 - t1)) 'BEGIN{printf "%.2f", d/400}')"
  sleep 3
  if has_limits; then
    sb_tmux A new-window -d -t main "tail /dev/zero; echo \$? > $(ws_in A)/memhog.rc"
    sleep 6
    emit memhog_exit_code "$(cat "$RUN/A/ws/memhog.rc" 2>/dev/null || echo none)"
    emit host_free_mb "$(free -m | awk '/Mem:/{print $4}')"
    emit sandbox_alive_after_oom "$(sb_tmux A has-session -t main 2>&1 && echo yes || echo no)"
  else
    emit memhog_exit_code "not_run_no_limit"
  fi
  sb_stop A; sleep 1
}

t_mounts() {
  sb_start A; sb_start B; wait_ready A; wait_ready B
  local a; a=$(auth_in A)
  sb_exec A sh -c "echo rw > $(ws_in A)/f"; emit ws_rw_visible_on_host "$(cat "$RUN/A/ws/f" 2>/dev/null || echo NO)"
  sb_exec A sh -c "echo v1 > $a/cred.json"
  sb_exec A sh -c "echo v2 > $a/.cred.tmp && mv $a/.cred.tmp $a/cred.json"
  emit shared_dir_rename_seen_by_B "$(sb_exec B cat "$(auth_in B)/cred.json")"
  # single-file bind mount
  echo orig > "$RUN/single.json"
  case "$V" in
    docker|bwrap|bwrap-cg)
      sb_stop A; sleep 1
      if [ "$V" = docker ]; then
        docker run -d --rm --init --name spike-iso-A -v "$RUN/single.json:/single/cred.json" "$IMG" sleep 30 >/dev/null
        sleep 1
        emit single_file_bind_rename "$(docker exec spike-iso-A sh -c 'echo new > /single/.t && mv /single/.t /single/cred.json' 2>&1 | head -1)"
        docker stop -t 1 spike-iso-A >/dev/null
      else
        emit single_file_bind_rename "$(bwrap --ro-bind / / --tmpfs /mnt --bind "$RUN/single.json" /mnt/cred.json sh -c "echo new > /mnt/.t && mv /mnt/.t /mnt/cred.json" 2>&1 | head -1)"
        [ -f "$RUN/.t" ] || true
      fi
      sb_start A; wait_ready A ;;
    *) emit single_file_bind_rename "n/a_no_mount_ns" ;;
  esac
  # symlink approaches, written from A, read from B
  local w; w=$(ws_in A)
  sb_exec A sh -c "mkdir -p $w/cli1 $w/cli2 && ln -sfn $a/cred.json $w/cli1/cred.json && ln -sfn $a $w/cli2/auth"
  sb_exec A sh -c "echo v3 > $w/cli1/.t && mv $w/cli1/.t $w/cli1/cred.json"
  emit symlink_file_rename_B_sees "$(sb_exec B cat "$(auth_in B)/cred.json")"
  emit symlink_file_still_symlink "$(sb_exec A sh -c "test -L $w/cli1/cred.json && echo yes || echo no")"
  sb_exec A sh -c "echo v4 > $w/cli2/auth/.t && mv $w/cli2/auth/.t $w/cli2/auth/cred.json"
  emit symlink_dir_rename_B_sees "$(sb_exec B cat "$(auth_in B)/cred.json")"
  sb_stop A; sb_stop B; sleep 1
}

t_tools() {
  sb_start A; wait_ready A
  local c r
  for c in "claude --version" "codex --version" "pi --version" "grok --version" "gcc --version" "git --version" "node --version" "go version" "tmux -V"; do
    r=$(sb_exec A sh -c "$c 2>&1 | head -1" 2>&1 | head -1)
    emit "tool_${c%% *}" "${r:-EMPTY}"
  done
  emit sees_host_home_files "$(sb_exec A sh -c "ls $HOME" 2>&1 | tr '\n' ' ')"
  sb_stop A; sleep 1
}

t_tmux() {
  sb_start A; wait_ready A
  local f; f="$(ws_in A)/tmuxtest"; : > "$RUN/A/ws/tmuxtest"
  sb_tmux A send-keys -t main "echo hub-sent > $f" Enter; sleep 1.5
  emit hub_send_keys_effect "$(cat "$RUN/A/ws/tmuxtest" 2>/dev/null || echo NO)"
  emit hub_capture_pane "$(sb_tmux A capture-pane -p -t main | grep -c hub-sent)"
  local out
  out=$( (sleep 1; printf 'echo attached-ok\r'; sleep 0.7; printf '\002d'; sleep 0.5) | timeout 8 script -qec "env -u TMUX tmux -S $(sock A) attach -t main" /dev/null 2>&1 | tr -d '\r' | grep -c attached-ok)
  emit hub_interactive_attach_via_pty "$out"
  emit tmux_server_pid_seen_by_itself "$(sb_tmux A display-message -p -t main "#{pid}")"
  [ "$V" = docker ] && emit docker_exec_tmux "$(docker exec spike-iso-A tmux -S /ctl/tmux.sock list-sessions 2>&1 | head -1)"
  sb_stop A; sleep 1
}

for t in $TESTS; do "t_$t"; done
