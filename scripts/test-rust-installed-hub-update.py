#!/usr/bin/env python3
"""Exercise a Rust hub update under an isolated, real systemd user unit.

The fixture keeps its temporary files for inspection and never addresses the
installed protectorate-hub.service unit.
"""

import hashlib
import http.server
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import urllib.request


ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = pathlib.Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
DEBUG = (TARGET if TARGET.is_absolute() else ROOT / TARGET) / "debug"
COMPONENTS = (
    "protectorate-rs",
    "protectorate-sleeve-runner",
    "protectorate-sleeve-sandbox-helper",
    "protectorate-node-sandbox-helper",
    "protectorate-cli",
    "protectorate-mcpbridge",
)
FIRST = "0.1.0"
SECOND = "0.1.1"
FAILED = "0.1.2"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_until(label, predicate, seconds=120):
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
        result = run(destination, "version", "--json", capture_output=True)
        assert json.loads(result.stdout)["version"] == version, result.stdout
        return
    env = dict(os.environ, PROTECTORATE_BUILD_VERSION=version)
    run("cargo", "build", "-p", "protectorate-server", "--bin", "protectorate-rs",
        cwd=ROOT, env=env, stdout=subprocess.DEVNULL)
    shutil.copy2(DEBUG / "protectorate-rs", destination)
    result = run(destination, "version", "--json", capture_output=True)
    assert json.loads(result.stdout)["version"] == version, result.stdout


def make_bundle(path, version, server_binary, arch):
    path.mkdir()
    (path / "VERSION").write_text(version + "\n")
    for component in COMPONENTS:
        source = server_binary if component == "protectorate-rs" else DEBUG / component
        assert source.is_file(), f"build missing companion {source}"
        target = path / f"{component}-linux-{arch}"
        shutil.copy2(source, target)
        if shutil.which("strip") and not (version == FAILED and component == "protectorate-rs"):
            run("strip", "--strip-debug", target, stdout=subprocess.DEVNULL)
    web = path / f"protectorate-web-{version}.tar.gz"
    index = path / "index.html"
    index.write_text("<!doctype html><title>Rust update fixture</title>\n")
    with tarfile.open(web, "w:gz") as archive:
        archive.add(index, arcname="index.html")
    lines = []
    for asset in sorted(path.iterdir()):
        if asset.name not in ("SHA256SUMS", "index.html"):
            lines.append(f"{hashlib.sha256(asset.read_bytes()).hexdigest()}  {asset.name}\n")
    (path / "SHA256SUMS").write_text("".join(lines))


class Releases(http.server.BaseHTTPRequestHandler):
    bundles = None
    base = None

    def do_GET(self):
        prefix = "/repos/owner/repo/releases/tags/"
        if self.path.startswith(prefix):
            version = self.path[len(prefix):]
            bundle = self.bundles.get(version)
            if bundle is not None:
                body = json.dumps({
                    "tag_name": version,
                    "prerelease": False,
                    "html_url": self.base + "/notes/" + version,
                    "assets": [{
                        "name": asset.name,
                        "browser_download_url": self.base + f"/assets/{version}/{asset.name}",
                    } for asset in bundle.iterdir() if asset.name != "index.html"],
                }).encode()
                return self.reply(200, body, "application/json")
        if self.path.startswith("/assets/"):
            fields = self.path.split("/")
            if len(fields) == 4 and fields[2] in self.bundles:
                asset = self.bundles[fields[2]] / fields[3]
                if asset.is_file():
                    return self.reply(200, asset.read_bytes(), "application/octet-stream")
        return self.reply(404, b"missing", "text/plain")

    def reply(self, status, body, content_type):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def status(cli, hub_url, token):
    result = subprocess.run((cli, "--hub", hub_url, "--token-file", token,
                             "--json", "status"), text=True, capture_output=True)
    if result.returncode:
        return None
    return json.loads(result.stdout).get("version")


def backlog_request(hub_url, token_path, method, body):
    token = pathlib.Path(token_path).read_text().strip()
    request = urllib.request.Request(
        f"{hub_url}/protectorate.v1.BacklogService/{method}",
        data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.load(response)


def assert_note(hub_url, token_path, note_id):
    notes = backlog_request(hub_url, token_path, "ListNotes", {})["notes"]
    assert any(note["id"] == note_id and note["title"] == "installed update fixture"
               for note in notes), notes


def assert_companions(bundle, release, arch):
    for component in COMPONENTS:
        source = bundle / f"{component}-linux-{arch}"
        installed = release / component
        assert installed.is_file(), installed
        assert hashlib.sha256(installed.read_bytes()).digest() == hashlib.sha256(source.read_bytes()).digest(), installed


def main():
    if sys.platform != "linux" or not shutil.which("systemd-run"):
        print("skipping: systemd user manager is required")
        return
    if subprocess.run(("systemctl", "--user", "show-environment"),
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        print("skipping: systemd user manager is unavailable")
        return
    root = pathlib.Path(tempfile.mkdtemp(prefix="protectorate-installed-update-"))
    print(f"fixture retained at {root}", flush=True)
    arch = "arm64" if os.uname().machine in ("aarch64", "arm64") else "amd64"
    binaries = root / "binaries"
    binaries.mkdir()
    first_binary = binaries / "first"
    second_binary = binaries / "second"
    build_server(FIRST, first_binary)
    build_server(SECOND, second_binary)
    failed_binary = binaries / "failed"
    failed_binary.write_text("#!/bin/sh\nexit 42\n")
    failed_binary.chmod(0o755)
    bundles = {}
    for version, binary in ((FIRST, first_binary), (SECOND, second_binary), (FAILED, failed_binary)):
        bundle = root / f"bundle-{version}"
        make_bundle(bundle, version, binary, arch)
        bundles[version] = bundle

    handler = type("FixtureReleases", (Releases,), {})
    handler.bundles = bundles
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    handler.base = f"http://127.0.0.1:{server.server_port}"
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    home = root / "home"
    home.mkdir()
    mock = root / "mock"
    mock.mkdir()
    systemctl = mock / "systemctl"
    systemctl.write_text("#!/bin/sh\ncase \"$*\" in *is-active*|*is-enabled*) exit 1;; esac\nexit 0\n")
    systemctl.chmod(0o755)
    no_delete = root / "no-delete.bash"
    no_delete.write_text("rm() { :; }\n")
    env = dict(os.environ, HOME=str(home), BASH_ENV=str(no_delete),
               PATH=str(mock) + os.pathsep + os.environ["PATH"], TMPDIR=str(root))
    state = root / "state"
    releases = root / "releases"
    port = free_port()
    unit = f"protectorate-rust-update-trial-{os.getpid()}.service"
    cli = str(DEBUG / "protectorate-cli")
    assert pathlib.Path(cli).is_file(), "build protectorate-cli first"
    try:
        run("bash", str(ROOT / "install-hub.sh"), "--bundle-dir", str(bundles[FIRST]),
            "--home", str(state), "--release-dir", str(releases),
            "--listen", f"127.0.0.1:{port}", "--no-start", cwd=ROOT, env=env)
        hub_url = f"http://127.0.0.1:{port}"
        token = str(state / "token")
        run("systemd-run", "--user", "--collect", "--unit", unit,
            "--property=Type=exec", "--property=KillMode=control-group",
            "--property=Restart=no", str(releases / "current/protectorate-rs"), "serve",
            "--home", str(state), "--owner-token-file", token,
            "--listen", f"127.0.0.1:{port}", "--release-repo", "owner/repo",
            "--release-api-url", handler.base, "--release-dir", str(releases))
        wait_until("initial installed hub", lambda: status(cli, hub_url, token) == FIRST)
        note_id = backlog_request(hub_url, token, "CreateNote",
                                  {"title": "installed update fixture"})["note"]["id"]
        assert_note(hub_url, token, note_id)
        assert_companions(bundles[FIRST], releases / FIRST, arch)
        guardian = run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                       capture_output=True).stdout.strip()
        assert guardian not in ("", "0")
        for version, expected_code in ((SECOND, 0), (FAILED, 1)):
            result = subprocess.run((cli, "--hub", hub_url, "--token-file", token,
                                     "hub", "update", "--version", version),
                                    text=True, capture_output=True, timeout=180)
            print(f"update {version}: exit={result.returncode}\n{result.stdout}{result.stderr}", flush=True)
            assert (result.returncode == 0) == (expected_code == 0)
            wait_until("final hub version", lambda: status(cli, hub_url, token) == SECOND)
            wait_until("guardian activation", lambda: os.readlink(releases / "current") == SECOND)
            assert os.readlink(releases / "current") == SECOND
            assert_note(hub_url, token, note_id)
            assert_companions(bundles[SECOND], releases / SECOND, arch)
            wait_until("guardian trial completion", lambda: not (releases / ".hub-trial.json").exists())
            pid = run("systemctl", "--user", "show", unit, "--property=MainPID", "--value",
                      capture_output=True).stdout.strip()
            assert pid == guardian, f"guardian changed: {guardian} -> {pid}"
        assert subprocess.run(("systemctl", "--user", "is-active", "--quiet", unit)).returncode == 0
        run("systemctl", "--user", "restart", unit)
        wait_until("hub after installed service restart", lambda: status(cli, hub_url, token) == SECOND)
        assert os.readlink(releases / "current") == SECOND
        assert_note(hub_url, token, note_id)
        assert_companions(bundles[SECOND], releases / SECOND, arch)
        assert not (releases / ".hub-trial.json").exists()
        print("installed Rust hub update and failed-candidate rollback passed", flush=True)
    finally:
        subprocess.run(("systemctl", "--user", "stop", unit), check=False)
        server.shutdown()


if __name__ == "__main__":
    main()
