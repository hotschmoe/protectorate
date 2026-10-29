#!/usr/bin/env python3
"""Pair the Flutter web client through its web link against a disposable Rust hub."""

import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote
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
        raise RuntimeError("build the Rust hub with cargo build --workspace")
    fixture = Path(tempfile.mkdtemp(prefix="protectorate-browser-pairing-"))
    home = fixture / "hub"
    home.mkdir()
    profile = fixture / "chrome"
    profile.mkdir()
    token_file = fixture / "owner-token"
    token = "rust-browser-pairing-owner-token"
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
        added = rpc(base, "DeviceService/AddDevice", {"name": "browser-pairing-trial"}, token)
        device_id = added["device"]["id"]
        device_token = added["token"]
        if not device_token.startswith("pd1."):
            raise RuntimeError("AddDevice did not return a pd1. device token")
        device_token_file = fixture / "device-token"
        device_token_file.write_text(device_token)
        device_token_file.chmod(0o600)
        link = f"{base}/#/pair?url={quote(base, safe='')}&token={device_token}"
        command = [
            "node", str(ROOT / "scripts/e2e-screens.mjs"),
            os.environ.get("CHROMIUM", "chromium"), str(profile), "1200,800",
            link, "light", str(fixture / "paired.png"), base,
            str(device_token_file), "false", "pair",
        ]
        browser = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=90)
        if browser.returncode:
            raise RuntimeError(
                f"browser pairing failed: {browser.stdout}\n{browser.stderr}"
            )
        devices = rpc(base, "DeviceService/ListDevices", {}, token).get("devices", [])
        device = next((d for d in devices if d.get("id") == device_id), None)
        if device is None or not device.get("lastUsedAt"):
            raise RuntimeError("the paired browser never called the hub with its device token")
        rpc(base, "DeviceService/RevokeDevice", {"id": device_id}, token)
        try:
            rpc(base, "HubService/GetStatus", {}, device_token)
            raise RuntimeError("the hub admitted the revoked browser token")
        except HTTPError as error:
            if error.code != 401:
                raise RuntimeError(f"revoked browser token got HTTP {error.code}") from error
        print(f"Flutter browser pairing passed; fixture retained at {fixture}")
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
