//! Real-binary fixtures: a hub with a TLS listener, and nodes that join it
//! with an invite code and serve mutual TLS.
#![allow(dead_code)]
use protectorate_connect_client::Client;
use serde_json::{json, Value};
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const OWNER: &str = "real-binary-owner-token";

pub struct Process(pub Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn free_port() -> String {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .to_string()
}

pub fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_protectorate-rs")
}

fn command(args: &[&str], home: &Path) -> Command {
    let mut command = Command::new(binary());
    command
        .args(args)
        .env("HOME", home)
        .env_remove("PROTECTORATE_HOME")
        .env_remove("PROTECTORATE_HUB")
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .env_remove("PROTECTORATE_JOIN_CODE");
    command
}

pub fn run(args: &[&str], home: &Path) -> Output {
    command(args, home).output().unwrap()
}

pub fn spawn(args: &[&str], home: &Path, log: &Path) -> Process {
    Process(
        command(args, home)
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(log).unwrap()))
            .spawn()
            .unwrap(),
    )
}

pub fn until(what: &str, log: &Path, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{what} timed out: {}",
            fs::read_to_string(log).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

/// A hub serving plain HTTP on loopback for the owner and HTTPS with its own
/// CA for joining nodes.
pub struct Hub {
    pub process: Option<Process>,
    pub client: Client,
    pub home: PathBuf,
    pub user_home: PathBuf,
    pub plain: String,
    pub tls: String,
    args: Vec<String>,
    log: PathBuf,
}

impl Hub {
    pub fn start(root: &Path) -> Self {
        Self::start_with(root, &[])
    }

    pub fn start_with(root: &Path, extra: &[&str]) -> Self {
        let user_home = root.join("user");
        let home = root.join("hub");
        fs::create_dir_all(&user_home).unwrap();
        fs::create_dir_all(&home).unwrap();
        let token = root.join("owner-token");
        fs::write(&token, OWNER).unwrap();
        fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
        let plain = free_port();
        let tls = free_port();
        let args = [
            "serve",
            "--home",
            home.to_str().unwrap(),
            "--owner-token-file",
            token.to_str().unwrap(),
            "--listen",
            &plain,
            "--tls-listen",
            &tls,
            "--node-interval",
            "1s",
            "--release-api-url",
            "http://127.0.0.1:1",
            "--tmux-socket",
            &format!("p-real-{}", std::process::id()),
        ]
        .map(str::to_owned)
        .into_iter()
        .chain(extra.iter().map(|arg| (*arg).to_owned()))
        .collect::<Vec<_>>();
        let client =
            Client::new(&format!("http://{plain}"), OWNER, Duration::from_secs(5)).unwrap();
        let mut hub = Self {
            process: None,
            client,
            home,
            user_home,
            plain: format!("http://{plain}"),
            tls: format!("https://{tls}"),
            args,
            log: root.join("hub.log"),
        };
        hub.restart();
        hub
    }

    pub fn args(&self) -> Vec<&str> {
        self.args.iter().map(String::as_str).collect()
    }

    pub fn stop(&mut self) {
        self.process = None;
    }

    /// Runs the hub server itself, without its update guardian, so that
    /// stopping the process stops the listeners.
    pub fn restart(&mut self) {
        self.process = Some(Process(
            command(&self.args(), &self.user_home)
                .env("PROTECTORATE_HUB_GUARD_CHILD", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::from(fs::File::create(&self.log).unwrap()))
                .spawn()
                .unwrap(),
        ));
        let client = self.client.clone();
        until("hub start", &self.log, || {
            client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .is_ok()
        });
    }

    pub fn ca(&self) -> PathBuf {
        self.home.join("ca/ca.pem")
    }

    pub fn call(&self, procedure: &str, input: Value) -> Value {
        self.client.call(procedure, &input).unwrap()
    }

    /// Invites `name` and joins `node_home` to the hub with the invite code,
    /// listening on a fresh loopback port. Returns the invite response.
    pub fn join(&self, name: &str, node_home: &Path) -> Value {
        let invite = self.call(
            "/protectorate.v1.NodeService/InviteNode",
            json!({"name": name, "tier": "care"}),
        );
        let output = run(
            &[
                "node",
                "join",
                "--hub",
                invite["hubUrl"].as_str().unwrap(),
                "--code",
                invite["code"].as_str().unwrap(),
                "--ca-cert",
                self.ca().to_str().unwrap(),
                "--name",
                name,
                "--listen",
                &free_port(),
                "--home",
                node_home.to_str().unwrap(),
            ],
            &self.user_home,
        );
        assert!(
            output.status.success(),
            "node join: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        invite
    }

    pub fn status(&self, name: &str) -> Value {
        self.call(
            "/protectorate.v1.NodeService/GetNode",
            json!({"name": name}),
        )
    }

    pub fn wait_fresh(&self, name: &str, node_log: &Path) {
        until("node poll over mutual TLS", node_log, || {
            self.status(name)["freshness"] == "NODE_FRESHNESS_FRESH"
        });
    }
}

/// `node serve` for a joined node home, with plain jobs allowed.
pub fn serve_node(node_home: &Path, user_home: &Path, log: &Path) -> Process {
    let helper = Path::new(binary()).with_file_name("protectorate-node-sandbox-helper");
    spawn(
        &[
            "node",
            "serve",
            "--home",
            node_home.to_str().unwrap(),
            "--allow-plain-jobs",
            "--sandbox-helper",
            helper.to_str().unwrap(),
        ],
        user_home,
        log,
    )
}
