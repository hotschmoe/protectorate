//! Runs the built `pt-doctor-probe` against stand-in harness CLIs on a private PATH and a home
//! with planted logins, and checks the report, the cached `doctor.json` and that no secret leaks.
#![cfg(target_os = "linux")]

use pt_doctor::{Install, Report, Tri};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

const SECRET: &str = "SECRET-do-not-print-7f3a";

fn dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("pt-doctor-{}-{name}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}

fn script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn plant(home: &Path, rel: &str) {
    let p = home.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, format!("{{\"token\":\"{SECRET}\"}}")).unwrap();
}

struct World {
    bin: PathBuf,
    home: PathBuf,
    net: PathBuf,
}

fn world() -> World {
    let root = dir("world");
    let (bin, home, net) = (root.join("bin"), root.join("home"), root.join("net"));
    for d in [&bin, &home, &net] {
        fs::create_dir_all(d).unwrap();
    }
    // A compiled executable: native.
    symlink("/usr/bin/env", bin.join("claude")).unwrap();
    // A launcher under node_modules: npm.
    let codex_js = root.join("npm/lib/node_modules/@openai/codex/bin/codex.js");
    script(&codex_js, "echo codex-cli 9.9.9");
    symlink(&codex_js, bin.join("codex")).unwrap();
    // Present but broken.
    script(&bin.join("grok"), "echo 'grok: cannot start' >&2; exit 3");
    // A shell wrapper that execs an npm install (how pi is set up here).
    let pi_js = root.join("pi/node_modules/pi/cli.js");
    script(&pi_js, "echo 1.4.0");
    script(
        &bin.join("pi"),
        &format!("export X=1\nexec '{}' \"$@\"", pi_js.display()),
    );
    script(
        &bin.join("gh"),
        "case \"$1\" in\n--version) echo 'gh version 0.0.0' ;;\n\
         auth) echo 'github.com'; echo '  Logged in to github.com account tester (keyring)';\n\
         echo \"  Token: $GH_FAKE_TOKEN\" ;;\nesac",
    );
    script(&bin.join("bwrap"), "echo bubblewrap 0.0.0");
    script(
        &bin.join("systemd-run"),
        "[ -z \"$FAIL_SCOPE\" ] || { echo 'Failed to connect to bus: No medium found' >&2; exit 1; }",
    );
    script(
        &bin.join("timedatectl"),
        "echo NTP=yes; echo NTPSynchronized=no",
    );
    symlink("/usr/bin/df", bin.join("df")).unwrap();
    script(&net.join("headscale"), "echo v0.29.7");
    script(&net.join("member"), "exit 0");
    plant(&home, ".claude/.credentials.json");
    plant(&home, ".pi/agent/auth.json");
    World { bin, home, net }
}

fn probe(w: &World, fail_scope: bool, save: Option<&Path>) -> (Report, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_pt-doctor-probe"));
    c.env_clear()
        .env("PATH", &w.bin)
        .env("HOME", &w.home)
        .env("USER", "nobody-here")
        .env("XDG_RUNTIME_DIR", "/run/user/0")
        .env("XAI_API_KEY", SECRET)
        .env("GH_FAKE_TOKEN", SECRET)
        .arg("--headscale")
        .arg(w.net.join("headscale"))
        .arg("--member")
        .arg(w.net.join("member"))
        .arg("--disk")
        .arg(&w.home);
    if fail_scope {
        c.env("FAIL_SCOPE", "1");
    }
    if let Some(p) = save {
        c.arg("--save").arg(p);
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        !text.contains(SECRET),
        "a secret reached the report: {text}"
    );
    (pt_json::from_str(&text).unwrap(), text)
}

fn harness<'a>(r: &'a Report, name: &str) -> &'a pt_doctor::Harness {
    r.harnesses.iter().find(|h| h.name == name).unwrap()
}

#[test]
fn report_gates_harnesses() {
    let w = world();
    let cache = dir("cache").join("doctor.json");
    let (r, text) = probe(&w, false, Some(&cache));

    let names: Vec<&str> = r.harnesses.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "claude",
            "codex",
            "grok",
            "antigravity",
            "pi_vendor",
            "pi_local",
            "pi_durable"
        ]
    );
    let claude = harness(&r, "claude");
    assert_eq!(claude.binary.install, Install::Native);
    assert_eq!(claude.binary.real_path, "/usr/bin/env");
    assert!(
        claude.binary.version.contains("coreutils"),
        "{:?}",
        claude.binary
    );
    assert_eq!(claude.credentials.found, [".claude/.credentials.json"]);
    let claude_path = w.bin.join("claude").display().to_string();
    assert_eq!(claude.update, [claude_path.as_str(), "update"]);
    assert!(claude.eligible);

    let codex = harness(&r, "codex");
    assert_eq!(codex.binary.install, Install::Npm);
    assert_eq!(codex.binary.version, "codex-cli 9.9.9");
    assert!(
        !codex.eligible
            && codex.reasons == ["no login found; run `protectorate harness login codex`"],
        "{:?}",
        codex.reasons
    );

    let grok = harness(&r, "grok");
    assert_eq!(grok.binary.install, Install::Script);
    assert_eq!(grok.binary.error, "grok: cannot start");
    assert!(
        grok.credentials.found.is_empty(),
        "an API key in the hub env is not a login"
    );
    assert!(!grok.eligible);

    let agy = harness(&r, "antigravity");
    assert!(!agy.binary.present && agy.binary.install == Install::Missing);
    assert!(!agy.credentials.checked && agy.update.is_empty());

    let pi = harness(&r, "pi_vendor");
    assert_eq!(
        pi.binary.install,
        Install::Npm,
        "wrapper not followed: {:?}",
        pi.binary
    );
    assert!(pi.binary.real_path.ends_with("pi/node_modules/pi/cli.js"));
    let pi_path = w.bin.join("pi").display().to_string();
    assert_eq!(pi.update, [pi_path.as_str(), "update", "self"]);
    let codex_path = w.bin.join("codex").display().to_string();
    assert_eq!(harness(&r, "codex").update, [codex_path.as_str(), "update"]);
    assert!(pi.eligible);
    assert!(!harness(&r, "pi_local").eligible);
    assert!(
        harness(&r, "pi_durable")
            .reasons
            .iter()
            .any(|x| x.contains("node")),
        "{:?}",
        harness(&r, "pi_durable").reasons
    );

    assert_eq!(
        r.ineligible,
        ["codex", "grok", "antigravity", "pi_local", "pi_durable"]
    );
    assert!(r.eligible("claude") && r.eligible("pi_vendor"));
    assert!(!r.eligible("codex") && !r.eligible("gemini"));

    assert!(r.gh.logged_in);
    assert_eq!(r.gh.account, "tester");
    assert_eq!(r.sandbox.bwrap.version, "bubblewrap 0.0.0");
    assert!(r.sandbox.systemd_run_user);
    assert_eq!(r.sandbox.linger, Tri::No);
    assert_eq!(r.clock.ntp, Tri::Yes);
    assert_eq!(r.clock.synchronized, Tri::No);
    assert!(!r.curl.present);
    assert!(r.net.headscale.present && r.net.headscale.version == "v0.29.7");
    assert!(r.net.member.present && !r.net.go.present);
    assert_eq!(r.disks.len(), 1);
    assert!(r.disks[0].total_bytes >= r.disks[0].free_bytes && r.disks[0].total_bytes > 0);
    for want in ["clock:", "curl:", "linger is no", "codex:", "grok:"] {
        assert!(
            r.problems
                .iter()
                .any(|p| p.starts_with(want) || p.contains(want)),
            "no {want} problem in {:?}",
            r.problems
        );
    }

    // The cache is the same report, and pt-hub's reader key is where it looks.
    let cached = Report::load(&cache).unwrap();
    assert_eq!(cached, r);
    let raw = pt_json::parse(&fs::read_to_string(&cache).unwrap()).unwrap();
    assert_eq!(raw["ineligible"].as_array().unwrap().len(), 5);
    assert_eq!(fs::read_to_string(&cache).unwrap().trim(), text.trim());

    // Without working scopes no harness is eligible.
    let (r, _) = probe(&w, true, None);
    assert!(r.harnesses.iter().all(|h| !h.eligible));
    assert!(!r.sandbox.systemd_run_user);
    assert!(r.sandbox.systemd_run_error.contains("No medium found"));
}
