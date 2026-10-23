#![cfg(unix)]
#![cfg(target_os = "linux")]
use protectorate_hub::Hub;
use protectorate_session::{CreateOptions, Manager};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

/// A real Hub backed by a private tmux server, so the CLI under test talks to
/// the same RPCs and WebSocket a terminal user would.
struct Fixture {
    root: tempfile::TempDir,
    socket: String,
    manager: Arc<Manager>,
    url: String,
    server: tokio::task::JoinHandle<()>,
}

async fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let socket = format!("protectorate-cli-attach-{}-{nonce}", std::process::id());
    let manager = Arc::new(Manager::new(&socket));
    let hub = Hub::open(root.path(), "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    Fixture {
        root,
        socket,
        manager,
        url,
        server,
    }
}

fn shell(manager: &Manager, cwd: &Path, name: &str) {
    manager
        .create(&CreateOptions {
            name: name.into(),
            cwd: Some(cwd.to_path_buf()),
            command: vec!["/bin/sh".into()],
            ..Default::default()
        })
        .unwrap();
}

/// A session that prints on its own, so anything the client sees that was not
/// typed can only have come from the session.
fn printer(manager: &Manager, cwd: &Path, name: &str) {
    manager
        .create(&CreateOptions {
            name: name.into(),
            cwd: Some(cwd.to_path_buf()),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf marked; exec sleep 30".into(),
            ],
            ..Default::default()
        })
        .unwrap();
}

/// Turns off the pty's own echo, so anything the client sees can only have
/// come from the session.
fn mute_echo(master: &OwnedFd) {
    unsafe {
        let mut termios = std::mem::zeroed::<libc::termios>();
        assert_eq!(libc::tcgetattr(master.as_raw_fd(), &mut termios), 0);
        termios.c_lflag &= !libc::ECHO;
        assert_eq!(
            libc::tcsetattr(master.as_raw_fd(), libc::TCSANOW, &termios),
            0
        );
    }
}

fn spawn_attach(url: &str, args: &[&str], slave: &OwnedFd, stderr: Stdio) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate-cli"));
    command
        .arg("session")
        .arg("attach")
        .args(args)
        .env("PROTECTORATE_HUB", url)
        .env("PROTECTORATE_TOKEN", "owner-secret")
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(stderr);
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command.spawn().unwrap()
}

fn pty() -> (OwnedFd, OwnedFd) {
    let (mut master, mut slave) = (-1, -1);
    let size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    assert_eq!(
        unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        },
        0
    );
    unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) }
}

fn type_into(master: &OwnedFd, text: &str) {
    assert!(unsafe { libc::write(master.as_raw_fd(), text.as_ptr().cast(), text.len(),) } > 0);
}

fn press_detach(master: &OwnedFd) {
    let escape = [0x1d_u8];
    unsafe {
        libc::write(master.as_raw_fd(), escape.as_ptr().cast(), 1);
    }
}

/// Collects what the pty shows for `duration`, without ever blocking.
fn watch(fd: RawFd, duration: Duration) -> String {
    unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) };
    let mut out = String::new();
    let mut buf = [0_u8; 4096];
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let read = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if read > 0 {
            out.push_str(&String::from_utf8_lossy(&buf[..read as usize]));
        } else if read == 0 {
            break;
        } else {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(libc::EAGAIN) | Some(libc::EINTR)) {
                std::thread::sleep(Duration::from_millis(20));
            } else {
                break;
            }
        }
    }
    out
}

fn wait_exit(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    None
}

fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn teardown(fix: &Fixture, names: &[&str]) {
    for name in names {
        let _ = fix.manager.kill(name);
    }
    let _ = Command::new("tmux")
        .args(["-L", fix.socket.as_str(), "kill-server"])
        .output();
    fix.server.abort();
}

fn tmux_ready() -> bool {
    Command::new("tmux").arg("-V").output().is_ok()
}

#[test]
fn attach_writes_to_the_session_and_ctrl_bracket_detaches() {
    if !tmux_ready() {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fix = runtime.block_on(fixture());
    let proof = fix.root.path().join("proof");
    shell(&fix.manager, fix.root.path(), "runner");

    let (master, slave) = pty();
    mute_echo(&master);
    let mut child = spawn_attach(
        &fix.url,
        &["runner"],
        &slave,
        Stdio::from(slave.try_clone().unwrap()),
    );
    let watcher = master.try_clone().unwrap();
    let screen = std::thread::spawn(move || watch(watcher.as_raw_fd(), Duration::from_secs(8)));
    std::thread::sleep(Duration::from_millis(400));
    type_into(&master, "printf marked\n");
    type_into(&master, &format!("printf yes > {}\n", proof.display()));
    let reached = wait_for_file(&proof, Duration::from_secs(6));
    press_detach(&master);
    let status = wait_exit(&mut child, Duration::from_secs(6));
    let screen = screen.join().unwrap();
    teardown(&fix, &["runner"]);

    assert!(reached, "keystrokes never reached the session: {screen}");
    assert_eq!(std::fs::read_to_string(&proof).unwrap(), "yes");
    assert_eq!(status, Some(ExitStatus::default()), "{screen}");
    assert!(
        screen.contains("marked"),
        "session output never arrived: {screen}"
    );
}

#[test]
fn readonly_attach_shows_the_session_but_sends_nothing() {
    if !tmux_ready() {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fix = runtime.block_on(fixture());
    let proof = fix.root.path().join("proof");
    printer(&fix.manager, fix.root.path(), "watcher");

    let (master, slave) = pty();
    let mut child = spawn_attach(
        &fix.url,
        &["watcher", "--readonly"],
        &slave,
        Stdio::from(slave.try_clone().unwrap()),
    );
    let watcher = master.try_clone().unwrap();
    let screen = std::thread::spawn(move || watch(watcher.as_raw_fd(), Duration::from_secs(6)));
    std::thread::sleep(Duration::from_millis(400));
    type_into(&master, &format!("printf yes > {}\n", proof.display()));
    std::thread::sleep(Duration::from_secs(2));
    let reached = proof.exists();
    let status = wait_exit(&mut child, Duration::from_millis(300));
    let screen = screen.join().unwrap();
    teardown(&fix, &["watcher"]);

    assert!(!reached, "readonly attach typed into the session: {screen}");
    assert!(
        screen.contains("marked"),
        "readonly attach saw nothing: {screen}"
    );
    assert!(status.is_none(), "readonly attach must stay attached");
}

#[test]
fn attach_reports_a_session_that_is_not_there() {
    if !tmux_ready() {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fix = runtime.block_on(fixture());

    let (master, slave) = pty();
    let mut child = spawn_attach(&fix.url, &["never-existed"], &slave, Stdio::piped());
    let status = wait_exit(&mut child, Duration::from_secs(8));
    let mut message = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        stderr.read_to_string(&mut message).unwrap();
    }
    drop(master);
    teardown(&fix, &[]);

    let exited = status.expect("attach should fail fast");
    assert!(!exited.success(), "{message}");
    assert!(message.contains("404"), "{message}");
}
