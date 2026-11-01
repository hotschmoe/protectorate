#!/usr/bin/env bash
# Release assets, built locally (SPEC 6, D36: no CI build, no signing key).
#
#   scripts/release.sh assets    (make release-assets) builds every asset into dist/<version>/,
#                                writes VERSION and SHA256SUMS, and prints the wall time per step.
#   scripts/release.sh publish   (make release-publish) uploads dist/<version>/ with
#                                `gh release create`; a dry run that only prints the command
#                                unless PUBLISH=1. Refuses a dirty tree or an untagged HEAD.
#
# Steps of `assets`, overlapped where they use different machines or toolchains:
#   go            scripts/net-build.sh: headscale and member for every target (skipped when
#                 go/bin is newer than every Go source)
#   web           the Flutter web build (make web), once for both hub assets
#   windows       officewin (ssh) builds protectorate-node for x64 and arm64 in a scratch folder,
#                 embedding member-windows-*.exe sent over gzipped; the .exe files come back as
#                 base64 over ssh (scp pulls truncate) and are checked by sha256 against the
#                 remote copy
#   linux-arm64   cargo, native
#   linux-amd64   cargo zigbuild for glibc 2.31
# The hub assets embed the web build and that arch's headscale and member, and every node asset
# embeds its target's member, so the cargo steps and windows wait for go (and the hub for web).
# headscale and member are not release assets of their own.
#   app           the Android APK and the Flutter desktop builds: later steps (REVISIT)
# Every step reuses its caches (cargo target dirs, the Go build cache, officewin's target dir),
# so a rebuild with no change is quick.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
REPO=${REPO:-hotschmoe/protectorate}
VERSION=${VERSION:-$(git describe --tags --match 'v[0-9]*' --always --dirty)}
DIST=$ROOT/dist/$VERSION
LOGS=$ROOT/dist/$VERSION.logs
NOTES=$ROOT/dist/$VERSION.notes.md
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
ZIG_DIR=${ZIG_DIR:-/home/hotschmoe/tools/zig-aarch64-linux-0.16.0}
WIN_HOST=${WIN_HOST:-officewin}
# Under %USERPROFILE% on the Windows host; the build's only footprint there.
WIN_DIR=${WIN_DIR:-pt-release}
WINDOWS=${WINDOWS:-1}
WEB_DIR=$ROOT/app/build/web
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}

ASSETS=(
	protectorate-linux-arm64 protectorate-linux-amd64
	protectorate-node-linux-arm64 protectorate-node-linux-amd64
	protectorate-node-windows-amd64.exe protectorate-node-windows-arm64.exe
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

cargo_linux() { # TRIPLE [zigbuild]
	local sub=build
	if [ "${2:-}" = zig ]; then sub=zigbuild; fi
	PROTECTORATE_BUILD_VERSION=$VERSION PROTECTORATE_WEB_DIR=$WEB_DIR PROTECTORATE_NET_DIR=$ROOT/go/bin \
		cargo $sub --locked --release --workspace --bins --target "$1"
}

build_linux_arm64() { cargo_linux aarch64-unknown-linux-gnu; }

build_linux_amd64() {
	[ -x "$ZIG_DIR/zig" ] || die "no zig at $ZIG_DIR (set ZIG_DIR)"
	PATH=$ZIG_DIR:$PATH cargo_linux x86_64-unknown-linux-gnu.2.31 zig
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
	(cd app && flutter build web --release --no-web-resources-cdn)
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
	# The node embeds member; officewin has no gzip, so it gets the gzip, which its build script
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
	local triple arch remote want got out
	for arch in amd64 arm64; do
		triple=$([ $arch = amd64 ] && echo x86_64 || echo aarch64)-pc-windows-msvc
		remote="%USERPROFILE%\\$WIN_DIR\\target\\$triple\\release\\protectorate-node.exe"
		out=$TARGET/release-windows/protectorate-node-windows-$arch.exe
		mkdir -p "$(dirname "$out")"
		want=$(win "powershell -NoProfile -NonInteractive -Command (Get-FileHash -Algorithm SHA256 '$remote').Hash" |
			tr -d '\r\n' | tr 'A-F' 'a-f')
		[[ "$want" =~ ^[0-9a-f]{64}$ ]] || die "no sha256 for $remote on $WIN_HOST"
		if [ -f "$out" ] && [ "$(sha256sum <"$out" | cut -d' ' -f1)" = "$want" ]; then
			echo "$out is current ($want)"
			continue
		fi
		win "powershell -NoProfile -NonInteractive -Command [Console]::Out.Write([Convert]::ToBase64String([IO.File]::ReadAllBytes('$remote')))" |
			tr -d '\r\n' | base64 -d >"$out.part"
		got=$(sha256sum <"$out.part" | cut -d' ' -f1)
		[ "$got" = "$want" ] || die "pulled $out.part has sha256 $got, $WIN_HOST has $want"
		mv -f "$out.part" "$out"
		echo "pulled $out ($want)"
	done
}

skip_app() {
	echo "skipped: the Android APK (protectorate-android.apk) and the Flutter desktop builds"
	echo "(protectorate-desktop-*) are later steps; see REVISIT. APP=1 would fail here."
	[ "${APP:-0}" = 0 ]
}

collect() {
	local f t
	mkdir -p "$DIST"
	for t in aarch64-unknown-linux-gnu:arm64 x86_64-unknown-linux-gnu:amd64; do
		install -m 755 "$TARGET/${t%%:*}/release/protectorate" "$DIST/protectorate-linux-${t##*:}"
		install -m 755 "$TARGET/${t%%:*}/release/protectorate-node" "$DIST/protectorate-node-linux-${t##*:}"
	done
	if [ "$WINDOWS" = 1 ]; then
		install -m 755 "$TARGET"/release-windows/protectorate-node-windows-{amd64,arm64}.exe "$DIST/"
	fi
	for f in install.sh install-node.sh install-node.ps1; do install -m 644 "scripts/$f" "$DIST/$f"; done
	printf '%s\n' "$VERSION" >"$DIST/VERSION"
	local present=()
	for f in "${ASSETS[@]}"; do
		if [ -f "$DIST/$f" ]; then present+=("$f"); else echo "missing $f (WINDOWS=$WINDOWS)"; fi
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
		echo "Hub:            curl -fsSLO <install.sh URL> && sha256sum install.sh && bash install.sh --version $VERSION"
		echo "Linux node:     curl -fsSLO <install-node.sh URL> && sha256sum install-node.sh && sudo bash install-node.sh SERVER_URL --version $VERSION"
		echo "Windows node:   curl.exe -fsSLO <install-node.ps1 URL>; Get-FileHash .\\install-node.ps1; .\\install-node.ps1 -ServerUrl SERVER_URL -Version $VERSION"
	} >"$NOTES"
}

assets() {
	mkdir -p "$DIST" "$LOGS"
	local t0 win_pid='' go_pid='' web_pid='' f
	t0=$(now_ms)
	say "release $VERSION into $DIST (logs in $LOGS)"
	for f in cargo cargo-zigbuild go flutter gzip git sha256sum; do command -v "$f" >/dev/null || die "$f is required"; done
	if [ "$WINDOWS" = 1 ]; then
		win "ver" >/dev/null || die "cannot reach $WIN_HOST over ssh (WINDOWS=0 skips the Windows assets)"
	fi
	step go build_go &
	go_pid=$!
	step web build_web &
	web_pid=$!
	wait "$go_pid" || die "the go step failed"
	if [ "$WINDOWS" = 1 ]; then
		step windows build_windows &
		win_pid=$!
	else
		echo "   windows      skipped (WINDOWS=0)"
	fi
	wait "$web_pid" || die "the web step failed"
	step linux-arm64 build_linux_arm64
	check_embedded
	step linux-amd64 build_linux_amd64
	[ -z "$win_pid" ] || wait "$win_pid" || die "the windows step failed"
	step app skip_app
	step collect collect
	local total=$(($(now_ms) - t0))

	say "assets"
	for f in "${ASSETS[@]}" SHA256SUMS; do
		if [ -f "$DIST/$f" ]; then printf '   %-38s %10s bytes\n' "$f" "$(wc -c <"$DIST/$f")"; fi
	done
	say "wall time per step (go and web overlap each other; windows overlaps everything after go)"
	for f in windows go web linux-arm64 linux-amd64 app collect; do
		if [ -f "$LOGS/$f.ms" ]; then printf '   %-12s %8s s\n' "$f" "$(secs "$(cat "$LOGS/$f.ms")")"; fi
	done
	printf '   %-12s %8s s\n' total "$(secs $total)"
	say "release notes: $NOTES"
}

publish() {
	local problems=() tag f
	[ -z "$(git status --porcelain)" ] || die "the tree is dirty; commit first (a release is built from a commit)"
	[ -f "$DIST/SHA256SUMS" ] || die "no $DIST/SHA256SUMS; run make release-assets"
	(cd "$DIST" && sha256sum --quiet --strict -c SHA256SUMS) || die "$DIST does not match its SHA256SUMS"
	[ "$(cat "$DIST/VERSION")" = "$VERSION" ] || die "$DIST/VERSION is not $VERSION"
	command -v gh >/dev/null || die "gh is required"
	tag=$(git describe --tags --exact-match --match 'v[0-9]*' HEAD 2>/dev/null || true)
	if [ -z "$tag" ]; then
		problems+=("HEAD has no v* tag; tag it (vX.Y.Z stable, vX.Y.Z-PRE dev) and push the tag")
		tag=$VERSION
	elif [ -z "$(git ls-remote --tags origin "refs/tags/$tag" 2>/dev/null)" ]; then
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
assets) assets ;;
publish) publish ;;
*) echo "usage: $0 assets | publish" >&2; exit 2 ;;
esac
