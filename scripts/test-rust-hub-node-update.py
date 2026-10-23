#!/usr/bin/env python3
"""Push an installed node update through a disposable Rust hub and CLI.

The fixture is retained for inspection. It never uses the live Go service or
the installed protectorate-node.service unit.
"""

import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
module_path = ROOT / "scripts/test-rust-installed-node-update.py"
spec = importlib.util.spec_from_file_location("installed_node_update", module_path)
installed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installed)
build_server = installed.build_server
free_port = installed.free_port
metrics = installed.metrics
wait_until = installed.wait_until
start_hub = installed.start_hub
join = installed.join
OWNER = installed.OWNER
FIRST = "0.1.0"
SECOND = "0.1.1"


def cli(executable, hub, token, *args, expected=0):
    result = subprocess.run(
        (str(executable), "--hub", hub, "--token-file", str(token), *args),
        capture_output=True, text=True, timeout=420,
    )
    if expected is not None:
        assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
    return result


def phase_lines(result):
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def marker(path, phase):
    try:
        return json.loads(path.read_text()).get("phase") == phase
    except FileNotFoundError:
        return False


def write_failed_candidate(candidate, arch):
    candidate.write_text(
        "#!/bin/sh\n"
        "if [ \"$1\" = version ] && [ \"$2\" = --json ]; then\n"
        f"  printf '%s\\n' '{{\"version\":\"{SECOND}\",\"os\":\"linux\",\"arch\":\"{arch}\"}}'\n"
        "  exit 0\n"
        "fi\n"
        "exit 42\n"
    )
    candidate.chmod(0o755)


def main():
    if sys.platform != "linux" or not shutil.which("systemd-run"):
        print("skipping: systemd user manager is required")
        return
    if subprocess.run(("systemctl", "--user", "show-environment"),
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        print("skipping: systemd user manager is unavailable")
        return

    fixture = pathlib.Path(tempfile.mkdtemp(prefix="protectorate-hub-node-update-"))
    print(f"fixture retained at {fixture}", flush=True)
    for name in ("node-bin", "hub-bin", "node-files", "node-state", "hub-state", "releases"):
        (fixture / name).mkdir()
    first = fixture / "first"
    second = fixture / "second"
    build_server(FIRST, first)
    build_server(SECOND, second)
    node_exe = fixture / "node-bin/protectorate-rs"
    hub_exe = fixture / "hub-bin/protectorate-rs"
    shutil.copy2(first, node_exe)
    shutil.copy2(second, hub_exe)
    arch = "arm64" if os.uname().machine in ("aarch64", "arm64") else "amd64"
    candidate = fixture / f"releases/protectorate-rs-linux-{arch}"
    shutil.copy2(second, candidate)
    for filename in ("protectorate-node-sandbox-helper",):
        binary = ROOT / "target/debug" / filename
        assert binary.is_file(), f"build {filename} first"
        shutil.copy2(binary, fixture / "node-bin" / filename)
    cli_exe = ROOT / "target/debug/protectorate"
    assert cli_exe.is_file(), "build protectorate first"

    owner_token = fixture / "owner-token"
    node_port = free_port()
    unit = f"protectorate-hub-node-update-{os.getpid()}.service"
    hub_process = None
    try:
        hub_process, hub_url = start_hub(fixture, hub_exe, "--node-binary-dir", str(fixture / "releases"))
        join(hub_url, fixture, node_exe, fixture / "node-state", "trial-peer", node_port)
        subprocess.run(
            ("systemd-run", "--user", "--collect", "--unit", unit,
             "--property=Type=exec", "--property=KillMode=control-group",
             "--property=Restart=no", str(node_exe), "node", "serve",
             "--home", str(fixture / "node-state"),
             "--file-root", str(fixture / "node-files"), "--allow-plain-jobs"),
            check=True, capture_output=True, text=True,
        )
        wait_until("hub node poll", lambda: metrics(hub_url, "trial-peer") == FIRST)

        write_failed_candidate(candidate, arch)
        failed = cli(cli_exe, hub_url, owner_token, "node", "update", "trial-peer",
                     "--json", expected=None)
        failed_phases = phase_lines(failed)
        assert failed.returncode != 0, (failed.stdout, failed.stderr)
        assert failed_phases, (failed.returncode, failed.stdout, failed.stderr)
        assert failed_phases[-1]["phase"] == "UPDATE_PHASE_ROLLED_BACK", (failed_phases, failed.stderr)
        wait_until("rollback marker", lambda: marker(pathlib.Path(str(node_exe) + ".update"), "rolledback"))
        assert hashlib.sha256(node_exe.read_bytes()).digest() == hashlib.sha256(first.read_bytes()).digest()
        wait_until("node after rollback", lambda: metrics(hub_url, "trial-peer") == FIRST)

        shutil.copy2(second, candidate)
        update = cli(cli_exe, hub_url, owner_token, "node", "update", "trial-peer", "--json")
        phases = phase_lines(update)
        assert any(event["phase"] == "UPDATE_PHASE_SENDING" for event in phases), phases
        assert any(event["phase"] == "UPDATE_PHASE_VERIFYING" for event in phases), phases
        assert phases[-1]["phase"] == "UPDATE_PHASE_CONFIRMED", phases
        wait_until("node confirmation", lambda: marker(pathlib.Path(str(node_exe) + ".update"), "confirmed"))
        assert hashlib.sha256(node_exe.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest()
        wait_until("updated node", lambda: metrics(hub_url, "trial-peer") == SECOND)
        skipped = phase_lines(cli(cli_exe, hub_url, owner_token, "node", "update", "trial-peer", "--json"))
        assert skipped[-1]["phase"] == "UPDATE_PHASE_SKIPPED", skipped

        write_failed_candidate(candidate, arch)
        same_version = cli(cli_exe, hub_url, owner_token, "node", "update", "trial-peer",
                           "--force", "--json", expected=None)
        same_phases = phase_lines(same_version)
        assert same_version.returncode != 0, (same_version.stdout, same_version.stderr)
        assert same_phases, (same_version.returncode, same_version.stdout, same_version.stderr)
        assert same_phases[-1]["phase"] == "UPDATE_PHASE_ROLLED_BACK", (same_phases, same_version.stderr)
        wait_until("same-version rollback marker", lambda: marker(pathlib.Path(str(node_exe) + ".update"), "rolledback"))
        assert hashlib.sha256(node_exe.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest()

        print("hub NodeService/UpdateNode, CLI confirmation and both rollback cases, installed node and skip passed", flush=True)
    finally:
        if hub_process is not None and hub_process.poll() is None:
            hub_process.terminate()
            try:
                hub_process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                hub_process.kill()
                hub_process.wait(timeout=10)
        subprocess.run(("systemctl", "--user", "stop", unit), check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == "__main__":
    main()
