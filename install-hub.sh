#!/usr/bin/env bash
# Per-user install of the protectorate hub on Linux (x86_64, aarch64). No sudo,
# no system packages, nothing is deleted. Re-run to upgrade.
set -euo pipefail

DEFAULT_VERSION=v0.2.1
REPO=hotschmoe/protectorate

say() { printf '%s\n' "$*"; }
warn() { printf '%s\n' "$*" >&2; }
die() { warn "$*"; exit 1; }

download() {
  local proto=(--proto '=https' --tlsv1.2)
  [[ "$1" == https://* ]] || proto=()
  curl --fail --silent --show-error --location --retry 3 "${proto[@]}" "$1" -o "$2"
}

verify() {
  local file=$1 manifest=$2 name=$3 expected actual
  expected=$(awk -v name="$name" '$2 == name || $2 == "*" name { print $1 }' "$manifest")
  [[ "$expected" =~ ^[a-fA-F0-9]{64}$ ]] || die "missing or invalid checksum for $name in SHA256SUMS"
  actual=$(sha256sum "$file")
  [[ "${actual%% *}" == "$expected" ]] || die "checksum mismatch for $name"
}

fetch() {
  local name=$1
  say "download $name"
  download "$base/$name" "$cache/$name"
  verify "$cache/$name" "$cache/SHA256SUMS" "$name"
}

# act runs a command, or only prints it under --dry-run.
act() {
  if [[ $dry == 1 ]]; then say "would run: $*"; else "$@"; fi
}

package_hint() {
  local id="" like="" line
  if [[ -r /etc/os-release ]]; then
    id=$(. /etc/os-release; printf '%s' "${ID:-}")
    like=$(. /etc/os-release; printf '%s' "${ID_LIKE:-}")
  fi
  case " $id $like " in
    *" debian "*|*" ubuntu "*) line="sudo apt install $*" ;;
    *" fedora "*|*" rhel "*|*" centos "*) line="sudo dnf install $*" ;;
    *" arch "*) line="sudo pacman -S $*" ;;
    *" suse "*|*" opensuse "*) line="sudo zypper install $*" ;;
    *" alpine "*) line="sudo apk add $*" ;;
    *) line="install with your package manager: $*" ;;
  esac
  printf '%s' "$line"
}

check_prereqs() {
  local pkgs=() missing=()
  command -v bwrap >/dev/null || { missing+=("bwrap (sleeve sandbox)"); pkgs+=(bubblewrap); }
  command -v tmux >/dev/null || { missing+=("tmux (harness sessions)"); pkgs+=(tmux); }
  command -v git >/dev/null || { missing+=("git"); pkgs+=(git); }
  if ! command -v systemctl >/dev/null || ! systemctl --user show-environment >/dev/null 2>&1; then
    missing+=("a systemd user manager (systemctl --user must work in this login session)")
    pkgs+=(systemd)
  fi
  [[ ${#missing[@]} -eq 0 ]] && return 0
  warn "missing prerequisites:"
  printf '  %s\n' "${missing[@]}" >&2
  warn "$(package_hint "${pkgs[@]}")"
  warn "(on Debian and Ubuntu a user manager in ssh sessions also needs: dbus-user-session)"
  [[ $dry == 1 ]] || die "install them and run this again"
}

main() {
  local version=$DEFAULT_VERSION listen="" public_url="" phome="" base="" dry=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version|--listen|--public-url|--home|--base-url)
        [[ $# -ge 2 && -n "$2" ]] || die "missing value for $1"
        case "$1" in
          --version) version=$2 ;; --listen) listen=$2 ;; --public-url) public_url=$2 ;;
          --home) phome=$2 ;; --base-url) base=$2 ;;
        esac
        shift 2 ;;
      --dry-run) dry=1; shift ;;
      --help|-h)
        say "Usage: bash install-hub.sh [--version vX.Y.Z] [--listen ADDR] [--public-url URL] [--home DIR] [--dry-run]"
        say "Defaults: version $DEFAULT_VERSION, listen 127.0.0.1:7470, home ~/.protectorate."
        say "--dry-run downloads and verifies into a temporary directory, installs nothing."
        return 0 ;;
      *) die "unknown option: $1" ;;
    esac
  done
  [[ "$version" == v* ]] || die "version must be a tag such as $DEFAULT_VERSION"
  [[ -n "$base" ]] || base="https://github.com/$REPO/releases/download/$version"
  [[ -z "$phome" || "$phome" == /* ]] || die "--home must be an absolute path"

  [[ "$(uname -s)" == Linux ]] || die "the hub runs on Linux only (sleeves need bwrap)"
  local arch
  case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; *) die "only x86_64 and aarch64 are supported" ;; esac
  local c
  for c in curl tar sha256sum; do command -v "$c" >/dev/null || die "$c is required"; done
  check_prereqs

  if [[ -z "$listen" ]]; then
    listen=127.0.0.1:7470
    if [[ -t 2 && -r /dev/tty ]]; then
      local reply
      read -r -p "listen address [$listen]: " reply </dev/tty || true
      [[ -z "$reply" ]] || listen=$reply
      if [[ -z "$public_url" ]]; then
        read -r -p "public URL other machines use to reach the hub (empty to skip): " public_url </dev/tty || true
      fi
    fi
  fi

  local bin_dir="$HOME/.local/bin" data="$HOME/.local/share/protectorate" cache
  if [[ $dry == 1 ]]; then cache=$(mktemp -d "${TMPDIR:-/tmp}/protectorate-install.XXXXXXXX"); else cache="$data/downloads/$version"; fi
  mkdir -p -- "$cache"
  say "protectorate hub $version for linux-$arch"
  download "$base/SHA256SUMS" "$cache/SHA256SUMS"

  local bin_name="protectorate-linux-$arch" web_name="protectorate-web-$version.tar.gz"
  fetch "$bin_name"
  fetch "$web_name"
  local others=() t
  for t in linux-amd64 linux-arm64 windows-amd64.exe windows-arm64.exe android-arm64; do
    [[ "$t" == "linux-$arch" ]] && continue
    others+=("protectorate-$t")
  done
  for t in "${others[@]}"; do fetch "$t"; done

  local bin="$bin_dir/protectorate" web="$data/web" web_version="$data/web-$version"
  if [[ -e "$web" && ! -L "$web" ]]; then die "$web exists and is not a symlink; move it aside, the installer manages it"; fi
  act mkdir -p -- "$bin_dir" "$data/bin" "$web_version"
  act install -m 755 "$cache/$bin_name" "$bin"
  for t in "${others[@]}"; do act install -m 755 "$cache/$t" "$data/bin/$t"; done
  act tar -xzf "$cache/$web_name" -C "$web_version"
  act ln -sfn "web-$version" "$web"

  [[ -z "$phome" ]] || export PROTECTORATE_HOME=$phome
  if [[ $dry == 1 ]]; then
    say "would run: $bin hub tailnet install"
  else
    "$bin" hub tailnet install
  fi

  local unit="$HOME/.config/systemd/user/protectorate-hub.service" serve=(--listen "$listen" --web "$web")
  [[ -z "$public_url" ]] || serve+=(--public-url "$public_url")
  local svc=(service systemd hub --binary "$bin" --install)
  [[ -z "$phome" ]] || svc+=(--home "$phome")
  if [[ -e "$unit" ]]; then
    say "unit $unit exists and is left as it is (edit it to change flags); restarting the hub on the new binary"
    act systemctl --user daemon-reload
    if systemctl --user is-active --quiet protectorate-hub 2>/dev/null; then act systemctl --user restart protectorate-hub; else act systemctl --user enable --now protectorate-hub; fi
  else
    if [[ $dry == 1 ]]; then say "would run: $bin ${svc[*]} -- ${serve[*]}"; else "$bin" "${svc[@]}" -- "${serve[@]}"; fi
    act systemctl --user daemon-reload
    act systemctl --user enable --now protectorate-hub
  fi

  local url=${public_url:-http://$listen}
  say ""
  say "Hub $version installed. Open $url"
  case ":$PATH:" in *":$bin_dir:"*) ;; *) say "Add $bin_dir to PATH to use the protectorate command." ;; esac
  say ""
  say "Next:"
  say "  protectorate hub tailnet init --server-url http://LAN-IP:7480   # or an https URL; then: systemctl --user restart protectorate-hub"
  say "  protectorate login claude                                         # and codex, grok, pi: once per harness"
  say "  cat ${phome:-$HOME/.protectorate}/token                          # owner token for the app"
  if [[ "$(loginctl show-user "$(id -u)" -p Linger 2>/dev/null || true)" == Linger=no ]]; then
    say "  sudo loginctl enable-linger \$USER                                # keep the hub running when logged out"
  fi
  if [[ "$listen" == 127.0.0.1:* || "$listen" == localhost:* ]]; then
    say "The hub listens on loopback only; other machines need --listen 0.0.0.0:7470 (and --public-url) in the unit's ExecStart; a new unit takes them from --listen and --public-url on a first install."
  fi
  say "Downloads are kept in $cache; delete them yourself if you want the space."
  return 0
}

if [[ -z "${BASH_SOURCE[0]:-}" || "${BASH_SOURCE[0]:-}" == "$0" ]]; then main "$@"; fi
