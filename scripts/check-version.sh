#!/bin/sh
# Fails when a build path could take its version from anywhere but the git tag
# through the Makefile. PUBSPEC and WORKFLOW override the files checked, for
# the tests.
set -u
cd "$(dirname "$0")/.."
pubspec=${PUBSPEC:-app/pubspec.yaml}
workflows=${WORKFLOW:-$(ls .github/workflows/*.yml 2>/dev/null)}
fail=0
bad() { echo "check-version: $*" >&2; fail=1; }

grep -qx 'version: 0\.0\.0+0' "$pubspec" || bad "$pubspec: version must stay the placeholder 0.0.0+0; the version is the git tag"

for target in app-build-web apk app-build-linux; do
	lines=$(${MAKE:-make} -n "$target" 2>&1 | grep 'flutter build') || { bad "make $target runs no flutter build"; continue; }
	echo "$lines" | while IFS= read -r l; do
		for f in --build-name= --build-number= --dart-define=PROTECTORATE_VERSION=; do
			case $l in *"$f"*) ;; *) exit 1 ;; esac
		done
	done || bad "make $target: flutter build lacks --build-name, --build-number or PROTECTORATE_VERSION"
done

for target in build cross; do
	${MAKE:-make} -n "$target" 2>&1 | grep 'go build' | while IFS= read -r l; do
		case $l in *-X\ main.version=*) ;; *) exit 1 ;; esac
	done || bad "make $target: go build lacks -X main.version"
done

for f in $workflows scripts/*.sh; do
	[ "$f" = scripts/check-version.sh ] && continue
	if grep -nE 'flutter build|go build' "$f" >&2; then
		bad "$f: build through make (build, cross, apk, app-build-web, app-build-linux) so the version is applied"
	fi
done

exit $fail
