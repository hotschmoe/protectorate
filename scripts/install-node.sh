#!/usr/bin/env bash
# Installs a Protectorate node on Linux from a GitHub release (SPEC 3.5, 6a; D10: a system
# service, so it runs as root). It downloads protectorate-node and member for this machine,
# checks each against the release's SHA256SUMS before using it, stages them in
# <state>/releases/<version>/ (the layout pt-update keeps), points releases/current at that
# version, then runs `protectorate-node join SERVER_URL` (scan to join: it shows a QR code and
# waits for the owner to approve on the hub) and `install-service`. It prints the plan first;
# --dry-run checks the release and the machine and stops there, writing nothing.
#
# Fetch this script at a commit, never a tag (SPEC 12a item 9), and compare its sha256 with the
# one in the release notes before running it:
#   curl -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install-node.sh
#   sha256sum install-node.sh
#   sudo bash install-node.sh https://protectorate.hotschmoe.com --dry-run
set -euo pipefail

REPO=hotschmoe/protectorate
API=https://api.github.com
UNIT_PATH=/etc/systemd/system/protectorate-node.service

usage() {
	cat <<-EOF
	usage: sudo bash install-node.sh SERVER_URL [--name NAME] [--version TAG | --channel stable|dev]
	                                 [--from DIR] [--repo OWNER/NAME] [--state-dir DIR] [--dry-run]
	  SERVER_URL      the hub's headscale URL that join registers with
	  --version TAG   install this release (default: the newest on --channel, stable)
	  --from DIR      use release assets already in DIR (as make release-assets writes them)
	  --state-dir DIR the node's state (default /var/lib/protectorate-node)
	  --dry-run       check the release against SHA256SUMS and print the plan; write nothing
	EOF
}

die() { printf 'install-node: %s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }

curl_https() { curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --retry 3 "$@"; }

# The newest tag on a channel: stable is vX.Y.Z, dev also takes vX.Y.Z-PRE. Ordered by semver
# precedence (sort -V with the pre-release dash as a tilde, which sorts before the release).
resolve_tag() {
	local json pat
	json=$(curl_https -H 'Accept: application/vnd.github+json' "$API/repos/$REPO/releases?per_page=100") ||
		die "listing the releases of $REPO failed"
	if [ "$1" = stable ]; then pat='^v[0-9]+\.[0-9]+\.[0-9]+$'; else pat='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; fi
	printf '%s' "$json" | grep -o '"tag_name": *"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/' |
		grep -E "$pat" | sed 's/-/~/' | sort -V | tail -n 1 | sed 's/~/-/' || true
}

fetch() { # NAME DEST
	if [ -n "$FROM" ]; then
		[ -f "$FROM/$1" ] || die "$FROM has no $1"
		cp -- "$FROM/$1" "$2"
	else
		curl_https -o "$2" "$BASE/$1" || die "downloading $1 failed"
	fi
}

sum_of() { # NAME: streamed, so a dry run stores nothing
	local out
	if [ -n "$FROM" ]; then
		[ -f "$FROM/$1" ] || die "$FROM has no $1"
		out=$(sha256sum <"$FROM/$1")
	else
		out=$(curl_https "$BASE/$1" | sha256sum) || die "downloading $1 failed"
	fi
	printf '%s' "${out%% *}"
}

want() { # NAME: the one sha256 SHA256SUMS lists for it
	local hits
	hits=$(printf '%s\n' "$SUMS" | awk -v n="$1" '$2 == n || $2 == "*" n { print $1 }')
	[ "$(printf '%s\n' "$hits" | grep -c .)" = 1 ] && [[ "$hits" =~ ^[0-9a-f]{64}$ ]] ||
		die "SHA256SUMS of $TAG has no single valid entry for $1"
	printf '%s' "$hits"
}

check_file() { # NAME PATH
	local got exp
	got=$(sha256sum <"$2")
	exp=$(want "$1")
	[ "${got%% *}" = "$exp" ] || die "$1 does not match SHA256SUMS (got ${got%% *})"
}

# verify NAME: streams an asset and compares it with SHA256SUMS. Each value is its own
# assignment, so a failure inside the substitution stops the script (set -e).
verify() {
	local got exp
	got=$(sum_of "$1")
	exp=$(want "$1")
	[ "$got" = "$exp" ] || die "$1 does not match SHA256SUMS (got $got)"
}

main() {
	local url='' name='' channel=stable version='' state=/var/lib/protectorate-node dry=0
	FROM=''
	while [ $# -gt 0 ]; do
		case "$1" in
		--name | --version | --channel | --from | --repo | --state-dir)
			[ $# -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
			case "$1" in
			--name) name=$2 ;; --version) version=$2 ;; --channel) channel=$2 ;;
			--from) FROM=$2 ;; --repo) REPO=$2 ;; --state-dir) state=$2 ;;
			esac
			shift 2 ;;
		--dry-run) dry=1; shift ;;
		-h | --help) usage; return 0 ;;
		-*) usage >&2; die "unknown option: $1" ;;
		*) [ -z "$url" ] || die "unexpected argument: $1"; url=$1; shift ;;
		esac
	done
	[ -n "$url" ] || { usage >&2; die "SERVER_URL is required"; }
	local url_re='^https?://[]A-Za-z0-9.:[-]+(/.*)?$'
	[[ "$url" =~ $url_re ]] || die "SERVER_URL must be http(s)://host[:port]"
	[ "$channel" = stable ] || [ "$channel" = dev ] || die "--channel must be stable or dev"
	[[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "--repo must be OWNER/NAME"
	[[ "$state" == /* && "$state" =~ ^[A-Za-z0-9/._+-]+$ ]] || die "--state-dir must be an absolute path of plain characters"
	[ "$(uname -s)" = Linux ] || die "this installer is for Linux; use install-node.ps1 on Windows"
	local arch
	case "$(uname -m)" in
	x86_64 | amd64) arch=amd64 ;; aarch64 | arm64) arch=arm64 ;;
	*) die "no node build for $(uname -m)" ;;
	esac
	for t in sha256sum awk sed grep sort; do command -v "$t" >/dev/null || die "$t is required"; done
	[ -n "$FROM" ] || command -v curl >/dev/null || die "curl is required"
	local root=1
	[ "$(id -u)" = 0 ] || root=0
	[ "$root" = 1 ] || [ "$dry" = 1 ] || die "run as root (sudo): the node is a system service (D10)"

	if [ -n "$FROM" ]; then
		[ -d "$FROM" ] || die "--from $FROM is not a directory"
		BASE=$FROM
		TAG=''
	else
		TAG=$version
		[ -n "$TAG" ] || TAG=$(resolve_tag "$channel")
		[ -n "$TAG" ] || die "$REPO has no $channel release"
		BASE=https://github.com/$REPO/releases/download/$TAG
	fi
	if [ -n "$FROM" ]; then SUMS=$(cat -- "$FROM/SHA256SUMS") || die "$FROM has no SHA256SUMS"
	else SUMS=$(curl_https "$BASE/SHA256SUMS") || die "downloading SHA256SUMS failed"; fi
	local said
	if [ -n "$FROM" ]; then said=$(cat -- "$FROM/VERSION"); else said=$(curl_https "$BASE/VERSION") || die "downloading VERSION failed"; fi
	TAG=${TAG:-$said}
	verify VERSION
	[ "$said" = "$TAG" ] || die "the release's VERSION says $said, not $TAG"
	[[ "$TAG" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ && "$TAG" != current && "$TAG" != previous ]] || die "bad version $TAG"

	local assets=("protectorate-node-linux-$arch" "member-linux-$arch") names=(protectorate-node member) i
	say "Protectorate node $TAG for linux-$arch from $BASE"
	if [ "$dry" = 1 ]; then
		for i in 0 1; do
			verify "${assets[$i]}"
			say "  verified ${assets[$i]} against SHA256SUMS"
		done
	fi

	# What is on the machine already. Without root the state dir may be unreadable (0700).
	local rel=$state/releases joined=no service=no old=''
	if [ -d "$state" ] && [ ! -x "$state" ]; then
		joined=unknown service=unknown
	else
		[ -f "$state/node.json" ] && joined=yes
		[ -f "$state/install-manifest.json" ] && service=yes
		old=$(readlink -- "$rel/current" 2>/dev/null || true)
	fi
	if [ -e "$UNIT_PATH" ]; then
		case "$service" in no) service="unit only" ;; unknown) service="unit present" ;; esac
	fi
	local active
	active=$(systemctl is-active protectorate-node.service 2>/dev/null || true)
	say "Found: state $state (joined: $joined, service installed: $service, service ${active:-unknown})"

	local cur=$rel/current
	local join=("$cur/protectorate-node" join "$url" --state-dir "$state" --member "$cur/member")
	[ -z "$name" ] || join+=(--name "$name")
	say "Plan:"
	if [ -d "$rel/$TAG" ]; then say "  reuse    $rel/$TAG (checked against SHA256SUMS)"
	else say "  stage    $rel/$TAG/{protectorate-node,member}"; fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then say "  point    $cur -> $TAG (previous -> $old)"
	else say "  point    $cur -> $TAG"; fi
	case "$joined" in
	yes) say "  skip     join: $state/node.json exists (already joined)" ;;
	no) say "  run      ${join[*]}" ;;
	*) say "  run      ${join[*]} (unless $state/node.json exists; needs root to tell)" ;;
	esac
	case "$service" in
	no) say "  run      $cur/protectorate-node install-service --state-dir $state --member $cur/member" ;;
	unknown) say "  run      $cur/protectorate-node install-service --state-dir $state --member $cur/member (unless installed; needs root to tell)" ;;
	*) say "  skip     install-service: already installed ($service); this installer does not replace a running node" ;;
	esac
	if [ "$joined" != no ] && [ "$service" != no ] && [ "$service" != unknown ]; then
		say "  stop     already joined and installed: a real run refuses; update from the hub"
	fi
	if [ "$dry" = 1 ]; then say "Dry run: nothing was written."; return 0; fi
	[ "$joined" != unknown ] || die "cannot read $state"
	[ "$joined" = no ] || [ "$service" = no ] ||
		die "this node is already joined and installed; update it from the hub instead"

	umask 077
	mkdir -p -- "$rel"
	chmod 700 -- "$state"
	if [ -d "$rel/$TAG" ]; then
		for i in 0 1; do check_file "${assets[$i]}" "$rel/$TAG/${names[$i]}"; done
	else
		STAGING=$rel/.staging-$TAG-$$
		mkdir -- "$STAGING"
		# Removes only this run's own staging directory inside releases/, on any failure.
		trap 'rm -rf -- "${STAGING:-}"' EXIT
		for i in 0 1; do
			fetch "${assets[$i]}" "$STAGING/${names[$i]}"
			check_file "${assets[$i]}" "$STAGING/${names[$i]}"
			chmod 755 -- "$STAGING/${names[$i]}"
		done
		printf '%s\n' "$SUMS" >"$STAGING/SHA256SUMS"
		fetch VERSION "$STAGING/VERSION"
		check_file VERSION "$STAGING/VERSION"
		"$STAGING/protectorate-node" --version >/dev/null || die "the staged node binary does not run"
		mv -T -- "$STAGING" "$rel/$TAG"
		STAGING=''
		trap - EXIT
	fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then
		ln -s -- "$old" "$rel/.previous-$$" && mv -Tf -- "$rel/.previous-$$" "$rel/previous"
	fi
	ln -s -- "$TAG" "$rel/.current-$$" && mv -Tf -- "$rel/.current-$$" "$cur"
	say "$cur -> $TAG"

	if [ "$joined" = no ]; then
		say "Joining $url: approve this node on the hub when the QR code shows."
		"${join[@]}"
	fi
	if [ "$service" = no ]; then
		"$cur/protectorate-node" install-service --state-dir "$state" --member "$cur/member"
	fi
	say "Installed Protectorate node $TAG."
}

main "$@"
