#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
binary_dir=${PROTECTORATE_TEST_RUST_BIN_DIR:-$root/target/debug}
temp=$(mktemp -d)
trap 'printf "Installed Rust bundle fixture retained at %s\n" "$temp"' EXIT
export HOME="$temp/home" TMPDIR="$temp/staging"
mkdir -p "$HOME" "$TMPDIR" "$temp/bundle" "$temp/mock-bin" "$temp/web"
cat > "$temp/no-delete.bash" <<'EOF'
rm() { :; }
EOF
export BASH_ENV="$temp/no-delete.bash"
cat > "$temp/mock-bin/systemctl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$HOME/systemctl.log"
EOF
cat > "$temp/mock-bin/systemd-run" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$temp/mock-bin/systemctl" "$temp/mock-bin/systemd-run"
export PATH="$temp/mock-bin:$PATH"
case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; *) echo 'unsupported test architecture' >&2; exit 1 ;; esac
source_bytes=0
for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
  [[ -x "$binary_dir/$component" ]] || { echo "missing Rust binary: $binary_dir/$component" >&2; exit 1; }
  source_bytes=$((source_bytes + $(stat -c %s "$binary_dir/$component")))
done
available_bytes=$(df -PB1 "$temp" | awk 'NR == 2 { print $4 }')
required_bytes=$((source_bytes * 3 + 64 * 1024 * 1024))
if (( available_bytes < required_bytes )); then
  printf 'fixture needs %s free bytes at %s; found %s (set TMPDIR to a larger filesystem)\n' "$required_bytes" "$temp" "$available_bytes" >&2
  exit 1
fi
version=$("$binary_dir/protectorate-rs" version --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')
printf '%s\n' "$version" > "$temp/bundle/VERSION"
printf '%s\n' '<html>installed Rust hub</html>' > "$temp/web/index.html"
tar -czf "$temp/bundle/protectorate-web-$version.tar.gz" -C "$temp/web" .
for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
  cp --reflink=auto -- "$binary_dir/$component" "$temp/bundle/$component-linux-$arch"
done
(cd "$temp/bundle" && sha256sum VERSION *-linux-"$arch" "protectorate-web-$version.tar.gz" > SHA256SUMS)
bash "$root/install-hub.sh" --bundle-dir "$temp/bundle" --home "$HOME/state" --release-dir "$HOME/releases" --no-start > "$temp/install.log"
unit="$HOME/.config/systemd/user/protectorate-hub.service"
[[ "$(readlink "$HOME/releases/current")" == "$version" ]]
[[ "$(stat -c %a "$HOME/state/token")" == 600 ]]
for component in protectorate-rs protectorate-sleeve-runner protectorate-sleeve-sandbox-helper protectorate protectorate-mcpbridge; do
  cmp -s "$binary_dir/$component" "$HOME/releases/$version/$component"
done
systemd-analyze verify "$unit" > "$temp/systemd-verify.log" 2>&1
python3 - "$unit" "$HOME/state/token" "$version" <<'PY'
import json, os, pathlib, shlex, signal, socket, subprocess, sys, time, urllib.request

unit, token_path, expected = sys.argv[1:]
line = next(line for line in pathlib.Path(unit).read_text().splitlines() if line.startswith('ExecStart='))
command = shlex.split(line.removeprefix('ExecStart='))
state = pathlib.Path(token_path).parent
sock = socket.socket()
sock.bind(('127.0.0.1', 0))
port = sock.getsockname()[1]
sock.close()
command[command.index('--listen') + 1] = f'127.0.0.1:{port}'
process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
def request_for(procedure, body):
    return urllib.request.Request(
        f'http://127.0.0.1:{port}/protectorate.v1.{procedure}',
        data=json.dumps(body).encode(),
        headers={'Authorization': f'Bearer {token}', 'Content-Type': 'application/json'},
        method='POST',
    )

try:
    token = pathlib.Path(token_path).read_text().strip()
    request = request_for('HubService/GetStatus', {})
    deadline = time.monotonic() + 120
    while True:
        if process.poll() is not None:
            raise AssertionError(f'installed hub exited {process.returncode}: {process.stderr.read().decode()}')
        try:
            with urllib.request.urlopen(request, timeout=1) as response:
                status = json.load(response)
            assert status['version'] == expected, status
            break
        except (OSError, TimeoutError):
            if time.monotonic() >= deadline:
                raise
            time.sleep(.1)
    with urllib.request.urlopen(request_for('BacklogService/CreateNote',
        {'title': 'installed restore fixture'}), timeout=5) as response:
        note_id = json.load(response)['note']['id']
    archives = state.parent / 'archives'
    backup = subprocess.run([command[0], 'hub', 'backup', '--home', str(state),
        '--to', str(archives)], check=True, capture_output=True, text=True)
    archive = pathlib.Path(backup.stdout.splitlines()[0])
    assert archive.is_file(), archive
finally:
    process.send_signal(signal.SIGTERM)
    process.communicate(timeout=10)
assert process.returncode == 0, process.returncode
restored = state.parent / 'restored-state'
subprocess.run([command[0], 'hub', 'restore', str(archive), '--home', str(restored)],
    check=True, capture_output=True, text=True)
restored_token = state.parent / 'restored-owner-token'
restored_token.write_bytes(pathlib.Path(token_path).read_bytes())
restored_token.chmod(0o600)
recovered_command = command.copy()
recovered_command[recovered_command.index('--home') + 1] = str(restored)
recovered_command[recovered_command.index('--owner-token-file') + 1] = str(restored_token)
recovered = subprocess.Popen(recovered_command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    deadline = time.monotonic() + 120
    while True:
        if recovered.poll() is not None:
            raise AssertionError(f'restored hub exited {recovered.returncode}: {recovered.stderr.read().decode()}')
        try:
            with urllib.request.urlopen(request, timeout=1) as response:
                status = json.load(response)
            if status['version'] == expected:
                break
        except (OSError, TimeoutError):
            pass
        if time.monotonic() >= deadline:
            raise AssertionError(f'restored hub did not serve version {expected}')
        time.sleep(.1)
    with urllib.request.urlopen(request_for('BacklogService/ListNotes', {}), timeout=5) as response:
        notes = json.load(response)['notes']
    assert any(note['id'] == note_id and note['title'] == 'installed restore fixture'
               for note in notes), notes
finally:
    recovered.send_signal(signal.SIGTERM)
    recovered.communicate(timeout=10)
assert recovered.returncode == 0, recovered.returncode
print(f'Installed Rust hub {expected} restored a live backup and served its note from the generated unit')

if os.environ.get('PROTECTORATE_TEST_SYSTEMD_RESTORE') == '1':
    name = f'protectorate-restore-{os.getpid()}-{time.time_ns()}.service'
    lines = pathlib.Path(unit).read_text().splitlines()
    properties = []
    for key in ('Restart', 'RestartSec', 'KillMode', 'TimeoutStopSec'):
        value = next(line.split('=', 1)[1] for line in lines if line.startswith(f'{key}='))
        properties.append(f'--property={key}={value}')
    launch = subprocess.run([
        '/usr/bin/systemd-run', '--user', f'--unit={name}', '--collect',
        f'--setenv=HOME={state.parent}', *properties, *recovered_command,
    ], capture_output=True, text=True)
    assert launch.returncode == 0, f'systemd-run failed: {launch.stderr}'

    def service_property(key):
        result = subprocess.run([
            '/usr/bin/systemctl', '--user', 'show', f'--property={key}', '--value', name,
        ], check=True, capture_output=True, text=True)
        return result.stdout.strip()

    def wait_manager_ready(previous_pid=None):
        deadline = time.monotonic() + 150
        while time.monotonic() < deadline:
            pid = int(service_property('MainPID') or '0')
            if pid and pid != previous_pid:
                try:
                    with urllib.request.urlopen(request, timeout=1) as response:
                        status = json.load(response)
                    if status['version'] == expected:
                        return pid
                except (OSError, TimeoutError):
                    pass
            time.sleep(.1)
        raise AssertionError(f'systemd manager did not serve restored hub; state={service_property("ActiveState")}')

    try:
        first_pid = wait_manager_ready()
        assert first_pid > 0
        assert int(service_property('NRestarts')) == 0
        with urllib.request.urlopen(request_for('BacklogService/ListNotes', {}), timeout=5) as response:
            notes = json.load(response)['notes']
        assert any(note['id'] == note_id and note['title'] == 'installed restore fixture'
                   for note in notes), notes
        os.kill(first_pid, signal.SIGKILL)
        restarted_pid = wait_manager_ready(first_pid)
        assert restarted_pid != first_pid
        assert int(service_property('NRestarts')) >= 1
        with urllib.request.urlopen(request_for('BacklogService/ListNotes', {}), timeout=5) as response:
            notes = json.load(response)['notes']
        assert any(note['id'] == note_id and note['title'] == 'installed restore fixture'
                   for note in notes), notes
        print(f'Real systemd user manager restarted restored Rust hub {first_pid} -> {restarted_pid} and retained note {note_id}')
    finally:
        subprocess.run(['/usr/bin/systemctl', '--user', 'stop', name], check=False,
                       capture_output=True, text=True)
PY
