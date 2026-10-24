#![cfg(unix)]
#![cfg(target_os = "linux")]
use crate::common;

use common::PrivateTmux;
use protectorate_hub::Hub;
use protectorate_session::{CreateOptions, Manager};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use std::{
    fs,
    os::fd::{FromRawFd, OwnedFd},
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;

#[tokio::test]
async fn cli_attach_sends_terminal_input_and_detaches_from_private_tmux() {
    if Command::new("tmux").arg("-V").output().is_err() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let socket = format!("protectorate-cli-{}-{nonce}", std::process::id());
    let _tmux = PrivateTmux(socket.clone());
    let manager = Arc::new(Manager::new(&socket));
    manager
        .create(&CreateOptions {
            name: "sleeve-worker".into(),
            cwd: Some(root.path().to_path_buf()),
            command: vec!["/bin/sh".into()],
            ..Default::default()
        })
        .unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager));
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Shell,
                Mode::Terminal,
                root.path(),
                "2026-10-22T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let proof = root.path().join("attach-proof");
    let result = tokio::task::spawn_blocking(move || {
        let mut master = -1;
        let mut slave = -1;
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
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate"));
        command
            .args(["sleeve", "attach", "worker"])
            .env("PROTECTORATE_HUB", url)
            .env("PROTECTORATE_TOKEN", "owner-secret")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        std::thread::sleep(Duration::from_millis(300));
        let text = format!("printf yes > {}\n", proof.display());
        assert!(
            unsafe {
                libc::write(
                    std::os::fd::AsRawFd::as_raw_fd(&master),
                    text.as_ptr().cast(),
                    text.len(),
                )
            } > 0
        );
        while !proof.exists() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let escape = [0x1d_u8];
        unsafe {
            libc::write(
                std::os::fd::AsRawFd::as_raw_fd(&master),
                escape.as_ptr().cast(),
                1,
            );
        }
        let exited = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(7) {
                child.kill().unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        (exited, proof.exists())
    })
    .await
    .unwrap();
    assert!(result.0.success(), "CLI attach exited {:?}", result.0);
    assert!(result.1, "terminal input did not reach tmux");
    let _ = manager.kill("sleeve-worker");
    let _ = fs::remove_file(root.path().join("attach-proof"));
    server.abort();
}
