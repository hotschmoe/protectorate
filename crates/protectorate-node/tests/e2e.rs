//! Runs the built binary on this machine and checks the shape and sanity of what it prints.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_protectorate-node"))
        .args(args)
        .output()
        .expect("spawn protectorate-node")
}

fn stdout_ok(args: &[&str]) -> String {
    let out = run(args);
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// Strict JSON syntax check, so the hand-written writers are held to the grammar.
fn assert_valid_json(s: &str) {
    struct P<'a> {
        b: &'a [u8],
        i: usize,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.b.len() && b" \n\r\t".contains(&self.b[self.i]) {
                self.i += 1;
            }
        }
        fn eat(&mut self, c: u8) {
            self.ws();
            assert_eq!(
                self.b.get(self.i),
                Some(&c),
                "expected '{}' at {}",
                c as char,
                self.i
            );
            self.i += 1;
        }
        fn lit(&mut self, w: &str) {
            assert!(
                self.b[self.i..].starts_with(w.as_bytes()),
                "bad literal at {}",
                self.i
            );
            self.i += w.len();
        }
        fn string(&mut self) {
            self.eat(b'"');
            loop {
                let c = *self.b.get(self.i).expect("unterminated string");
                self.i += 1;
                match c {
                    b'"' => return,
                    b'\\' => {
                        let e = self.b[self.i];
                        self.i += 1;
                        if e == b'u' {
                            assert!(self.b[self.i..self.i + 4].iter().all(u8::is_ascii_hexdigit));
                            self.i += 4;
                        } else {
                            assert!(b"\"\\/bfnrt".contains(&e), "bad escape");
                        }
                    }
                    c => assert!(c >= 0x20, "raw control byte in string"),
                }
            }
        }
        fn value(&mut self) {
            self.ws();
            match self.b[self.i] {
                b'{' => {
                    self.i += 1;
                    self.ws();
                    if self.b[self.i] == b'}' {
                        self.i += 1;
                        return;
                    }
                    loop {
                        self.string();
                        self.eat(b':');
                        self.value();
                        self.ws();
                        if self.b[self.i] == b',' {
                            self.i += 1;
                        } else {
                            return self.eat(b'}');
                        }
                    }
                }
                b'[' => {
                    self.i += 1;
                    self.ws();
                    if self.b[self.i] == b']' {
                        self.i += 1;
                        return;
                    }
                    loop {
                        self.value();
                        self.ws();
                        if self.b[self.i] == b',' {
                            self.i += 1;
                        } else {
                            return self.eat(b']');
                        }
                    }
                }
                b'"' => self.string(),
                b't' => self.lit("true"),
                b'f' => self.lit("false"),
                b'n' => self.lit("null"),
                _ => {
                    let start = self.i;
                    while self.i < self.b.len() && b"-+.eE0123456789".contains(&self.b[self.i]) {
                        self.i += 1;
                    }
                    assert!(self.i > start, "unexpected byte at {start}");
                }
            }
        }
    }
    let mut p = P {
        b: s.trim().as_bytes(),
        i: 0,
    };
    p.value();
    p.ws();
    assert_eq!(p.i, p.b.len(), "trailing bytes after JSON value");
}

/// First top-level-ish occurrence of `"key":` and the raw token after it.
fn raw_field<'a>(json: &'a str, key: &str) -> &'a str {
    let pat = format!("\"{key}\":");
    let at = json
        .find(&pat)
        .unwrap_or_else(|| panic!("no field {key} in {json}"))
        + pat.len();
    let rest = &json[at..];
    let end = rest.find([',', '}', ']']).unwrap_or(rest.len());
    &rest[..end]
}

fn num(json: &str, key: &str) -> u64 {
    raw_field(json, key)
        .parse()
        .unwrap_or_else(|_| panic!("field {key} is not an unsigned number in {json}"))
}

#[test]
fn version() {
    let out = stdout_ok(&["--version"]);
    assert!(out.starts_with("protectorate-node "), "{out}");
}

#[test]
fn unknown_command_exits_2() {
    let out = run(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown command"));
}

#[test]
fn inventory_is_sane() {
    let out = stdout_ok(&["inventory"]);
    assert_valid_json(&out);
    assert!(num(&out, "ram_total_bytes") > 0);
    assert!(num(&out, "threads") >= 1);
    assert!(num(&out, "cores") >= 1 && num(&out, "cores") <= num(&out, "threads"));
    assert!(out.contains("\"disks\":[{"), "no disk in {out}");
    assert!(num(&out, "total_bytes") > 0);
    assert!(!raw_field(&out, "hostname").is_empty() && raw_field(&out, "hostname") != "\"\"");
    assert!(!raw_field(&out, "cpu_model").is_empty() && raw_field(&out, "cpu_model") != "\"\"");
    assert!(!raw_field(&out, "arch").contains("null"));
    assert!(out.contains("\"toolchains\":["));
}

#[test]
fn sample_is_sane() {
    let out = stdout_ok(&["sample"]);
    assert_valid_json(&out);
    let total = num(&out, "mem_total_bytes");
    let avail = num(&out, "mem_available_bytes");
    assert!(total > 0 && avail > 0 && avail <= total);
    assert_eq!(num(&out, "mem_used_bytes"), total - avail);
    assert!(num(&out, "uptime_secs") > 0);
    assert!(out.contains("\"disks\":[{"), "no disk in {out}");
    let cpu: f64 = raw_field(&out, "cpu_percent").parse().expect("cpu_percent");
    assert!((0.0..=100.0).contains(&cpu));
}

fn check_presence(out: &str, away_after: u64) {
    assert_valid_json(out);
    let state = raw_field(out, "state");
    assert!(
        ["\"PRESENT\"", "\"AWAY\"", "\"UNKNOWN\""].contains(&state),
        "state {state}"
    );
    let source = raw_field(out, "source");
    assert!(source.len() > 2 && source != "null", "no source in {out}");
    assert_eq!(num(out, "away_after_secs"), away_after);
    if state == "\"AWAY\"" {
        // An idle figure, when there is one, must be at or past the threshold or the session absent.
        let idle = raw_field(out, "idle_secs");
        if idle != "null" {
            let idle: u64 = idle.parse().expect("idle_secs");
            let locked = raw_field(out, "locked") == "true";
            assert!(
                idle >= away_after || locked,
                "AWAY with idle {idle} in {out}"
            );
        }
    }
}

#[test]
fn presence_names_its_source() {
    check_presence(&stdout_ok(&["presence"]), 600);
}

#[test]
fn presence_threshold_is_configurable() {
    check_presence(&stdout_ok(&["presence", "--away-after", "5"]), 5);
}

#[test]
fn presence_rejects_bad_flags() {
    assert_eq!(run(&["presence", "--away-after"]).status.code(), Some(2));
    assert_eq!(run(&["presence", "--bogus"]).status.code(), Some(2));
}

#[test]
fn watch_presence_prints_first_state() {
    let out = stdout_ok(&["watch-presence", "--limit", "1", "--interval", "1"]);
    assert_eq!(out.lines().count(), 1, "{out}");
    check_presence(out.trim(), 600);
}

#[cfg(target_os = "linux")]
#[test]
fn lower_idle_sets_nice_19() {
    let mut child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id().to_string();
    let result = std::panic::catch_unwind(|| {
        stdout_ok(&["lower-idle", &pid]);
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("stat");
        let after_comm = stat.rsplit_once(')').expect("comm").1;
        let nice: i32 = after_comm
            .split_whitespace()
            .nth(16)
            .expect("nice")
            .parse()
            .expect("nice number");
        assert_eq!(nice, 19);
    });
    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn lower_idle_missing_process_fails() {
    let out = run(&["lower-idle", "4000000"]);
    assert_eq!(out.status.code(), Some(2));
}
