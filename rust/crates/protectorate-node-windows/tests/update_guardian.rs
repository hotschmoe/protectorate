#![cfg(windows)]

use protectorate_connect_client::{Client, NodeTls};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, mem,
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
            PROCESS_TERMINATE,
        },
    },
};

const METRICS: &str = "/protectorate.v1.NodeAgentService/GetMetrics";
const APPLY: &str = "/protectorate.v1.NodeAgentService/ApplyUpdate";

struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// What `node join` leaves in a home, issued by a CA this test owns, plus
/// the client side of the hub's mutual TLS.
struct Joined {
    server_name: String,
    address: String,
    tls: NodeTls,
}

impl Joined {
    fn create(home: &Path, address: SocketAddr) -> Self {
        let ca = protectorate_tls::Ca::open_or_create(&home.join("hub-ca")).unwrap();
        let (key_pem, public_key, _) = protectorate_tls::node_key(b"join").unwrap();
        let (name, id) = ("guardian", "cert1");
        let (cert, _) = ca.issue_node(name, id, &public_key).unwrap();
        let tls_dir = home.join("tls");
        fs::create_dir_all(&tls_dir).unwrap();
        fs::write(tls_dir.join("ca.pem"), ca.pem()).unwrap();
        fs::write(tls_dir.join("node-key.pem"), &key_pem).unwrap();
        fs::write(tls_dir.join("node.pem"), &cert).unwrap();
        fs::write(
            home.join("node.json"),
            json!({"hub": "https://127.0.0.1:1", "name": name, "address": address}).to_string(),
        )
        .unwrap();
        let hub = ca.hub_identity(&[]).unwrap();
        Self {
            server_name: protectorate_tls::node_name(name, id),
            address: address.to_string(),
            tls: NodeTls::from_pem(
                ca.pem().as_bytes(),
                hub.cert_pem.as_bytes(),
                hub.key_pem.as_bytes(),
            )
            .unwrap(),
        }
    }

    fn call(&self, method: &str, input: Value) -> Result<Value, String> {
        Client::new_node(
            &self.server_name,
            &self.address,
            Some(&self.tls),
            Duration::from_secs(2),
        )
        .and_then(|client| client.call(method, &input))
        .map_err(|error| error.to_string())
    }
}

fn wait_metrics(node: &Joined, phase: Option<&str>, guard: &mut Guard) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    while Instant::now() < deadline {
        assert!(
            guard.0.try_wait().unwrap().is_none(),
            "guardian exited: {last}"
        );
        match node.call(METRICS, json!({})) {
            Ok(value) if phase.is_none_or(|phase| value["update"]["phase"] == phase) => {
                return value;
            }
            Ok(value) => last = value.to_string(),
            Err(error) => last = error,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("node did not report phase {phase:?}: {last}");
}

fn apply(node: &Joined, stage: &Path, name: &str, version: &str) -> Value {
    let bytes = fs::read(stage).unwrap();
    node.call(
        APPLY,
        json!({"path":name,"size":bytes.len().to_string(),
            "sha256":format!("{:x}", Sha256::digest(&bytes)), "expectedVersion":version}),
    )
    .unwrap()
}

/// A task-style install in `root`: the guardian copy runs `node serve`, which
/// launches protectorate-rs.exe beside it as the node child.
fn start_guardian(root: &Path) -> (Joined, Guard, PathBuf) {
    let source = Path::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"));
    let guardian = root.join("protectorate-rs-guardian.exe");
    fs::copy(source, &guardian).unwrap();
    fs::copy(source, root.join("protectorate-rs.exe")).unwrap();
    let home = root.join("state");
    let files = home.join("node-files");
    fs::create_dir_all(&files).unwrap();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let node = Joined::create(&home, addr);
    let child = Command::new(&guardian)
        .args(["node", "serve", "--home"])
        .arg(&home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    (node, Guard(child), files)
}

struct Process(HANDLE);

impl Process {
    /// Opens the single protectorate-rs.exe child of the live process `parent`.
    fn child_of(parent: u32) -> Self {
        // SAFETY: a process snapshot takes no pointers and its handle is closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        assert_ne!(snapshot, INVALID_HANDLE_VALUE, "process snapshot failed");
        let mut entry = PROCESSENTRY32W {
            dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut children = Vec::new();
        // SAFETY: entry is a properly sized PROCESSENTRY32W for this snapshot.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if entry.th32ParentProcessID == parent
                && name.eq_ignore_ascii_case("protectorate-rs.exe")
            {
                children.push(entry.th32ProcessID);
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: the snapshot handle is owned here and closed once.
        unsafe { CloseHandle(snapshot) };
        assert_eq!(
            children.len(),
            1,
            "node children of guardian {parent}: {children:?}"
        );
        // SAFETY: opening a PID only yields a handle or null.
        let handle =
            unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, children[0]) };
        assert!(!handle.is_null(), "open node child {}", children[0]);
        Self(handle)
    }

    fn exited_within(&self, timeout: Duration) -> bool {
        // SAFETY: the handle is live and opened with SYNCHRONIZE.
        unsafe { WaitForSingleObject(self.0, timeout.as_millis() as u32) == WAIT_OBJECT_0 }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle names the node child this test located; it is closed once.
        unsafe {
            TerminateProcess(self.0, 1);
            CloseHandle(self.0);
        }
    }
}

#[test]
fn node_child_dies_with_its_guardian() {
    let temp = tempfile::tempdir().unwrap();
    let (node, mut guard, _) = start_guardian(temp.path());
    wait_metrics(&node, None, &mut guard);
    let child = Process::child_of(guard.0.id());
    guard.0.kill().unwrap();
    guard.0.wait().unwrap();
    assert!(
        child.exited_within(Duration::from_secs(10)),
        "node child outlived its killed guardian"
    );
}

#[test]
fn update_keeps_task_guardian_and_recovers_failing_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"));
    let binary = temp.path().join("protectorate-rs.exe");
    let (node, mut guard, files) = start_guardian(temp.path());
    let original = fs::read(&binary).unwrap();
    let version = wait_metrics(&node, None, &mut guard)["agentVersion"]
        .as_str()
        .unwrap()
        .to_owned();

    let good = files.join("good.exe");
    fs::copy(source, &good).unwrap();
    let reply = apply(&node, &good, "good.exe", &version);
    assert_eq!(reply["version"], version);
    let confirmed = wait_metrics(&node, Some("confirmed"), &mut guard);
    assert_eq!(confirmed["update"]["attempts"], 1);
    assert_eq!(fs::read(&binary).unwrap(), original);

    let failing = files.join("failing.exe");
    let code = temp.path().join("failing.rs");
    fs::write(
        &code,
        format!(
            "fn main() {{ if std::env::args().nth(1).as_deref() == Some(\"version\") {{ println!(\"{{}}\", r#\"{{\"version\":\"{version}\",\"os\":\"windows\",\"arch\":\"amd64\"}}\"#); }} else {{ std::process::exit(23); }} }}"
        ),
    )
    .unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2021"])
        .arg(&code)
        .arg("-o")
        .arg(&failing)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    apply(&node, &failing, "failing.exe", &version);
    let rolled_back = wait_metrics(&node, Some("rolledback"), &mut guard);
    assert_eq!(rolled_back["agentVersion"], version);
    assert_eq!(fs::read(&binary).unwrap(), original);
    assert!(guard.0.try_wait().unwrap().is_none());
}
