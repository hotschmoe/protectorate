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
    let scratch = sub(base, &format!("{sleeve}-scratch"));
    sub(&scratch, "target");
    sub(&scratch, "tmp");
    let mut s = SandboxSpec::new(sleeve, 1, ws, home, scratch);
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
fn disk_scratch_is_short_and_survives_incarnations() {
    let base = fixture("scratch");
    let mut a = spec("scratch-a", &base);
    let mut b = spec("scratch-b", &base);
    a.scope = false;
    b.scope = false;
    let script = "test \"$CARGO_TARGET_DIR\" = /tmp/pt/target && \
        test \"$TMPDIR\" = /tmp/pt/tmp && \
        echo target > \"$CARGO_TARGET_DIR/file\" && echo temp > \"$TMPDIR/file\" && \
        df -T /tmp /tmp/pt | tail -2";
    let first = sh(&a, script);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let df = stdout(&first);
    assert!(df.lines().next().unwrap().contains("tmpfs"), "{df}");
    assert!(!df.lines().last().unwrap().contains("tmpfs"), "{df}");
    assert_eq!(
        fs::read_to_string(a.scratch.join("target/file"))
            .unwrap()
            .trim(),
        "target"
    );
    a.generation += 1;
    assert_eq!(stdout(&sh(&a, "cat $TMPDIR/file")), "temp");
    assert!(sh(&b, "test ! -e $TMPDIR/file").status.success());
}

#[test]
fn bwrap_does_not_preserve_the_harness_kill_signal() {
    use std::os::unix::process::ExitStatusExt;

    let base = fixture("exit-status");
    let mut s = spec("exit-status", &base);
    s.scope = false;
    let killed = sh(&s, "kill -9 $$").status;
    let exited = sh(&s, "exit 137").status;
    assert_eq!((killed.code(), killed.signal()), (Some(137), None));
    assert_eq!((exited.code(), exited.signal()), (Some(137), None));
}

#[test]
fn mask_and_runtime_tmpfs_have_small_limits() {
    let base = fixture("mask-limits");
    let mask = sub(&base, "masked");
    let mut s = spec("mask-limits", &base);
    s.scope = false;
    s.masks.push(mask.clone());
    let script = format!(
        "for dir in '{}' \"$XDG_RUNTIME_DIR\"; do \
         df -Pk \"$dir\" | tail -1; \
         dd if=/dev/zero of=\"$dir/full\" bs=1048576 count=2 2>/dev/null \
         && exit 1; done; true",
        mask.display()
    );
    let out = sh(&s, &script);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    assert_eq!(text.lines().count(), 2, "{text}");
    for line in text.lines() {
        assert_eq!(line.split_whitespace().nth(1), Some("1024"), "{line}");
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

fn git(dir: &Path, args: &[&str]) {
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
}

/// A repository `main` with one commit and a linked worktree `wt` on branch `branch`.
fn worktree(base: &Path, branch: &str) -> (PathBuf, PathBuf) {
    let main = sub(base, "main");
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
            &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", branch],
        );
    }
    (main, wt)
}

#[test]
fn worktree_hooks_are_read_only() {
    let base = fixture("hooks");
    let (main, wt) = worktree(&base, "pt-hooks-test");
    let dirs = resolve_git_dirs(&wt, &main).unwrap();
    assert_eq!(dirs.len(), 2, "{dirs:?}");

    let mut s = spec("hooks", &base);
    s.workspace = wt.clone();
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

/// A worktree child rewrites its own `.git` file: to `/`, to the owner's home, and to a fake git
/// dir whose `commondir` climbs out of the fixture. The next launch must refuse those git dirs
/// instead of binding them writable over the masks.
#[test]
fn a_rewritten_gitdir_never_reaches_the_next_launch() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = fixture(&format!("gitdir/{nanos}"));
    let owner = sub(&base, "owner-home");
    fs::create_dir_all(owner.join(".ssh")).unwrap();
    fs::write(owner.join(".ssh/id_ed25519"), "SECRET").unwrap();
    let (main, wt) = worktree(&base, "pt-gitdir-test");
    let dot = wt.join(".git");
    let honest = fs::read_to_string(&dot).unwrap();
    let fake = sub(&base, "fake");
    fs::write(fake.join("commondir"), "..").unwrap();

    let mut s = spec("sbxgitdir", &base);
    s.workspace = wt.clone();
    s.tmp_size_bytes = 16 << 20;
    s.masks = default_masks(&owner);
    let probe = format!(
        "cat {o}/.ssh/id_ed25519 2>/dev/null; touch {o}/planted 2>/dev/null && echo PLANTED; \
         echo ran",
        o = owner.display()
    );
    s.git_dirs = resolve_git_dirs(&wt, &main).expect("the honest worktree resolves");
    assert_eq!(stdout(&sh(&s, &probe)), "ran");

    // Each launch gets its own generation: a scope name is not reusable until systemd unloads
    // the last one.
    for evil in [Path::new("/"), &owner, &fake] {
        s.generation += 1;
        let out = sh(
            &s,
            &format!(
                "printf 'gitdir: {}\\n' > .git && echo REWROTE",
                evil.display()
            ),
        );
        assert_eq!(
            stdout(&out),
            "REWROTE",
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if let Ok(dirs) = resolve_git_dirs(&wt, &main) {
            let mut next = s.clone();
            next.generation += 100;
            next.git_dirs = dirs;
            if let Ok(mut cmd) = next.command() {
                let o = cmd.args(["sh", "-c", &probe]).output().unwrap();
                let out = String::from_utf8_lossy(&o.stdout);
                assert!(
                    !out.contains("SECRET"),
                    "gitdir {evil:?} unmasked ~/.ssh: {out}"
                );
                assert!(
                    !out.contains("PLANTED"),
                    "gitdir {evil:?} opened the host: {out}"
                );
            }
        }
        assert!(!owner.join("planted").exists());
        assert!(
            resolve_git_dirs(&wt, &main).is_err(),
            "gitdir {evil:?} was accepted"
        );
        fs::write(&dot, &honest).unwrap();
    }
}

/// The hub layout with a root claimed at the owner's home: the workspace bind contains the
/// credential masks, and the masked state dir contains the private home. Every mask must hold.
#[test]
fn binds_never_undo_masks() {
    let base = fixture("nested");
    let owner = sub(&base, "owner-home");
    fs::create_dir_all(owner.join(".ssh")).unwrap();
    fs::write(owner.join(".ssh/id_ed25519"), "SECRET").unwrap();
    let state = sub(&owner, ".local/share/pt-state");
    fs::write(state.join("owner.token"), "OWNER").unwrap();
    let home = sub(&state, "sleeves/1/home");
    let extra = sub(&base, "extra");

    let mut s = spec("sbxnested", &base);
    s.workspace = owner.clone();
    s.home = home.clone();
    s.tmp_size_bytes = 16 << 20;
    s.masks = default_masks(&owner);
    s.masks.push(state.clone());
    // A writable path that holds the state dir must not uncover it either.
    s.rw_paths.push(owner.join(".local"));
    s.ro_paths.push(extra.clone());
    let script = format!(
        "cat .ssh/id_ed25519 2>/dev/null; cat {tok} 2>/dev/null; ls {st}; \
         echo w > ws-file && echo WS_OK; echo h > $HOME/h && echo HOME_OK",
        tok = state.join("owner.token").display(),
        st = state.display(),
    );
    let out = stdout(&sh(&s, &script));
    for secret in ["SECRET", "OWNER", "owner.token"] {
        assert!(!out.contains(secret), "{secret} leaked:\n{out}");
    }
    for ok in ["WS_OK", "HOME_OK", "sleeves"] {
        assert!(out.contains(ok), "missing {ok}:\n{out}");
    }
    assert_eq!(fs::read_to_string(home.join("h")).unwrap().trim(), "h");

    let mut root = spec("sbxslash", &base);
    root.workspace = owner.clone();
    root.home = home.clone();
    root.rw_paths.push(PathBuf::from("/"));
    let refused = root.command().err().map(|e| e.to_string());
    assert!(refused.is_some(), "a writable / was accepted");
}

/// Through the user bus or the systemd user manager a sleeve could start a transient unit that
/// lives outside its scope and sandbox. The sandbox gets a private, empty runtime dir instead.
#[test]
fn a_sleeve_cannot_reach_the_user_manager() {
    let base = fixture("bus");
    let rt = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is set for these tests");
    assert!(Path::new(&rt).join("bus").exists(), "no user bus at {rt}");
    let unit = format!("pt-sbxescape-{}.service", std::process::id());
    let s = spec("sbxbus", &base);
    let script = format!(
        "ls -A {rt}; echo LISTED; echo $XDG_RUNTIME_DIR; touch $XDG_RUNTIME_DIR/x && echo RT_OK; \
         for a in {rt}/bus {rt}/systemd/private; do \
           busctl --address=unix:path=$a call org.freedesktop.systemd1 /org/freedesktop/systemd1 \
             org.freedesktop.systemd1.Manager StartTransientUnit 'ssa(sv)a(sa(sv))' {unit} fail 1 \
             ExecStart 'a(sasb)' 1 /bin/sleep 2 /bin/sleep 300 false 0 >/dev/null 2>&1 \
             && echo ESCAPED; \
         done; \
         XDG_RUNTIME_DIR={rt} systemd-run --user --quiet --unit={unit} /bin/sleep 300 \
           >/dev/null 2>&1 && echo ESCAPED; true"
    );
    let out = stdout(&sh(&s, &script));
    let active = Command::new("systemctl")
        .args(["--user", "is-active", &unit])
        .output()
        .unwrap();
    if String::from_utf8_lossy(&active.stdout).trim() != "inactive" {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", &unit])
            .status();
    }
    assert!(!out.contains("ESCAPED"), "a sleeve started {unit}:\n{out}");
    let listed = out.split("LISTED").next().unwrap().trim();
    assert!(listed.is_empty(), "the runtime dir shows {listed}");
    assert!(
        out.contains("RT_OK"),
        "no writable private runtime dir:\n{out}"
    );
}

/// Light isolation between roots: the hub masks every other sleeve's claim, but not a child's own
/// root, which holds the child's git dirs.
#[test]
fn other_roots_are_masked_and_a_worktree_child_still_commits() {
    let base = fixture("roots");
    let other = sub(&base, "other-root");
    fs::write(other.join("plan.md"), "OTHER").unwrap();
    let (main, wt) = worktree(&base, "pt-roots-test");
    fs::write(main.join("root-file"), "ROOT").unwrap();

    let mut s = spec("sbxroots", &base);
    s.workspace = wt.clone();
    s.tmp_size_bytes = 16 << 20;
    s.git_dirs = resolve_git_dirs(&wt, &main).unwrap();
    s.masks = vec![other.clone()];
    let script = format!(
        "cat {o}/plan.md 2>/dev/null; ls {o}; cat {m}/root-file; \
         git -c user.name=t -c user.email=t@t commit -q --allow-empty -m child && echo COMMIT_OK",
        o = other.display(),
        m = main.display()
    );
    let out = stdout(&sh(&s, &script));
    assert!(!out.contains("OTHER") && !out.contains("plan.md"), "{out}");
    assert!(out.contains("ROOT") && out.contains("COMMIT_OK"), "{out}");

    // Masking the child's own root would hide its git dirs: refused, not half-mounted.
    s.masks.push(main.clone());
    let refused = s.command().err().map(|e| e.to_string()).unwrap_or_default();
    assert!(refused.contains("overlaps the mask"), "{refused}");
}

/// A quota does not change the core count tools detect, so the sandbox sizes their worker pools
/// to it; a caller's own value wins, and without a scope nothing is capped.
#[test]
fn build_jobs_follow_the_cpu_quota() {
    let base = fixture("jobs");
    let probe = "echo $CARGO_BUILD_JOBS $GOMAXPROCS $GOFLAGS $FLUTTER_TEST_CONCURRENCY";
    let mut s = spec("jobs", &base);
    s.limits.cpu_quota_percent = 250;
    assert_eq!(stdout(&sh(&s, probe)), "2 2 -p=2 2");
    // A new incarnation each time, as the hub does: the last scope may still be going away.
    s.generation += 1;
    s.limits.cpu_quota_percent = 50;
    assert_eq!(stdout(&sh(&s, probe)), "1 1 -p=1 1");
    s.generation += 1;
    s.env.push(("CARGO_BUILD_JOBS".into(), "7".into()));
    assert_eq!(stdout(&sh(&s, probe)), "7 1 -p=1 1");
    s.scope = false;
    assert_eq!(stdout(&sh(&s, probe)), "7");
}
