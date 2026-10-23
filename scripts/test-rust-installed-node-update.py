#!/usr/bin/env python3
"""Exercise node binary update and rollback in a disposable systemd user unit.

The fixture leaves its files for inspection and never addresses the installed
protectorate-node.service unit.
"""

import hashlib
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = pathlib.Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
DEBUG = (TARGET if TARGET.is_absolute() else ROOT / TARGET) / "debug"
FIRST = "0.1.0"
SECOND = "0.1.1"
FAILED = "0.1.2"
METHOD = "http://127.0.0.1:{port}/protectorate.v1.NodeAgentService/{method}"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_until(label, predicate, seconds=90):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.2)
    raise AssertionError(f"timed out waiting for {label}")


def build_server(version, destination):
    supplied = os.environ.get("PROTECTORATE_TEST_" + ("FIRST" if version == FIRST else "SECOND") + "_BINARY")
    if supplied:
        shutil.copy2(supplied, destination)
    else:
        env = dict(os.environ, PROTECTORATE_BUILD_VERSION=version)
        run("cargo", "build", "-p", "protectorate-server", "--bin", "protectorate-rs",
            cwd=ROOT, env=env, stdout=subprocess.DEVNULL)
        shutil.copy2(DEBUG / "protectorate-rs", destination)
    if shutil.which("strip"):
        run("strip", "--strip-debug", destination, stdout=subprocess.DEVNULL)
    reported = run(destination, "version", "--json", capture_output=True)
    assert json.loads(reported.stdout)["version"] == version, reported.stdout


def call(port, token, method, body, timeout=5):
    request = urllib.request.Request(
        METHOD.format(port=port, method=method),
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + token},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def metrics(port, token):
    try:
        status, body = call(port, token, "GetMetrics", {})
        return body.get("agentVersion") if status == 200 else None
    except (ConnectionError, TimeoutError, urllib.error.URLError):
        return None


def apply(port, token, staged, filename, version):
    body = {
        "path": filename,
        "size": str(staged.stat().st_size),
        "sha256": hashlib.sha256(staged.read_bytes()).hexdigest(),
        "expectedVersion": version,
    }
    status, reply = call(port, token, "ApplyUpdate", body, timeout=90)
    assert status == 200, (status, reply)
    assert reply["version"] == version, reply


def marker(path, phase):
    try:
        return json.loads(path.read_text()).get("phase") == phase
    except FileNotFoundError:
        return False


def main():
    if sys.platform != "linux" or not shutil.which("systemd-run"):
        print("skipping: systemd user manager is required")
        return
    if subprocess.run(("systemctl", "--user", "show-environment"),
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        print("skipping: systemd user manager is unavailable")
        return
    fixture = pathlib.Path(tempfile.mkdtemp(prefix="protectorate-node-update-"))
    print(f"fixture retained at {fixture}", flush=True)
    installed = fixture / "installed"
    staged = fixture / "files"
    state = fixture / "state"
    installed.mkdir()
    staged.mkdir()
    state.mkdir()
    binaries = fixture / "binaries"
    binaries.mkdir()
    first = binaries / "first"
    second = binaries / "second"
    build_server(FIRST, first)
    build_server(SECOND, second)
    exe = installed / "protectorate-rs"
    shutil.copy2(first, exe)
    helper = DEBUG / "protectorate-node-sandbox-helper"
    if not helper.is_file():
        run("cargo", "build", "-p", "protectorate-node",
            "--bin", "protectorate-node-sandbox-helper", cwd=ROOT,
            stdout=subprocess.DEVNULL)
    shutil.copy2(helper, installed / helper.name)
    failed = binaries / "failed"
    arch = "arm64" if os.uname().machine in ("aarch64", "arm64") else "amd64"
    failed.write_text(
        "#!/bin/sh\n"
        "if [ \"$1\" = version ] && [ \"$2\" = --json ]; then\n"
        f"  printf '%s\\n' '{{\"version\":\"{FAILED}\",\"os\":\"linux\",\"arch\":\"{arch}\"}}'\n"
        "  exit 0\n"
        "fi\n"
        "exit 42\n"
    )
    failed.chmod(0o755)
    token = "node-update-fixture-token"
    token_path = fixture / "agent-token"
    token_path.write_text(token + "\n")
    token_path.chmod(0o600)
    port = free_port()
    unit = f"protectorate-rust-node-update-trial-{os.getpid()}.service"
    marker_path = pathlib.Path(str(exe) + ".update")
    try:
        run("systemd-run", "--user", "--collect", "--unit", unit,
            "--property=Type=exec", "--property=KillMode=control-group",
            "--property=Restart=no", str(exe), "node", "serve",
            "--home", str(state), "--agent-token-file", str(token_path),
            "--file-root", str(staged), "--allow-plain-jobs",
            "--listen", f"127.0.0.1:{port}")
        wait_until("initial node", lambda: metrics(port, token) == FIRST)
        guardian = run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                       capture_output=True).stdout.strip()
        assert guardian not in ("", "0")

        staged_second = staged / "second"
        shutil.copy2(second, staged_second)
        apply(port, token, staged_second, staged_second.name, SECOND)
        wait_until("updated node", lambda: metrics(port, token) == SECOND)
        wait_until("confirmed update", lambda: marker(marker_path, "confirmed"))
        assert hashlib.sha256(exe.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest()
        assert run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                   capture_output=True).stdout.strip() == guardian

        staged_failed = staged / "failed"
        shutil.copy2(failed, staged_failed)
        apply(port, token, staged_failed, staged_failed.name, FAILED)
        wait_until("failed candidate rollback", lambda: marker(marker_path, "rolledback"))
        wait_until("restored node", lambda: metrics(port, token) == SECOND)
        assert hashlib.sha256(exe.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest()
        assert run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                   capture_output=True).stdout.strip() == guardian
        assert subprocess.run(("systemctl", "--user", "is-active", "--quiet", unit)).returncode == 0
        print("installed Rust node update, confirmation, and failed-candidate rollback passed", flush=True)
    finally:
        subprocess.run(("systemctl", "--user", "stop", unit), check=False)


if __name__ == "__main__":
    main()
