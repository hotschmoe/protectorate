//! Real OpenSSH and node processes, using only a private loopback sshd and private keys.
#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const NODE: &str = env!("CARGO_BIN_EXE_protectorate-node");
const POC: &str = env!("CARGO_BIN_EXE_pt-ssh-poc");

struct Process {
    child: Child,
    group: bool,
    journal: Option<PathBuf>,
}

impl Process {
    fn start(command: &mut Command, group: bool) -> Self {
        if group {
            command.process_group(0);
        }
        Self {
            child: command.spawn().expect("start process"),
            group,
            journal: None,
        }
    }

    fn wait(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("wait for process") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "process did not exit in {timeout:?}"
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // On a failed assertion, give the bounded demo job time to finish before killing its
        // resident agent. Successful runs already recorded the end and do not wait here.
        if let Some(journal) = &self.journal {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let text = fs::read_to_string(journal).unwrap_or_default();
                if !text.contains("\"type\":\"submit\"")
                    || text.contains("\"type\":\"end\"")
                    || Instant::now() >= deadline
                {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        if self.child.try_wait().ok().flatten().is_none() {
            if self.group {
                let _ = Command::new("/bin/kill")
                    .args(["-TERM", "--", &format!("-{}", self.child.id())])
                    .status();
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn tool(name: &str) -> bool {
    Command::new(name)
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn make_key(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&path)
        .status()
        .expect("ssh-keygen");
    assert!(status.success());
    path
}

fn output_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

fn known_host(key: &Path, port: u16) -> String {
    let public = fs::read_to_string(output_path(key, ".pub")).unwrap();
    let mut fields = public.split_whitespace();
    format!(
        "[127.0.0.1]:{port} {} {}\n",
        fields.next().unwrap(),
        fields.next().unwrap()
    )
}

fn log(path: &Path) -> Stdio {
    Stdio::from(File::create(path).expect("create log"))
}

fn run_poc(dir: &Path, user: &str, port: u16, key: &Path, known: &Path, name: &str) -> ExitStatus {
    let mut command = Command::new(POC);
    command
        .args([user, "127.0.0.1", &port.to_string()])
        .arg(key)
        .arg(known)
        .arg(dir.join(format!("{name}.artifact")))
        .stdout(log(&dir.join(format!("{name}.out"))))
        .stderr(log(&dir.join(format!("{name}.err"))));
    Process::start(&mut command, true).wait(Duration::from_secs(45))
}

#[test]
fn openssh_forced_command_runs_job_and_fetches_artifact() {
    if !["ssh", "ssh-keygen", "/sbin/sshd"].into_iter().all(tool) {
        eprintln!("skipping: ssh, ssh-keygen or /sbin/sshd is missing");
        return;
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ssh-poc-{unique}"));
    let state = dir.join("state");
    fs::create_dir_all(&state).unwrap();
    let host = make_key(&dir, "host");
    let other_host = make_key(&dir, "other-host");
    let hub = make_key(&dir, "hub");
    let stranger = make_key(&dir, "stranger");
    let public = fs::read_to_string(output_path(&hub, ".pub")).unwrap();
    fs::write(dir.join("authorized_keys"), format!("restrict {public}")).unwrap();

    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let known = dir.join("known_hosts");
    let wrong_known = dir.join("wrong_known_hosts");
    fs::write(&known, known_host(&host, port)).unwrap();
    fs::write(&wrong_known, known_host(&other_host, port)).unwrap();
    let user = Command::new("id").arg("-un").output().unwrap();
    assert!(user.status.success());
    let user = String::from_utf8(user.stdout).unwrap();
    let user = user.trim();

    // The temporary paths have no spaces. sshd invokes ForceCommand through the user's shell.
    let config = format!(
        "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\n\
         AuthorizedKeysFile {}\nStrictModes no\nUsePAM no\nPasswordAuthentication no\n\
         KbdInteractiveAuthentication no\nPubkeyAuthentication yes\nDisableForwarding yes\n\
         PermitTTY no\nPermitUserRC no\nPrintMotd no\nLogLevel VERBOSE\n\
         ForceCommand {NODE} serve --stdio --state-dir {} --no-scope\n",
        host.display(),
        dir.join("sshd.pid").display(),
        dir.join("authorized_keys").display(),
        state.display(),
    );
    let config_path = dir.join("sshd_config");
    fs::write(&config_path, config).unwrap();
    let syntax = Command::new("/sbin/sshd")
        .args(["-t", "-f"])
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        syntax.status.success(),
        "{}",
        String::from_utf8_lossy(&syntax.stderr)
    );

    let mut agent_cmd = Command::new(NODE);
    agent_cmd
        .current_dir(&state)
        .args(["serve", "--state-dir"])
        .arg(&state)
        .args([
            "--forward-socket",
            "agent.sock",
            "--no-member",
            "--no-scope",
            "--idle-exit",
            "60",
        ])
        .stdout(Stdio::null())
        .stderr(log(&dir.join("agent.log")));
    let mut agent = Process::start(&mut agent_cmd, false);
    agent.journal = Some(state.join("jobs.jsonl"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !state.join("agent.sock").exists() {
        assert!(agent.child.try_wait().unwrap().is_none(), "agent exited");
        assert!(Instant::now() < deadline, "agent did not start");
        thread::sleep(Duration::from_millis(20));
    }
    let mut sshd_cmd = Command::new("/sbin/sshd");
    sshd_cmd
        .args(["-D", "-e", "-f"])
        .arg(&config_path)
        .stdout(Stdio::null())
        .stderr(log(&dir.join("sshd.log")));
    let mut sshd = Process::start(&mut sshd_cmd, true);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        assert!(
            sshd.child.try_wait().unwrap().is_none(),
            "sshd exited: {}",
            fs::read_to_string(dir.join("sshd.log")).unwrap_or_default()
        );
        assert!(Instant::now() < deadline, "sshd did not start");
        thread::sleep(Duration::from_millis(20));
    }

    let bad_host = run_poc(&dir, user, port, &hub, &wrong_known, "bad-host");
    assert!(!bad_host.success(), "changed host key was accepted");
    let err = fs::read_to_string(dir.join("bad-host.err")).unwrap();
    assert!(err.contains("HOST IDENTIFICATION HAS CHANGED"), "{err}");
    assert!(!dir.join("bad-host.artifact").exists());

    let bad_key = run_poc(&dir, user, port, &stranger, &known, "bad-key");
    assert!(!bad_key.success(), "unauthorized key was accepted");
    let err = fs::read_to_string(dir.join("bad-key.err")).unwrap();
    assert!(err.contains("Permission denied"), "{err}");
    assert!(!dir.join("bad-key.artifact").exists());

    let success = run_poc(&dir, user, port, &hub, &known, "good");
    assert!(
        success.success(),
        "client: {}\nsshd: {}\nagent: {}",
        fs::read_to_string(dir.join("good.err")).unwrap_or_default(),
        fs::read_to_string(dir.join("sshd.log")).unwrap_or_default(),
        fs::read_to_string(dir.join("agent.log")).unwrap_or_default(),
    );
    assert_eq!(
        fs::read(dir.join("good.artifact")).unwrap(),
        b"artifact over ssh\n"
    );
    let out = fs::read_to_string(dir.join("good.out")).unwrap();
    assert!(out.contains("job ssh-poc succeeded"), "{out}");
    drop(sshd);
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
        assert!(Instant::now() < deadline, "sshd still listens on {port}");
        thread::sleep(Duration::from_millis(20));
    }
    drop(agent);
}
