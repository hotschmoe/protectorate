#!/usr/bin/env python3
"""Exercise node binary update and rollback in a disposable systemd user unit.

A disposable hub with a TLS listener invites the node, the node joins it, and
every node call goes through the hub over mutual TLS. The fixture leaves its
files for inspection and never addresses the installed protectorate-node.service
unit.
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
OWNER = "node-update-fixture-owner-token"


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


def hub_call(hub, method, body, timeout=10):
    request = urllib.request.Request(
        f"{hub}/protectorate.v1.{method}",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + OWNER},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def start_hub(fixture, hub_exe, *extra):
    """Starts a hub beside its sleeve companions; returns (process, plain URL)."""
    for filename in ("protectorate-sleeve-runner", "protectorate-sleeve-sandbox-helper"):
        companion = DEBUG / filename
        assert companion.is_file(), f"build {filename} first"
        shutil.copy2(companion, hub_exe.parent / filename)
    token = fixture / "owner-token"
    token.write_text(OWNER + "\n")
    token.chmod(0o600)
    plain, tls = free_port(), free_port()
    hub = f"http://127.0.0.1:{plain}"
    log = (fixture / "hub.log").open("w")
    process = subprocess.Popen(
        (str(hub_exe), "serve", "--home", str(fixture / "hub-state"),
         "--owner-token-file", str(token), "--listen", f"127.0.0.1:{plain}",
         "--tls-listen", f"127.0.0.1:{tls}", "--node-interval", "1s", *extra),
        stdout=log, stderr=subprocess.STDOUT,
    )

    def ready():
        assert process.poll() is None, (fixture / "hub.log").read_text()
        try:
            return hub_call(hub, "HubService/GetStatus", {})[0] == 200
        except (ConnectionError, urllib.error.URLError):
            return False

    wait_until("hub", ready)
    return process, hub


def join(hub, fixture, node_exe, home, name, port):
    """Invites `name` and joins `home` to the fixture hub on 127.0.0.1:`port`."""
    status, invite = hub_call(hub, "NodeService/InviteNode", {"name": name, "tier": "care"})
    assert status == 200, invite
    run(node_exe, "node", "join", "--hub", invite["hubUrl"], "--code", invite["code"],
        "--ca-cert", str(fixture / "hub-state/ca/ca.pem"),
        "--name", name, "--listen", f"127.0.0.1:{port}", "--home", str(home))


def metrics(hub, name):
    """The node's agent version from a fresh hub poll, else None."""
    try:
        status, body = hub_call(hub, "NodeService/GetNode", {"name": name})
    except (ConnectionError, TimeoutError, urllib.error.URLError):
        return None
    if status != 200 or body.get("freshness") != "NODE_FRESHNESS_FRESH":
        return None
    return body.get("metrics", {}).get("agentVersion")


def apply(hub, name, staged, filename, version):
    body = {
        "name": name,
        "path": filename,
        "size": str(staged.stat().st_size),
        "sha256": hashlib.sha256(staged.read_bytes()).hexdigest(),
        "expectedVersion": version,
    }
    status, reply = hub_call(hub, "NodeService/ApplyUpdate", body, timeout=90)
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
    hub_bin = fixture / "hub-bin"
    hub_bin.mkdir()
    shutil.copy2(second, hub_bin / "protectorate-rs")
    hub_process, hub = start_hub(fixture, hub_bin / "protectorate-rs")
    port = free_port()
    join(hub, fixture, exe, state, "trial-peer", port)
    unit = f"protectorate-node-update-trial-{os.getpid()}.service"
    marker_path = pathlib.Path(str(exe) + ".update")
    try:
        run("systemd-run", "--user", "--collect", "--unit", unit,
            "--property=Type=exec", "--property=KillMode=control-group",
            "--property=Restart=no", str(exe), "node", "serve",
            "--home", str(state), "--file-root", str(staged), "--allow-plain-jobs")
        wait_until("initial node", lambda: metrics(hub, "trial-peer") == FIRST)
        guardian = run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                       capture_output=True).stdout.strip()
        assert guardian not in ("", "0")

        staged_second = staged / "second"
        shutil.copy2(second, staged_second)
        apply(hub, "trial-peer", staged_second, staged_second.name, SECOND)
        wait_until("updated node", lambda: metrics(hub, "trial-peer") == SECOND)
        wait_until("confirmed update", lambda: marker(marker_path, "confirmed"))
        assert hashlib.sha256(exe.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest()
        assert run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                   capture_output=True).stdout.strip() == guardian

        staged_failed = staged / "failed"
        shutil.copy2(failed, staged_failed)
        apply(hub, "trial-peer", staged_failed, staged_failed.name, FAILED)
        wait_until("failed candidate rollback", lambda: marker(marker_path, "rolledback"))
        wait_until("restored node", lambda: metrics(hub, "trial-peer") == SECOND)
        assert hashlib.sha256(exe.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest()
        assert run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                   capture_output=True).stdout.strip() == guardian
        assert subprocess.run(("systemctl", "--user", "is-active", "--quiet", unit)).returncode == 0
        print("installed Rust node update, confirmation, and failed-candidate rollback passed", flush=True)
    finally:
        subprocess.run(("systemctl", "--user", "stop", unit), check=False)
        hub_process.terminate()
        hub_process.wait(timeout=10)


if __name__ == "__main__":
    main()
