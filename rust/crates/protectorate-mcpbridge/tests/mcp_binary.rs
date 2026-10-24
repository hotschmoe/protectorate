//! The distributed binary must keep stdout as an MCP protocol stream.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn list_needs_no_hub_or_token_and_respects_filter() {
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"))
        .args(["--allow", "hub_get_status", "--list"])
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "hub_get_status");
}

#[test]
fn list_exits_cleanly_when_reader_closes_pipe() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"))
        .args(["--allow", "*", "--list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn comma_separated_filters_select_and_exclude_tools() {
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"))
        .args(["--allow", "hub_*,backlog_*", "--deny=backlog_*", "--list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["hub_get_sleeve_runtime", "hub_get_status"]);
}

#[test]
fn list_matches_the_committed_mcp_catalog() {
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"))
        .args(["--allow", "*", "--list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools: Value = serde_json::from_slice(&output.stdout).unwrap();
    let golden: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/testdata/mcp_tools.json"
    )))
    .unwrap();
    assert_eq!(tools, golden);
}

#[test]
fn stdio_initialize_and_list_are_json_rpc_only() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"))
        .env("PROTECTORATE_TOKEN", "test-token")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(
        input,
        "{}",
        json!({
            "jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
                "protocolVersion":"2025-11-25", "capabilities":{},
                "clientInfo":{"name":"binary-test","version":"1"}
            }
        })
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({
            "jsonrpc":"2.0", "id":2, "method":"tools/list", "params":{}
        })
    )
    .unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<Value> = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(
        replies.len(),
        2,
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[1]["id"], 2);
    assert!(replies[1]["result"]["tools"].as_array().is_some());
}

#[test]
fn https_binds_only_explicit_private_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let ca = protectorate_tls::Ca::open_or_create(&dir.path().join("ca")).unwrap();
    let identity = ca.hub_identity(&["127.0.0.1".to_owned()]).unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");
    protectorate_fsutil::write_file(&cert, identity.cert_pem.as_bytes(), 0o644).unwrap();
    protectorate_fsutil::write_file(&key, identity.key_pem.as_bytes(), 0o600).unwrap();
    let bridge = |listen: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"));
        command
            .args(["--https", listen, "--tls-cert"])
            .arg(&cert)
            .arg("--tls-key")
            .arg(&key)
            .env("PROTECTORATE_TOKEN", "test-token")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        command
    };
    for listen in ["0.0.0.0:0", "[::]:0", "8.8.8.8:7471", "localhost:0"] {
        let output = bridge(listen).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{listen} was accepted: {stderr}");
        assert!(
            stderr.contains("refusing to listen") || stderr.contains("an IP address and a port"),
            "{listen}: {stderr}"
        );
    }
    let mut child = bridge("127.0.0.1:0").spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        line.starts_with("mcp listening on https://127.0.0.1:"),
        "{line}"
    );
}
