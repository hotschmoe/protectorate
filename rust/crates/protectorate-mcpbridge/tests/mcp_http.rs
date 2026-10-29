//! The bridge binary's streamable HTTP endpoint in front of a hub peer that
//! answers one procedure and holds the other open.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

const TOKEN: &str = "bridge-token";
const WAIT: Duration = Duration::from_secs(30);

struct Bridge {
    child: Child,
    addr: String,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, PartialEq)]
enum Hub {
    Answered { authorization: String },
    Held,
    Closed,
}

/// A hub that answers GetStatus and never answers anything else, reporting
/// when a held request's connection goes away.
fn hub() -> (String, Receiver<Hub>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (events, received) = channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let events = events.clone();
            thread::spawn(move || {
                let (head, _) = read_request(&mut stream);
                let header = |name: &str| {
                    head.lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case(name)
                                .then(|| value.trim().to_owned())
                        })
                        .unwrap_or_default()
                };
                if head.starts_with("POST /protectorate.v1.HubService/GetStatus ") {
                    let body = r#"{"version":"hub-peer-1"}"#;
                    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = events.send(Hub::Answered {
                        authorization: header("authorization"),
                    });
                    return;
                }
                let _ = events.send(Hub::Held);
                let mut rest = [0_u8; 256];
                while matches!(stream.read(&mut rest), Ok(n) if n > 0) {}
                let _ = events.send(Hub::Closed);
            });
        }
    });
    (url, received)
}

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            break;
        }
        bytes.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&bytes).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    let _ = stream.read_exact(&mut body);
    (head, body)
}

fn bridge(hub: &str) -> Bridge {
    let mut child = Command::new(env!("CARGO_BIN_EXE_protectorate-mcpbridge"))
        // A hub timeout far past WAIT, so only cancellation can end a held call.
        .args(["--http", "127.0.0.1:0", "--hub", hub, "--timeout", "600"])
        .env("PROTECTORATE_TOKEN", TOKEN)
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = BufReader::new(child.stderr.take().unwrap());
    let (ready, listening) = channel();
    thread::spawn(move || {
        for line in stderr.lines().map_while(Result::ok) {
            if let Some(addr) = line.strip_prefix("mcp listening on http://") {
                let _ = ready.send(addr.to_owned());
            }
        }
    });
    let addr = listening
        .recv_timeout(WAIT)
        .expect("bridge never reported its address");
    Bridge { child, addr }
}

struct Reply {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
}

fn http(addr: &str, method: &str, headers: &[(&str, &str)], body: Option<Value>) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    let body = body.map(|body| body.to_string()).unwrap_or_default();
    let mut request = format!(
        "{method} /mcp HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let raw = String::from_utf8(raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_owned(),
    }
}

const BEARER: (&str, &str) = ("Authorization", "Bearer bridge-token");
const JSON: (&str, &str) = ("Content-Type", "application/json");

fn initialize(addr: &str) -> String {
    let reply = http(
        addr,
        "POST",
        &[BEARER, JSON],
        Some(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"http-test","version":"1"}}}),
        ),
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.json()["result"]["protocolVersion"], "2025-11-25");
    reply.headers["mcp-session-id"].clone()
}

fn call(addr: &str, session: &str, id: u64, tool: &str) -> Reply {
    http(
        addr,
        "POST",
        &[BEARER, JSON, ("Mcp-Session-Id", session)],
        Some(json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":tool,"arguments":{}}})),
    )
}

#[test]
fn http_sessions_need_the_bearer_a_same_origin_and_a_live_session() {
    let (hub_url, hub) = hub();
    let bridge = bridge(&hub_url);
    let addr = bridge.addr.as_str();
    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
    let same_origin = format!("http://{addr}");
    for (headers, status) in [
        (vec![JSON], 401),
        (vec![("Authorization", "Bearer wrong"), JSON], 401),
        (
            vec![BEARER, JSON, ("Origin", "http://attacker.example")],
            403,
        ),
        (vec![BEARER, JSON, ("Origin", "null")], 403),
        (vec![BEARER, JSON, ("Origin", same_origin.as_str())], 400),
        (vec![BEARER, JSON], 400),
        (
            vec![BEARER, JSON, ("Mcp-Session-Id", "no-such-session")],
            404,
        ),
    ] {
        let reply = http(addr, "POST", &headers, Some(list.clone()));
        assert_eq!(reply.status, status, "{headers:?}: {}", reply.body);
    }
    assert!(hub.try_recv().is_err(), "a refused request reached the hub");

    let session = initialize(addr);
    assert_eq!(session.len(), 48, "{session}");
    let reply = http(
        addr,
        "POST",
        &[BEARER, JSON, ("Mcp-Session-Id", &session)],
        Some(list.clone()),
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let tools = reply.json()["result"]["tools"].clone();
    assert!(
        tools
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "hub_get_status"),
        "{tools}"
    );
    let reply = call(addr, &session, 3, "hub_get_status");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let result = &reply.json()["result"];
    assert_ne!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hub-peer-1"),
        "{result}"
    );
    assert_eq!(
        hub.recv_timeout(WAIT).unwrap(),
        Hub::Answered {
            authorization: format!("Bearer {TOKEN}")
        }
    );
    let reply = http(
        addr,
        "POST",
        &[
            BEARER,
            JSON,
            ("Mcp-Session-Id", &session),
            ("MCP-Protocol-Version", "2025-06-18"),
        ],
        Some(list.clone()),
    );
    assert_eq!(reply.status, 400, "{}", reply.body);

    let other = initialize(addr);
    assert_ne!(other, session);
    let reply = http(
        addr,
        "DELETE",
        &[BEARER, ("Mcp-Session-Id", &session)],
        None,
    );
    assert_eq!(reply.status, 204, "{}", reply.body);
    let reply = http(
        addr,
        "POST",
        &[BEARER, JSON, ("Mcp-Session-Id", &session)],
        Some(list.clone()),
    );
    assert_eq!(reply.status, 404, "{}", reply.body);
    let reply = http(
        addr,
        "POST",
        &[BEARER, JSON, ("Mcp-Session-Id", &other)],
        Some(list),
    );
    assert_eq!(reply.status, 200, "ending one session ended another");
}

#[test]
fn cancelling_a_tool_call_drops_its_hub_request() {
    let (hub_url, hub) = hub();
    let bridge = bridge(&hub_url);
    let addr = bridge.addr.clone();
    for (how, request) in [("notification", 7), ("session end", 8)] {
        let session = initialize(&addr);
        let pending = {
            let (addr, session) = (addr.clone(), session.clone());
            thread::spawn(move || call(&addr, &session, request, "hub_get_sleeve_runtime"))
        };
        assert_eq!(hub.recv_timeout(WAIT).unwrap(), Hub::Held, "{how}");
        let reply = if how == "notification" {
            http(
                &addr,
                "POST",
                &[BEARER, JSON, ("Mcp-Session-Id", &session)],
                Some(json!({"jsonrpc":"2.0","method":"notifications/cancelled",
                    "params":{"requestId":request,"reason":"test"}})),
            )
        } else {
            http(
                &addr,
                "DELETE",
                &[BEARER, ("Mcp-Session-Id", &session)],
                None,
            )
        };
        assert!(matches!(reply.status, 202 | 204), "{how}: {}", reply.body);
        assert_eq!(
            hub.recv_timeout(WAIT).unwrap(),
            Hub::Closed,
            "{how}: the hub request outlived its cancellation"
        );
        let reply = pending.join().unwrap();
        assert_eq!(reply.status, 200, "{how}: {}", reply.body);
        let result = &reply.json()["result"];
        assert_eq!(result["isError"], true, "{how}: {result}");
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("cancelled"),
            "{how}: {result}"
        );
    }
}
