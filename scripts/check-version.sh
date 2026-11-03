#!/bin/sh
# Fails when a build path could take its version from anywhere but the git tag, through the
# Makefile's VERSION (make version). Run by make check-version, part of make check.
#
#   - Cargo.toml's workspace version stays the placeholder 0.0.0 and every crate inherits it;
#   - app/pubspec.yaml stays the placeholder 0.0.0+0;
#   - every Makefile target that runs `flutter build` passes --build-name, --build-number and
#     --dart-define=PROTECTORATE_VERSION, and a script runs `flutter build` only with the
#     Makefile's APP_VERSION_FLAGS (release.sh, whose app builds run on other machines);
#   - every cargo build, test or clippy in the Makefile and the scripts sets
#     PROTECTORATE_BUILD_VERSION;
#   - the app reads its version only from PROTECTORATE_VERSION, and go/, scripts/ and app/ hold
#     no other version literal;
#   - the default release repo is the same in every installer and in the hub.
set -u
cd "$(dirname "$0")/.."
make=${MAKE:-make}
fail=0
bad() { echo "check-version: $*" >&2; fail=1; }

awk '/^\[workspace\.package\]/ { p = 1; next } /^\[/ { p = 0 } p && /^version *=/ { print }' Cargo.toml |
	grep -qx 'version = "0\.0\.0"' || bad "Cargo.toml: [workspace.package] version must stay the placeholder \"0.0.0\""
for f in crates/*/Cargo.toml; do
	grep -qx 'version\.workspace = true' "$f" || bad "$f: the package version must be version.workspace = true"
	grep -nE '^version *=' "$f" >&2 && bad "$f: no version literal; the version is the git tag"
done

grep -qx 'version: 0\.0\.0+0' app/pubspec.yaml || bad "app/pubspec.yaml: version must stay the placeholder 0.0.0+0"

# Makefile targets whose recipe has a line matching $1.
targets_running() {
	awk -v re="$1" '
		/^[A-Za-z0-9_.-]+:/ { t = $0; sub(/:.*/, "", t); next }
		/^\t/ && $0 ~ re && t != "" { print t }
	' Makefile | sort -u
}

for t in $(targets_running 'FLUTTER\) build|flutter build'); do
	lines=$($make -s -n "$t" 2>&1 | grep 'flutter build') || { bad "make $t runs no flutter build"; continue; }
	echo "$lines" | while IFS= read -r l; do
		for f in --build-name= --build-number= --dart-define=PROTECTORATE_VERSION=; do
			case $l in *"$f"*) ;; *) exit 1 ;; esac
		done
	done || bad "make $t: flutter build lacks --build-name, --build-number or PROTECTORATE_VERSION"
done

for t in $(targets_running 'cargo (build|test|clippy)'); do
	$make -s -n "$t" 2>&1 | grep -E '(^|[ ;&])cargo (\+[^ ]+ )?(build|test|clippy)' |
		grep -v 'PROTECTORATE_BUILD_VERSION=' >&2 && bad "make $t: cargo runs without PROTECTORATE_BUILD_VERSION"
done

for f in scripts/*.sh scripts/*.ps1; do
	[ "$f" = scripts/check-version.sh ] && continue
	grep -n 'flutter build' "$f" | grep -v 'flutter build [a-z]* --release \$APP_FLAGS' >&2 &&
		bad "$f: build the app through make (make web) so the version is applied"
	if grep -q 'flutter build' "$f"; then
		grep -qx 'APP_FLAGS=${APP_VERSION_FLAGS:-}' "$f" || bad "$f: APP_FLAGS must be the Makefile's APP_VERSION_FLAGS"
	fi
	# One logical line per backslash-continued command.
	awk '{ if (sub(/\\$/, "")) { acc = acc $0 " "; next } print acc $0; acc = "" }' "$f" |
		grep -vE '^[[:space:]]*#' | grep -E '(^|[ ;&"(])cargo (\+[^ ]+ )?("?\$sub"?|build|zigbuild|test|clippy)( |$)' |
		grep -v 'PROTECTORATE_BUILD_VERSION=' >&2 && bad "$f: cargo builds without PROTECTORATE_BUILD_VERSION"
done

grep -q "const kVersion = String.fromEnvironment('PROTECTORATE_VERSION', defaultValue: 'dev');" app/lib/hub.dart ||
	bad "app/lib/hub.dart: kVersion must come from --dart-define=PROTECTORATE_VERSION"
[ "$(git grep -c "PROTECTORATE_VERSION" -- app/lib | wc -l)" = 1 ] ||
	bad "app/lib: only hub.dart's kVersion reads PROTECTORATE_VERSION"
# Quoted x.y.z literals. Not ours, so left out: build tool pins (Gradle plugins, go.mod), locked
# packages, and the fake hub's toolchain entries ({'name': 'rustc', 'version': ...}).
git grep -nIE "[\"']v?[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]*)?[\"']" -- go scripts app \
	':!go/go.mod' ':!go/go.sum' ':!app/pubspec.lock' ':!app/android/settings.gradle.kts' |
	grep -v "'name': " | grep -vE "[\"']0\.0\.0[\"']" >&2 &&
	bad "a version literal above; the version is the git tag (make version)"

# The default release origin: each installer is fetched on its own, so each carries it.
repo=$(sed -n 's/^REPO=//p' scripts/install.sh)
[ -n "$repo" ] || bad "scripts/install.sh: no REPO= default"
grep -qx "REPO=$repo" scripts/install-node.sh || bad "scripts/install-node.sh: REPO default is not $repo"
grep -qF "[string]\$Repo = '$repo'" scripts/install-node.ps1 || bad "scripts/install-node.ps1: -Repo default is not $repo"
grep -qF "pub release_repo: String = \"$repo\".to_string()," crates/pt-hub/src/home.rs ||
	bad "crates/pt-hub/src/home.rs: release_repo default is not $repo"

[ "$fail" = 0 ] && echo "check-version: ok"
exit $fail
