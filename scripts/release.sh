#!/usr/bin/env bash
# Release assets, built locally (no CI build, no signing key).
#
#   scripts/release.sh assets [--snapshot]
#                                (make release-assets) builds every asset into dist/<version>/,
#                                writes VERSION and SHA256SUMS, and prints the wall time per step.
#                                Refuses unless VERSION is a semver tag on HEAD of a clean tree;
#                                --snapshot (make release-snapshot) builds any VERSION, for trying
#                                the build, never for publishing.
#   scripts/release.sh publish   (make release-publish) uploads dist/<version>/ with
#                                `gh release create`; a dry run that only prints the command
#                                unless PUBLISH=1. Refuses a dirty tree or an untagged HEAD.
#
# Machine-specific settings, from the environment or the gitignored .env.e2e at the repo root:
#   ZIG                           the zig binary for all hub & node cross-compiles (default: zig)
#   PT_RELEASE_REPO               OWNER/NAME of the GitHub repo (default: install.sh's REPO)
#   PT_RELEASE_WIN_HOST           ssh destination of a Windows x64 machine with Flutter (optional)
#   PT_RELEASE_WIN_DIR            the build's folder under %USERPROFILE% there (default pt-release)
#   PT_RELEASE_WIN_FLUTTER        Flutter's bin folder on the Windows host (C:\dev\flutter\bin)
#   PT_RELEASE_APK_HOST           ssh destination of a Linux machine with Flutter & Android SDK
#   PT_RELEASE_APK_SSH_KEY        an ssh identity for it (optional)
#   PT_RELEASE_APK_PATH, PT_RELEASE_APK_ANDROID_HOME   relative paths under HOME
#   PT_RELEASE_APK_SIGNING        Android signing properties (~/.config/protectorate/android-release.properties)
#   PT_RELEASE_APKSIGNER          the apksigner binary here (default: apksigner on PATH)
#   PT_RELEASE_ARM64_NODE         fleet node (Windows arm64, Flutter, VS) that builds desktop-arm64
#   PT_RELEASE_ARM64_DIR, PT_RELEASE_ARM64_PATH, PT_RELEASE_ARM64_TIMEOUT
#   PT_RELEASE_HUB_CLI            hub CLI command used to dispatch desktop-arm64 (default: protectorate)
#   PT_RELEASE_SERVE_ADDR         address of this machine reachable by PT_RELEASE_ARM64_NODE
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
if [ -f .env.e2e ]; then set -a; . ./.env.e2e; set +a; fi
REPO=${PT_RELEASE_REPO:-$(sed -n 's/^REPO=//p' scripts/install.sh)}
[ -n "${VERSION:-}" ] || { echo "release: VERSION is not set; run make release-assets or make release-publish" >&2; exit 2; }
DIST=$ROOT/dist/$VERSION
LOGS=$ROOT/dist/$VERSION.logs
NOTES=$ROOT/dist/$VERSION.notes.md
WORK=${PT_RELEASE_WORK_DIR:-$ROOT/zig-out/release-work}
ZIG=${ZIG:-$(command -v zig || true)}
WIN_HOST=${PT_RELEASE_WIN_HOST:-}
WIN_DIR=${PT_RELEASE_WIN_DIR:-pt-release}
WEB_DIR=$ROOT/app/build/web
APP_OUT=$WORK/$VERSION
APP_TAR=$APP_OUT/app-src.tar
APP_FLAGS=${APP_VERSION_FLAGS:-}
APK_HOST=${PT_RELEASE_APK_HOST:-}
APK_KEY=${PT_RELEASE_APK_SSH_KEY:-}
APK_PATH=${PT_RELEASE_APK_PATH:-flutter/bin}
APK_ANDROID_HOME=${PT_RELEASE_APK_ANDROID_HOME:-Android/Sdk}
APK_SIGNING=${PT_RELEASE_APK_SIGNING:-$HOME/.config/protectorate/android-release.properties}
APKSIGNER=${PT_RELEASE_APKSIGNER:-$(command -v apksigner || true)}
WIN_FLUTTER=${PT_RELEASE_WIN_FLUTTER:-'C:\dev\flutter\bin'}
ARM64_NODE=${PT_RELEASE_ARM64_NODE:-}
ARM64_DIR=${PT_RELEASE_ARM64_DIR:-'C:\dev\pt-release'}
ARM64_PATH=${PT_RELEASE_ARM64_PATH:-'C:\dev\flutter\bin;C:\Program Files\Git\cmd'}
ARM64_TIMEOUT=${PT_RELEASE_ARM64_TIMEOUT:-3600}
SERVE_ADDR=${PT_RELEASE_SERVE_ADDR:-}
read -r -a HUB_CLI <<<"${PT_RELEASE_HUB_CLI:-protectorate}"

ASSETS=(
	protectorate-linux-arm64 protectorate-linux-amd64 protectorate-macos-arm64
	protectorate-windows-amd64.exe protectorate-windows-arm64.exe
	protectorate-node-linux-arm64 protectorate-node-linux-amd64 protectorate-node-macos-arm64
	protectorate-node-windows-amd64.exe protectorate-node-windows-arm64.exe
	protectorate-android.apk protectorate-desktop-windows-amd64.zip protectorate-desktop-windows-arm64.zip
	install.sh install-node.sh install-node.ps1
	VERSION
)

say() { printf '== %s\n' "$*"; }
die() { printf 'release: %s\n' "$*" >&2; exit 1; }
now_ms() { echo $(($(date +%s%N) / 1000000)); }

step() {
	local name=$1 t0 pid rc=0
	shift
	t0=$(now_ms)
	"$@" >"$LOGS/$name.log" 2>&1 &
	pid=$!
	wait "$pid" || rc=$?
	if [ "$rc" = 0 ]; then
		echo $(($(now_ms) - t0)) >"$LOGS/$name.ms"
		printf '   %-14s done in %s s\n' "$name" "$(secs "$(cat "$LOGS/$name.ms")")"
	else
		echo failed >"$LOGS/$name.ms"
		printf '   %-14s FAILED (exit %s); last lines of %s:\n' "$name" "$rc" "$LOGS/$name.log" >&2
		tail -n 30 "$LOGS/$name.log" >&2
		return "$rc"
	fi
}

secs() { [ "$1" = failed ] && echo failed || printf '%d.%01d' $(($1 / 1000)) $((($1 % 1000) / 100)); }

build_web() {
	make --no-print-directory web VERSION="$VERSION"
	[ -f "$WEB_DIR/index.html" ] || die "no $WEB_DIR/index.html after the web build"
}

build_zig() {
	local commit host_triple
	commit=$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
	"$ZIG" build release-all -Doptimize=safe -Dversion="$VERSION" -Dgit-commit="$commit" -Dweb-dir="$WEB_DIR"
	case "$(uname -m)" in
	aarch64 | arm64) host_triple=aarch64-linux-musl ;;
	*) host_triple=x86_64-linux-musl ;;
	esac
	local hub_ver node_ver
	hub_ver=$("$ROOT/zig-out/release/$host_triple/protectorate" --version) || die "host hub binary failed"
	case "$hub_ver" in
	"protectorate $VERSION"|"protectorate $VERSION "*) ;;
	*) die "host hub reports '$hub_ver', expected 'protectorate $VERSION'" ;;
	esac
	node_ver=$("$ROOT/zig-out/release/$host_triple/protectorate-node" --version) || die "host node binary failed"
	case "$node_ver" in
	"protectorate-node $VERSION"|"protectorate-node $VERSION "*) ;;
	*) die "host node reports '$node_ver', expected 'protectorate-node $VERSION'" ;;
	esac
}

win() { ssh -o BatchMode=yes "$WIN_HOST" "$@"; }

win_pull() {
	local remote=$1 out=$2 want got
	mkdir -p "$(dirname "$out")"
	want=$(win "powershell -NoProfile -NonInteractive -Command (Get-FileHash -Algorithm SHA256 '$remote').Hash" |
		tr -d '\r\n' | tr 'A-F' 'a-f')
	[[ "$want" =~ ^[0-9a-f]{64}$ ]] || die "no sha256 for $remote on $WIN_HOST"
	if [ -f "$out" ] && [ "$(sha256sum <"$out" | cut -d' ' -f1)" = "$want" ]; then
		echo "$out is current ($want)"
		return 0
	fi
	win "powershell -NoProfile -NonInteractive -Command [Console]::Out.Write([Convert]::ToBase64String([IO.File]::ReadAllBytes('$remote')))" |
		tr -d '\r\n' | base64 -d >"$out.part"
	got=$(sha256sum <"$out.part" | cut -d' ' -f1)
	[ "$got" = "$want" ] || die "pulled $out.part has sha256 $got, $WIN_HOST has $want"
	mv -f "$out.part" "$out"
	echo "pulled $out ($want)"
}

app_src() {
	mkdir -p "$APP_OUT"
	git ls-files -z --cached --others --exclude-standard -- app |
		tar --null --ignore-failed-read -cf "$APP_TAR" -T -
}

apk_ssh() { ssh -o BatchMode=yes ${APK_KEY:+-i "$APK_KEY" -o IdentitiesOnly=yes} "$APK_HOST" "$@"; }

build_apk() {
	local out=$APP_OUT/protectorate-android.apk ks alias
	[ -f "$APK_SIGNING" ] || die "no signing properties at $APK_SIGNING (PT_RELEASE_APK_SIGNING)"
	[ -x "$APKSIGNER" ] || die "no apksigner at $APKSIGNER (PT_RELEASE_APKSIGNER)"
	apk_ssh "mkdir -p pt-release"
	scp -q -o BatchMode=yes ${APK_KEY:+-i "$APK_KEY" -o IdentitiesOnly=yes} "$APP_TAR" "$APK_HOST:pt-release/app.tar"
	apk_ssh "cd pt-release && tar -xf app.tar && cd app" \
		"&& export ANDROID_HOME=\"\$HOME/$APK_ANDROID_HOME\" PATH=\"\$HOME/$APK_PATH:\$PATH\"" \
		"&& flutter build apk --release $APP_FLAGS"
	scp -q -o BatchMode=yes ${APK_KEY:+-i "$APK_KEY" -o IdentitiesOnly=yes} \
		"$APK_HOST:pt-release/app/build/app/outputs/flutter-apk/app-release.apk" "$out.unsigned"
	ks=$(sed -n 's/^storeFile=//p' "$APK_SIGNING")
	alias=$(sed -n 's/^keyAlias=//p' "$APK_SIGNING")
	KS_PASS=$(sed -n 's/^storePassword=//p' "$APK_SIGNING") KEY_PASS=$(sed -n 's/^keyPassword=//p' "$APK_SIGNING") \
		"$APKSIGNER" sign --v4-signing-enabled false --ks "$ks" --ks-key-alias "$alias" --ks-pass env:KS_PASS --key-pass env:KEY_PASS \
		--out "$out.part" "$out.unsigned"
	"$APKSIGNER" verify --print-certs "$out.part" | grep 'SHA-256'
	mv -f "$out.part" "$out"
	rm -f "$out.unsigned"
}

desktop_zip() { echo "protectorate-desktop-windows-$1.zip"; }

build_desktop_amd64() {
	local zip
	zip=$(desktop_zip amd64)
	win "if not exist %USERPROFILE%\\$WIN_DIR mkdir %USERPROFILE%\\$WIN_DIR"
	scp -q -o BatchMode=yes "$APP_TAR" "$WIN_HOST:$WIN_DIR/app.tar"
	win "cd /d %USERPROFILE%\\$WIN_DIR && tar -xf app.tar && cd app" \
		"&& set \"PATH=$WIN_FLUTTER;%PATH%\"" \
		"&& call flutter build windows --release $APP_FLAGS" \
		"&& powershell -NoProfile -NonInteractive -Command Compress-Archive -Force" \
		"-Path build\\windows\\x64\\runner\\Release\\* -DestinationPath %USERPROFILE%\\$WIN_DIR\\$zip"
	win_pull "%USERPROFILE%\\$WIN_DIR\\$zip" "$APP_OUT/$zip"
}

build_desktop_arm64() {
	local zip sum nonce srv port='' i id='' state='' ps want got
	zip=$(desktop_zip arm64)
	sum=$(sha256sum <"$APP_TAR" | cut -d' ' -f1)
	"${HUB_CLI[@]}" --json node show "$ARM64_NODE" | grep -q '"online":true' ||
		die "arm64 node $ARM64_NODE is offline"
	nonce=$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')
	python3 -I -c '
import http.server, sys
addr, path, file = sys.argv[1], sys.argv[2], sys.argv[3]
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != path:
            self.send_response(404); self.end_headers(); return
        data = open(file, "rb").read()
        self.send_response(200); self.send_header("content-length", str(len(data))); self.end_headers()
        self.wfile.write(data)
    def log_message(self, *a): pass
s = http.server.HTTPServer((addr, 0), H)
print(s.server_address[1], flush=True)
s.serve_forever()' "$SERVE_ADDR" "/$nonce/app.tar" "$APP_TAR" >"$LOGS/arm64-serve.port" &
	srv=$!
	for i in $(seq 50); do
		port=$(cat "$LOGS/arm64-serve.port")
		[ -n "$port" ] && break
		sleep 0.1
	done
	[ -n "$port" ] || { kill "$srv"; die "the source server did not start on $SERVE_ADDR"; }
	ps=$(cat <<EOF
\$ProgressPreference = 'SilentlyContinue'
\$job = Join-Path (Get-Location).Path '$zip'
\$env:PATH = '$ARM64_PATH;' + \$env:PATH
\$env:PUB_CACHE = '$ARM64_DIR\\pub-cache'
\$env:HOME = \$env:USERPROFILE
New-Item -ItemType Directory -Force '$ARM64_DIR' | Out-Null
Set-Location '$ARM64_DIR'
curl.exe -fsS -o app.tar 'http://$SERVE_ADDR:$port/$nonce/app.tar'
if (\$LASTEXITCODE) { "fetching app.tar failed (\$LASTEXITCODE)"; exit 1 }
if ((Get-FileHash app.tar).Hash -ne '$sum') { 'app.tar does not match its sha256'; exit 1 }
tar -xf app.tar
if (\$LASTEXITCODE) { "tar failed (\$LASTEXITCODE)"; exit 1 }
Set-Location app
flutter build windows --release $APP_FLAGS 2>&1 | Out-String -Stream
if (\$LASTEXITCODE) { "flutter exited \$LASTEXITCODE"; exit 2 }
Compress-Archive -Force -Path 'build\\windows\\arm64\\runner\\Release\\*' -DestinationPath \$job
(Get-FileHash \$job).Hash
EOF
)
	id=$("${HUB_CLI[@]}" --json job run --node "$ARM64_NODE" --even-if-present --timeout "$ARM64_TIMEOUT" \
		--artifact "$zip" -- powershell -NoProfile -NonInteractive -EncodedCommand \
		"$(printf '%s' "$ps" | iconv -f utf-8 -t utf-16le | base64 -w0)" |
		sed -n 's/.*"id":"\([^"]*\)".*/\1/p' | head -n1) || true
	if [ -z "$id" ]; then
		kill "$srv"
		die "the arm64 job did not start on node $ARM64_NODE"
	fi
	echo "arm64 build: job $id on node $ARM64_NODE"
	for i in $(seq $((ARM64_TIMEOUT / 10 + 6))); do
		state=$("${HUB_CLI[@]}" --json job show "$id" | sed -n 's/.*"state":"\([^"]*\)".*/\1/p' | head -n1)
		case $state in succeeded | failed | interrupted | lost | cancelled | abandoned) break ;; esac
		sleep 10
	done
	kill "$srv"
	"${HUB_CLI[@]}" job log "$id" | tail -n 20
	case $state in
	succeeded) ;;
	failed | interrupted | lost | cancelled | abandoned) die "the arm64 job $id ended $state" ;;
	*)
		"${HUB_CLI[@]}" job cancel "$id" || true
		die "the arm64 job $id was still $state after ${ARM64_TIMEOUT}s; cancelled it"
		;;
	esac
	want=$("${HUB_CLI[@]}" job log "$id" | tr -d '\r' | grep -E '^[0-9A-F]{64}$' | tail -n1 | tr 'A-F' 'a-f')
	[ -n "$want" ] || die "the arm64 job $id printed no sha256 for $zip"
	"${HUB_CLI[@]}" job artifact "$id" "$zip" --out "$APP_OUT/$zip.part"
	got=$(sha256sum <"$APP_OUT/$zip.part" | cut -d' ' -f1)
	[ "$got" = "$want" ] || die "fetched $zip has sha256 $got, the node built $want"
	mv -f "$APP_OUT/$zip.part" "$APP_OUT/$zip"
	echo "fetched $APP_OUT/$zip ($want)"
}

collect() {
	local f rel_dir=$ROOT/zig-out/release
	mkdir -p "$DIST"
	install -m 755 "$rel_dir/aarch64-linux-musl/protectorate" "$DIST/protectorate-linux-arm64"
	install -m 755 "$rel_dir/aarch64-linux-musl/protectorate-node" "$DIST/protectorate-node-linux-arm64"
	install -m 755 "$rel_dir/x86_64-linux-musl/protectorate" "$DIST/protectorate-linux-amd64"
	install -m 755 "$rel_dir/x86_64-linux-musl/protectorate-node" "$DIST/protectorate-node-linux-amd64"
	install -m 755 "$rel_dir/aarch64-macos/protectorate" "$DIST/protectorate-macos-arm64"
	install -m 755 "$rel_dir/aarch64-macos/protectorate-node" "$DIST/protectorate-node-macos-arm64"
	install -m 755 "$rel_dir/x86_64-windows-gnu/protectorate.exe" "$DIST/protectorate-windows-amd64.exe"
	install -m 755 "$rel_dir/x86_64-windows-gnu/protectorate-node.exe" "$DIST/protectorate-node-windows-amd64.exe"
	install -m 755 "$rel_dir/aarch64-windows-gnu/protectorate.exe" "$DIST/protectorate-windows-arm64.exe"
	install -m 755 "$rel_dir/aarch64-windows-gnu/protectorate-node.exe" "$DIST/protectorate-node-windows-arm64.exe"

	for f in protectorate-android.apk "$(desktop_zip amd64)" "$(desktop_zip arm64)"; do
		if [ -f "$APP_OUT/$f" ]; then install -m 644 "$APP_OUT/$f" "$DIST/$f"; fi
	done
	for f in install.sh install-node.sh install-node.ps1; do install -m 644 "scripts/$f" "$DIST/$f"; done
	printf '%s\n' "$VERSION" >"$DIST/VERSION"
	local present=()
	for f in "${ASSETS[@]}"; do
		if [ -f "$DIST/$f" ]; then present+=("$f"); else echo "missing $f"; fi
	done
	(cd "$DIST" && sha256sum -- "${present[@]}") >"$DIST/SHA256SUMS.tmp"
	mv -f "$DIST/SHA256SUMS.tmp" "$DIST/SHA256SUMS"
	write_notes
}

write_notes() {
	local commit sum f
	commit=$(git rev-parse HEAD)
	{
		echo "Protectorate $VERSION"
		echo
		echo "Built locally from commit $commit and checked by SHA256SUMS."
		if [ -n "$(git status --porcelain)" ]; then
			echo
			echo "WARNING: built from a dirty tree; the scripts below may differ at that commit."
		fi
		echo
		echo "Installers, fetched at the commit (compare the sha256 before running):"
		echo
		for f in install.sh install-node.sh install-node.ps1; do
			sum=$(sha256sum <"scripts/$f" | cut -d' ' -f1)
			echo "    https://raw.githubusercontent.com/$REPO/$commit/scripts/$f"
			echo "    sha256 $sum"
		done
		echo
		echo "Hub:            curl -q -fsSLO <install.sh URL> && sha256sum install.sh && bash install.sh --version $VERSION"
		echo "Linux node:     curl -q -fsSLO <install-node.sh URL> && sha256sum install-node.sh && sudo bash install-node.sh JOIN_TOKEN --version $VERSION"
		echo "Windows node:   curl.exe -q -fsSLO <install-node.ps1 URL>; Get-FileHash .\\install-node.ps1; .\\install-node.ps1 -JoinToken JOIN_TOKEN -Version $VERSION"
	} >"$NOTES"
}

SEMVER='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-((0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)\.)*(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))?$'

release_version() {
	if [[ "$VERSION" == *-dirty ]]; then die "VERSION $VERSION has uncommitted changes; commit first"; fi
	if [[ "$VERSION" =~ -[0-9]+-g[0-9a-f]+$ ]]; then die "VERSION $VERSION is commits past a tag; tag HEAD"; fi
	[[ "$VERSION" =~ $SEMVER ]] || die "VERSION $VERSION is not a semver tag (vX.Y.Z or vX.Y.Z-PRE)"
	git tag --points-at HEAD | grep -qxF "$VERSION" || die "HEAD is not tagged $VERSION"
	[ -z "$(git status --porcelain)" ] || die "the tree is dirty; commit first (a release is built from a commit)"
}

assets() {
	if [ "${1:-}" = --snapshot ]; then
		say "snapshot $VERSION: not a release, never publish it"
	else
		release_version
	fi
	mkdir -p "$DIST" "$LOGS"
	local t0 desk_pid='' apk_pid='' arm_pid='' f
	t0=$(now_ms)
	say "release $VERSION into $DIST (logs in $LOGS)"
	[ -n "$ZIG" ] && [ -x "$ZIG" ] || die "zig binary not found (set ZIG)"
	for f in flutter git sha256sum; do command -v "$f" >/dev/null || die "$f is required"; done
	if [ -n "$WIN_HOST" ]; then
		win "ver" >/dev/null || die "cannot reach $WIN_HOST over ssh (unset PT_RELEASE_WIN_HOST to skip desktop-amd64)"
	fi
	if [ -n "$APK_HOST$WIN_HOST$ARM64_NODE" ]; then
		[ -n "$APP_FLAGS" ] || die "APP_VERSION_FLAGS is not set; run this through make"
	fi
	if [ -n "$APK_HOST" ]; then
		apk_ssh true || die "cannot reach $APK_HOST over ssh (unset PT_RELEASE_APK_HOST to skip the APK)"
	fi
	if [ -n "$ARM64_NODE" ]; then
		[ -n "$SERVE_ADDR" ] || die "PT_RELEASE_SERVE_ADDR is not set: the address node $ARM64_NODE fetches the app sources from"
		for f in python3 iconv "${HUB_CLI[0]}"; do command -v "$f" >/dev/null || die "$f is required for the arm64 app"; done
	fi

	step app-src app_src
	if [ -n "$APK_HOST" ]; then
		step apk build_apk &
		apk_pid=$!
	else
		echo "   apk            skipped: PT_RELEASE_APK_HOST is not set"
	fi
	if [ -n "$WIN_HOST" ]; then
		step desktop-amd64 build_desktop_amd64 &
		desk_pid=$!
	else
		echo "   desktop-amd64  skipped: PT_RELEASE_WIN_HOST is not set"
	fi
	if [ -n "$ARM64_NODE" ]; then
		step desktop-arm64 build_desktop_arm64 &
		arm_pid=$!
	else
		echo "   desktop-arm64  skipped: PT_RELEASE_ARM64_NODE is not set"
	fi

	step web build_web
	step zig build_zig
	[ -z "$desk_pid" ] || wait "$desk_pid" || die "the desktop-amd64 step failed"
	[ -z "$apk_pid" ] || wait "$apk_pid" || die "the apk step failed"
	[ -z "$arm_pid" ] || wait "$arm_pid" || die "the desktop-arm64 step failed"
	step collect collect
	local total=$(($(now_ms) - t0))

	say "assets"
	for f in "${ASSETS[@]}" SHA256SUMS; do
		if [ -f "$DIST/$f" ]; then printf '   %-38s %10s bytes\n' "$f" "$(wc -c <"$DIST/$f")"; fi
	done
	say "wall time per step"
	for f in app-src apk desktop-amd64 desktop-arm64 web zig collect; do
		if [ -f "$LOGS/$f.ms" ]; then printf '   %-14s %8s s\n' "$f" "$(secs "$(cat "$LOGS/$f.ms")")"; fi
	done
	printf '   %-14s %8s s\n' total "$(secs $total)"
	say "release notes: $NOTES"
}

publish() {
	local problems=() tag f
	release_version
	[ -f "$DIST/SHA256SUMS" ] || die "no $DIST/SHA256SUMS; run make release-assets"
	(cd "$DIST" && sha256sum --quiet --strict -c SHA256SUMS) || die "$DIST does not match its SHA256SUMS"
	[ "$(cat "$DIST/VERSION")" = "$VERSION" ] || die "$DIST/VERSION is not $VERSION"
	command -v gh >/dev/null || die "gh is required"
	tag=$VERSION
	if [ -z "$(git ls-remote --tags origin "refs/tags/$tag" 2>/dev/null)" ]; then
		problems+=("tag $tag is not on origin; push it first (gh --verify-tag needs it)")
	fi
	local files=()
	while read -r _ f; do files+=("$DIST/$f"); done <"$DIST/SHA256SUMS"
	files+=("$DIST/SHA256SUMS")
	[ -f "$NOTES" ] || write_notes
	local cmd=(gh release create "$tag" --repo "$REPO" --verify-tag --title "Protectorate $tag" --notes-file "$NOTES")
	if [[ "$tag" == *-* ]]; then cmd+=(--prerelease); fi
	cmd+=("${files[@]}")
	if [ "${PUBLISH:-0}" != 1 ]; then
		say "dry run (PUBLISH=1 uploads); would run:"
		printf ' %q' "${cmd[@]:0:11}"
		if [[ "$tag" == *-* ]]; then printf ' --prerelease'; fi
		echo
		for f in "${files[@]}"; do printf '     %s\n' "${f#"$ROOT/"}"; done
		for f in "${problems[@]}"; do echo "   would refuse: $f"; done
		return 0
	fi
	[ ${#problems[@]} = 0 ] || die "${problems[0]}"
	"${cmd[@]}"
}

case "${1:-}" in
assets) assets "${2:-}" ;;
publish) publish ;;
*) echo "usage: $0 assets [--snapshot] | publish" >&2; exit 2 ;;
esac
