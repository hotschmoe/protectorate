//! Shared by the e2e test files; each file uses part of it.
#![allow(dead_code)]

use pt_json::Value;

pub const BIN: &str = env!("CARGO_BIN_EXE_protectorate-node");

/// Parses with pt-json's strict parser.
pub fn json(s: &str) -> Value {
    pt_json::parse(s.trim()).unwrap_or_else(|e| panic!("invalid JSON ({e}): {s}"))
}

pub fn num(v: &Value, key: &str) -> u64 {
    v[key]
        .as_u64()
        .unwrap_or_else(|| panic!("field {key} is not an unsigned number in {v}"))
}

pub fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("field {key} is not a string in {v}"))
}

pub fn check_inventory(v: &Value) {
    assert!(num(v, "ram_total_bytes") > 0);
    assert!(num(v, "threads") >= 1);
    assert!(num(v, "cores") >= 1 && num(v, "cores") <= num(v, "threads"));
    let disks = v["disks"].as_array().expect("disks");
    assert!(!disks.is_empty(), "no disk in {v}");
    assert!(num(&disks[0], "total_bytes") > 0);
    assert!(!text(v, "hostname").is_empty());
    assert!(!text(v, "cpu_model").is_empty());
    assert!(!text(v, "arch").is_empty());
    assert!(v["toolchains"].as_array().is_some(), "{v}");
}

pub fn check_sample(v: &Value) {
    let total = num(v, "mem_total_bytes");
    let avail = num(v, "mem_available_bytes");
    assert!(total > 0 && avail > 0 && avail <= total);
    assert_eq!(num(v, "mem_used_bytes"), total - avail);
    assert!(num(v, "uptime_secs") > 0);
    assert!(
        !v["disks"].as_array().expect("disks").is_empty(),
        "no disk in {v}"
    );
    let cpu = v["cpu_percent"].as_f64().expect("cpu_percent");
    assert!((0.0..=100.0).contains(&cpu));
}

pub fn check_presence_value(v: &Value, away_after: u64) {
    let state = text(v, "state");
    assert!(
        ["PRESENT", "AWAY", "UNKNOWN"].contains(&state),
        "state {state}"
    );
    assert!(!text(v, "source").is_empty(), "no source in {v}");
    assert_eq!(num(v, "away_after_secs"), away_after);
    if state == "AWAY" {
        // An idle figure, when there is one, must be at or past the threshold or the screen locked.
        if let Some(idle) = v["idle_secs"].as_u64() {
            let locked = v["locked"].as_bool() == Some(true);
            assert!(idle >= away_after || locked, "AWAY with idle {idle} in {v}");
        }
    }
}

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A `serve` agent in its own fresh directory, reached through a relative unix socket (or a
/// loopback port), with presence forced through the test override file (AWAY at start).
pub struct Agent {
    child: Option<Child>,
    pub dir: PathBuf,
    pub connect: String,
    extra: Vec<String>,
    tcp: bool,
    starts: u32,
}

impl Agent {
    pub fn start(name: &str, extra: &[&str]) -> Agent {
        Agent::launch(name, extra, false)
    }

    pub fn start_tcp(name: &str, extra: &[&str]) -> Agent {
        Agent::launch(name, extra, true)
    }

    fn launch(name: &str, extra: &[&str], tcp: bool) -> Agent {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("serve")
            .join(format!("{name}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let mut a = Agent {
            child: None,
            dir,
            connect: String::new(),
            extra: extra.iter().map(|s| s.to_string()).collect(),
            tcp,
            starts: 0,
        };
        a.set_presence("AWAY");
        a.spawn();
        a
    }

    fn spawn(&mut self) {
        self.starts += 1;
        let log = self.dir.join(format!("serve-{}.log", self.starts));
        let mut cmd = Command::new(BIN);
        cmd.current_dir(&self.dir).args([
            "serve",
            "--state-dir",
            "state",
            "--presence-interval",
            "1",
        ]);
        if self.tcp {
            cmd.args(["--listen-tcp", "127.0.0.1:0"]);
        } else {
            cmd.args(["--forward-socket", "sock"]);
        }
        let child = cmd
            .args(&self.extra)
            .env("PROTECTORATE_NODE_TEST_PRESENCE", self.dir.join("presence"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .expect("spawn serve");
        self.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let text = fs::read_to_string(&log).unwrap_or_default();
            if let Some(at) = text
                .lines()
                .find_map(|l| l.strip_prefix("protectorate-node: listening on "))
            {
                self.connect = at.trim().to_string();
                return;
            }
            if let Some(Ok(Some(status))) = self.child.as_mut().map(Child::try_wait) {
                panic!("serve exited with {status}: {text}");
            }
            assert!(Instant::now() < deadline, "serve did not start: {text}");
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// The agent's stderr from every start so far.
    pub fn log(&self) -> String {
        (1..=self.starts)
            .map(|i| {
                fs::read_to_string(self.dir.join(format!("serve-{i}.log"))).unwrap_or_default()
            })
            .collect()
    }

    /// SIGKILL, so nothing gets a chance to clean up.
    pub fn kill(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn restart(&mut self) {
        self.kill();
        self.spawn();
    }

    pub fn set_presence(&self, state: &str) {
        fs::write(self.dir.join("presence"), state).unwrap();
    }

    pub fn rpc_with(&self, pre: &[&str], method: &str, fields: &str) -> Output {
        Command::new(BIN)
            .current_dir(&self.dir)
            .args(["rpc", "--connect", &self.connect])
            .args(pre)
            .args([method, fields])
            .output()
            .expect("spawn rpc")
    }

    /// Exit code and the parsed answer of a request that gets a response or an error frame.
    pub fn call(&self, method: &str, fields: &str) -> (i32, Value) {
        let out = self.rpc_with(&[], method, fields);
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            code == 0 || code == 1,
            "{method} {fields}: exit {code}: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            self.log()
        );
        (code, json(&stdout))
    }

    pub fn ok(&self, method: &str, fields: &str) -> Value {
        let (code, v) = self.call(method, fields);
        assert_eq!(code, 0, "{method} {fields} answered an error: {v}");
        v
    }

    pub fn err(&self, method: &str, fields: &str) -> Value {
        let (code, v) = self.call(method, fields);
        assert_eq!(code, 1, "{method} {fields} did not answer an error: {v}");
        v
    }

    /// Sends the fields unchecked and expects an error answer.
    pub fn err_raw(&self, method: &str, fields: &str) -> Value {
        let out = self.rpc_with(&["--raw"], method, fields);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{method} {fields}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        json(&String::from_utf8_lossy(&out.stdout))
    }

    pub fn submit(&self, fields: &str) -> Value {
        self.ok("SubmitJob", fields)["job"].clone()
    }

    pub fn status(&self, job: &str, attempt: u64) -> Value {
        self.ok(
            "JobStatus",
            &format!(r#"{{"job_id":"{job}","attempt":{attempt}}}"#),
        )["job"]
            .clone()
    }

    /// Polls JobStatus until the state is one of `states`.
    pub fn wait_state(&self, job: &str, attempt: u64, states: &[&str], within: Duration) -> Value {
        let deadline = Instant::now() + within;
        loop {
            let j = self.status(job, attempt);
            if states.contains(&j["state"].as_str().unwrap_or("")) {
                return j;
            }
            assert!(
                Instant::now() < deadline,
                "job {job} attempt {attempt} never reached {states:?}: {j}\n{}",
                self.log()
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// The bytes of a chunked answer (stdout) and its end frame (stderr).
    pub fn stream(&self, method: &str, fields: &str) -> (Vec<u8>, Value) {
        let out = self.rpc_with(&[], method, fields);
        assert!(
            out.status.success(),
            "{method} {fields} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.stdout, json(&String::from_utf8_lossy(&out.stderr)))
    }

    /// Log bytes from `offset` and the end frame.
    pub fn fetch(&self, job: &str, attempt: u64, offset: u64, max: u64) -> (Vec<u8>, Value) {
        self.stream(
            "FetchLog",
            &format!(r#"{{"job_id":"{job}","attempt":{attempt},"offset":{offset},"max":{max}}}"#),
        )
    }

    pub fn journal(&self) -> String {
        fs::read_to_string(self.dir.join("state").join("jobs.jsonl")).unwrap()
    }

    pub fn state_dir(&self) -> PathBuf {
        self.dir.join("state")
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        self.kill();
    }
}

/// A live, non-zombie process.
pub fn alive(pid: u64) -> bool {
    match fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat")) {
        Ok(stat) => stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|s| s != "Z"),
        Err(_) => false,
    }
}
