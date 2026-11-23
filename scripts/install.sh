#!/usr/bin/env bash
# Installs the Protectorate hub for this user from a GitHub release. It downloads protectorate
# for this machine, checks it against SHA256SUMS, stages it in $PROTECTORATE_HOME/releases/<version>/,
# points releases/current at that version (releases/previous keeps the old one), and configures
# a systemd user service with linger.
#
# Fetch this script at a commit and compare its sha256 before running:
#   curl -q -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install.sh
#   sha256sum install.sh
#   bash install.sh --dry-run
set -euo pipefail

REPO=hotschmoe/protectorate
API=https://api.github.com
DEFAULT_UNIT=protectorate.service
MARK='# Written by Protectorate install.sh'

usage() {
	cat <<-EOF
	usage: bash install.sh [--version TAG | --channel stable|dev] [--from DIR] [--repo OWNER/NAME]
	                       [--home DIR] [--unit-name NAME] [--listen ADDR:PORT]
	                       [--proxy-listen ADDR:PORT] [--trusted-proxy IP]...
	                       [--fleet-listen ADDR:PORT] [--no-start] [--dry-run]
	  --version TAG           install this release (default: the newest on --channel, stable)
	  --from DIR              use release assets already in DIR (as make release-assets writes them)
	  --home DIR              the hub's root, PROTECTORATE_HOME (default ~/.local/share/protectorate)
	  --unit-name NAME        systemd user unit name (default $DEFAULT_UNIT)
	  --listen ADDR:PORT      HTTP listen address passed to serve
	  --proxy-listen ADDR:PORT reverse-proxy HTTP listener address passed to serve
	  --trusted-proxy IP      allow proxy connections from IP (repeatable)
	  --fleet-listen ADDR:PORT Noise_IK fleet listener address passed to serve
	  --no-start              install the unit but do not start or restart the hub
	  --dry-run               check the release against SHA256SUMS and print the plan; write nothing
	EOF
}

die() { printf 'install: %s\n' "$*" >&2; exit 1; }
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

safe_path() { [[ "$2" == /* && "$2" =~ ^[A-Za-z0-9/._+-]+$ ]] || die "$1 must be an absolute path of plain characters: $2"; }
safe_token() { [[ "$2" =~ ^[A-Za-z0-9.:_+\[\]-]+$ ]] || die "$1 contains unsupported characters: $2"; }

main() {
	local channel=stable version='' home='' dry=0 start=1 unit_name=$DEFAULT_UNIT
	local listen_addr='' proxy_listen='' fleet_listen=''
	local trusted_proxies=()
	FROM=''
	while [ $# -gt 0 ]; do
		case "$1" in
		--version | --channel | --from | --repo | --home | --unit-name | --listen | --proxy-listen | --trusted-proxy | --fleet-listen)
			[ $# -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
			case "$1" in
			--version) version=$2 ;; --channel) channel=$2 ;; --from) FROM=$2 ;;
			--repo) REPO=$2 ;; --home) home=$2 ;; --unit-name) unit_name=$2 ;;
			--listen) safe_token --listen "$2"; listen_addr=$2 ;;
			--proxy-listen) safe_token --proxy-listen "$2"; proxy_listen=$2 ;;
			--trusted-proxy) safe_token --trusted-proxy "$2"; trusted_proxies+=("$2") ;;
			--fleet-listen) safe_token --fleet-listen "$2"; fleet_listen=$2 ;;
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
	[[ "$unit_name" =~ ^[A-Za-z0-9._@-]+$ ]] || die "--unit-name contains unsupported characters: $unit_name"
	case "$unit_name" in *.service) ;; *) unit_name="$unit_name.service" ;; esac
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
	local unit=$unit_dir/$unit_name

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

	local asset=protectorate-linux-$arch
	say "Protectorate hub $TAG for linux-$arch from $BASE"
	if [ "$dry" = 1 ]; then
		verify "$asset"
		say "  verified $asset against SHA256SUMS"
	fi

	local old linger exec_path=$rel/current/protectorate
	local serve_args="--home $home serve"
	[ -z "$listen_addr" ] || serve_args="$serve_args --listen $listen_addr"
	[ -z "$proxy_listen" ] || serve_args="$serve_args --proxy-listen $proxy_listen"
	for tp in "${trusted_proxies[@]}"; do
		serve_args="$serve_args --trusted-proxy $tp"
	done
	[ -z "$fleet_listen" ] || serve_args="$serve_args --fleet-listen $fleet_listen"

	local bin_dir=$HOME/.local/bin link_cli=0 cli_other='' cli_name=protectorate
	[ "$unit_name" = "$DEFAULT_UNIT" ] || cli_name=${unit_name%.service}
	local cli=$bin_dir/$cli_name
	if [ -L "$cli" ] && [ "$(readlink -- "$cli")" = "$exec_path" ]; then :
	elif [ -L "$cli" ]; then cli_other="a link to $(readlink -- "$cli")"
	elif [ -e "$cli" ]; then cli_other='a file'
	else link_cli=1; fi
	old=$(readlink -- "$rel/current" 2>/dev/null || true)
	linger=$(loginctl show-user "$USER" --property=Linger --value 2>/dev/null || echo unknown)
	say "Plan:"
	if [ -d "$rel/$TAG" ]; then say "  reuse    $rel/$TAG (checked against SHA256SUMS)"
	else say "  stage    $rel/$TAG/protectorate"; fi
	if [ -n "$old" ] && [ "$old" != "$TAG" ]; then say "  point    $rel/current -> $TAG (previous -> $old)"
	else say "  point    $rel/current -> $TAG"; fi
	say "  write    $unit"
	say "           ExecStartPre=$rel/previous/protectorate --home $home hub probation"
	say "           ExecStart=$exec_path $serve_args"
	if [ "$link_cli" = 1 ]; then say "  link     $cli -> $exec_path"
	elif [ -n "$cli_other" ]; then say "  keep     $cli ($cli_other, not this hub's CLI)"; fi
	[ "$linger" = yes ] || say "  run      loginctl enable-linger $USER (linger is $linger; a headless hub needs it)"
	if [ "$start" = 1 ]; then say "  run      systemctl --user daemon-reload; systemctl --user enable $unit_name; (re)start it"
	else say "  run      systemctl --user daemon-reload; systemctl --user enable $unit_name (not started: --no-start)"; fi
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
		trap 'rm -rf -- "${STAGING:-}"' EXIT
		fetch "$asset" "$STAGING/protectorate"
		check_file "$asset" "$STAGING/protectorate"
		chmod 755 -- "$STAGING/protectorate"
		printf '%s\n' "$SUMS" >"$STAGING/SHA256SUMS"
		fetch VERSION "$STAGING/VERSION"
		check_file VERSION "$STAGING/VERSION"
		local reports first_line
		reports=$("$STAGING/protectorate" --version) || die "the staged hub does not run"
		first_line=${reports%%$'\n'*}
		case "$first_line" in
		"protectorate $TAG"|"protectorate $TAG "*) ;;
		*) die "the staged hub reports '$first_line', not $TAG" ;;
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
	ExecStart=$exec_path $serve_args
	Restart=on-failure
	RestartSec=5

	[Install]
	WantedBy=default.target
	EOF
	mv -f -- "$unit.tmp-$$" "$unit"
	say "wrote $unit"
	if [ "$link_cli" = 1 ]; then
		if mkdir -p -- "$bin_dir" && ln -s -- "$exec_path" "$cli"; then
			say "linked $cli"
		else
			say "warning: could not link $cli to $exec_path"
		fi
	elif [ -n "$cli_other" ]; then
		say "warning: $cli is $cli_other; to use this hub's CLI: ln -sfn '$exec_path' '$cli'"
	fi
	case ":$PATH:" in *":$bin_dir:"*) ;; *) say "note: $bin_dir is not on PATH; add it to run $cli_name" ;; esac
	[ "$home" = "$HOME/.local/share/protectorate" ] ||
		say "note: the CLI defaults to ~/.local/share/protectorate; set PROTECTORATE_HOME=$home in your shell profile"

	if [ "$linger" != yes ]; then
		loginctl enable-linger "$USER" 2>/dev/null && say "linger enabled for $USER" ||
			say "warning: could not enable linger; run: sudo loginctl enable-linger $USER"
	fi
	systemctl --user daemon-reload
	systemctl --user enable "$unit_name" >/dev/null
	if [ "$start" = 1 ]; then
		if systemctl --user is-active --quiet "$unit_name"; then systemctl --user restart "$unit_name"; else systemctl --user start "$unit_name"; fi
		say "started $unit_name; logs: journalctl --user -u $unit_name"
	fi
	say "Installed Protectorate hub $TAG."
}

main "$@"
