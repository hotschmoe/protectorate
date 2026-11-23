#!/usr/bin/env bash
# Installs a Protectorate node on Linux from a GitHub release. It downloads protectorate-node for
# this machine, checks it against SHA256SUMS, stages it in <home>/releases/<version>/, points
# releases/current (and current.txt) at that version, joins the hub using JOIN_TOKEN (ptjoin1:...),
# waits for authorization if needed, and runs install-service.
#
# Fetch this script at a commit and compare its sha256 before running:
#   curl -q -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install-node.sh
#   sha256sum install-node.sh
#   sudo bash install-node.sh ptjoin1:... --dry-run
set -euo pipefail

REPO=hotschmoe/protectorate
API=https://api.github.com

usage() {
	cat <<-EOF
	usage: bash install-node.sh JOIN_TOKEN [--name NAME] [--version TAG | --channel stable|dev]
	                            [--from DIR] [--repo OWNER/NAME] [--home DIR] [--state-dir DIR]
	                            [--service-name NAME] [--user] [--wait-secs N] [--dry-run]
	  JOIN_TOKEN          ptjoin1:... token printed by 'protectorate node join-token'
	  --version TAG       install this release (default: the newest on --channel, stable)
	  --from DIR          use release assets already in DIR (as make release-assets writes them)
	  --home DIR          node home directory (default /var/lib/protectorate-node, or ~/.local/share/protectorate-node with --user)
	  --state-dir DIR     alias for --home
	  --service-name NAME systemd unit name (default protectorate-node)
	  --user              install as a systemd --user service without root
	  --wait-secs N       seconds to poll authorize while waiting for hub approval (default 60)
	  --dry-run           check the release against SHA256SUMS and print the plan; write nothing
	EOF
}

die() { printf 'install-node: %s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }

curl_https() { curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --retry 3 "$@"; }

resolve_tag() {
	local json pat
	json=$(curl_https -H 'Accept: application/vnd.github+json' "$API/repos/$REPO/releases?per_page=100") ||
		die "listing the releases of $REPO failed"
	if [ "$1" = stable ]; then pat='^v[0-9]+\.[0-9]+\.[0-9]+$'; else pat='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; fi
	printf '%s' "$json" | grep -o '"tag_name": *"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/' |
		grep -E "$pat" | sed 's/-/~/' | sort -V | tail -n 1 | sed 's/~/-/' || true
}

fetch() {
	if [ -n "$FROM" ]; then
		[ -f "$FROM/$1" ] || die "$FROM has no $1"
		cp -- "$FROM/$1" "$2"
	else
		curl_https -o "$2" "$BASE/$1" || die "downloading $1 failed"
	fi
}

sum_of() {
	local out
	if [ -n "$FROM" ]; then
		[ -f "$FROM/$1" ] || die "$FROM has no $1"
		out=$(sha256sum <"$FROM/$1")
	else
		out=$(curl_https "$BASE/$1" | sha256sum) || die "downloading $1 failed"
	fi
	printf '%s' "${out%% *}"
}

want() {
	local hits
	hits=$(printf '%s\n' "$SUMS" | awk -v n="$1" '$2 == n || $2 == "*" n { print $1 }')
	[ "$(printf '%s\n' "$hits" | grep -c .)" = 1 ] && [[ "$hits" =~ ^[0-9a-f]{64}$ ]] ||
		die "SHA256SUMS of $TAG has no single valid entry for $1"
	printf '%s' "$hits"
}

check_file() {
	local got exp
	got=$(sha256sum <"$2")
	exp=$(want "$1")
	[ "${got%% *}" = "$exp" ] || die "$1 does not match SHA256SUMS (got ${got%% *})"
}

verify() {
	local got exp
	got=$(sum_of "$1")
	exp=$(want "$1")
	[ "$got" = "$exp" ] || die "$1 does not match SHA256SUMS (got $got)"
}

main() {
	local token='' name='' channel=stable version='' state='' svc_name='protectorate-node'
	local dry=0 user_mode=0 wait_secs=60
	FROM=''
	while [ $# -gt 0 ]; do
		case "$1" in
		--name | --version | --channel | --from | --repo | --home | --state-dir | --service-name | --wait-secs)
			[ $# -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
			case "$1" in
			--name) name=$2 ;; --version) version=$2 ;; --channel) channel=$2 ;;
			--from) FROM=$2 ;; --repo) REPO=$2 ;; --home | --state-dir) state=$2 ;;
			--service-name) svc_name=$2 ;; --wait-secs) wait_secs=$2 ;;
			esac
			shift 2 ;;
		--user) user_mode=1; shift ;;
		--dry-run) dry=1; shift ;;
		-h | --help) usage; return 0 ;;
		-*) usage >&2; die "unknown option: $1" ;;
		*) [ -z "$token" ] || die "unexpected argument: $1"; token=$1; shift ;;
		esac
	done
	[ -n "$token" ] || { usage >&2; die "JOIN_TOKEN is required"; }
	[[ "$token" =~ ^ptjoin1:[A-Za-z0-9_-]+$ ]] || die "JOIN_TOKEN must look like ptjoin1:..."
	[ "$channel" = stable ] || [ "$channel" = dev ] || die "--channel must be stable or dev"
	[[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "--repo must be OWNER/NAME"
	[[ "$svc_name" =~ ^[A-Za-z0-9._@-]+$ ]] || die "--service-name contains unsupported characters"
	[[ "$wait_secs" =~ ^[0-9]+$ ]] || die "--wait-secs must be a non-negative integer"
	[ "$(uname -s)" = Linux ] || die "this installer is for Linux; use install-node.ps1 on Windows"

	if [ -z "$state" ]; then
		if [ "$user_mode" = 1 ]; then state=${PROTECTORATE_NODE_HOME:-$HOME/.local/share/protectorate-node}
		else state=/var/lib/protectorate-node; fi
	fi
	[[ "$state" == /* && "$state" =~ ^[A-Za-z0-9/._+-]+$ ]] || die "--home must be an absolute path of plain characters"

	local arch
	case "$(uname -m)" in
	x86_64 | amd64) arch=amd64 ;; aarch64 | arm64) arch=arm64 ;;
	*) die "no node build for $(uname -m)" ;;
	esac
	for t in sha256sum awk sed grep sort; do command -v "$t" >/dev/null || die "$t is required"; done
	[ -n "$FROM" ] || command -v curl >/dev/null || die "curl is required"
	local root=1
	[ "$(id -u)" = 0 ] || root=0
	if [ "$user_mode" = 0 ] && [ "$root" = 0 ] && [ "$dry" = 0 ]; then
		die "run as root (sudo) or pass --user for a user systemd unit"
	fi

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

	local asset=protectorate-node-linux-$arch
	say "Protectorate node $TAG for linux-$arch from $BASE"
	if [ "$dry" = 1 ]; then
		verify "$asset"
		say "  verified $asset against SHA256SUMS"
	fi

	local rel=$state/releases joined=no service=no old=''
	if [ -d "$state" ] && [ ! -x "$state" ]; then
		joined=unknown service=unknown
	else
		[ -f "$state/config.json" ] && joined=yes
		[ -f "$state/install-manifest.json" ] && service=yes
		old=$(readlink -- "$rel/current" 2>/dev/null || true)
	fi
	local unit_file=$svc_name
	case "$unit_file" in *.service) ;; *) unit_file="$unit_file.service" ;; esac
	local unit_path=/etc/systemd/system/$unit_file
	[ "$user_mode" = 0 ] || unit_path=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/$unit_file
	if [ -e "$unit_path" ]; then
		case "$service" in no) service="unit only" ;; unknown) service="unit present" ;; esac
	fi
	say "Found: state $state (joined: $joined, service installed: $service)"

	local cur=$rel/current
	local join=("$cur/protectorate-node" join "$token" --home "$state")
	[ -z "$name" ] || join+=(--name "$name")
	local svc_args=(install-service --home "$state" --service-name "$svc_name")
	[ "$user_mode" = 0 ] || svc_args+=(--user)
	say "Plan:"
	if [ -d "$rel/$TAG" ]; then say "  reuse    $rel/$TAG (checked against SHA256SUMS)"
	else say "  stage    $rel/$TAG/protectorate-node"; fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then say "  point    $cur -> $TAG (previous -> $old)"
	else say "  point    $cur -> $TAG"; fi
	case "$joined" in
	yes) say "  skip     join: $state/config.json exists (already joined)" ;;
	no) say "  run      ${join[*]}" ;;
	*) say "  run      ${join[*]} (unless $state/config.json exists; needs privileges to inspect)" ;;
	esac
	case "$service" in
	no) say "  run      $cur/protectorate-node ${svc_args[*]}" ;;
	unknown) say "  run      $cur/protectorate-node ${svc_args[*]}" ;;
	*) say "  skip     install-service: already installed ($service)" ;;
	esac
	if [ "$joined" != no ] && [ "$service" != no ] && [ "$service" != unknown ]; then
		say "  stop     already joined and installed: update from the hub instead"
	fi
	if [ "$dry" = 1 ]; then say "Dry run: nothing was written."; return 0; fi
	[ "$joined" != unknown ] || die "cannot read $state"
	[ "$joined" = no ] || [ "$service" = no ] ||
		die "this node is already joined and installed; update it from the hub instead"

	umask 077
	mkdir -p -- "$rel"
	chmod 700 -- "$state"
	if [ -d "$rel/$TAG" ]; then
		check_file "$asset" "$rel/$TAG/protectorate-node"
	else
		STAGING=$rel/.staging-$TAG-$$
		mkdir -- "$STAGING"
		trap 'rm -rf -- "${STAGING:-}"' EXIT
		fetch "$asset" "$STAGING/protectorate-node"
		check_file "$asset" "$STAGING/protectorate-node"
		chmod 755 -- "$STAGING/protectorate-node"
		printf '%s\n' "$SUMS" >"$STAGING/SHA256SUMS"
		fetch VERSION "$STAGING/VERSION"
		check_file VERSION "$STAGING/VERSION"
		local reports first_line
		reports=$("$STAGING/protectorate-node" --version) || die "the staged node binary does not run"
		first_line=${reports%%$'\n'*}
		case "$first_line" in
		"protectorate-node $TAG"|"protectorate-node $TAG "*) ;;
		*) die "the staged node reports '$first_line', not $TAG" ;;
		esac
		mv -T -- "$STAGING" "$rel/$TAG"
		STAGING=''
		trap - EXIT
	fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then
		printf '%s\n' "$old" >"$rel/previous.txt"
		ln -s -- "$old" "$rel/.previous-$$" && mv -Tf -- "$rel/.previous-$$" "$rel/previous"
	fi
	printf '%s\n' "$TAG" >"$rel/current.txt"
	ln -s -- "$TAG" "$rel/.current-$$" && mv -Tf -- "$rel/.current-$$" "$cur"
	if [ ! -L "$rel/previous" ]; then
		ln -s -- "$TAG" "$rel/previous"
	fi
	say "$cur -> $TAG"

	if [ "$joined" = no ]; then
		local jout n_name n_key
		jout=$("${join[@]}")
		printf '%s\n' "$jout"
		n_name=$(printf '%s\n' "$jout" | sed -n 's/^  name: *//p' | head -n1)
		n_key=$(printf '%s\n' "$jout" | sed -n 's/^  node_key: *//p' | head -n1)
		if [ -n "$n_name" ] && [ -n "$n_key" ]; then
			say "If not already pre-registered, run on the hub: protectorate node add $n_name:$n_key"
		fi
		local elapsed=0 authed=0
		while [ "$elapsed" -le "$wait_secs" ]; do
			if "$cur/protectorate-node" authorize --home "$state" >/dev/null 2>&1; then
				authed=1
				break
			fi
			[ "$elapsed" = 0 ] && say "Waiting for hub authorization (up to ${wait_secs}s)..."
			[ "$elapsed" -ge "$wait_secs" ] && break
			sleep 3
			elapsed=$((elapsed + 3))
		done
		if [ "$authed" = 1 ]; then
			say "Authorized against hub."
		else
			say "warning: node not authorized yet; service will connect once approved on the hub."
		fi
	fi
	if [ "$service" = no ]; then
		"$cur/protectorate-node" "${svc_args[@]}"
	fi
	say "Installed Protectorate node $TAG."
}

main "$@"
