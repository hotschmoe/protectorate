#!/usr/bin/env python3
"""Run against a real binary and isolated hub; --require-runtime requires sleeves."""

import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--require-runtime", action="store_true")
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    root = Path(tempfile.mkdtemp(prefix="protectorate-scopes-"))
    home = root / "hub"
    home.mkdir()
    workspace = root / "workspace"
    workspace.mkdir()
    env = {k: v for k, v in os.environ.items() if not k.startswith("PROTECTORATE_")}
    env["PROTECTORATE_HOME"] = str(home)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    tmux = "scopes-" + root.name
    log = (root / "hub.log").open("w")

    def start():
        return subprocess.Popen(
            [binary, "hub", "serve", "--listen", f"127.0.0.1:{port}",
             "--tmux-socket", tmux], env=env, stdout=log, stderr=log)

    def rpc(service, method, secret, body=None, expected=200):
        request = urllib.request.Request(
            f"{url}/protectorate.v1.{service}/{method}",
            data=json.dumps(body or {}).encode(),
            headers={"Authorization": "Bearer " + secret, "Content-Type": "application/json"})
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        data = json.loads(response.read())
        assert response.code == expected, (service, method, response.code, data)
        return data

    def ready(process):
        for _ in range(200):
            assert process.poll() is None, (root / "hub.log").read_text()
            try:
                owner = (home / "token").read_text().strip()
                rpc("HubService", "GetStatus", owner)
                return owner
            except (FileNotFoundError, urllib.error.URLError, ConnectionError):
                time.sleep(0.05)
        raise AssertionError("hub did not become ready")

    def cli(secret, *command):
        command_env = dict(env, PROTECTORATE_TOKEN=secret)
        result = subprocess.run([binary, "--hub", url, "--json", *command],
                                env=command_env, text=True, capture_output=True, timeout=30)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)

    process = start()
    worker = "scope-worker-" + str(os.getpid())
    child = "scope-child-" + str(os.getpid())
    alive = set()
    try:
        owner = ready(process)
        envoy = cli(owner, "token", "create", "orchestrator-envoy", "--scope", "sleeve_spawn_sleeve")
        other = cli(owner, "token", "create", "sleeve:other", "--scope", "read")
        secret = envoy["secret"]
        token_id = envoy["token"]["id"]
        rpc("HubService", "GetStatus", secret, expected=403)
        rpc("TokenService", "UpdateTokenScopes", secret,
            {"id": token_id, "add": ["read"]}, expected=403)
        rpc("SleeveService", "KillSleeve", secret, {"name": "envoy"}, expected=403)
        changed = cli(owner, "orchestrator", "scopes", "--add", "read")
        assert changed["token"]["id"] == token_id
        rpc("HubService", "GetStatus", secret)
        cli(owner, "token", "scopes", token_id, "--remove", "read")
        rpc("HubService", "GetStatus", secret, expected=403)
        request = {"name": worker, "workspace": str(workspace), "harness": "HARNESS_SHELL",
                   "args": ["bash", "-c", "sleep 600"]}
        try:
            spawned = rpc("SleeveService", "SpawnSleeve", secret, request)
        except AssertionError as error:
            if args.require_runtime or "failed_precondition" not in str(error):
                raise
            print("SKIP sleeve lifecycle: runtime unavailable:", error)
        else:
            alive.add(worker)
            assert spawned["sleeve"]["parent"] == "envoy"
            assert "envoy" in cli(owner, "sleeve", "show", worker)["sleeve"]["parent"]
            rpc("SleeveService", "KillSleeve", other["secret"], {"name": worker}, expected=403)
            # The same token secret now grants a new RPC on the very next call.
            cli(owner, "token", "scopes", "sleeve:other", "--add", "sleeve_spawn_sleeve")
            request["name"] = child
            spawned_child = rpc("SleeveService", "SpawnSleeve", other["secret"], request)
            alive.add(child)
            assert spawned_child["sleeve"]["parent"] == "sleeve:other"
            process.terminate()
            process.wait(timeout=20)
            process = start()
            ready(process)
            assert rpc("SleeveService", "GetSleeve", owner, {"name": worker})["sleeve"]["parent"] == "envoy"
            rpc("SleeveService", "KillSleeve", secret, {"name": worker})
            alive.remove(worker)
            rpc("SleeveService", "KillSleeve", other["secret"], {"name": child})
            alive.remove(child)
            print("PASS persisted ownership, cross-sleeve denial, parent kills over HTTP")
        cli(owner, "token", "scopes", token_id, "--remove", "sleeve_spawn_sleeve")
        rpc("HubService", "GetStatus", secret, expected=403)
        process.terminate()
        process.wait(timeout=20)
        process = start()
        ready(process)
        rpc("HubService", "GetStatus", secret, expected=403)
        cli(owner, "token", "scopes", token_id, "--add", "read")
        rpc("HubService", "GetStatus", secret)
        lines = [json.loads(line) for path in (home / "audit").glob("*.jsonl")
                 for line in path.read_text().splitlines()]
        changes = [line for line in lines if line["procedure"].endswith("/UpdateTokenScopes")
                   and line["outcome"] == "ok"]
        assert len(changes) >= 4
        assert all(line["actor"] == "owner" and "scopes " in line["detail"] for line in changes)
        print("PASS owner-only in-place widening/narrowing, empty scopes, restart persistence, audit")
    finally:
        for name in alive:
            try:
                rpc("SleeveService", "KillSleeve", owner, {"name": name})
            except Exception as error:
                print("cleanup:", error)
        process.terminate()
        process.wait(timeout=20)
        subprocess.run(["tmux", "-L", tmux, "kill-server"], capture_output=True)
        log.close()
        print("Evidence retained at", root)


if __name__ == "__main__":
    main()
