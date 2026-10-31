//! Real bwrap and real systemd user scopes. Fixture dirs live under the cargo target tmpdir, not
//! /tmp, because the sandbox puts a fresh tmpfs over /tmp.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use pt_sandbox::{SandboxSpec, default_masks, kill_scope, resolve_git_dirs, scope_alive};

fn fixture(test: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sandbox")
        .join(test);
    fs::create_dir_all(&d).unwrap();
    fs::canonicalize(d).unwrap()
}

fn sub(base: &Path, name: &str) -> PathBuf {
    let d = base.join(name);
    fs::create_dir_all(&d).unwrap();
    d
}

fn spec(sleeve: &str, base: &Path) -> SandboxSpec {
    let ws = sub(base, &format!("{sleeve}-ws"));
    let home = sub(base, &format!("{sleeve}-home"));
    let mut s = SandboxSpec::new(sleeve, 1, ws, home);
    s.limits.memory_max_bytes = 512 << 20;
    s.tmp_size_bytes = 32 << 20;
    s
}

fn sh(s: &SandboxSpec, script: &str) -> Output {
    let out = s
        .command()
        .unwrap()
        .args(["sh", "-c", script])
        .output()
        .unwrap();
    assert!(
        out.status.success() || !out.stderr.starts_with(b"bwrap:"),
        "sandbox setup failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        sleep(Duration::from_millis(50));
    }
}

#[test]
fn masks_hide_credentials_and_filesystem_is_read_only_outside_workspace() {
    let base = fixture("masks");
    let host_home = sub(&base, "owner-home");
    fs::create_dir_all(host_home.join(".ssh")).unwrap();
    fs::write(host_home.join(".ssh/id_ed25519"), "SECRET").unwrap();
    fs::write(host_home.join(".claude.json"), "TOKEN").unwrap();
    let state = sub(&base, "state");
    fs::write(state.join("owner.token"), "OWNER").unwrap();
    let sibling = sub(&base, "sibling");

    let mut s = spec("masks", &base);
    s.masks = default_masks(&host_home);
    s.masks.push(state.clone());
    let script = format!(
        "cat {ssh} 2>&1; cat {cj} 2>&1; cat {tok} 2>&1; \
         echo ws > ws-file && echo WS_OK; echo h > $HOME/h-file && echo HOME_OK; \
         touch /usr/pt-x 2>&1 || echo USR_RO; touch {sib}/x 2>&1 || echo SIB_RO; \
         echo $HOME; pwd; ls /proc | grep -c '^[0-9]'; df -k /tmp | tail -1; \
         touch /tmp/ok && echo TMP_OK",
        ssh = host_home.join(".ssh/id_ed25519").display(),
        cj = host_home.join(".claude.json").display(),
        tok = state.join("owner.token").display(),
        sib = sibling.display(),
    );
    let o = sh(&s, &script);
    let out = stdout(&o);
    for secret in ["SECRET", "TOKEN", "OWNER"] {
        assert!(!out.contains(secret), "{secret} leaked:\n{out}");
    }
    for ok in ["WS_OK", "HOME_OK", "USR_RO", "SIB_RO", "TMP_OK"] {
        assert!(out.contains(ok), "missing {ok}:\n{out}");
    }
    assert!(out.contains(&s.home.display().to_string()));
    assert_eq!(
        fs::read_to_string(s.workspace.join("ws-file"))
            .unwrap()
            .trim(),
        "ws"
    );
    assert_eq!(
        fs::read_to_string(s.home.join("h-file")).unwrap().trim(),
        "h"
    );
    assert!(!sibling.join("x").exists());
    assert!(!Path::new("/usr/pt-x").exists());

    let lines: Vec<&str> = out.lines().collect();
    let pids: usize = lines
        .iter()
        .rev()
        .skip(2)
        .find_map(|l| l.parse().ok())
        .expect("pid count");
    assert!(pids <= 6, "sees {pids} pids");
    let df = lines
        .iter()
        .find(|l| l.starts_with("tmpfs"))
        .expect("tmp is tmpfs");
    assert!(df.contains("32768"), "tmp not sized: {df}");
}

#[test]
fn sleeves_share_nothing_private() {
    let base = fixture("two");
    let state = sub(&base, "state");
    let a = spec("alpha", &base);
    let b = spec("beta", &base);
    let (a_home, b_home) = (a.home.clone(), b.home.clone());
    let mut a = a;
    let mut b = b;
    for s in [&mut a, &mut b] {
        s.masks = vec![state.clone()];
    }
    fs::write(a_home.join("mine"), "A").unwrap();
    fs::write(b_home.join("mine"), "B").unwrap();
    // Both homes live outside the masked state dir here, so the other home is visible
    // read-only: the hub must put sleeve homes under the masked state dir to hide them.
    let ao = std::thread::scope(|t| {
        let h1 = t.spawn(|| sh(&a, "echo a > /tmp/who; sleep 1; cat /tmp/who"));
        let h2 = t.spawn(|| sh(&b, "echo b > /tmp/who; sleep 1; cat /tmp/who"));
        (h1.join().unwrap(), h2.join().unwrap())
    });
    assert_eq!(stdout(&ao.0), "a");
    assert_eq!(stdout(&ao.1), "b");
    assert!(!Path::new("/tmp/who").exists());

    // The hub layout: sleeve homes under the masked state dir.
    let mut c = spec("gamma", &base);
    let hidden = sub(&state, "sleeves");
    let other = sub(&hidden, "delta-home");
    fs::write(other.join("login"), "DELTA").unwrap();
    c.home = sub(&hidden, "gamma-home");
    c.masks = vec![state.clone()];
    let o = sh(
        &c,
        &format!(
            "cat {}/login 2>/dev/null; ls {}",
            other.display(),
            hidden.display()
        ),
    );
    let out = stdout(&o);
    assert!(!out.contains("DELTA"));
    assert!(
        out.contains("gamma-home") && !out.contains("delta-home"),
        "{out}"
    );
}

#[test]
fn scope_limits_and_kill_reaches_double_forked_tree() {
    let base = fixture("scope");
    let mut s = spec("scopetest", &base);
    s.limits.memory_max_bytes = 300 << 20;
    s.limits.tasks_max = 100;
    let name = s.scope_name();
    assert_eq!(name, "pt-scopetest-g1.scope");
    let marker = s.workspace.join("started");
    let script = format!(
        "( sleep 300 & ) ; ( setsid sleep 301 & ) ; sleep 300 & touch {} ; wait",
        marker.display()
    );
    let mut child = s
        .command()
        .unwrap()
        .args(["sh", "-c", &script])
        .stdin(Stdio::null())
        .spawn()
        .unwrap();

    let pids = wait_for("scope with grandchildren", || {
        if !marker.exists() {
            return None;
        }
        scope_alive(&name).unwrap().filter(|p| p.len() >= 4)
    });
    let cg = pt_sandbox::scope_cgroup(&name).unwrap().expect("cgroup");
    let max = fs::read_to_string(cg.join("memory.max")).unwrap();
    assert_eq!(max.trim(), (300u64 << 20).to_string());
    let tasks = fs::read_to_string(cg.join("pids.max")).unwrap();
    assert_eq!(tasks.trim(), "100");
    let cpu = fs::read_to_string(cg.join("cpu.max")).unwrap();
    assert!(cpu.starts_with("400000 100000"), "cpu.max {cpu}");

    kill_scope(&name).unwrap();
    child.wait().unwrap();
    wait_for("scope empty", || {
        scope_alive(&name).unwrap().is_none().then_some(())
    });
    for p in pids {
        assert!(
            !Path::new(&format!("/proc/{p}")).exists() || zombie(p),
            "pid {p} survived"
        );
    }
    kill_scope(&name).unwrap();
}

fn zombie(p: u32) -> bool {
    fs::read_to_string(format!("/proc/{p}/stat"))
        .map(|s| s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')))
        .unwrap_or(true)
}

#[test]
fn worktree_hooks_are_read_only() {
    let base = fixture("hooks");
    let main = sub(&base, "main");
    let git = |dir: &Path, args: &[&str]| {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    if !main.join(".git").exists() {
        git(&main, &["init", "-q"]);
        git(
            &main,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "i",
            ],
        );
    }
    let wt = base.join("wt");
    if !wt.exists() {
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                wt.to_str().unwrap(),
                "-b",
                "pt-hooks-test",
            ],
        );
    }
    let dirs = resolve_git_dirs(&wt).unwrap();
    assert_eq!(dirs.len(), 2, "{dirs:?}");

    let mut s = SandboxSpec::new("hooks", 1, wt.clone(), sub(&base, "home"));
    s.tmp_size_bytes = 16 << 20;
    s.git_dirs = dirs.clone();
    let hooks_main = dirs[1].join("hooks");
    let hooks_wt = dirs[0].join("hooks");
    fs::create_dir_all(&hooks_wt).unwrap();
    let script = format!(
        "echo x > {m}/pre-commit 2>/dev/null || echo MAIN_HOOKS_RO; \
         echo x > {w}/pre-commit 2>/dev/null || echo WT_HOOKS_RO; \
         git -c user.name=t -c user.email=t@t commit -q --allow-empty -m child && echo COMMIT_OK",
        m = hooks_main.display(),
        w = hooks_wt.display()
    );
    let out = stdout(&sh(&s, &script));
    for ok in ["MAIN_HOOKS_RO", "WT_HOOKS_RO", "COMMIT_OK"] {
        assert!(out.contains(ok), "missing {ok}:\n{out}");
    }
    assert!(!hooks_main.join("pre-commit").exists());
    assert!(!hooks_wt.join("pre-commit").exists());
}
