use std::{ffi::OsStr, process::Command};

/// Runs the CLI binary against `url` with `token`; Ok is stdout, Err is stderr.
pub fn cli<S: AsRef<OsStr>>(
    url: &str,
    token: &str,
    args: impl IntoIterator<Item = S>,
) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
        .env("PROTECTORATE_HUB", url)
        .env("PROTECTORATE_TOKEN", token)
        .args(args)
        .output()
        .unwrap();
    if output.status.success() {
        Ok(String::from_utf8(output.stdout).unwrap())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}
