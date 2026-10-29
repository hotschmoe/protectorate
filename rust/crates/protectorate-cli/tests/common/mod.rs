#![allow(dead_code)]
#[cfg(unix)]
use std::path::PathBuf;
use std::{ffi::OsStr, path::Path, process::Command};

/// Runs the CLI binary against `url` with `token`; Ok is stdout, Err is stderr.
pub fn cli<S: AsRef<OsStr>>(
    url: &str,
    token: &str,
    args: impl IntoIterator<Item = S>,
) -> Result<String, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate"));
    command
        .env("PROTECTORATE_HUB", url)
        .env("PROTECTORATE_TOKEN", token);
    run(command, args)
}

fn run<S: AsRef<OsStr>>(
    mut command: Command,
    args: impl IntoIterator<Item = S>,
) -> Result<String, String> {
    let output = command.args(args).output().unwrap();
    if output.status.success() {
        Ok(String::from_utf8(output.stdout).unwrap())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// Writes `body` as an executable script at `path` from a child process. A
/// write descriptor opened in this process leaks into children that other
/// test threads fork meanwhile, and running the file fails with ETXTBSY
/// until they exec.
#[cfg(unix)]
pub fn write_executable(path: &Path, body: &str) {
    use std::{io::Write, process::Stdio};
    let mut child = Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success(), "write {}", path.display());
}

/// The built CLI copied into `root`, where it looks for its companions,
/// by a child process for the reason `write_executable` gives.
#[cfg(unix)]
pub fn installed_cli(root: &Path) -> PathBuf {
    let cli = root.join("protectorate");
    let status = Command::new("cp")
        .arg(env!("CARGO_BIN_EXE_protectorate"))
        .arg(&cli)
        .status()
        .unwrap();
    assert!(status.success(), "copy the CLI into {}", root.display());
    cli
}

/// A `protectorate-rs` in `root` that prints each argument on its own line.
#[cfg(unix)]
pub fn echo_companion(root: &Path) {
    write_executable(
        &root.join("protectorate-rs"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\"\n",
    );
}

/// A private tmux server for one test. Dropping it stops the server and
/// removes the socket, which tmux itself leaves behind.
#[cfg(unix)]
pub struct PrivateTmux(pub String);

#[cfg(unix)]
impl Drop for PrivateTmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.0, "kill-server"])
            .output();
        let dir =
            std::env::var_os("TMUX_TMPDIR").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let uid = unsafe { libc::getuid() };
        let _ = std::fs::remove_file(dir.join(format!("tmux-{uid}")).join(&self.0));
    }
}

/// A one-hour capability minted straight into the hub's store.
#[cfg(unix)]
pub fn mint(hub: &protectorate_hub::Hub, principal: &str, scopes: &[&str]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (_, secret) = hub
        .identity
        .mint(
            protectorate_identity::Mint {
                principal,
                scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
                task: "",
                minted_by: "owner",
                ttl: std::time::Duration::from_secs(3600),
                file: None,
            },
            now,
        )
        .unwrap();
    secret.expose().to_owned()
}
