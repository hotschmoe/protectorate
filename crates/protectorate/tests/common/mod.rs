#![allow(dead_code)]
//! Runs the real `protectorate` binary as a hub on a temp `--home`, and drives it through the
//! real CLI. Sleeves run Pi against the owner's local model (free); harnesses that spend
//! subscription quota run only under `PT_E2E_PAID=1`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pt_json::Value;

pub const READY: &str = "Reply with exactly the word READY and nothing else.";
pub const LONG: &str =
    "Write the numbers from 1 to 400 as words, one per line, with no other text.";

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_protectorate")
}

/// Short on purpose: the hub's socket path must fit in a sockaddr_un (108 bytes).
fn unique(name: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() % 100_000);
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{t}-{n}"))
}

pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// Pi, bwrap and a systemd user manager, or a SKIP line and false.
pub fn have_runtime() -> bool {
    for tool in ["pi", "bwrap", "systemd-run", "git"] {
        if !on_path(tool) {
            eprintln!("SKIP: {tool} is not installed");
            return false;
        }
    }
    true
}

pub fn pi_provider() -> String {
    std::env::var("PT_E2E_PI_PROVIDER").unwrap_or_else(|_| "hotschmoe".into())
}

pub fn pi_model() -> String {
    std::env::var("PT_E2E_PI_MODEL").unwrap_or_else(|_| {
        "qwen3.8-27b-FP8-official-W8A16-mtp3-r187-xpugraph-cacheon-ctx200000-daily".into()
    })
}

/// A fresh git repo with one commit (left in place under the target tmp dir).
pub fn temp_repo(name: &str) -> PathBuf {
    let dir = unique(&format!("repo-{name}"));
    fs::create_dir_all(&dir).expect("create repo");
    fs::write(dir.join("notes.txt"), "The secret word is PELICAN.\n").expect("write notes");
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "-A"],
        &["commit", "-qm", "init"],
    ] {
        let ok = git(&dir, args).status.success();
        assert!(ok, "git {args:?}");
    }
    fs::canonicalize(dir).expect("canonical repo")
}

pub fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "e2e")
        .env("GIT_AUTHOR_EMAIL", "e2e@localhost")
        .env("GIT_COMMITTER_NAME", "e2e")
        .env("GIT_COMMITTER_EMAIL", "e2e@localhost")
        .output()
        .expect("run git")
}

pub fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git(dir, args).stdout)
        .trim()
        .to_string()
}

pub fn eventually<T>(what: &str, timeout: Duration, mut f: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub struct Run {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn ok(&self) -> &str {
        assert_eq!(self.status, 0, "command failed: {}", self.stderr);
        &self.stdout
    }

    pub fn json(&self) -> Value {
        pt_json::parse(self.ok().trim()).expect("JSON output")
    }
}

pub struct Hub {
    pub home: PathBuf,
    pub log: PathBuf,
    child: Option<Child>,
    restarts: usize,
    extra: Vec<String>,
    env: Vec<(String, String)>,
}

impl Hub {
    /// A hub whose Envoy also runs on pi_local, unless `envoy` names another harness.
    pub fn start(name: &str) -> Hub {
        Hub::start_with(name, "pi_local", &[], &[])
    }

    pub fn start_with(name: &str, envoy: &str, extra: &[&str], env: &[(&str, &str)]) -> Hub {
        let home = unique(name).join("home");
        fs::create_dir_all(&home).expect("create home");
        let config = pt_json::obj! {
            "envoy" => pt_json::obj! { "harness" => envoy },
            "pi_local_provider" => pi_provider(),
            "pi_local_model" => pi_model(),
        };
        fs::write(home.join("config.json"), config.to_string()).expect("write config");
        let mut hub = Hub {
            log: home.with_file_name("serve-0.log"),
            home,
            child: None,
            restarts: 0,
            extra: extra.iter().map(|s| s.to_string()).collect(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        hub.spawn();
        hub
    }

    fn spawn(&mut self) {
        self.log = self
            .home
            .with_file_name(format!("serve-{}.log", self.restarts));
        let log = fs::File::create(&self.log).expect("create serve log");
        let mut cmd = Command::new(bin());
        cmd.arg("--home")
            .arg(&self.home)
            .args(["serve", "--log-level", "debug"])
            .args(if self.extra.iter().any(|a| a == "--listen") {
                &[][..]
            } else {
                &["--listen", "off"][..]
            })
            .args(&self.extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log);
        // This suite may run under Claude Code; the hub and its sleeves must not see that.
        for (k, _) in std::env::vars().filter(|(k, _)| k.starts_with("CLAUDE")) {
            cmd.env_remove(k);
        }
        cmd.env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_SOCKET");
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        self.child = Some(cmd.spawn().expect("start protectorate serve"));
        eventually("the hub to answer", Duration::from_secs(20), || {
            (self.cli(&["--json", "state"]).status == 0).then_some(())
        });
    }

    /// SIGKILL, as a crash: no listener shutdown, nothing flushed beyond what was fsynced.
    pub fn kill9(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn restart(&mut self) {
        self.kill9();
        self.restarts += 1;
        self.spawn();
    }

    pub fn log_text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    pub fn cli_env(&self, env: &[(&str, &str)], args: &[&str]) -> Run {
        let mut cmd = Command::new(bin());
        cmd.arg("--home").arg(&self.home).args(args);
        cmd.env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_SOCKET");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("run protectorate");
        Run {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    pub fn cli(&self, args: &[&str]) -> Run {
        self.cli_env(&[], args)
    }

    pub fn state(&self) -> Value {
        self.cli(&["--json", "state"]).json()
    }

    pub fn sleeve(&self, name: &str) -> Value {
        let st = self.state();
        st["sleeves"]
            .as_array()
            .unwrap_or(&[])
            .iter()
            .find(|s| s["name"].as_str() == Some(name))
            .cloned()
            .unwrap_or(Value::Null)
    }

    pub fn spawn_pi(&self, name: &str, repo: &Path) -> Value {
        let v = self
            .cli(&[
                "--json",
                "sleeve",
                "spawn",
                name,
                "--path",
                repo.to_str().expect("utf-8 path"),
                "--harness",
                "pi_local",
            ])
            .json();
        self.wait(name, "idle", 120);
        v["sleeve"].clone()
    }

    pub fn wait(&self, sleeve: &str, until: &str, secs: u64) {
        let r = self.cli(&[
            "wait",
            sleeve,
            "--until",
            until,
            "--timeout",
            &secs.to_string(),
        ]);
        assert_eq!(
            r.status,
            0,
            "wait {sleeve} until {until}: {} {}\n--- hub log tail ---\n{}",
            r.stdout,
            r.stderr,
            tail(&self.log_text(), 40)
        );
    }

    pub fn send(&self, sleeve: &str, text: &str) -> String {
        let v = self.cli(&["--json", "sleeve", "send", sleeve, text]).json();
        v["request_id"].as_str().expect("request id").to_string()
    }

    /// The stored transcript, raw normalized entries.
    pub fn transcript(&self, sleeve: &str) -> Vec<Value> {
        let v = self.cli(&["--json", "transcript", sleeve, "--raw"]).json();
        v["entries"].as_array().unwrap_or(&[]).to_vec()
    }

    pub fn hub_tag(&self) -> String {
        let t = fs::read_to_string(self.home.join("tokens")).expect("tokens file");
        let v = pt_json::parse(&t).expect("tokens JSON");
        v["hub_tag"].as_str().expect("hub tag").to_string()
    }

    pub fn scope(&self, id: &str, g: u64) -> String {
        format!("pt-{}-{id}-g{g}.scope", self.hub_tag())
    }

    /// The line `<prefix><value>` the first start printed to stderr.
    pub fn printed(&self, prefix: &str) -> String {
        let first = self.home.with_file_name("serve-0.log");
        let text = fs::read_to_string(first).unwrap_or_default();
        text.lines()
            .find_map(|l| l.strip_prefix(prefix))
            .expect("printed line")
            .trim()
            .to_string()
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        // The sandboxes die with the hub (bwrap --die-with-parent).
        self.kill9();
    }
}

pub fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Pids in a systemd user scope, empty when the scope is gone.
pub fn scope_pids(scope: &str) -> Vec<u32> {
    let out = Command::new("systemctl")
        .args(["--user", "show", "-p", "ControlGroup", "--value", scope])
        .output()
        .expect("systemctl");
    let cg = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if cg.is_empty() {
        return Vec::new();
    }
    let procs = fs::read_to_string(format!("/sys/fs/cgroup{cg}/cgroup.procs")).unwrap_or_default();
    procs
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

/// The text of every assistant entry.
pub fn assistant_text(entries: &[Value]) -> String {
    entries
        .iter()
        .filter(|e| e["entry"]["type"].as_str() == Some("assistant_text"))
        .filter_map(|e| e["entry"]["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn of_type<'a>(entries: &'a [Value], t: &str) -> Vec<&'a Value> {
    entries
        .iter()
        .filter(|e| e["entry"]["type"].as_str() == Some(t))
        .collect()
}
