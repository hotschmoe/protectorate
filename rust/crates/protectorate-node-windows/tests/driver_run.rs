#![cfg(windows)]

use std::{
    env, fs, io,
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    System::{
        Console::{AllocConsole, GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT},
        Threading::{
            OpenProcess, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP, PROCESS_SYNCHRONIZE,
        },
    },
};

fn fake_path(root: &Path) -> io::Result<std::ffi::OsString> {
    let mut paths = vec![root.to_path_buf()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    env::join_paths(paths).map_err(io::Error::other)
}

#[test]
fn pi_driver_streams_and_reaps_descendants() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("pi.cmd"),
        "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0pi.ps1\"\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("pi.ps1"),
        r#"
while ($null -ne ($line = [Console]::ReadLine())) {
  $m = $line | ConvertFrom-Json
  if ($m.type -eq 'get_state') {
    $child = Start-Process -FilePath "$env:SystemRoot\System32\ping.exe" -ArgumentList '-t','127.0.0.1' -PassThru -WindowStyle Hidden
    [IO.File]::WriteAllText('descendant.pid', [string]$child.Id)
    [Console]::Out.WriteLine((@{type='response';id=$m.id;command='get_state';success=$true;data=@{sessionId='test-session';model=@{id='test-model';provider='test-local';baseUrl='http://localhost';contextWindow=8192};thinkingLevel='medium'}} | ConvertTo-Json -Depth 8 -Compress))
  } elseif ($m.type -eq 'prompt') {
    [Console]::Out.WriteLine((@{type='response';id=$m.id;command='prompt';success=$true;data=@{disposition='started'}} | ConvertTo-Json -Depth 8 -Compress))
    [Console]::Out.WriteLine('{"type":"message_start","message":{"role":"assistant"}}')
    [Console]::Out.WriteLine('{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"answer: hello peer"}}')
    [Console]::Out.WriteLine('{"type":"message_end","message":{"role":"assistant","model":"test-model","provider":"test-local","usage":{"totalTokens":13,"input":10,"output":3}}}')
    [Console]::Out.WriteLine('{"type":"agent_settled","aborted":false}')
  }
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"))
        .args([
            "driver",
            "run",
            "--harness",
            "pi",
            "--cwd",
            root.path().to_str().unwrap(),
            "hello peer",
        ])
        .env("PATH", fake_path(root.path()).unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line).unwrap()["payload"]["text"]["delta"]
            == "answer: hello peer"
    }));
    assert!(stdout.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line).unwrap()["payload"]
            .get("settled")
            .is_some()
    }));

    let pid = fs::read_to_string(root.path().join("descendant.pid"))
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        // SAFETY: OpenProcess returns an owned handle or null for this PID.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            break;
        }
        // SAFETY: the handle remains live for this nonblocking wait.
        let exited = unsafe { WaitForSingleObject(handle, 0) } == WAIT_OBJECT_0;
        // SAFETY: release the handle obtained by OpenProcess above.
        unsafe { CloseHandle(handle) };
        if exited {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "descendant {pid} survived driver exit"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn early_harness_exit_is_a_command_failure() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("pi.cmd"), "@echo off\r\nexit /b 7\r\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"))
        .args([
            "driver",
            "run",
            "--harness",
            "pi",
            "--cwd",
            root.path().to_str().unwrap(),
            "hello",
        ])
        .env("PATH", fake_path(root.path()).unwrap())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pi exited"));
}

#[test]
fn settled_claude_error_is_a_command_failure() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("claude.cmd"),
        "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0claude.ps1\"\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("claude.ps1"),
        r#"
while ($null -ne ($line = [Console]::ReadLine())) {
  $m = $line | ConvertFrom-Json
  if ($m.type -eq 'user') {
    [Console]::Out.WriteLine('{"type":"result","stop_reason":"end_turn","is_error":true,"result":"test auth denied"}')
    break
  }
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"))
        .args([
            "driver",
            "run",
            "--harness",
            "claude",
            "--cwd",
            root.path().to_str().unwrap(),
            "hello",
        ])
        .env("PATH", fake_path(root.path()).unwrap())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line).unwrap()["payload"]["error"]["text"]
            == "test auth denied"
    }));
}

#[test]
fn ctrl_break_uses_claudes_native_interrupt_request() {
    // SAFETY: an existing console is equally suitable; a headless test needs one.
    unsafe { AllocConsole() };
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("claude.cmd"),
        "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0claude.ps1\"\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("claude.ps1"),
        r#"
while ($null -ne ($line = [Console]::ReadLine())) {
  $m = $line | ConvertFrom-Json
  if ($m.type -eq 'user') {
    [IO.File]::WriteAllText('ready', 'yes')
  } elseif ($m.type -eq 'control_request' -and $m.request.subtype -eq 'interrupt') {
    [IO.File]::WriteAllText('interrupted', 'yes')
    [Console]::Out.WriteLine('{"type":"result","stop_reason":"interrupted","is_error":false}')
    break
  }
}
"#,
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"))
        .args([
            "driver",
            "run",
            "--harness",
            "claude",
            "--cwd",
            root.path().to_str().unwrap(),
            "wait",
        ])
        .env("PATH", fake_path(root.path()).unwrap())
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.path().join("ready").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !root.path().join("ready").exists() {
        let _ = child.kill();
        let output = child.wait_with_output().unwrap();
        panic!(
            "fake Claude did not receive prompt: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // SAFETY: this targets the new child process group in our console.
    let sent = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) };
    if sent == 0 {
        let _ = child.kill();
        let _ = child.wait();
        panic!("send Ctrl-Break: {}", io::Error::last_os_error());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.path().join("interrupted").exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !root.path().join("interrupted").exists() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        root.path().join("interrupted").exists(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
