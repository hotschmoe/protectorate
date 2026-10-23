#![cfg(target_os = "linux")]
//! A real sandbox has only loopback and reaches the outside through its
//! allowlisting CONNECT proxy. Local echo servers stand in for destinations.
use protectorate_sandbox::{
    egress::{Decision, Verdict},
    runtime::{self, LaunchOptions},
    Egress, Limits, Policy, Request, Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

const CLIENT: &str = r#"
import os, socket, sys
proxy = os.environ["HTTPS_PROXY"].rsplit(":", 1)
assert proxy[0] == "http://127.0.0.1", proxy
for key in ("HTTP_PROXY", "https_proxy", "http_proxy", "ALL_PROXY", "NO_PROXY", "NODE_USE_ENV_PROXY"):
    assert os.environ.get(key), key
def tunnel(target, method="CONNECT"):
    s = socket.create_connection(("127.0.0.1", int(proxy[1])), timeout=10)
    s.sendall(f"{method} {target} HTTP/1.1\r\nHost: {target}\r\n\r\n".encode())
    head = b""
    while b"\r\n\r\n" not in head:
        chunk = s.recv(1)
        if not chunk:
            break
        head += chunk
    status = head.split(b" ")[1].decode() if head else "none"
    if status == "200":
        s.sendall(b"ping\n")
        status += ":" + s.makefile().readline().strip()
    s.close()
    return status
def direct(host, port):
    try:
        socket.create_connection((host, port), timeout=3).close()
        return "connected"
    except OSError as error:
        return type(error).__name__
results = {}
for target in sys.argv[1].split(","):
    results[target] = tunnel(target)
results["GET"] = tunnel("http://example.com/", "GET")
results["direct-public"] = direct("1.1.1.1", 443)
results["direct-host-loopback"] = direct("127.0.0.1", int(sys.argv[2]))
if len(sys.argv) > 3:
    s = socket.create_connection(("127.0.0.1", int(sys.argv[3])), timeout=10)
    s.sendall(b"ping\n")
    results["hub-direct"] = s.makefile().readline().strip()
print(";".join(f"{k}={v}" for k, v in sorted(results.items())))
"#;

fn echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                if reader.read_line(&mut line).is_ok() {
                    let _ = (&stream).write_all(format!("echo-{}", line).as_bytes());
                }
            });
        }
    });
    port
}

fn run(egress: Egress, args: Vec<String>) -> (String, Vec<Decision>) {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let workspace = root.path().join("project");
    let home = state.join("sleeves/net/home");
    let runtime_dir = root.path().join("runtime");
    for dir in [&state, &owner, &workspace, &home] {
        fs::create_dir_all(dir).unwrap();
    }
    let policy = Policy {
        state_root: state,
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let mut command = vec!["/usr/bin/python3".into(), "-c".into(), CLIENT.into()];
    command.extend(args);
    let plan = policy
        .plan(Request {
            workload: Workload::ChatSleeve,
            command,
            workspace: Some(workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(home),
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            limits: Limits {
                cpu_percent: 100,
                memory_bytes: 256 << 20,
                tasks_max: 64,
            },
            egress: Some(egress),
        })
        .unwrap();
    let decisions = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&decisions);
    let mut options = LaunchOptions::new(
        env!("CARGO_BIN_EXE_protectorate-sandbox-helper"),
        &runtime_dir,
        &owner,
        &workspace,
    );
    options.egress_log = Some(Arc::new(move |decision: &Decision| {
        sink.lock().unwrap().push(decision.clone())
    }));
    let child = runtime::spawn(&plan, &options)
        .unwrap_or_else(|e| panic!("REAL sandbox capability failure (not skipped): {e}"));
    let leftovers = fs::read_dir(&runtime_dir).unwrap().count();
    assert_eq!(leftovers, 0, "host egress socket names outlived setup");
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let decisions = decisions.lock().unwrap().clone();
    (
        String::from_utf8(output.stdout).unwrap().trim().to_owned(),
        decisions,
    )
}

#[test]
fn sandbox_reaches_only_admitted_destinations_through_its_proxy() {
    let local = echo_server();
    let hub = echo_server();
    let (out, decisions) = run(
        Egress {
            hub: Some(format!("127.0.0.1:{hub}")),
            allow: vec!["*.example.invalid".into()],
            private_hosts: vec!["localhost".into()],
        },
        vec![
            format!("localhost:{local},127.0.0.1:{local},not-listed.test:443,169.254.169.254:80,127.0.0.1:{hub}"),
            local.to_string(),
            hub.to_string(),
        ],
    );
    let expected = [
        format!("127.0.0.1:{hub}=200:echo-ping"),
        format!("127.0.0.1:{local}=403"),
        "169.254.169.254:80=403".into(),
        "GET=405".into(),
        "direct-host-loopback=ConnectionRefusedError".into(),
        "direct-public=OSError".into(),
        "hub-direct=echo-ping".into(),
        format!("localhost:{local}=200:echo-ping"),
        "not-listed.test:443=403".into(),
    ];
    for item in &expected {
        assert!(
            out.split(';').any(|got| got == item),
            "missing {item} in {out}"
        );
    }
    let verdict = |host: &str, port: u16| {
        decisions
            .iter()
            .find(|d| d.host == host && d.port == port)
            .map(|d| d.verdict)
    };
    assert_eq!(
        verdict("localhost", local),
        Some(Verdict::Allow),
        "{decisions:?}"
    );
    assert_eq!(verdict("127.0.0.1", local), Some(Verdict::Deny));
    assert_eq!(verdict("not-listed.test", 443), Some(Verdict::Deny));
    assert_eq!(verdict("169.254.169.254", 80), Some(Verdict::Deny));
    assert_eq!(verdict("127.0.0.1", hub), Some(Verdict::Allow));
    assert!(decisions.iter().any(|d| d.reason.contains("only CONNECT")));
}

#[test]
fn allowlisted_name_with_private_addresses_is_refused() {
    let local = echo_server();
    let (out, decisions) = run(
        Egress {
            hub: None,
            allow: vec![format!("localhost:{local}")],
            private_hosts: Vec::new(),
        },
        vec![format!("localhost:{local}"), local.to_string()],
    );
    assert!(out.contains(&format!("localhost:{local}=403")), "{out}");
    let decision = decisions.iter().find(|d| d.host == "localhost").unwrap();
    assert_eq!(decision.verdict, Verdict::Deny);
    assert!(
        decision.reason.contains("refused addresses"),
        "{decision:?}"
    );
}

#[test]
fn sandbox_without_egress_has_no_proxy_and_no_route() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let workspace = root.path().join("project");
    let home = state.join("sleeves/net/home");
    for dir in [&state, &owner, &workspace, &home] {
        fs::create_dir_all(dir).unwrap();
    }
    let policy = Policy {
        state_root: state,
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let script = "set -eu; test -z \"${HTTPS_PROXY:-}\"; test ! -e /run/protectorate/egress; \
        if /usr/bin/python3 -c 'import socket; socket.create_connection((\"1.1.1.1\", 443), timeout=3)' 2>/dev/null; then exit 9; fi; \
        /usr/bin/python3 -c 'import socket; s=socket.socket(); s.bind((\"127.0.0.1\", 0)); s.listen(); socket.create_connection(s.getsockname()).close()'";
    let plan = policy
        .plan(Request {
            workload: Workload::ChatSleeve,
            command: vec!["/usr/bin/sh".into(), "-c".into(), script.into()],
            workspace: Some(workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(home),
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            limits: Limits {
                cpu_percent: 100,
                memory_bytes: 256 << 20,
                tasks_max: 64,
            },
            egress: None,
        })
        .unwrap();
    let options = LaunchOptions::new(
        env!("CARGO_BIN_EXE_protectorate-sandbox-helper"),
        root.path().join("runtime"),
        &owner,
        &workspace,
    );
    let output = runtime::spawn(&plan, &options)
        .unwrap_or_else(|e| panic!("REAL sandbox capability failure (not skipped): {e}"))
        .wait_with_output()
        .unwrap();
    assert!(
        output.status.success(),
        "status {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
