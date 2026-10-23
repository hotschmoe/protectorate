#!/usr/bin/env python3
"""Exercise the Flutter approval button against a disposable Rust hub."""

import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parent.parent
WEB = Path(os.environ.get("PROTECTORATE_TEST_WEB_DIR", ROOT / "app/build/web"))
HUB = ROOT / "target/debug/protectorate-rs"


def rpc(base, method, body, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = Request(
        f"{base}/protectorate.v1.{method}",
        data=json.dumps(body).encode(),
        headers=headers,
    )
    with urlopen(request, timeout=8) as response:
        return json.load(response)


def main():
    if not (WEB / "index.html").is_file():
        raise RuntimeError("build the Flutter web client with make app-build-web first")
    if not HUB.is_file():
        raise RuntimeError("build the Rust hub with cargo build -p protectorate-server")
    fixture = Path(tempfile.mkdtemp(prefix="protectorate-browser-approval-"))
    home = fixture / "hub"
    home.mkdir()
    profile = fixture / "chrome"
    profile.mkdir()
    token_file = fixture / "owner-token"
    token = "rust-browser-approval-owner-token"
    token_file.write_text(token)
    token_file.chmod(0o600)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    log = (fixture / "hub.log").open("wb")
    hub = subprocess.Popen(
        [
            str(HUB), "serve", "--home", str(home),
            "--owner-token-file", str(token_file), "--listen", f"127.0.0.1:{port}",
            "--web", str(WEB), "--release-api-url", "http://127.0.0.1:1",
        ],
        cwd=ROOT,
        env={**os.environ, "HOME": str(fixture)},
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    try:
        for _ in range(100):
            if hub.poll() is not None:
                raise RuntimeError(f"Rust hub exited early; see {fixture / 'hub.log'}")
            try:
                rpc(base, "HubService/GetStatus", {}, token)
                break
            except URLError:
                time.sleep(0.1)
        else:
            raise RuntimeError("Rust hub was not ready in 10 seconds")
        started = rpc(
            base,
            "DeviceService/StartDeviceAuth",
            {"name": "browser-approval-trial", "platform": "web"},
        )
        device_code = started["deviceCode"]
        command = [
            "node", str(ROOT / "scripts/e2e-screens.mjs"),
            os.environ.get("CHROMIUM", "chromium"), str(profile), "1200,800",
            base, "light", str(fixture / "approved.png"), base,
            str(token_file), "true", "pair-approve",
        ]
        browser = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=90)
        if browser.returncode:
            raise RuntimeError(
                f"browser approval failed: {browser.stdout}\n{browser.stderr}"
            )
        result = rpc(base, "DeviceService/PollDeviceAuth", {"deviceCode": device_code})
        if result.get("status") != "approved" or not result.get("secret"):
            raise RuntimeError(f"browser click did not approve device: {result.get('status')}")
        print(f"Flutter browser approval passed; fixture retained at {fixture}")
    finally:
        hub.terminate()
        try:
            hub.wait(timeout=5)
        except subprocess.TimeoutExpired:
            hub.kill()
            hub.wait()
        log.close()


if __name__ == "__main__":
    main()
