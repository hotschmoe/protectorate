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
if [[ "$*" == '--user show protectorate-hub.service -p FragmentPath --value' ]]; then
  printf '%s\n' "$HOME/.config/systemd/user/protectorate-hub.service"
  exit 0
fi
if [[ "$*" == '--user show protectorate-hub.service -p DropInPaths --value' ]]; then
  exit 0
fi
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
for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate-node-sandbox-helper protectorate-cli protectorate-mcpbridge; do
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
  --public-url https://example.test/ --trusted-proxy 127.0.0.2 --trusted-proxy 127.0.0.3 \
  --local-llm-url http://127.0.0.1:1234 --local-llm-url http://127.0.0.1:5678 \
  --local-model-host model.test --local-model-host backup.test --dead-retention 0 \
  --no-start > "$temp/hub.log"
[[ -x "$HOME/hub-releases/v9.8.7/protectorate-rs" ]]
[[ -x "$HOME/hub-releases/v9.8.7/protectorate-sleeve-runner" ]]
[[ -x "$HOME/hub-releases/v9.8.7/protectorate-mcpbridge" ]]
[[ -f "$HOME/hub-releases/v9.8.7/web/index.html" ]]
grep -Fq '"version":"v9.8.7"' "$HOME/hub-releases/v9.8.7/release.json"
[[ "$(readlink "$HOME/hub-releases/current")" == v9.8.7 ]]
[[ "$(stat -c %a "$HOME/hub/token")" == 600 ]]
grep -Fq -- '--owner-token-file' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--public-url "https://example.test/"' "$HOME/.config/systemd/user/protectorate-hub.service"
grep -Fq -- '--dead-retention "0"' "$HOME/.config/systemd/user/protectorate-hub.service"
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

bash "$root/install-node.sh" --bundle-dir "$temp/bundle" --home "$HOME/node" --release-dir "$HOME/node-releases" --no-start > "$temp/node.log"
[[ -x "$HOME/node-releases/v9.8.7/protectorate-node-sandbox-helper" ]]
grep -Fq -- 'node serve' "$HOME/.config/systemd/user/protectorate-node.service"
grep -Fq -- '--agent-token-file' "$HOME/.config/systemd/user/protectorate-node.service"
! grep -Fq -- '--allow-plain-jobs' "$HOME/.config/systemd/user/protectorate-node.service"
bash "$root/install-node.sh" --bundle-dir "$temp/bundle" --home "$HOME/node" --release-dir "$HOME/node-releases" --allow-plain-jobs --no-start > "$temp/node-care.log"
grep -Fq -- '--allow-plain-jobs' "$HOME/.config/systemd/user/protectorate-node.service"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --allow-plain-jobs --dry-run > "$temp/hub-care.log" 2>&1; then
  echo 'hub accepted node-only plain job option' >&2
  exit 1
fi
grep -Fq -- '--allow-plain-jobs requires --role node' "$temp/hub-care.log"

bash "$root/install-node.sh" --bundle-dir "$temp/bundle" --home "$HOME/shared-node" --release-dir "$HOME/shared-releases" --no-start > "$temp/shared-node.log"
[[ ! -e "$HOME/shared-releases/v9.8.7/web" ]]
original_binary=$(sha256sum "$HOME/shared-releases/v9.8.7/protectorate-rs-linux-$arch")
bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/shared-hub" --release-dir "$HOME/shared-releases" --no-start > "$temp/shared-hub.log"
grep -Fq 'Added verified web archive to node-first release' "$temp/shared-hub.log"
[[ -f "$HOME/shared-releases/v9.8.7/web/index.html" ]]
[[ "$original_binary" == "$(sha256sum "$HOME/shared-releases/v9.8.7/protectorate-rs-linux-$arch")" ]]
[[ "$(readlink "$HOME/shared-releases/current")" == v9.8.7 ]]
printf '%s\n' 'changed' > "$HOME/shared-releases/v9.8.7/protectorate-web-v9.8.7.tar.gz"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/shared-hub" --release-dir "$HOME/shared-releases" --no-start > "$temp/shared-tamper.log" 2>&1; then
  echo 'existing changed web archive was accepted' >&2
  exit 1
fi
grep -Fq 'existing web archive differs' "$temp/shared-tamper.log"

mkdir -p "$HOME/.protectorate"
printf '%s\n' 'Go state stays here' > "$HOME/.protectorate/token"
bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --no-start > "$temp/default.log"
[[ -f "$HOME/.protectorate-rust/token" ]]
[[ "$(cat "$HOME/.protectorate/token")" == 'Go state stays here' ]]
[[ "$(readlink "$HOME/.local/share/protectorate-rust/releases/current")" == v9.8.7 ]]
printf '%s\n' '[Service]' 'ExecStart=/bin/true' > "$HOME/.config/systemd/user/protectorate-hub.service"
if bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --no-start > "$temp/unmanaged.log" 2>&1; then
  echo 'unmanaged unit was overwritten' >&2
  exit 1
fi
grep -Fq 'existing unmanaged service unit' "$temp/unmanaged.log"
grep -Fq 'ExecStart=/bin/true' "$HOME/.config/systemd/user/protectorate-hub.service"

cut_home="$temp/cutover-home"
mkdir -p "$cut_home/.config/systemd/user"
go_unit="$cut_home/.config/systemd/user/protectorate-hub.service"
printf '%s\n' '[Service]' 'ExecStart=/usr/local/bin/protectorate hub serve --listen 127.0.0.1:7470' > "$go_unit"
go_sha=$(sha256sum "$go_unit")
go_sha=${go_sha%% *}
HOME="$cut_home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$cut_home/state" --release-dir "$cut_home/releases" --stage-for-cutover --no-start > "$temp/cutover-stage.log"
[[ "$(sha256sum "$go_unit" | cut -d ' ' -f 1)" == "$go_sha" ]]
[[ -f "$cut_home/.config/systemd/user/protectorate-rust-hub.service" ]]
if HOME="$cut_home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$cut_home/state" --release-dir "$cut_home/releases" --listen 127.0.0.1:7470 --adopt-go-unit --expect-go-unit-sha256 "$go_sha" > "$temp/cutover-active.log" 2>&1; then
  echo 'active Go unit was adopted' >&2
  exit 1
fi
grep -Fq 'stop the Go hub service before adoption' "$temp/cutover-active.log"
[[ "$(sha256sum "$go_unit" | cut -d ' ' -f 1)" == "$go_sha" ]]
printf '%s\n' inactive > "$cut_home/systemctl-inactive"
wrong_go_sha="0${go_sha:1}"
[[ "$wrong_go_sha" != "$go_sha" ]] || wrong_go_sha="1${go_sha:1}"
if HOME="$cut_home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$cut_home/state" --release-dir "$cut_home/releases" --listen 127.0.0.1:7470 --adopt-go-unit --expect-go-unit-sha256 "$wrong_go_sha" > "$temp/cutover-hash.log" 2>&1; then
  echo 'changed Go unit fingerprint was accepted' >&2
  exit 1
fi
grep -Fq 'existing Go unit fingerprint changed' "$temp/cutover-hash.log"
HOME="$cut_home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$cut_home/state" --release-dir "$cut_home/releases" --listen 127.0.0.1:7470 --adopt-go-unit --expect-go-unit-sha256 "$go_sha" > "$temp/cutover-adopt.log"
go_backup="$cut_home/.config/systemd/user/protectorate-hub.go-$go_sha.service.backup"
[[ "$(sha256sum "$go_backup" | cut -d ' ' -f 1)" == "$go_sha" ]]
grep -Fq '# Managed by Protectorate Rust installer' "$go_unit"
rust_sha=$(sha256sum "$go_unit")
rust_sha=${rust_sha%% *}
wrong_rust_sha="0${rust_sha:1}"
[[ "$wrong_rust_sha" != "$rust_sha" ]] || wrong_rust_sha="1${rust_sha:1}"
if HOME="$cut_home" bash "$root/install-hub.sh" --rollback-go-unit --expect-go-unit-sha256 "$go_sha" --expect-rust-unit-sha256 "$wrong_rust_sha" > "$temp/cutover-rollback-hash.log" 2>&1; then
  echo 'changed Rust unit fingerprint was accepted for rollback' >&2
  exit 1
fi
grep -Fq 'current Rust unit fingerprint changed' "$temp/cutover-rollback-hash.log"
[[ "$(sha256sum "$go_unit" | cut -d ' ' -f 1)" == "$rust_sha" ]]
HOME="$cut_home" bash "$root/install-hub.sh" --rollback-go-unit --expect-go-unit-sha256 "$go_sha" --expect-rust-unit-sha256 "$rust_sha" > "$temp/cutover-rollback.log"
[[ "$(sha256sum "$go_unit" | cut -d ' ' -f 1)" == "$go_sha" ]]
[[ "$(sha256sum "$go_backup" | cut -d ' ' -f 1)" == "$go_sha" ]]
[[ -f "$cut_home/.config/systemd/user/protectorate-hub.rust-before-rollback-$rust_sha.service.backup" ]]

cut_fail_home="$temp/cutover-fail-home"
mkdir -p "$cut_fail_home/.config/systemd/user"
cut_fail_unit="$cut_fail_home/.config/systemd/user/protectorate-hub.service"
printf '%s\n' '[Service]' 'ExecStart=/usr/local/bin/protectorate hub serve --listen 127.0.0.1:7470' > "$cut_fail_unit"
printf '%s\n' inactive > "$cut_fail_home/systemctl-inactive"
printf '%s\n' fail > "$cut_fail_home/systemctl-fail-once"
if HOME="$cut_fail_home" bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$cut_fail_home/state" --release-dir "$cut_fail_home/releases" --listen 127.0.0.1:7470 --adopt-go-unit --expect-go-unit-sha256 "$go_sha" > "$temp/cutover-start-fail.log" 2>&1; then
  echo 'failed Rust cutover start was accepted' >&2
  exit 1
fi
grep -Fq 'restored the prior release and service' "$temp/cutover-start-fail.log"
[[ "$(sha256sum "$cut_fail_unit" | cut -d ' ' -f 1)" == "$go_sha" ]]

printf '%s\n' altered > "$temp/bundle/protectorate-cli-linux-$arch"
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
