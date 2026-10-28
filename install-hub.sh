#!/usr/bin/env bash
# Per-user Linux hub installer for the checked Rust release bundle.
set -euo pipefail

die() { printf 'install: %s\n' "$*" >&2; exit 1; }
note() { printf '%s\n' "$*"; }

valid_version() {
  [[ "$1" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ && ${#1} -le 160 && "$1" != current && "$1" != previous ]]
}

regular_file() {
  [[ -f "$1" && ! -L "$1" ]] || die "expected a regular file: $1"
}

fetch() {
  local name=$1 destination=$2
  if [[ -n "$bundle_dir" ]]; then
    regular_file "$bundle_dir/$name"
    cp -- "$bundle_dir/$name" "$destination"
  else
    curl --fail --silent --show-error --location --retry 3 --proto '=https' --proto-redir '=https' --tlsv1.2 \
      "$base_url/$name" -o "$destination"
  fi
}

verify() {
  local name=$1 path=$2 expected actual count
  expected=$(awk -v name="$name" '$2 == name || $2 == "*" name { print $1 }' "$stage/SHA256SUMS")
  count=$(awk -v name="$name" '$2 == name || $2 == "*" name { n++ } END { print n+0 }' "$stage/SHA256SUMS")
  [[ "$count" == 1 && "$expected" =~ ^[a-fA-F0-9]{64}$ ]] || die "one valid SHA256SUMS entry is required for $name"
  actual=$(sha256sum "$path")
  [[ "${actual%% *}" == "$expected" ]] || die "checksum mismatch for $name"
}

systemd_quote() {
  local value=$1
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  value=${value//%/%%}
  value=${value//\$/\$\$}
  printf '"%s"' "$value"
}

require_path() {
  [[ "$2" == /* && "$2" != *$'\n'* && "$2" != *$'\r'* ]] || die "$1 requires an absolute path without newlines"
}

extract_web() {
  python3 - "$1" "$2" <<'PY'
import pathlib, sys, tarfile
destination = pathlib.Path(sys.argv[2])
with tarfile.open(sys.argv[1], 'r:gz') as tf:
    for member in tf:
        path = pathlib.PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
            raise SystemExit('unsafe web archive member: ' + member.name)
        target = destination.joinpath(*path.parts)
        if member.isdir():
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            source = tf.extractfile(member)
            if source is None:
                raise SystemExit('unreadable web archive member: ' + member.name)
            with target.open('xb') as output:
                while chunk := source.read(1024 * 1024):
                    output.write(chunk)
            target.chmod(0o644)
PY
}

main() {
  local channel=stable version='' repo=hotschmoe/protectorate base_url='' bundle_dir=''
  local release_root='' state_home='' listen='' token_file='' dead_retention='' backup_dir=''
  local -a tls_listens=() tls_names=() trusted_proxies=() local_llm_urls=() local_model_hosts=()
  local dry=0 no_start=0
  while (($#)); do
    case "$1" in
      --channel|--version|--release-repo|--base-url|--bundle-dir|--release-dir|--home|--listen|--tls-listen|--tls-name|--token-file|--trusted-proxy|--local-llm-url|--local-model-host|--dead-retention|--backup-dir)
        (($# >= 2)) && [[ -n "$2" ]] || die "missing value for $1"
        case "$1" in
          --channel) channel=$2 ;; --version) version=$2 ;;
          --release-repo) repo=$2 ;; --base-url) base_url=$2 ;; --bundle-dir) bundle_dir=$2 ;;
          --release-dir) release_root=$2 ;; --home) state_home=$2 ;; --listen) listen=$2 ;;
          --tls-listen) tls_listens+=("$2") ;; --tls-name) tls_names+=("$2") ;;
          --token-file) token_file=$2 ;; --trusted-proxy) trusted_proxies+=("$2") ;;
          --local-llm-url) local_llm_urls+=("$2") ;; --local-model-host) local_model_hosts+=("$2") ;;
          --dead-retention) dead_retention=$2 ;; --backup-dir) backup_dir=$2 ;;
        esac
        shift 2 ;;
      --dry-run) dry=1; shift ;; --no-start) no_start=1; shift ;;
      --help|-h)
        note "Usage: bash $(basename -- "$0") [--bundle-dir DIR | --version TAG] [--channel stable|dev] [--release-repo OWNER/REPO] [--base-url HTTPS_URL] [--release-dir DIR] [--home DIR] [--listen IP:PORT] [--token-file FILE] [--tls-listen IP:PORT] [--tls-name HOST] [--trusted-proxy IP] [--local-llm-url URL] [--local-model-host HOST] [--dead-retention 24h|0] [--backup-dir DIR] [--no-start] [--dry-run]"
        note "--dry-run verifies assets and writes nothing. --no-start stages the unit without starting it."
        note "--listen is the hub's loopback HTTP listener (default 127.0.0.1:7470); each --tls-listen is a private address served over HTTPS with the hub CA."
        return 0 ;;
      *) die "unknown option: $1" ;;
    esac
  done
  [[ -z "$dead_retention" || "$dead_retention" =~ ^(0|[0-9]+[smhd])$ ]] || die "--dead-retention requires a duration such as 24h or 0"
  local local_llm_url
  for local_llm_url in "${local_llm_urls[@]}"; do
    [[ "$local_llm_url" == https://* || "$local_llm_url" == http://* ]] || die "--local-llm-url must be an HTTP or HTTPS URL"
  done
  local hub_option
  for hub_option in "$listen" "${tls_listens[@]}" "${tls_names[@]}" "${trusted_proxies[@]}" "${local_llm_urls[@]}" "${local_model_hosts[@]}"; do
    [[ "$hub_option" != *$'\n'* && "$hub_option" != *$'\r'* ]] || die "hub options must not contain newlines"
  done
  [[ "$channel" == stable || "$channel" == dev ]] || die "--channel must be stable or dev"
  [[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "invalid --release-repo"
  [[ -z "$version" ]] || valid_version "$version" || die "invalid --version"
  [[ -z "$bundle_dir" || -z "$base_url" ]] || die "--bundle-dir and --base-url are exclusive"
  [[ -z "$bundle_dir" || -d "$bundle_dir" ]] || die "bundle directory does not exist"
  [[ -z "$base_url" || "$base_url" == https://* ]] || die "--base-url must use HTTPS"
  [[ "$(uname -s)" == Linux ]] || die "Linux is required"
  local arch
  case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; *) die "unsupported architecture" ;; esac
  for tool in sha256sum mktemp install systemctl; do command -v "$tool" >/dev/null || die "$tool is required"; done
  command -v openssl >/dev/null || die "openssl is required to create a token"
  command -v python3 >/dev/null || die "python3 is required to inspect the web archive"
  [[ -n "$bundle_dir" ]] || command -v curl >/dev/null || die "curl is required"
  local path
  for path in "$bundle_dir" "$release_root" "$state_home" "$token_file" "$backup_dir"; do
    [[ -z "$path" ]] || require_path path "$path"
  done
  listen=${listen:-127.0.0.1:7470}
  [[ "$listen" == 127.0.0.1:* || "$listen" == '[::1]:'* ]] || die "the hub --listen is plain HTTP and must be loopback; use --tls-listen for private addresses"
  state_home=${state_home:-$HOME/.protectorate}
  release_root=${release_root:-${XDG_DATA_HOME:-$HOME/.local/share}/protectorate/releases}
  require_path home "$state_home"; require_path release-dir "$release_root"
  token_file=${token_file:-$state_home/token}
  require_path token-file "$token_file"
  if [[ -z "$base_url" && -z "$bundle_dir" ]]; then
    if [[ -n "$version" ]]; then base_url="https://github.com/$repo/releases/download/$version"
    elif [[ "$channel" == dev ]]; then base_url="https://github.com/$repo/releases/download/dev"
    else base_url="https://github.com/$repo/releases/latest/download"; fi
  fi
  local stage
  stage=$(mktemp -d "${TMPDIR:-/tmp}/protectorate-install.XXXXXXXX")
  trap 'rm -rf -- "${INSTALL_STAGE:-}"' EXIT
  INSTALL_STAGE=$stage
  fetch SHA256SUMS "$stage/SHA256SUMS"
  fetch VERSION "$stage/VERSION"
  verify VERSION "$stage/VERSION"
  local build_version
  build_version=$(cat "$stage/VERSION")
  valid_version "$build_version" || die "invalid release VERSION"
  [[ "$build_version" != *-dirty ]] || die "dirty build refused"
  [[ -z "$version" || "$version" == dev || "$version" == "$build_version" ]] || die "VERSION differs from requested tag"
  local component name
  for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
    name="$component-linux-$arch"
    fetch "$name" "$stage/$name"
    verify "$name" "$stage/$name"
  done
  name="protectorate-web-$build_version.tar.gz"
  fetch "$name" "$stage/$name"
  verify "$name" "$stage/$name"
  # Check the complete archive before any installed files are written.
  python3 - "$stage/$name" <<'PY'
import pathlib, sys, tarfile
archive = pathlib.Path(sys.argv[1])
total = 0
with tarfile.open(archive, 'r:gz') as tf:
    for member in tf:
        path = pathlib.PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
            raise SystemExit('unsafe web archive member: ' + member.name)
        total += member.size
        if member.size > 32 * 1024 * 1024 or total > 256 * 1024 * 1024:
            raise SystemExit('web archive exceeds size limit')
PY
  local binary="$stage/protectorate-rs-linux-$arch"
  if [[ -z "${PROTECTORATE_INSTALL_SKIP_BINARY_CHECK:-}" ]]; then
    local reported
    reported=$("$binary" version --json) || die "Rust binary version check failed"
    [[ "$reported" == *'"version":"'"$build_version"'"'* ]] || die "Rust binary reports a different version"
  fi
  python3 - "$stage/release.json" "$build_version" "$channel" "$repo" "$version" <<'PY'
import json, pathlib, sys
path, build_version, channel, repo, requested = sys.argv[1:]
tag = requested or ('dev' if channel == 'dev' else build_version)
info = {'version': build_version, 'tag_name': tag, 'name': build_version,
        'channel': channel, 'prerelease': channel == 'dev',
        'html_url': f'https://github.com/{repo}/releases/tag/{tag}'}
pathlib.Path(path).write_text(json.dumps(info, separators=(',', ':')) + '\n')
PY
  note "Verified Rust $build_version bundle for linux-$arch"
  local unit_dir="$HOME/.config/systemd/user" service="protectorate-hub.service"
  local unit="$unit_dir/$service"
  if ((dry)); then note "Dry run: no release, token, unit, or service was changed"; return 0; fi
  systemctl --user show-environment >/dev/null 2>&1 || die "a working systemd user manager is required"
  command -v systemd-run >/dev/null || die "systemd-run is required for sleeve scopes"
  command -v git >/dev/null || die "git is required for workspaces"
  local install_dir="$release_root/$build_version" active="$release_root/current"
  if [[ -e "$install_dir" || -L "$install_dir" ]]; then
    [[ -d "$install_dir" && ! -L "$install_dir" ]] || die "release path is not a real directory: $install_dir"
    cmp -s "$stage/SHA256SUMS" "$install_dir/SHA256SUMS" || die "existing release has different checksums: $install_dir"
    for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
      cmp -s "$stage/$component-linux-$arch" "$install_dir/$component-linux-$arch" || die "existing release component differs: $component"
      [[ -x "$install_dir/$component" && ! -L "$install_dir/$component" ]] || die "existing release companion is missing: $component"
    done
    local installed_archive="$install_dir/protectorate-web-$build_version.tar.gz"
    regular_file "$installed_archive"
    cmp -s "$stage/protectorate-web-$build_version.tar.gz" "$installed_archive" || die "existing web archive differs"
    [[ -d "$install_dir/web" && ! -L "$install_dir/web" ]] || die "existing web directory is invalid"
    [[ -f "$install_dir/release.json" && ! -L "$install_dir/release.json" ]] || die "existing release metadata is missing"
    note "Reusing verified release $build_version"
  fi
  if [[ -e "$unit" ]]; then
    [[ ! -L "$unit" ]] || die "service unit is a symlink: $unit"
    grep -Fxq '# Managed by Protectorate Rust installer' "$unit" || die "existing unmanaged service unit: $unit"
  fi
  local pointer old_current='' old_previous=''
  for pointer in current previous; do
    if [[ -e "$release_root/$pointer" || -L "$release_root/$pointer" ]]; then
      [[ -L "$release_root/$pointer" ]] || die "$pointer release pointer is not a symlink"
      valid_version "$(readlink -- "$release_root/$pointer")" || die "$pointer release pointer has an invalid target"
      if [[ "$pointer" == current ]]; then
        old_current=$(readlink -- "$release_root/$pointer")
      else
        old_previous=$(readlink -- "$release_root/$pointer")
      fi
    fi
  done
  umask 077
  mkdir -p -- "$release_root" "$state_home" "$unit_dir"
  if [[ ! -e "$install_dir" ]]; then
    local prepared="$release_root/.install-$build_version-$$"
    mkdir -m 700 -- "$prepared"
    for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
      install -m 755 -- "$stage/$component-linux-$arch" "$prepared/$component-linux-$arch"
      ln -- "$prepared/$component-linux-$arch" "$prepared/$component"
    done
    install -m 644 -- "$stage/SHA256SUMS" "$prepared/SHA256SUMS"
    install -m 644 -- "$stage/VERSION" "$prepared/VERSION"
    install -m 644 -- "$stage/release.json" "$prepared/release.json"
    install -m 644 -- "$stage/protectorate-web-$build_version.tar.gz" "$prepared/protectorate-web-$build_version.tar.gz"
    mkdir -m 755 -- "$prepared/web"
    extract_web "$stage/protectorate-web-$build_version.tar.gz" "$prepared/web"
    mv -- "$prepared" "$install_dir"
  fi
  if [[ ! -e "$token_file" && ! -L "$token_file" ]]; then
    mkdir -p -- "$(dirname -- "$token_file")"
    local token_tmp="$token_file.install-$$"
    openssl rand -hex 32 > "$token_tmp"
    chmod 600 "$token_tmp"
    [[ ! -e "$token_file" ]] || die "token appeared while installing"
    mv -- "$token_tmp" "$token_file"
  else
    regular_file "$token_file"
    [[ -s "$token_file" ]] || die "token is empty"
  fi
  local unit_tmp="$unit.install-$$" unit_backup="$unit.rollback-$$" exec_path="$active/protectorate-rs"
  {
    note '# Managed by Protectorate Rust installer'
    note '[Unit]'
    note "Description=Protectorate Rust hub"
    note 'After=network-online.target'
    note 'Wants=network-online.target'
    note '[Service]'
    note 'Environment="PATH=%h/.local/bin:/usr/local/bin:/usr/bin:/bin"'
    printf 'ExecStart='; systemd_quote "$exec_path"; printf ' '
    printf 'serve --home '; systemd_quote "$state_home"
    printf ' --owner-token-file '; systemd_quote "$token_file"
    printf ' --listen '; systemd_quote "$listen"
    for hub_option in "${tls_listens[@]}"; do printf ' --tls-listen '; systemd_quote "$hub_option"; done
    for hub_option in "${tls_names[@]}"; do printf ' --tls-name '; systemd_quote "$hub_option"; done
    printf ' --release-repo '; systemd_quote "$repo"
    printf ' --update-channel '; systemd_quote "$channel"
    printf ' --release-dir '; systemd_quote "$release_root"
    if [[ -n "$dead_retention" ]]; then printf ' --dead-retention '; systemd_quote "$dead_retention"; fi
    if [[ -n "$backup_dir" ]]; then printf ' --backup-dir '; systemd_quote "$backup_dir"; fi
    for hub_option in "${trusted_proxies[@]}"; do printf ' --trusted-proxy '; systemd_quote "$hub_option"; done
    for hub_option in "${local_llm_urls[@]}"; do printf ' --local-llm-url '; systemd_quote "$hub_option"; done
    for hub_option in "${local_model_hosts[@]}"; do printf ' --local-model-host '; systemd_quote "$hub_option"; done
    printf '\nRestart=on-failure\nRestartSec=5\nKillMode=control-group\nTimeoutStopSec=45\n[Install]\nWantedBy=default.target\n'
  } > "$unit_tmp"
  chmod 600 "$unit_tmp"
  if [[ -e "$unit" ]]; then cp -p -- "$unit" "$unit_backup"; fi
  local was_active=0 was_enabled=0
  if systemctl --user is-active --quiet "$service"; then was_active=1; fi
  if systemctl --user is-enabled --quiet "$service"; then was_enabled=1; fi
  if [[ -L "$active" ]]; then
    local old
    old=$(readlink -- "$active")
    [[ "$old" == "$build_version" ]] || { ln -s "$old" "$release_root/.previous-install-$$"; mv -Tf -- "$release_root/.previous-install-$$" "$release_root/previous"; }
  elif [[ -e "$active" ]]; then
    die "current release pointer is not a symlink"
  fi
  ln -s "$build_version" "$release_root/.current-install-$$"
  mv -Tf -- "$release_root/.current-install-$$" "$active"
  mv -f -- "$unit_tmp" "$unit"
  local reload_failed=0 start_failed=0 rollback_failed=0 attempted_restart=0 attempted_enable=0
  systemctl --user daemon-reload || reload_failed=1
  if (( ! no_start && ! reload_failed )); then
    if (( was_active )); then
      attempted_restart=1
      systemctl --user restart "$service" || start_failed=1
    else
      attempted_enable=1
      systemctl --user enable --now "$service" || start_failed=1
    fi
  fi
  if (( reload_failed || start_failed )); then
    if (( attempted_enable )); then
      systemctl --user stop "$service" || rollback_failed=1
      if (( ! was_enabled )); then
        systemctl --user disable "$service" || rollback_failed=1
      fi
    fi
    if [[ -n "$old_current" ]]; then
      ln -s "$old_current" "$release_root/.current-rollback-$$"
      mv -Tf -- "$release_root/.current-rollback-$$" "$active" || rollback_failed=1
    else
      unlink -- "$active" || rollback_failed=1
    fi
    if [[ -n "$old_previous" ]]; then
      ln -s "$old_previous" "$release_root/.previous-rollback-$$"
      mv -Tf -- "$release_root/.previous-rollback-$$" "$release_root/previous" || rollback_failed=1
    elif [[ -L "$release_root/previous" ]]; then
      unlink -- "$release_root/previous" || rollback_failed=1
    fi
    if [[ -e "$unit_backup" ]]; then
      mv -f -- "$unit_backup" "$unit" || rollback_failed=1
    else
      unlink -- "$unit" || rollback_failed=1
    fi
    systemctl --user daemon-reload || rollback_failed=1
    if (( attempted_restart )); then
      systemctl --user restart "$service" || rollback_failed=1
    fi
    (( rollback_failed == 0 )) || die "service activation failed and rollback did not fully restore the prior service"
    if (( reload_failed )); then die "daemon reload failed; restored the prior release and service"; fi
    die "start failed; restored the prior release and service"
  fi
  if [[ -e "$unit_backup" ]]; then rm -f -- "$unit_backup"; fi
  note "Installed Rust hub $build_version at $active"
  note "Owner token: $token_file"
  note "Hub CA: $state_home/ca/ca.pem (created on first start)"
  (( ${#tls_listens[@]} )) || note "No --tls-listen given: the hub is reachable only on loopback until one is added"
  (( no_start == 0 )) || note "Service staged; start with systemctl --user enable --now $service"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then main "$@"; fi
