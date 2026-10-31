//! `serve --stdio` as sshd would run it: frames over the child's own stdin and stdout, no header
//! line. Each session is a splice into the resident agent for the state directory, so a job
//! outlives the session that submitted it.
#![cfg(target_os = "linux")]

mod common;

use common::{Agent, BIN, json, num, text};
use pt_frame::Kind;
use pt_frame::wire::{FrameReader, write_frame};
use pt_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: FrameReader<ChildStdout>,
    next: u64,
}

impl Session {
    fn open(dir: &Path) -> Session {
        let mut child = Command::new(BIN)
            .args([
                "serve",
                "--stdio",
                "--idle-exit",
                "2",
                "--presence-interval",
                "1",
            ])
            .arg("--state-dir")
            .arg(dir.join("state"))
            .env("PROTECTORATE_NODE_TEST_PRESENCE", dir.join("presence"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("session.log"))
                    .unwrap(),
            )
            .spawn()
            .expect("spawn serve --stdio");
        let stdin = child.stdin.take();
        let frames = FrameReader::new(child.stdout.take().unwrap(), &[]);
        Session {
            child,
            stdin,
            frames,
            next: 7,
        }
    }

    /// Sends one request; returns every frame of its answer.
    fn call(&mut self, request: &str) -> Vec<(Kind, Vec<u8>)> {
        self.next += 1;
        let id = self.next;
        write_frame(
            self.stdin.as_mut().unwrap(),
            Kind::Request,
            id,
            request.as_bytes(),
        )
        .unwrap();
        let mut got = Vec::new();
        loop {
            let f = self
                .frames
                .read()
                .expect("a frame")
                .expect("an answer before EOF");
            assert_eq!(f.id, id);
            let last = f.kind != Kind::Chunk;
            got.push((f.kind, f.payload));
            if last {
                return got;
            }
        }
    }

    fn ok(&mut self, request: &str) -> Value {
        let mut frames = self.call(request);
        assert_eq!(frames.len(), 1);
        let (kind, payload) = frames.remove(0);
        let v = json(&String::from_utf8(payload).unwrap());
        assert_eq!(kind, Kind::Response, "{request}: {v}");
        v
    }

    /// Closes stdin and expects the session to end cleanly with nothing more on stdout.
    fn close(mut self) -> ExitStatus {
        drop(self.stdin.take());
        assert!(
            self.frames.read().unwrap().is_none(),
            "bytes after the last answer"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                return s;
            }
            assert!(
                Instant::now() < deadline,
                "session did not end after stdin EOF"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fresh_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("stdio")
        .join(format!("{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("presence"), "AWAY").unwrap();
    dir
}

fn wait_for_log(dir: &Path, needle: &str, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let log = fs::read_to_string(dir.join("state").join("agent.log")).unwrap_or_default();
        if log.matches(needle).count() >= count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "no {needle:?} x{count} in: {log}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_job_outlives_the_stdio_session_that_submitted_it() {
    let dir = fresh_dir("outlive");
    let mut s1 = Session::open(&dir);
    let hello = s1.ok(r#"{"type":"Hello"}"#);
    let epoch = text(&hello, "journal_epoch").to_string();
    let job = s1.ok(
        r#"{"type":"SubmitJob","job_id":"j","attempt":1,"script":"echo one; sleep 2; echo two"}"#,
    );
    assert_eq!(text(&job["job"], "state"), "running");
    let started = Instant::now();
    assert!(s1.close().success());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the session waited for the job"
    );

    let mut s2 = Session::open(&dir);
    let deadline = Instant::now() + Duration::from_secs(10);
    let done = loop {
        let st = s2.ok(r#"{"type":"JobStatus","job_id":"j","attempt":1}"#);
        assert_eq!(text(&st, "journal_epoch"), epoch);
        if text(&st["job"], "state") != "running" {
            break st["job"].clone();
        }
        assert!(Instant::now() < deadline, "job never finished: {st}");
        thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(text(&done, "state"), "succeeded", "{done}");
    assert_eq!(num(&done, "log_bytes"), 8);
    let frames = s2.call(r#"{"type":"FetchLog","job_id":"j","attempt":1}"#);
    let (end_kind, end) = frames.last().unwrap();
    assert_eq!(*end_kind, Kind::End);
    let bytes: Vec<u8> = frames[..frames.len() - 1]
        .iter()
        .flat_map(|(k, p)| {
            assert_eq!(*k, Kind::Chunk);
            p.clone()
        })
        .collect();
    assert_eq!(bytes, b"one\ntwo\n");
    assert_eq!(
        json(&String::from_utf8_lossy(end))["done"].as_bool(),
        Some(true)
    );
    assert!(s2.close().success());

    // Idle, the resident leaves; the next session starts a new one over the same journal.
    wait_for_log(&dir, "idle for 2 s, exiting", 1);
    let mut s3 = Session::open(&dir);
    let st = s3.ok(r#"{"type":"JobStatus","job_id":"j","attempt":1}"#);
    assert_eq!(text(&st, "journal_epoch"), epoch);
    assert_eq!(text(&st["job"], "state"), "succeeded");
    assert!(s3.close().success());
    wait_for_log(&dir, "idle for 2 s, exiting", 2);
    let log = fs::read_to_string(dir.join("session.log")).unwrap();
    assert!(log.is_empty(), "the sessions logged: {log}");
}

#[test]
fn a_second_agent_on_one_state_dir_is_refused() {
    let a = Agent::start("lock", &[]);
    let out = Command::new(BIN)
        .current_dir(&a.dir)
        .args(["serve", "--state-dir", "state", "--forward-socket", "other"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("in use by another node agent"), "{err}");
    a.ok("Hello", "{}");
}
