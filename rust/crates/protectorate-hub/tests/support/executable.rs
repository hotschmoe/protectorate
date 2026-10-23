use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

/// Writes `body` as an executable script at `path` from a child process. A
/// write descriptor opened in this process leaks into children that other
/// test threads fork meanwhile, and running the file fails with ETXTBSY
/// until they exec.
pub fn write_executable(path: &Path, body: &str) {
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
