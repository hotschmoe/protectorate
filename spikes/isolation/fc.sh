#!/usr/bin/env bash
# Firecracker microVM measurements: boot-to-userspace latency and host memory overhead.
# Usage: fc.sh [mem_mib] [vcpus]. Needs ./fc-setup.sh first. No networking (TAP needs root).
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
C=$HERE/.cache
FC=$C/fc/release-v1.17.0-aarch64/firecracker-v1.17.0-aarch64
KERNEL=$C/ci/vmlinux-6.1.155
ROOTFS=$C/ci/ubuntu-24.04.squashfs
MEMMB=${1:-256}
VCPUS=${2:-1}
RUN=${SPIKE_RUN:-/run/user/$(id -u)/spike-iso}/fc
mkdir -p "$RUN"
now_ms() { echo $(( $(date +%s%N) / 1000000 )); }

boot() {
  local name=$1 args=$2 marker=$3
  cat > "$RUN/$name.json" <<EOF
{
  "boot-source": {"kernel_image_path": "$KERNEL", "boot_args": "$args"},
  "drives": [{"drive_id": "rootfs", "path_on_host": "$ROOTFS", "is_root_device": true, "is_read_only": true}],
  "machine-config": {"vcpu_count": $VCPUS, "mem_size_mib": $MEMMB}
}
EOF
  : > "$RUN/$name.log"
  local t0; t0=$(now_ms)
  "$FC" --no-api --config-file "$RUN/$name.json" > "$RUN/$name.log" 2>&1 &
  local pid=$!
  local i
  for i in $(seq 1 3000); do
    grep -q "$marker" "$RUN/$name.log" 2>/dev/null && break
    sleep 0.005
  done
  local t1; t1=$(now_ms)
  sleep 0.3
  local rss pss
  rss=$(awk '/VmRSS/{print $2}' /proc/$pid/status 2>/dev/null || echo 0)
  pss=$(awk '/^Pss:/{print $2}' /proc/$pid/smaps_rollup 2>/dev/null || echo 0)
  echo "RESULT|firecracker|${name}_boot_ms|$((t1 - t0))"
  echo "RESULT|firecracker|${name}_host_rss_kb|$rss (mem_size ${MEMMB}MiB, vcpus $VCPUS)"
  kill -9 $pid 2>/dev/null || true
  wait $pid 2>/dev/null || true
}

INIT_ARGS='console=ttyS0 reboot=k panic=1 pci=off 8250.nr_uarts=1 root=/dev/vda ro rootfstype=squashfs'
for n in 1 2 3; do
  boot "minit$n" "$INIT_ARGS init=/bin/echo -- BOOT_MARK" BOOT_MARK
done
boot systemd "$INIT_ARGS init=/sbin/init" "login:"

cat > "$RUN/probe.json" <<EOF
{
  "boot-source": {"kernel_image_path": "$KERNEL", "boot_args": "$INIT_ARGS init=/sbin/init"},
  "drives": [{"drive_id": "rootfs", "path_on_host": "$ROOTFS", "is_root_device": true, "is_read_only": true}],
  "machine-config": {"vcpu_count": $VCPUS, "mem_size_mib": $MEMMB}
}
EOF
(sleep 3; echo 'echo PROBE; which tmux gcc node claude; free -m | head -2; nproc; ip -br link; poweroff -f'; sleep 3) | timeout 15 "$FC" --no-api --config-file "$RUN/probe.json" 2>&1 | tr -d '\r' | sed -n '/^PROBE/,$p' | head -20
