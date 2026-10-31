#!/bin/sh
# Builds the Go blobs into go/bin/<name>-<goos>-<goarch>[.exe], one go build at a
# time (other builds share this machine).
# Usage: scripts/net-build.sh                       every release target
#        scripts/net-build.sh host                  the host's headscale and member
#        scripts/net-build.sh <name> <goos> <goarch>  one blob
set -eu
cd "$(dirname "$0")/../go"
export CGO_ENABLED=0 GOFLAGS="${GOFLAGS:--p=4}" GOMAXPROCS="${GOMAXPROCS:-4}"

# Features compiled out of member (go/README.md). peerapiserver is the one the
# hub member refuses to start without; the rest drop surface and WAN chatter.
MEMBER_TAGS=ts_omit_peerapiserver,ts_omit_ssh,ts_omit_taildrop,ts_omit_webclient,\
ts_omit_logtail,ts_omit_netlog,ts_omit_clientupdate,ts_omit_drive,ts_omit_capture,\
ts_omit_tailnetlock,ts_omit_relayserver,ts_omit_portmapper,ts_omit_captiveportal,\
ts_omit_advertiseexitnode,ts_omit_advertiseroutes,ts_omit_useexitnode,ts_omit_appconnectors,\
ts_omit_conn25,ts_omit_c2n,ts_omit_remoteconfig,ts_omit_oauthkey,ts_omit_identityfederation,\
ts_omit_outboundproxy,ts_omit_cloud,ts_omit_aws,ts_omit_kube,ts_omit_tap,ts_omit_bird,\
ts_omit_synology,ts_omit_systray,ts_omit_wakeonlan,ts_omit_debugeventbus,ts_omit_posture,\
ts_omit_desktop_sessions

mkdir -p bin
build() { # name goos goarch tags
	ext=""
	[ "$2" = windows ] && ext=.exe
	out="bin/$1-$2-$3$ext"
	start=$(date +%s)
	GOOS=$2 GOARCH=$3 go build -tags "$4" -trimpath -ldflags "-s -w" -o "$out" "./cmd/$1"
	echo "$out $(wc -c <"$out") bytes $(($(date +%s) - start))s"
}

tags_for() { [ "$1" = member ] && echo "$MEMBER_TAGS" || true; }
case $# in
0) ;;
1)
	[ "$1" = host ] || { echo "usage: $0 [host | <name> <goos> <goarch>]" >&2; exit 2; }
	build headscale "$(go env GOHOSTOS)" "$(go env GOHOSTARCH)" ""
	build member "$(go env GOHOSTOS)" "$(go env GOHOSTARCH)" "$MEMBER_TAGS"
	exit 0
	;;
3)
	build "$1" "$2" "$3" "$(tags_for "$1")"
	exit 0
	;;
*) echo "usage: $0 [host | <name> <goos> <goarch>]" >&2; exit 2 ;;
esac
for arch in arm64 amd64; do
	build headscale linux $arch ""
done
for target in linux/arm64 linux/amd64 windows/amd64 windows/arm64 android/arm64; do
	build member "${target%/*}" "${target#*/}" "$MEMBER_TAGS"
done
