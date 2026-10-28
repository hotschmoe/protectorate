#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
temp=$(mktemp -d)
trap 'printf "Rust installer fixture retained at %s\n" "$temp"' EXIT
mkdir -p "$temp/staging"
cat > "$temp/no-delete.bash" <<'EOF'
rm() { :; }
EOF
export BASH_ENV="$temp/no-delete.bash" TMPDIR="$temp/staging"
export HOME="$temp/home"
mkdir -p "$HOME" "$temp/bundle" "$temp/mock-bin"
case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; *) echo 'unsupported test architecture' >&2; exit 1 ;; esac
cat > "$temp/mock-bin/systemctl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$HOME/systemctl.log"
if [[ "$*" == *' is-active '* && -f "$HOME/systemctl-inactive" ]]; then
  exit 3
fi
if [[ "$*" == *' is-enabled '* && ! -f "$HOME/systemctl-enabled" ]]; then
  exit 1
fi
if [[ "$*" == *' enable --now '* ]]; then
  printf '%s\n' enabled > "$HOME/systemctl-enabled"
fi
if [[ "$*" == *' disable '* && -f "$HOME/systemctl-enabled" ]]; then
  mv -- "$HOME/systemctl-enabled" "$HOME/systemctl-disabled"
fi
if [[ "$*" == '--user daemon-reload' && -f "$HOME/systemctl-reload-fail-once" ]]; then
  mv -- "$HOME/systemctl-reload-fail-once" "$HOME/systemctl-reload-failure-consumed"
  exit 1
fi
if [[ ( "$*" == *' restart '* || "$*" == *' enable --now '* ) && -f "$HOME/systemctl-fail-once" ]]; then
  mv -- "$HOME/systemctl-fail-once" "$HOME/systemctl-failure-consumed"
  exit 1
fi
EOF
cat > "$temp/mock-bin/systemd-run" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$temp/mock-bin/"*
export PATH="$temp/mock-bin:$PATH"
printf '%s\n' v9.8.7 > "$temp/bundle/VERSION"
mkdir -p "$temp/web"
printf '%s\n' '<html>Rust bundle</html>' > "$temp/web/index.html"
tar -czf "$temp/bundle/protectorate-web-v9.8.7.tar.gz" -C "$temp/web" .
for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
  name="$component-linux-$arch"
  if [[ "$component" == protectorate-rs ]]; then
    printf '#!/usr/bin/env bash\nprintf '\''{"version":"v9.8.7"}\\n'\''\n' > "$temp/bundle/$name"
  else
    printf '#!/usr/bin/env bash\nexit 0\n' > "$temp/bundle/$name"
  fi
  chmod +x "$temp/bundle/$name"
done
(cd "$temp/bundle" && sha256sum VERSION *-linux-"$arch" protectorate-web-v9.8.7.tar.gz > SHA256SUMS)

bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/hub" --release-dir "$HOME/hub-releases" --dry-run > "$temp/dry.log"
[[ ! -e "$HOME/hub-releases" && ! -e "$HOME/.config/systemd/user/protectorate-hub.service" ]]
bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/hub" --release-dir "$HOME/hub-releases" \
  --trusted-proxy 127.0.0.2 --trusted-proxy 127.0.0.3 \
  --local-llm-url http://127.0.0.1:1234 --local-llm-url http://127.0.0.1:5678 \
  --local-model-host model.test --local-model-host backup.test --dead-retention 0 \
  --tls-listen 192.168.22.5:7470 --tls-name hub.lan --backup-dir "$HOME/hub-backups" --no-start > "$temp/hub.log"
[[ -x "$HOME/hub-releases/v9.8.7/protectorate-rs" ]]
[[ -x "$HOME/hub-releases/v9.8.7/protectorate-sleeve-runner" ]]
[[ -x "$HOME/hub-releases/v9.8.7/protectorate-mcpbridge" ]]
[[ -f "$HOME/hub-releases/v9.8.7/web/index.html" ]]
grep -Fq '"version":"v9.8.7"' "$HOME/hub-releases/v9.8.7/release.json"
[[ "$(readlink "$HOME/hub-releases/current")" == v9.8.7 ]]
[[ "$(stat -c %a "$HOME/hub/token")" == 600 ]]
grep -Fq -- '--owner-token-file' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--listen "127.0.0.1:7470"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--tls-listen "192.168.22.5:7470"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--tls-name "hub.lan"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq 'Hub CA:' "$temp/hub.log"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --listen 192.168.22.5:7470 --dry-run > "$temp/hub-plain-lan.log" 2>&1; then
  echo 'hub accepted a plain HTTP LAN listener' >&2
  exit 1
fi
grep -Fq -- 'must be loopback' "$temp/hub-plain-lan.log"
grep -Fq -- '--dead-retention "0"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- "--backup-dir \"$HOME/hub-backups\"" "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--trusted-proxy "127.0.0.2"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--trusted-proxy "127.0.0.3"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--local-llm-url "http://127.0.0.1:1234"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--local-llm-url "http://127.0.0.1:5678"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--local-model-host "model.test"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--local-model-host "backup.test"' "$HOME/.config/systemd/user/protectorate-hub.service"
! grep -Eq 'bwrap|protectorate-linux-' "$HOME/.config/systemd/user/protectorate-hub.service"
bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/hub" --release-dir "$HOME/hub-releases" --no-start > "$temp/hub-repeat.log"
grep -Fq 'Reusing verified release' "$temp/hub-repeat.log"
ln -s v9.8.6 "$HOME/hub-releases/previous"

cp -a -- "$temp/bundle" "$temp/bundle-next"
printf '%s\n' v9.8.8 > "$temp/bundle-next/VERSION"
printf '#!/usr/bin/env bash\nprintf '\''{"version":"v9.8.8"}\\n'\''\n' > "$temp/bundle-next/protectorate-rs-linux-$arch"
mv -- "$temp/bundle-next/protectorate-web-v9.8.7.tar.gz" "$temp/bundle-next/protectorate-web-v9.8.8.tar.gz"
(cd "$temp/bundle-next" && sha256sum VERSION *-linux-"$arch" protectorate-web-v9.8.8.tar.gz > SHA256SUMS)
original_unit=$(sha256sum "$HOME/.config/systemd/user/protectorate-hub.service")
printf '%s\n' fail > "$HOME/systemctl-fail-once"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle-next" --home "$HOME/hub" --release-dir "$HOME/hub-releases" > "$temp/hub-restart-fail.log" 2>&1; then
  echo 'failed hub restart was accepted' >&2
  exit 1
fi
grep -Fq 'restored the prior release and service' "$temp/hub-restart-fail.log"
[[ "$(readlink "$HOME/hub-releases/current")" == v9.8.7 ]]
[[ "$(readlink "$HOME/hub-releases/previous")" == v9.8.6 ]]
[[ "$original_unit" == "$(sha256sum "$HOME/.config/systemd/user/protectorate-hub.service")" ]]
[[ -f "$HOME/systemctl-failure-consumed" ]]
bash "$root/install-hub.sh" --bundle-dir "$temp/bundle-next" --home "$HOME/hub" --release-dir "$HOME/hub-releases" > "$temp/hub-restart-success.log"
[[ "$(readlink "$HOME/hub-releases/current")" == v9.8.8 ]]
[[ "$(readlink "$HOME/hub-releases/previous")" == v9.8.7 ]]
grep -Fq 'Installed Rust hub v9.8.8' "$temp/hub-restart-success.log"

reload_unit=$(sha256sum "$HOME/.config/systemd/user/protectorate-hub.service")
reload_calls_before=$(wc -l < "$HOME/systemctl.log")
printf '%s\n' fail > "$HOME/systemctl-reload-fail-once"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/hub" --release-dir "$HOME/hub-releases" > "$temp/hub-reload-fail.log" 2>&1; then
  echo 'failed hub daemon reload was accepted' >&2
  exit 1
fi
grep -Fq 'daemon reload failed; restored the prior release and service' "$temp/hub-reload-fail.log"
[[ "$(readlink "$HOME/hub-releases/current")" == v9.8.8 ]]
[[ "$(readlink "$HOME/hub-releases/previous")" == v9.8.7 ]]
[[ "$reload_unit" == "$(sha256sum "$HOME/.config/systemd/user/protectorate-hub.service")" ]]
python3 - "$HOME/systemctl.log" "$reload_calls_before" <<'PY'
import pathlib, sys
calls = pathlib.Path(sys.argv[1]).read_text().splitlines()[int(sys.argv[2]):]
assert calls.count('--user daemon-reload') == 2, calls
assert not any(' restart ' in call or ' enable --now ' in call for call in calls), calls
PY

mkdir -p "$temp/reload-first-home"
printf '%s\n' inactive > "$temp/reload-first-home/systemctl-inactive"
printf '%s\n' fail > "$temp/reload-first-home/systemctl-reload-fail-once"
if HOME="$temp/reload-first-home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle-next" --home "$temp/reload-first-home/state" --release-dir "$temp/reload-first-home/releases" --no-start > "$temp/hub-first-reload-fail.log" 2>&1; then
  echo 'failed first hub daemon reload was accepted' >&2
  exit 1
fi
grep -Fq 'daemon reload failed; restored the prior release and service' "$temp/hub-first-reload-fail.log"
[[ ! -e "$temp/reload-first-home/releases/current" ]]
[[ ! -e "$temp/reload-first-home/.config/systemd/user/protectorate-hub.service" ]]
[[ ! -e "$temp/reload-first-home/systemctl-enabled" ]]

mkdir -p "$temp/first-fail-home"
printf '%s\n' fail > "$temp/first-fail-home/systemctl-fail-once"
printf '%s\n' inactive > "$temp/first-fail-home/systemctl-inactive"
if HOME="$temp/first-fail-home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle-next" --home "$temp/first-fail-home/state" --release-dir "$temp/first-fail-home/releases" > "$temp/hub-first-fail.log" 2>&1; then
  echo 'failed first hub start was accepted' >&2
  exit 1
fi
grep -Fq 'restored the prior release and service' "$temp/hub-first-fail.log"
grep -Fq -- '--user enable --now protectorate-hub.service' "$temp/first-fail-home/systemctl.log"
grep -Fq -- '--user disable protectorate-hub.service' "$temp/first-fail-home/systemctl.log"
[[ -f "$temp/first-fail-home/systemctl-disabled" && ! -e "$temp/first-fail-home/systemctl-enabled" ]]
[[ ! -e "$temp/first-fail-home/releases/current" ]]
[[ ! -e "$temp/first-fail-home/.config/systemd/user/protectorate-hub.service" ]]
python3 - "$temp/first-fail-home/systemctl.log" <<'PY'
import pathlib, sys
calls = pathlib.Path(sys.argv[1]).read_text().splitlines()
enable = calls.index('--user enable --now protectorate-hub.service')
disable = calls.index('--user disable protectorate-hub.service')
assert enable < disable < calls.index('--user daemon-reload', disable)
PY

HOME="$temp/first-fail-home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle-next" --home "$temp/first-fail-home/state" --release-dir "$temp/first-fail-home/releases" --no-start > "$temp/hub-inactive-stage.log"
printf '%s\n' enabled > "$temp/first-fail-home/systemctl-enabled"
printf '%s\n' fail > "$temp/first-fail-home/systemctl-fail-once"
inactive_unit=$(sha256sum "$temp/first-fail-home/.config/systemd/user/protectorate-hub.service")
calls_before=$(wc -l < "$temp/first-fail-home/systemctl.log")
if HOME="$temp/first-fail-home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$temp/first-fail-home/state" --release-dir "$temp/first-fail-home/releases" > "$temp/hub-inactive-fail.log" 2>&1; then
  echo 'failed inactive hub update was accepted' >&2
  exit 1
fi
grep -Fq 'restored the prior release and service' "$temp/hub-inactive-fail.log"
[[ -f "$temp/first-fail-home/systemctl-enabled" ]]
[[ "$(readlink "$temp/first-fail-home/releases/current")" == v9.8.8 ]]
[[ "$inactive_unit" == "$(sha256sum "$temp/first-fail-home/.config/systemd/user/protectorate-hub.service")" ]]
python3 - "$temp/first-fail-home/systemctl.log" "$calls_before" <<'PY'
import pathlib, sys
calls = pathlib.Path(sys.argv[1]).read_text().splitlines()[int(sys.argv[2]):]
assert '--user is-enabled --quiet protectorate-hub.service' in calls
assert '--user enable --now protectorate-hub.service' in calls
assert '--user disable protectorate-hub.service' not in calls
PY

printf '%s\n' 'changed' > "$HOME/hub-releases/v9.8.7/protectorate-web-v9.8.7.tar.gz"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/hub" --release-dir "$HOME/hub-releases" --no-start > "$temp/hub-tamper.log" 2>&1; then
  echo 'existing changed web archive was accepted' >&2
  exit 1
fi
grep -Fq 'existing web archive differs' "$temp/hub-tamper.log"

bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --no-start > "$temp/default.log"
[[ -f "$HOME/.protectorate/token" ]]
[[ "$(readlink "$HOME/.local/share/protectorate/releases/current")" == v9.8.7 ]]
printf '%s\n' '[Service]' 'ExecStart=/bin/true' > "$HOME/.config/systemd/user/protectorate-hub.service"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --no-start > "$temp/unmanaged.log" 2>&1; then
  echo 'unmanaged unit was overwritten' >&2
  exit 1
fi
grep -Fq 'existing unmanaged service unit' "$temp/unmanaged.log"
grep -Fq 'ExecStart=/bin/true' "$HOME/.config/systemd/user/protectorate-hub.service"

printf '%s\n' altered > "$temp/bundle/protectorate-linux-$arch"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/fail" --release-dir "$HOME/fail-releases" --dry-run > "$temp/fail.log" 2>&1; then
  echo 'tampered bundle was accepted' >&2
  exit 1
fi
grep -Fq 'checksum mismatch' "$temp/fail.log"
[[ ! -e "$HOME/fail-releases" ]]
printf '%s\n' 'not a web archive' > "$temp/bundle/protectorate-web-v9.8.7.tar.gz"
(cd "$temp/bundle" && sha256sum VERSION *-linux-"$arch" protectorate-web-v9.8.7.tar.gz > SHA256SUMS)
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/fail" --release-dir "$HOME/fail-releases" --dry-run > "$temp/fail-web.log" 2>&1; then
  echo 'invalid web archive was accepted' >&2
  exit 1
fi
[[ ! -e "$HOME/fail-releases" ]]
printf 'Rust installer staging and checksum checks passed\n'
