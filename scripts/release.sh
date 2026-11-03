#!/usr/bin/env bash
# Release assets, built locally (SPEC 6, D36: no CI build, no signing key).
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
# VERSION comes from the Makefile (git describe), the one source of the version; run this
# through make. Machine-specific settings, from the environment or the gitignored .env.e2e at
# the repo root:
#   PT_RELEASE_WIN_HOST  ssh destination of a Windows machine (cmd.exe shell, rustup, MSVC) that
#                        builds the Windows node assets; unset skips them
#   PT_RELEASE_WIN_DIR   the build's folder under %USERPROFILE% there (default pt-release)
#   ZIG                  the zig binary for the glibc 2.31 amd64 build (default: zig on PATH)
#   PT_RELEASE_REPO      OWNER/NAME of the GitHub repo (default: install.sh's REPO)
#   PT_RELEASE_WIN_FLUTTER        Flutter's bin folder on the Windows host (C:\dev\flutter\bin)
#   PT_RELEASE_APK_HOST  ssh destination of a Linux amd64 machine with Flutter and the Android
#                        SDK that builds the APK; unset skips it
#   PT_RELEASE_APK_SSH_KEY        an ssh identity for it (optional)
#   PT_RELEASE_APK_PATH, PT_RELEASE_APK_ANDROID_HOME   Flutter's bin folder and the SDK there,
#                        relative to its home (flutter/bin, Android/Sdk)
#   PT_RELEASE_APK_SIGNING        the signing properties here: storeFile, keyAlias,
#                        storePassword, keyPassword (~/.config/protectorate/android-release.properties)
#   PT_RELEASE_APKSIGNER the apksigner binary here (default: apksigner on PATH)
#   PT_RELEASE_ARM64_NODE         the fleet node (Windows arm64, Flutter, Visual Studio) that
#                        builds the arm64 desktop app as a hub job; unset skips it
#   PT_RELEASE_ARM64_DIR, PT_RELEASE_ARM64_PATH   its build folder (C:\dev\pt-release) and the
#                        folders put first on PATH there (Flutter's bin and Git's cmd under C:\)
#   PT_RELEASE_ARM64_TIMEOUT      the job's timeout in seconds (3600)
#   PT_RELEASE_HUB_CLI   the hub CLI command that runs the job (default: protectorate)
#   PT_RELEASE_SERVE_ADDR         an address of this machine the arm64 node can reach; the
#                        job fetches the app sources from a one-time path there
#
# Steps of `assets`, overlapped where they use different machines or toolchains:
#   go            scripts/net-build.sh: headscale and member for every target (skipped when
#                 go/bin is newer than every Go source)
#   web           the Flutter web build (make web), once for both hub assets
#   windows       the Windows host (ssh) builds protectorate-node for x64 and arm64 in a scratch
#                 folder, embedding member-windows-*.exe sent over gzipped; the .exe files come
#                 back as base64 over ssh (scp pulls truncate) and are checked by sha256 against
#                 the remote copy
#   linux-arm64   cargo, native
#   linux-amd64   cargo zigbuild for glibc 2.31
# The hub assets embed the web build and that arch's headscale and member, and every node asset
# embeds its target's member, so the cargo steps and windows wait for go (and the hub for web).
# headscale and member are not release assets of their own.
#   app-src       the app's sources, as on disk, for the three app builds:
#   apk           protectorate-android.apk: built on the APK host over ssh, signed here
#   desktop-amd64 protectorate-desktop-windows-amd64.zip: built on the Windows host after its
#                 node build
#   desktop-arm64 protectorate-desktop-windows-arm64.zip: built on an arm64 fleet node as a
#                 hub job (Flutter does not cross-build Windows arm64)
# Each zip holds Flutter's runner\Release folder at its root. The app installs these itself
# from its hub (SPEC 6a). The Linux desktop app is not built yet (REVISIT).
# Every step reuses its caches (cargo target dirs, the Go build cache, the Windows target dir),
# so a rebuild with no change is quick.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
if [ -f .env.e2e ]; then set -a; . ./.env.e2e; set +a; fi
REPO=${PT_RELEASE_REPO:-$(sed -n 's/^REPO=//p' scripts/install.sh)}
[ -n "${VERSION:-}" ] || { echo "release: VERSION is not set; run make release-assets or make release-publish" >&2; exit 2; }
DIST=$ROOT/dist/$VERSION
LOGS=$ROOT/dist/$VERSION.logs
NOTES=$ROOT/dist/$VERSION.notes.md
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
ZIG=${ZIG:-$(command -v zig || true)}
WIN_HOST=${PT_RELEASE_WIN_HOST:-}
# Under %USERPROFILE% on the Windows host; the build's only footprint there.
WIN_DIR=${PT_RELEASE_WIN_DIR:-pt-release}
WEB_DIR=$ROOT/app/build/web
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
APP_OUT=$TARGET/release-app/$VERSION
APP_TAR=$APP_OUT/app-src.tar
# The Makefile's --build-name, --build-number and PROTECTORATE_VERSION, so the app builds carry the tag.
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
	protectorate-linux-arm64 protectorate-linux-amd64
	protectorate-node-linux-arm64 protectorate-node-linux-amd64
	protectorate-node-windows-amd64.exe protectorate-node-windows-arm64.exe
	protectorate-android.apk protectorate-desktop-windows-amd64.zip protectorate-desktop-windows-arm64.zip
	install.sh install-node.sh install-node.ps1
	VERSION
)
GO_OUT=(headscale-linux-arm64 headscale-linux-amd64 member-linux-arm64 member-linux-amd64
	member-windows-amd64.exe member-windows-arm64.exe member-android-arm64)

say() { printf '== %s\n' "$*"; }
die() { printf 'release: %s\n' "$*" >&2; exit 1; }
now_ms() { echo $(($(date +%s%N) / 1000000)); }

# step NAME CMD...: runs CMD with its output in $LOGS/NAME.log and its wall time in NAME.ms.
# CMD runs as a job of its own: under `if` or `||`, bash would ignore set -e inside it.
step() {
	local name=$1 t0 pid rc=0
	shift
	t0=$(now_ms)
	"$@" >"$LOGS/$name.log" 2>&1 &
	pid=$!
	wait "$pid" || rc=$?
	if [ "$rc" = 0 ]; then
		echo $(($(now_ms) - t0)) >"$LOGS/$name.ms"
		printf '   %-12s done in %s s\n' "$name" "$(secs "$(cat "$LOGS/$name.ms")")"
	else
		echo failed >"$LOGS/$name.ms"
		printf '   %-12s FAILED (exit %s); last lines of %s:\n' "$name" "$rc" "$LOGS/$name.log" >&2
		tail -n 30 "$LOGS/$name.log" >&2
		return "$rc"
	fi
}

secs() { [ "$1" = failed ] && echo failed || printf '%d.%01d' $(($1 / 1000)) $((($1 % 1000) / 100)); }

cargo_linux() { # TRIPLE [zig]
	local sub=build
	if [ "${2:-}" = zig ]; then sub=zigbuild; fi
	PROTECTORATE_BUILD_VERSION=$VERSION PROTECTORATE_WEB_DIR=$WEB_DIR PROTECTORATE_NET_DIR=$ROOT/go/bin \
		cargo "$sub" --locked --release --workspace --bins --target "$1"
}

build_linux_arm64() { cargo_linux aarch64-unknown-linux-gnu; }

build_linux_amd64() {
	[ -n "$ZIG" ] && [ -x "$ZIG" ] || die "no zig: put it on PATH or set ZIG to the binary"
	PATH=$(dirname "$ZIG"):$PATH cargo_linux x86_64-unknown-linux-gnu.2.31 zig
}

build_go() {
	local oldest="" f
	for f in "${GO_OUT[@]}"; do
		[ -f "go/bin/$f" ] || { oldest=""; break; }
		if [ -z "$oldest" ] || [ "go/bin/$f" -ot "$oldest" ]; then oldest=go/bin/$f; fi
	done
	if [ -n "$oldest" ] && [ -z "$(find go scripts/net-build.sh -path go/bin -prune -o -type f -newer "$oldest" -print -quit)" ]; then
		echo "go/bin is newer than every Go source; not rebuilding"
		return 0
	fi
	scripts/net-build.sh
}

build_web() {
	make --no-print-directory web VERSION="$VERSION"
	[ -f "$WEB_DIR/index.html" ] || die "no $WEB_DIR/index.html after the web build"
}

# The build scripts only warn when headscale or member is missing; a release must not.
check_embedded() {
	local arch f
	for arch in arm64 amd64; do
		for f in headscale member; do
			[ -f "go/bin/$f-linux-$arch" ] || die "no go/bin/$f-linux-$arch to embed"
		done
	done
	if "$TARGET/aarch64-unknown-linux-gnu/release/protectorate" version | grep -q missing; then
		die "the arm64 hub does not embed headscale and member"
	fi
	"$TARGET/aarch64-unknown-linux-gnu/release/protectorate-node" --version | grep -q '^member: embedded' ||
		die "the arm64 node does not embed member"
}

# win CMD: runs CMD through cmd.exe on the Windows host.
win() { ssh -o BatchMode=yes "$WIN_HOST" "$@"; }

build_windows() {
	local tar=$TARGET/release-windows-src.tar
	mkdir -p "$TARGET"
	# The workspace as it is on disk (dirty files too, matching the Linux builds); tar keeps
	# mtimes, so cargo on the far side rebuilds only what changed.
	git ls-files -z --cached --others --exclude-standard -- Cargo.toml Cargo.lock crates |
		tar --null --ignore-failed-read -cf "$tar" -T -
	win "if not exist %USERPROFILE%\\$WIN_DIR\\src mkdir %USERPROFILE%\\$WIN_DIR\\src"
	win "if not exist %USERPROFILE%\\$WIN_DIR\\net mkdir %USERPROFILE%\\$WIN_DIR\\net"
	scp -q -o BatchMode=yes "$tar" "$WIN_HOST:$WIN_DIR/src.tar"
	# The node embeds member; Windows has no gzip, so it gets the gzip, which its build script
	# inflates for the SHA-256.
	local arch m
	for arch in amd64 arm64; do
		m=member-windows-$arch.exe
		[ -f "go/bin/$m" ] || die "no go/bin/$m to embed"
		gzip -n -9 -c "go/bin/$m" >"$TARGET/$m.gz"
		scp -q -o BatchMode=yes "$TARGET/$m.gz" "$WIN_HOST:$WIN_DIR/net/$m.gz"
	done
	# The rustup toolchain first on PATH: the system Rust lacks the arm64 std (REVISIT).
	win "cd /d %USERPROFILE%\\$WIN_DIR && tar -xf src.tar -C src && cd src" \
		"&& set \"PATH=%USERPROFILE%\\.cargo\\bin;%PATH%\"" \
		"&& set \"PROTECTORATE_BUILD_VERSION=$VERSION\"" \
		"&& set \"PROTECTORATE_NET_DIR=%USERPROFILE%\\$WIN_DIR\\net\"" \
		"&& set \"CARGO_TARGET_DIR=%USERPROFILE%\\$WIN_DIR\\target\"" \
		"&& cargo +stable build --locked --release -j $CARGO_BUILD_JOBS -p protectorate-node" \
		"--target x86_64-pc-windows-msvc --target aarch64-pc-windows-msvc"
	local triple arch
	for arch in amd64 arm64; do
		triple=$([ $arch = amd64 ] && echo x86_64 || echo aarch64)-pc-windows-msvc
		win_pull "%USERPROFILE%\\$WIN_DIR\\target\\$triple\\release\\protectorate-node.exe" \
			"$TARGET/release-windows/protectorate-node-windows-$arch.exe"
	done
}

# win_pull REMOTE OUT: copies the file REMOTE on the Windows host to OUT, as base64 over ssh
# (scp pulls truncate), checked by sha256 against the remote copy. A current OUT is kept.
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

# The app's sources as they are on disk, for the three app builds on other machines.
app_src() {
	mkdir -p "$APP_OUT"
	git ls-files -z --cached --others --exclude-standard -- app |
		tar --null --ignore-failed-read -cf "$APP_TAR" -T -
}

apk_ssh() { ssh -o BatchMode=yes ${APK_KEY:+-i "$APK_KEY" -o IdentitiesOnly=yes} "$APK_HOST" "$@"; }

# On the APK host, then signed here: the key never leaves this machine.
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

# Flutter does not cross-build Windows arm64, so it builds natively on an arm64 fleet node, as a
# hub job. The job fetches the app's sources from this machine, by a one-time path, and builds in
# ARM64_DIR so Flutter's caches last; the zip goes in the job's own directory, the only place a
# node serves an artifact from.
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
	local f t
	mkdir -p "$DIST"
	for t in aarch64-unknown-linux-gnu:arm64 x86_64-unknown-linux-gnu:amd64; do
		install -m 755 "$TARGET/${t%%:*}/release/protectorate" "$DIST/protectorate-linux-${t##*:}"
		install -m 755 "$TARGET/${t%%:*}/release/protectorate-node" "$DIST/protectorate-node-linux-${t##*:}"
	done
	if [ -n "$WIN_HOST" ]; then
		install -m 755 "$TARGET"/release-windows/protectorate-node-windows-{amd64,arm64}.exe "$DIST/"
	fi
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
		echo "Built locally from commit $commit and checked by SHA256SUMS (no signing key yet)."
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
		echo "Linux node:     curl -q -fsSLO <install-node.sh URL> && sha256sum install-node.sh && sudo bash install-node.sh SERVER_URL --version $VERSION"
		echo "Windows node:   curl.exe -q -fsSLO <install-node.ps1 URL>; Get-FileHash .\\install-node.ps1; .\\install-node.ps1 -ServerUrl SERVER_URL -Version $VERSION"
	} >"$NOTES"
}

# A semver tag as pt-update's Version::parse takes it: vMAJOR.MINOR.PATCH[-PRE], no build metadata.
SEMVER='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-((0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)\.)*(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))?$'

# A release is exactly a tag: VERSION a semver tag (not git describe's -N-gHASH or -dirty), that
# tag on HEAD, and nothing uncommitted.
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
	local t0 win_pid='' go_pid='' web_pid='' apk_pid='' arm_pid='' f
	t0=$(now_ms)
	say "release $VERSION into $DIST (logs in $LOGS)"
	for f in cargo cargo-zigbuild go flutter gzip git sha256sum; do command -v "$f" >/dev/null || die "$f is required"; done
	if [ -n "$WIN_HOST" ]; then
		win "ver" >/dev/null || die "cannot reach $WIN_HOST over ssh (unset PT_RELEASE_WIN_HOST to skip the Windows assets)"
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
		echo "   apk          skipped: PT_RELEASE_APK_HOST is not set, so no Android APK"
	fi
	if [ -n "$ARM64_NODE" ]; then
		step desktop-arm64 build_desktop_arm64 &
		arm_pid=$!
	else
		echo "   desktop-arm64 skipped: PT_RELEASE_ARM64_NODE is not set, so no Windows arm64 app"
	fi
	step go build_go &
	go_pid=$!
	step web build_web &
	web_pid=$!
	wait "$go_pid" || die "the go step failed"
	if [ -n "$WIN_HOST" ]; then
		{ step windows build_windows && step desktop-amd64 build_desktop_amd64; } &
		win_pid=$!
	else
		echo "   windows      skipped: PT_RELEASE_WIN_HOST is not set, so no Windows node assets"
	fi
	wait "$web_pid" || die "the web step failed"
	step linux-arm64 build_linux_arm64
	check_embedded
	step linux-amd64 build_linux_amd64
	[ -z "$win_pid" ] || wait "$win_pid" || die "the windows or desktop-amd64 step failed"
	[ -z "$apk_pid" ] || wait "$apk_pid" || die "the apk step failed"
	[ -z "$arm_pid" ] || wait "$arm_pid" || die "the desktop-arm64 step failed"
	step collect collect
	local total=$(($(now_ms) - t0))

	say "assets"
	for f in "${ASSETS[@]}" SHA256SUMS; do
		if [ -f "$DIST/$f" ]; then printf '   %-38s %10s bytes\n' "$f" "$(wc -c <"$DIST/$f")"; fi
	done
	say "wall time per step (go and web overlap each other; windows overlaps everything after go)"
	for f in app-src apk desktop-arm64 windows desktop-amd64 go web linux-arm64 linux-amd64 collect; do
		if [ -f "$LOGS/$f.ms" ]; then printf '   %-12s %8s s\n' "$f" "$(secs "$(cat "$LOGS/$f.ms")")"; fi
	done
	printf '   %-12s %8s s\n' total "$(secs $total)"
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
