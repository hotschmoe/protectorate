#!/usr/bin/env bash
# Installs the Protectorate hub for this user from a GitHub release (SPEC 6, 6a). It downloads
# protectorate for this machine (one file: it embeds headscale and member), checks it against the
# release's SHA256SUMS before using it and checks that it reports that release with both
# embedded, stages it in $PROTECTORATE_HOME/releases/<version>/ (the layout pt-update keeps), points releases/current at that version (releases/previous keeps the old
# one), and runs the hub as a systemd user service with linger, so it starts at boot with no
# login. The unit runs releases/current, which `protectorate hub update` switches on probation. It prints the plan first; --dry-run checks the release and stops there, writing nothing.
#
# Fetch this script at a commit, never a tag (SPEC 12a item 9), and compare its sha256 with the
# one in the release notes before running it:
#   curl -q -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install.sh
#   sha256sum install.sh
#   bash install.sh --dry-run
set -euo pipefail

REPO=hotschmoe/protectorate
API=https://api.github.com
UNIT=protectorate.service
MARK='# Written by Protectorate install.sh'

usage() {
	cat <<-EOF
	usage: bash install.sh [--version TAG | --channel stable|dev] [--from DIR] [--repo OWNER/NAME]
	                       [--home DIR] [--no-start] [--dry-run]
	  --version TAG  install this release (default: the newest on --channel, stable)
	  --from DIR     use release assets already in DIR (as make release-assets writes them)
	  --home DIR     the hub's root, PROTECTORATE_HOME (default ~/.local/share/protectorate)
	  --no-start     install the unit but do not start or restart the hub
	  --dry-run      check the release against SHA256SUMS and print the plan; write nothing
	EOF
}

die() { printf 'install: %s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }

curl_https() { curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --retry 3 "$@"; }

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

# fetch NAME DEST: one asset into DEST.
fetch() {
	if [ -n "$FROM" ]; then
		[ -f "$FROM/$1" ] || die "$FROM has no $1"
		cp -- "$FROM/$1" "$2"
	else
		curl_https -o "$2" "$BASE/$1" || die "downloading $1 failed"
	fi
}

# sum_of NAME: the sha256 of an asset, streamed, so a dry run stores nothing.
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

# want NAME: the one sha256 SHA256SUMS lists for NAME.
want() {
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

safe_path() { [[ "$2" == /* && "$2" =~ ^[A-Za-z0-9/._+-]+$ ]] || die "$1 must be an absolute path of plain characters: $2"; }

main() {
	local channel=stable version='' home='' dry=0 start=1
	FROM=''
	while [ $# -gt 0 ]; do
		case "$1" in
		--version | --channel | --from | --repo | --home)
			[ $# -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
			case "$1" in
			--version) version=$2 ;; --channel) channel=$2 ;; --from) FROM=$2 ;;
			--repo) REPO=$2 ;; --home) home=$2 ;;
			esac
			shift 2 ;;
		--dry-run) dry=1; shift ;;
		--no-start) start=0; shift ;;
		-h | --help) usage; return 0 ;;
		*) usage >&2; die "unknown argument: $1" ;;
		esac
	done
	[ "$channel" = stable ] || [ "$channel" = dev ] || die "--channel must be stable or dev"
	[[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "--repo must be OWNER/NAME"
	[ "$(uname -s)" = Linux ] || die "the hub runs on Linux only"
	local arch
	case "$(uname -m)" in
	x86_64 | amd64) arch=amd64 ;; aarch64 | arm64) arch=arm64 ;;
	*) die "no hub build for $(uname -m)" ;;
	esac
	for t in sha256sum awk sed grep sort mktemp; do command -v "$t" >/dev/null || die "$t is required"; done
	[ -n "$FROM" ] || command -v curl >/dev/null || die "curl is required"
	[ "$(id -u)" != 0 ] || die "install the hub as its owner, not root (it runs as a user service)"

	home=${home:-${PROTECTORATE_HOME:-$HOME/.local/share/protectorate}}
	safe_path --home "$home"
	local rel=$home/releases unit_dir=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
	local unit=$unit_dir/$UNIT

	if [ -n "$FROM" ]; then
		[ -d "$FROM" ] || die "--from $FROM is not a directory"
		BASE=$FROM
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

	local asset=protectorate-linux-$arch
	say "Protectorate hub $TAG for linux-$arch from $BASE"
	if [ "$dry" = 1 ]; then
		verify "$asset"
		say "  verified $asset against SHA256SUMS"
	fi

	local old linger exec_path=$rel/current/protectorate
	old=$(readlink -- "$rel/current" 2>/dev/null || true)
	linger=$(loginctl show-user "$USER" --property=Linger --value 2>/dev/null || echo unknown)
	say "Plan:"
	if [ -d "$rel/$TAG" ]; then say "  reuse    $rel/$TAG (checked against SHA256SUMS)"
	else say "  stage    $rel/$TAG/protectorate"; fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then say "  point    $rel/current -> $TAG (previous -> $old)"
	else say "  point    $rel/current -> $TAG"; fi
	say "  write    $unit"
	say "           ExecStartPre=$rel/previous/protectorate --home $home hub probation"
	say "           ExecStart=$exec_path --home $home serve"
	[ "$linger" = yes ] || say "  run      loginctl enable-linger $USER (linger is $linger; a headless hub needs it)"
	if [ "$start" = 1 ]; then say "  run      systemctl --user daemon-reload; systemctl --user enable $UNIT; (re)start it"
	else say "  run      systemctl --user daemon-reload; systemctl --user enable $UNIT (not started: --no-start)"; fi
	if [ "$dry" = 1 ]; then say "Dry run: nothing was written."; return 0; fi

	command -v systemctl >/dev/null || die "systemctl is required"
	systemctl --user show-environment >/dev/null 2>&1 || die "no systemd user manager for $USER (log in on a seat or ssh with PAM, or enable linger first)"
	if [ -e "$unit" ] && ! grep -qxF "$MARK" "$unit"; then die "$unit exists and was not written by this installer"; fi
	[[ "$PATH" != *[\"\\%\$]* ]] || die "PATH holds a quote, backslash, % or \$; set a plain PATH for the unit"

	umask 077
	mkdir -p -- "$rel" "$unit_dir"
	if [ -d "$rel/$TAG" ]; then
		check_file "$asset" "$rel/$TAG/protectorate"
	else
		STAGING=$rel/.staging-$TAG-$$
		mkdir -- "$STAGING"
		# Removes only this run's own staging directory inside releases/, on any failure.
		trap 'rm -rf -- "${STAGING:-}"' EXIT
		fetch "$asset" "$STAGING/protectorate"
		check_file "$asset" "$STAGING/protectorate"
		chmod 755 -- "$STAGING/protectorate"
		printf '%s\n' "$SUMS" >"$STAGING/SHA256SUMS"
		fetch VERSION "$STAGING/VERSION"
		check_file VERSION "$STAGING/VERSION"
		local reports
		reports=$("$STAGING/protectorate" version) || die "the staged hub does not run"
		[ "${reports%%$'\n'*}" = "protectorate $TAG" ] || die "the staged hub reports '${reports%%$'\n'*}', not $TAG"
		[[ "$reports" != *missing* ]] || die "the staged hub does not embed headscale and member: $reports"
		mv -T -- "$STAGING" "$rel/$TAG"
		STAGING=''
		trap - EXIT
	fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then
		ln -s -- "$old" "$rel/.previous-$$" && mv -Tf -- "$rel/.previous-$$" "$rel/previous"
	fi
	ln -s -- "$TAG" "$rel/.current-$$" && mv -Tf -- "$rel/.current-$$" "$rel/current"
	if [ ! -L "$rel/previous" ]; then
		ln -s -- "$TAG" "$rel/previous"
	fi
	say "releases/current -> $TAG"

	local path_line=${PATH//%/%%}
	cat >"$unit.tmp-$$" <<-EOF
	$MARK
	[Unit]
	Description=Protectorate hub
	Wants=network-online.target
	After=network-online.target

	[Service]
	Type=simple
	Environment="PROTECTORATE_HOME=$home"
	Environment="PATH=$path_line"
	# A release on trial that keeps failing to start is switched back from the previous one.
	ExecStartPre=$rel/previous/protectorate --home $home hub probation
	ExecStart=$exec_path --home $home serve
	Restart=on-failure
	RestartSec=5

	[Install]
	WantedBy=default.target
	EOF
	mv -f -- "$unit.tmp-$$" "$unit"
	say "wrote $unit"

	if [ "$linger" != yes ]; then
		loginctl enable-linger "$USER" 2>/dev/null && say "linger enabled for $USER" ||
			say "warning: could not enable linger; run: sudo loginctl enable-linger $USER"
	fi
	systemctl --user daemon-reload
	systemctl --user enable "$UNIT" >/dev/null
	if [ "$start" = 1 ]; then
		if systemctl --user is-active --quiet "$UNIT"; then systemctl --user restart "$UNIT"; else systemctl --user start "$UNIT"; fi
		say "started $UNIT; logs: journalctl --user -u $UNIT"
	fi
	say "Installed Protectorate hub $TAG."
}

main "$@"
