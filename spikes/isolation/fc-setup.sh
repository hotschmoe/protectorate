#!/usr/bin/env bash
# Downloads Firecracker (release, sha256 verified) + CI kernel/rootfs into .cache/. Idempotent.
set -eu
C=$(cd "$(dirname "$0")" && pwd)/.cache
FCV=v1.17.0
mkdir -p "$C/fc" "$C/ci"
cd "$C/fc"
if [ ! -x "release-$FCV-aarch64/firecracker-$FCV-aarch64" ]; then
  base=https://github.com/firecracker-microvm/firecracker/releases/download/$FCV
  curl -fsSLO "$base/firecracker-$FCV-aarch64.tgz"
  curl -fsSLO "$base/firecracker-$FCV-aarch64.tgz.sha256.txt"
  sha256sum -c "firecracker-$FCV-aarch64.tgz.sha256.txt"
  tar xzf "firecracker-$FCV-aarch64.tgz"
fi
cd "$C/ci"
B=https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.15/aarch64
for f in vmlinux-6.1.155 ubuntu-24.04.squashfs; do
  [ -s "$f" ] || curl -fsSLO "$B/$f"
done
sha256sum vmlinux-6.1.155 ubuntu-24.04.squashfs
