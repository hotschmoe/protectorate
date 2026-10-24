#![cfg(target_os = "linux")]
use crate::support;

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn pi_driver_streams_a_prompt_to_an_official_rpc_peer() {
    let root = tempfile::tempdir().unwrap();
    let fake = root.path().join("pi");
    support::write_executable(
        &fake,
        r#"#!/usr/bin/python3
import json,sys
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='get_state':
  print(json.dumps({'type':'response','id':m['id'],'command':'get_state','success':True,'data':{'sessionId':'test-session','model':{'id':'test-model','provider':'test-local','baseUrl':'http://localhost','contextWindow':8192},'thinkingLevel':'medium'}}),flush=True)
 elif m.get('type')=='prompt':
  print(json.dumps({'type':'response','id':m['id'],'command':'prompt','success':True,'data':{'disposition':'started'}}),flush=True)
  print(json.dumps({'type':'message_start','message':{'role':'assistant'}}),flush=True)
  print(json.dumps({'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':'answer: '+m['message']}}),flush=True)
  print(json.dumps({'type':'message_end','message':{'role':'assistant','model':'test-model','provider':'test-local','usage':{'totalTokens':13,'input':10,'output':3}}}),flush=True)
  print(json.dumps({'type':'agent_settled','aborted':False}),flush=True)
"#,
    );
    let path = format!(
        "{}:{}",
        root.path().display(),
        std::env::var("PATH").unwrap()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
        .args([
            "driver",
            "run",
            "--harness",
            "pi",
            "--cwd",
            root.path().to_str().unwrap(),
            "hello peer",
        ])
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let events = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| event["payload"]["text"]["delta"] == "answer: hello peer"),
        "{stdout}"
    );
    assert!(
        events
            .iter()
            .any(|event| event["payload"].get("settled").is_some()),
        "{stdout}"
    );
}

#[test]
fn claude_driver_prompts_and_sends_native_approval_response() {
    let root = tempfile::tempdir().unwrap();
    let fake = root.path().join("claude");
    support::write_executable(
        &fake,
        r#"#!/usr/bin/python3
import json,sys
def emit(value): print(json.dumps(value),flush=True)
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='user':
  emit({'type':'control_request','request_id':'approval-1','request':{'subtype':'can_use_tool','tool_name':'Bash','input':{'command':'pwd'}}})
 elif m.get('type')=='control_response':
  with open('approval-received','w') as f: f.write(m['response']['response']['behavior'])
  emit({'type':'result','stop_reason':'end_turn','is_error':False})
"#,
    );
    let path = format!(
        "{}:{}",
        root.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
        .args([
            "driver",
            "run",
            "--harness",
            "claude",
            "--cwd",
            root.path().to_str().unwrap(),
            "approve this",
        ])
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("approval-received")).unwrap(),
        "allow"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.lines().any(
            |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["payload"]
                ["approvalResolved"]["allow"]
                == true
        ),
        "{stdout}"
    );
}

#[test]
fn harness_exit_before_settlement_is_a_command_failure() {
    let root = tempfile::tempdir().unwrap();
    let fake = root.path().join("claude");
    support::write_executable(
        &fake,
        "#!/usr/bin/python3\nimport sys\nsys.stdin.readline()\nsys.exit(7)\n",
    );
    let path = format!(
        "{}:{}",
        root.path().display(),
        std::env::var("PATH").unwrap()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
        .args([
            "driver",
            "run",
            "--harness",
            "claude",
            "--cwd",
            root.path().to_str().unwrap(),
            "hello",
        ])
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("claude exited"));
}

#[test]
fn first_sigint_uses_the_harness_interrupt_protocol() {
    let root = tempfile::tempdir().unwrap();
    let fake = root.path().join("claude");
    support::write_executable(
        &fake,
        r#"#!/usr/bin/python3
import json,sys
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='user':
  with open('ready','w') as f: f.write('yes')
 elif m.get('type')=='control_request' and m.get('request',{}).get('subtype')=='interrupt':
  with open('interrupted','w') as f: f.write('yes')
  print(json.dumps({'type':'result','stop_reason':'interrupted','is_error':False}),flush=True)
"#,
    );
    let path = format!(
        "{}:{}",
        root.path().display(),
        std::env::var("PATH").unwrap()
    );
    let child = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
        .args([
            "driver",
            "run",
            "--harness",
            "claude",
            "--cwd",
            root.path().to_str().unwrap(),
            "wait",
        ])
        .env("PATH", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    while !root.path().join("ready").exists() {
        assert!(
            Instant::now() < deadline,
            "the harness never got the prompt"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("interrupted")).unwrap(),
        "yes"
    );
}
