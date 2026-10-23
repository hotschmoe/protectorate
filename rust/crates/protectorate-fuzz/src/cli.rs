use protectorate_mcpbridge::{Bridge, Config};
use serde_json::Value;
use std::{
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

/// NUL-separated argv for the owner CLI, without the program name.
pub fn cli_args(data: &[u8]) {
    let args: Vec<String> = data
        .split(|&b| b == 0)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let parsed = protectorate_cli::parse(&args).map_err(|e| e.to_string());
    assert_eq!(
        parsed,
        protectorate_cli::parse(&args).map_err(|e| e.to_string()),
        "argument parsing is not deterministic"
    );
}

fn bridge() -> Arc<Bridge> {
    static BRIDGE: OnceLock<Arc<Bridge>> = OnceLock::new();
    Arc::clone(BRIDGE.get_or_init(|| {
        Arc::new(Bridge::new(Config::without_hub("fuzz")).expect("bridge from the catalog"))
    }))
}

/// Newline-delimited JSON-RPC from an MCP client over stdio. Every reply is
/// one JSON-RPC 2.0 object, and no input line is answered more than once.
pub fn mcp_stdio(data: &[u8]) {
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let _ = bridge().run(Cursor::new(data.to_vec()), &output);
    let bytes = std::mem::take(&mut *output.lock().unwrap());
    let text = String::from_utf8(bytes).expect("bridge output is UTF-8");
    let mut responses = 0;
    for line in text.lines() {
        let reply: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bridge wrote a non-JSON line {line:?}: {e}"));
        assert_eq!(reply["jsonrpc"], "2.0", "{line}");
        let is_response = reply.get("result").is_some() || reply.get("error").is_some();
        assert!(
            is_response != reply.get("method").is_some(),
            "neither a response nor a notification: {line}"
        );
        responses += usize::from(is_response);
    }
    let lines = data
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .count();
    assert!(responses <= lines, "{responses} responses to {lines} lines");
}
