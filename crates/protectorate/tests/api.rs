//! The API surface of the real hub binary: every method of pt-api's table served under its
//! scope, the events WebSocket with snapshot and resume, device pairing, the web build, the D9
//! approval rule, the stdio MCP for vendor agents, and the quota reader thread.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::*;
use pt_api::{METHODS, Scope};
use pt_json::{Value, obj};

#[test]
fn job_wait_reports_lost_cancelled_and_hub_restart() {
    use std::io::Read;
    use std::os::unix::net::UnixListener;

    for state in ["lost", "cancelled", "offline"] {
        let path = std::env::temp_dir().join(format!(
            "pt-wait-{}-{}.sock",
            std::process::id(),
            free_port()
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let (release, blocked) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':')
                        && k.eq_ignore_ascii_case("content-length")
                    {
                        length = v.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                if step == 1 {
                    blocked.recv_timeout(Duration::from_secs(10)).unwrap();
                    if state == "offline" {
                        return;
                    }
                }
                let response = if step == 2 {
                    format!(
                        r#"{{"job_id":"42","offset":0,"next_offset":0,"text":"","eof":{}}}"#,
                        state != "lost"
                    )
                } else {
                    let status = if step == 0 { "queued" } else { state };
                    format!(
                        r#"{{"id":"42","target":{{"type":"node","node_id":"1"}},"argv":["true"],"state":"{status}","requested_by":"owner","queued_at_ms":1,"exit_code":23}}"#
                    )
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
                if step == 1 && state == "offline" {
                    return;
                }
            }
        });
        let mut cli = Command::new(bin())
            .arg("--socket")
            .arg(&path)
            .args(["job", "run", "--node", "1", "--wait", "--", "true"])
            .env_remove("PROTECTORATE_TOKEN")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (lines, output) = mpsc::channel();
        let stderr = cli.stderr.take().unwrap();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                lines.send(line.unwrap()).unwrap();
            }
        });
        let first = output
            .recv_timeout(Duration::from_secs(10))
            .expect("job id before waiting for status");
        assert!(
            first.contains("job 42") && first.contains("waiting"),
            "{first}"
        );
        release.send(()).unwrap();
        let status = cli.wait().unwrap();
        reader.join().unwrap();
        server.join().unwrap();
        let rest = output.try_iter().collect::<Vec<_>>().join("\n");
        assert!(!status.success(), "{state}: {rest}");
        if state == "offline" {
            assert!(
                rest.contains("job 42") && rest.contains("hub") && rest.contains("job show 42"),
                "{rest}"
            );
        } else {
            assert!(rest.contains(&format!("job 42: {state}")), "{rest}");
            assert!(
                !rest.contains("exit 23"),
                "a stale exit code must not replace {state}: {rest}"
            );
        }
    }
}

fn get(port: u16, path: &str, headers: &[(&str, &str)]) -> pt_http::ClientResponse {
    pt_http::Client::tcp(SocketAddr::from(([127, 0, 0, 1], port)))
        .request("GET", path, headers, b"")
        .expect("GET from the hub")
}

/// Every method is routed and its scope enforced; unbuilt backings and node methods without a
/// fleet answer `unavailable`; the
/// event stream snapshots, streams and resumes; pairing issues hashed `pd1.` tokens; the web
/// build is served with ETags and an SPA fallback.
#[test]
fn api_surface() {
    let port = free_port();
    let web = std::env::temp_dir().join(format!("pt-web-{port}"));
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>APP</html>").unwrap();
    std::fs::write(web.join("main.dart.js"), "console.log(1)").unwrap();
    std::fs::write(web.join("assets/a.json"), "{}").unwrap();
    let listen = format!("127.0.0.1:{port}");
    let hub = Hub::start_with(
        "api",
        "pi_local",
        &["--listen", &listen, "--web-dir", web.to_str().unwrap()],
        &[],
    );
    let owner = hub.printed("owner token (shown once): ");
    let agent = hub.printed("envoy-only token: ");

    for m in METHODS {
        let (s, v) = post(port, m.name, None, "{}");
        if m.scope == Scope::Public {
            assert_ne!(s, 401, "{} is public: {v}", m.name);
        } else {
            assert_eq!(s, 401, "{} without a credential: {v}", m.name);
            assert_eq!(v["error"]["code"].as_str(), Some("unauthenticated"));
        }
        let (s, v) = post(port, m.name, Some(&agent), "{}");
        let allowed = matches!(m.scope, Scope::EnvoyOnly | Scope::Public);
        assert_eq!(s == 403, !allowed, "{} for envoy-only: {s} {v}", m.name);
        let (s, v) = post(port, m.name, Some(&owner), "{}");
        let msg = v["error"]["message"].as_str().unwrap_or("");
        assert!(
            !msg.contains("no handler") && !msg.starts_with("no method"),
            "{} is routed: {s} {v}",
            m.name
        );
    }
    assert_eq!(post(port, "NoSuchMethod", Some(&owner), "{}").0, 404);

    let unbuilt = [
        ("TakeoverTerminal", obj! { "sleeve_id" => "envoy" }),
        (
            "RunJob",
            obj! { "target" => obj! { "type" => "node", "node_id" => "n1" }, "argv" => vec![Value::from("true")] },
        ),
        ("GetNode", obj! { "node_id" => "n1" }),
        ("GetEnrollment", obj! {}),
        (
            "Wait",
            obj! { "sleeve_id" => "envoy", "until" => "report", "timeout_ms" => 10 },
        ),
    ];
    for (method, body) in unbuilt {
        let (s, v) = post(port, method, Some(&owner), &body.to_string());
        assert_eq!(s, 503, "{method}: {v}");
        assert_eq!(v["error"]["code"].as_str(), Some("unavailable"));
    }
    for method in ["GetJob", "GetJobLog", "CancelJob"] {
        let (s, v) = post(
            port,
            method,
            Some(&owner),
            &obj! { "job_id" => "j1" }.to_string(),
        );
        assert_eq!(s, 404, "{method}: {v}");
        assert_eq!(v["error"]["code"].as_str(), Some("not_found"));
    }
    let (s, jobs) = post(port, "ListJobs", Some(&owner), "{}");
    assert_eq!((s, jobs["jobs"].as_array().map(<[_]>::len)), (200, Some(0)));
    assert_eq!(jobs["scoped_to_caller"].as_bool(), Some(false));
    let (s, nodes) = post(port, "ListNodes", Some(&owner), "{}");
    assert_eq!(
        (s, nodes["nodes"].as_array().map(<[_]>::len)),
        (200, Some(0))
    );
    let (s, q) = post(port, "ListQuotas", Some(&owner), "{}");
    assert_eq!(s, 200);
    let quotas = q["quotas"].as_array().unwrap();
    assert!(
        quotas
            .iter()
            .all(|q| q["reading"].as_str() == Some("unknown")),
        "{q}"
    );
    if on_path("pi") {
        assert!(
            quotas
                .iter()
                .any(|q| q["subscription"].as_str() == Some("local"))
        );
    }

    // The event stream: a snapshot, then kept messages one seq apart.
    let auth = format!("?access_token={owner}");
    let mut s1 = ws(port, &auth, &[]).expect("events with access_token");
    let snap = next(&mut s1);
    assert_eq!(snap["type"].as_str(), Some("snapshot"), "{snap}");
    let start = snap["seq"].as_u64().unwrap();
    assert_eq!(snap["state"]["seq"].as_u64(), Some(start));
    assert_eq!(snap["state"]["hub"]["name"].as_str(), Some("protectorate"));
    let phone = pair(port, &owner, "phone");
    let seen = until(&mut s1, "event", |m| {
        m["event"]["text"].as_str() == Some("device phone paired")
    });
    let mut last = start;
    for m in &seen {
        let seq = m["seq"].as_u64().unwrap();
        let delta = m["type"].as_str() == Some("entry")
            && m["entry"]["entry"]["delta"].as_bool() == Some(true);
        if delta {
            assert_eq!(seq, last, "a delta repeats the last seq");
        } else {
            assert_eq!(seq, last + 1, "kept messages are one seq apart: {m}");
            last = seq;
        }
    }
    drop(s1);
    pair(port, &owner, "tablet");
    let all = hub
        .cli(&["--json", "event", "list", "--limit", "100"])
        .json();
    let events = all["events"].as_array().unwrap();
    assert!(events.len() >= 2, "{all}");
    let newest = hub.cli(&["--json", "event", "list", "--limit", "2"]).json();
    assert_eq!(
        newest["events"].as_array().unwrap(),
        &events[events.len() - 2..]
    );
    let from = events[0]["seq"].as_u64().unwrap().to_string();
    let first = hub
        .cli(&["--json", "event", "list", "--from", &from, "--limit", "2"])
        .json();
    assert_eq!(first["events"].as_array().unwrap(), &events[..2]);
    let mut s2 = ws(port, &format!("{auth}&from_seq={}", last + 1), &[]).unwrap();
    let first = next(&mut s2);
    assert_ne!(
        first["type"].as_str(),
        Some("snapshot"),
        "resume replays: {first}"
    );
    assert_eq!(first["seq"].as_u64(), Some(last + 1));
    let tablet = |m: &Value| m["event"]["text"].as_str() == Some("device tablet paired");
    if !tablet(&first) {
        until(&mut s2, "event", tablet);
    }
    let mut s3 = ws(port, &format!("{auth}&from_seq=1"), &[]).unwrap();
    assert_eq!(
        next(&mut s3)["type"].as_str(),
        Some("snapshot"),
        "too old: snapshot"
    );
    let refused = |r: std::io::Result<pt_http::WebSocket>, status: &str| match r {
        Ok(_) => panic!("the stream must refuse ({status})"),
        Err(e) => assert!(e.to_string().contains(status), "{e}"),
    };
    refused(ws(port, "", &[]), "401");
    refused(ws(port, &format!("?access_token={agent}"), &[]), "403");
    refused(ws(port, &auth, &[("Origin", "http://evil.example")]), "403");
    let r = pt_http::Client::tcp(SocketAddr::from(([127, 0, 0, 1], port)))
        .request("POST", &format!("/api/GetState{auth}"), &[], b"{}")
        .unwrap();
    assert_eq!(r.status, 401, "access_token works on the events path only");

    // Devices: a pd1. token is the owner's scope, stored hashed, and revocable.
    assert!(phone.starts_with("pd1."));
    assert_eq!(post(port, "GetState", Some(&phone), "{}").0, 200);
    let file = std::fs::read_to_string(hub.home.join("devices.json")).unwrap();
    assert!(
        !file.contains("pd1.") && file.contains("token_sha256"),
        "{file}"
    );
    let add = hub.cli(&["device", "add", "laptop"]);
    let code = add
        .ok()
        .lines()
        .find_map(|l| l.strip_prefix("pairing code for laptop: "))
        .expect("device add prints the code")
        .to_string();
    let paired = hub.cli(&["--json", "device", "pair", &code]).json();
    assert_eq!(paired["device"]["name"].as_str(), Some("laptop"));
    let again = post(
        port,
        "PairDevice",
        None,
        &obj! { "code" => code }.to_string(),
    );
    assert_eq!(again.0, 401, "a code works once");
    let list = hub.cli(&["--json", "device", "list"]).json();
    let devices = list["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 3);
    let id = devices
        .iter()
        .find(|d| d["name"].as_str() == Some("phone"))
        .and_then(|d| d["id"].as_str())
        .unwrap();
    let mut on_phone = ws(port, &format!("?access_token={phone}"), &[]).unwrap();
    assert_eq!(next(&mut on_phone)["type"].as_str(), Some("snapshot"));
    hub.cli(&["device", "revoke", id]).ok();
    assert_eq!(post(port, "GetState", Some(&phone), "{}").0, 401, "revoked");
    // A stream the device opened before is closed too, within a second or so.
    let closed = loop {
        match on_phone.recv() {
            Ok(pt_http::Message::Close(code, _)) => break Some(code),
            Ok(_) => {}
            Err(_) => break None,
        }
    };
    assert_eq!(closed, Some(1008), "the revoked device's stream ends");

    // The web build: files with ETags, the SPA fallback, 404 for a missing asset.
    let index = get(port, "/", &[]);
    assert_eq!(index.status, 200);
    assert_eq!(index.body, b"<html>APP</html>");
    assert_eq!(index.headers.get("Cache-Control"), Some("no-cache"));
    let etag = index.headers.get("ETag").expect("an ETag").to_string();
    assert_eq!(get(port, "/", &[("If-None-Match", &etag)]).status, 304);
    let js = get(port, "/main.dart.js", &[]);
    assert!(
        js.headers
            .get("Content-Type")
            .unwrap()
            .starts_with("text/javascript")
    );
    assert_eq!(
        get(port, "/sleeves/some/route", &[]).body,
        b"<html>APP</html>"
    );
    assert_eq!(get(port, "/assets/a.json", &[]).body, b"{}");
    assert_eq!(get(port, "/missing.js", &[]).status, 404);
    assert_eq!(get(port, "/../tokens", &[]).status, 404);
}

/// D9: in an Envoy turn an agent message started, Envoy's own capability cannot remove a sleeve
/// or spawn past the limit until the owner decides; outside such a turn it can.
#[test]
fn d9_agent_turns_need_owner_approval() {
    if !have_runtime() {
        return;
    }
    let hub = Hub::start_config(
        "d9",
        "pi_local",
        &[
            ("d9_spawn_limit", Value::from(1u64)),
            ("d9_approval_timeout_secs", Value::from(4u64)),
        ],
        &[],
        &[],
    );
    hub.wait("envoy", "idle", 120);
    let envoy_id = hub.sleeve("envoy")["id"].as_str().unwrap().to_string();
    let envoy_token = hub.sleeve_token(&envoy_id, 1);
    let as_envoy = [("PROTECTORATE_TOKEN", envoy_token.as_str())];
    let agent = hub.printed("envoy-only token: ");
    let as_agent = [("PROTECTORATE_TOKEN", agent.as_str())];
    let victim_repo = temp_repo("d9-victim");
    hub.spawn_pi("victim", &victim_repo);
    let spare_repo = temp_repo("d9-spare");
    hub.spawn_pi("spare", &spare_repo);

    // No agent turn: Envoy removes a sleeve on its own say-so.
    let r = hub.cli_env(&as_envoy, &["sleeve", "rm", "spare"]);
    assert_eq!(r.ok().trim(), "removing");
    hub.wait("spare", "ended", 60);

    let long = "Write the numbers from 1 to 1500 as words, one per line, with no other text.";
    let agent_turn = || {
        hub.cli_env(&as_agent, &["envoy", "send", "--as", "grokbot", long])
            .ok();
        eventually(
            "Envoy working on the agent's message",
            Duration::from_secs(90),
            || {
                let s = hub.sleeve("envoy");
                (s["incarnation"]["activity"]["state"].as_str() == Some("working")).then_some(())
            },
        );
    };
    let working = |what: &str| {
        let s = hub.sleeve("envoy");
        assert_eq!(
            s["incarnation"]["activity"]["state"].as_str(),
            Some("working"),
            "Envoy's agent turn must still run {what}"
        );
    };
    let pending = |title: &str| -> Value {
        eventually("the D9 approval", Duration::from_secs(20), || {
            let v = hub.cli(&["--json", "approval", "list", "--pending"]).json();
            v["approvals"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["title"].as_str().is_some_and(|t| t.starts_with(title)))
                .cloned()
        })
    };
    let blocked = |args: Vec<String>| {
        let (tx, rx) = mpsc::channel();
        let (home, token) = (hub.home.clone(), envoy_token.clone());
        std::thread::spawn(move || {
            let out = Command::new(bin())
                .arg("--home")
                .arg(home)
                .args(&args)
                .env("PROTECTORATE_TOKEN", token)
                .output()
                .expect("run protectorate");
            let _ = tx.send(out);
        });
        rx
    };

    // Turn A: a removal is held and denied; one spawn passes, the next times out.
    agent_turn();
    let rm = blocked(vec!["sleeve".into(), "rm".into(), "victim".into()]);
    let a = pending("remove sleeve victim");
    assert_eq!(a["kind"].as_str(), Some("action"));
    assert_eq!(a["sleeve_id"].as_str(), Some(envoy_id.as_str()));
    assert!(
        a["summary"].as_str().unwrap().contains("agent:grokbot"),
        "{a}"
    );
    assert!(rm.try_recv().is_err(), "the removal waits for the owner");
    assert_eq!(hub.sleeve("victim")["lifecycle"].as_str(), Some("running"));
    let id = a["id"].as_str().unwrap();
    hub.cli(&["approval", "decide", id, "deny"]).ok();
    let out = rm
        .recv_timeout(Duration::from_secs(20))
        .expect("rm returns");
    assert_eq!(out.status.code(), Some(4), "denied: {:?}", out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("did not allow"));
    assert_eq!(hub.sleeve("victim")["lifecycle"].as_str(), Some("running"));
    working("after the denial");

    let b1 = temp_repo("d9-b1");
    let b2 = temp_repo("d9-b2");
    let spawn = |name: &str, repo: &std::path::Path| {
        vec![
            "sleeve".to_string(),
            "spawn".into(),
            name.into(),
            "--path".into(),
            repo.to_str().unwrap().into(),
            "--harness".into(),
            "pi_local".into(),
        ]
    };
    let first = blocked(spawn("b1", &b1))
        .recv_timeout(Duration::from_secs(30))
        .unwrap();
    assert!(
        first.status.success(),
        "the first spawn is within the limit: {first:?}"
    );
    let started = Instant::now();
    let second = blocked(spawn("b2", &b2))
        .recv_timeout(Duration::from_secs(30))
        .unwrap();
    assert_eq!(second.status.code(), Some(1), "{second:?}");
    assert!(String::from_utf8_lossy(&second.stderr).contains("timeout"));
    assert!(
        started.elapsed() >= Duration::from_secs(4),
        "it waited for the owner"
    );
    assert_eq!(hub.sleeve("b2"), Value::Null, "b2 was never spawned");
    working("after the spawn timeout");
    let all = hub.cli(&["--json", "approval", "list"]).json();
    let states: Vec<(&str, &str)> = all["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["title"].as_str().unwrap(), a["state"].as_str().unwrap()))
        .collect();
    assert!(
        states.contains(&("remove sleeve victim", "denied")),
        "{states:?}"
    );
    assert!(
        states
            .iter()
            .any(|(t, s)| t.starts_with("spawn sleeve b2") && *s == "expired"),
        "{states:?}"
    );
    let entries = hub.transcript("envoy");
    let cards: Vec<&str> = of_type(&entries, "approval_request")
        .iter()
        .filter_map(|e| e["entry"]["title"].as_str())
        .collect();
    assert!(
        cards.contains(&"remove sleeve victim"),
        "inline in Envoy's chat: {cards:?}"
    );
    let from: Vec<&str> = of_type(&entries, "user_message")
        .iter()
        .filter_map(|e| e["entry"]["from"].as_str())
        .collect();
    assert!(from.contains(&"agent:grokbot"), "{from:?}");
    hub.cli(&["sleeve", "interrupt", "envoy"]).ok();
    hub.wait("envoy", "idle", 120);

    // Turn B: the owner allows the removal.
    agent_turn();
    let rm = blocked(vec!["sleeve".into(), "rm".into(), "victim".into()]);
    let a = pending("remove sleeve victim");
    hub.cli(&["approval", "decide", a["id"].as_str().unwrap(), "allow"])
        .ok();
    let out = rm
        .recv_timeout(Duration::from_secs(30))
        .expect("rm returns");
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("owner approved action:"));
    hub.wait("victim", "ended", 60);
    hub.cli(&["sleeve", "interrupt", "envoy"]).ok();
}

struct Mcp {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    out: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl Mcp {
    fn start(hub: &Hub, token_file: &std::path::Path) -> Mcp {
        let mut child = Command::new(bin())
            .arg("--home")
            .arg(&hub.home)
            .args(["mcp", "--envoy", "--token-file"])
            .arg(token_file)
            .env_remove("PROTECTORATE_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start protectorate mcp");
        let stdin = child.stdin.take().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        Mcp {
            child,
            stdin,
            out,
            next: 1,
        }
    }

    fn send(&mut self, msg: &Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&obj! { "jsonrpc" => "2.0", "id" => id, "method" => method, "params" => params });
        let mut line = String::new();
        loop {
            line.clear();
            assert!(self.out.read_line(&mut line).unwrap() > 0, "mcp exited");
            let v = pt_json::parse(&line).expect("one JSON-RPC message per line");
            if v["id"].as_u64() == Some(id) {
                assert_eq!(v["jsonrpc"].as_str(), Some("2.0"));
                return v;
            }
        }
    }

    fn tool(&mut self, name: &str, args: Value) -> (String, bool) {
        let v = self.request("tools/call", obj! { "name" => name, "arguments" => args });
        let r = &v["result"];
        let text = r["content"][0]["text"].as_str().unwrap_or("").to_string();
        (text, r["isError"].as_bool().unwrap_or(false))
    }
}

/// `protectorate mcp --envoy` over real pipes: the MCP handshake, the three tools against a real
/// hub with Envoy on the local model, and JSON-RPC errors.
#[test]
fn mcp_envoy_round_trip() {
    if !have_runtime() {
        return;
    }
    let hub = Hub::start("mcp");
    hub.wait("envoy", "idle", 120);
    let no_token = Command::new(bin())
        .arg("--home")
        .arg(&hub.home)
        .args(["mcp", "--envoy"])
        .env_remove("PROTECTORATE_TOKEN")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        no_token.status.code(),
        Some(2),
        "mcp needs an envoy-only token"
    );
    let token = hub.cli(&["envoy", "token"]).ok().trim().to_string();
    assert_eq!(token, hub.printed("envoy-only token: "));
    let file = hub.home.with_file_name("envoy-token");
    std::fs::write(&file, &token).unwrap();

    let mut mcp = Mcp::start(&hub, &file);
    let early = mcp.request("tools/list", obj! {});
    assert_eq!(early["error"]["code"].as_i64(), Some(-32600), "{early}");
    let init = mcp.request(
        "initialize",
        obj! {
            "protocolVersion" => "2025-06-18",
            "capabilities" => obj! {},
            "clientInfo" => obj! { "name" => "duet", "version" => "1" },
        },
    );
    let r = &init["result"];
    assert_eq!(r["protocolVersion"].as_str(), Some("2025-06-18"), "{init}");
    assert!(r["capabilities"]["tools"].as_object().is_some());
    assert_eq!(r["serverInfo"]["name"].as_str(), Some("protectorate-envoy"));
    mcp.send(&obj! { "jsonrpc" => "2.0", "method" => "notifications/initialized" });
    let ping = mcp.request("ping", obj! {});
    assert!(ping["result"].as_object().is_some(), "{ping}");
    let list = mcp.request("tools/list", obj! {});
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names, ["envoy_send", "envoy_wait", "envoy_read"]);
    assert_eq!(
        mcp.request("resources/list", obj! {})["error"]["code"].as_i64(),
        Some(-32601)
    );
    assert_eq!(
        mcp.request("tools/call", obj! { "name" => "sleeve_rm" })["error"]["code"].as_i64(),
        Some(-32602)
    );

    let (sent, err) = mcp.tool(
        "envoy_send",
        obj! { "text" => "Reply with exactly the word PONG and nothing else." },
    );
    assert!(!err && sent.starts_with("request_id: ga:duet:"), "{sent}");
    let (reply, err) = mcp.tool("envoy_wait", obj! { "timeout_secs" => 180 });
    assert!(!err && reply.contains("PONG"), "{reply}");
    let (read, err) = mcp.tool("envoy_read", obj! {});
    assert!(
        !err && read.contains("agent:duet: [from agent:duet] Reply with"),
        "{read}"
    );
    let (gone, err) = mcp.tool("envoy_wait", obj! { "request_id" => "ga:duet:nope" });
    assert!(err && gone.contains("not_found"), "{gone}");

    let old = Mcp::start(&hub, &file).request(
        "initialize",
        obj! { "protocolVersion" => "1999-01-01", "capabilities" => obj! {} },
    );
    assert_eq!(
        old["result"]["protocolVersion"].as_str(),
        Some("2025-06-18")
    );
    drop(mcp.stdin);
    let status = mcp.child.wait().unwrap();
    assert!(status.success(), "mcp exits cleanly when stdin closes");
}

/// The quota reader thread: with only the local model configured (vendor programs pointed at
/// nothing), a stand-in OpenAI-compatible endpoint makes the local row fresh, on ListQuotas and
/// as a `quota` stream message.
#[test]
fn quota_readers_run() {
    let models = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let models_port = models.local_addr().unwrap().port();
    let (ready_tx, ready_rx) = mpsc::channel();
    // Lists the hub's configured Pi model too, as the owner's endpoint would.
    let model = pi_model();
    std::thread::spawn(move || {
        let mut ready = Some(ready_rx);
        for c in models.incoming() {
            let Ok(mut c) = c else { continue };
            let mut req = [0u8; 4096];
            let _ = std::io::Read::read(&mut c, &mut req);
            if let Some(ready) = ready.take() {
                ready.recv_timeout(Duration::from_secs(30)).unwrap();
            }
            let body = format!(
                r#"{{"object":"list","data":[{{"id":"stand-in-model"}},{{"id":{}}}]}}"#,
                Value::from(model.as_str())
            );
            let _ = write!(
                c,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let none = "/nonexistent/program";
    let hub = Hub::start_config(
        "quota",
        "pi_local",
        &[
            (
                "local_url",
                format!("http://127.0.0.1:{models_port}/v1").into(),
            ),
            ("quota_poll_secs", 60u64.into()),
            ("claude_program", none.into()),
            ("codex_program", none.into()),
            ("grok_program", none.into()),
        ],
        &["--listen", &listen],
        &[("PT_TEST_QUOTA_STALE_MS", "2000")],
    );
    let owner = hub.printed("owner token (shown once): ");
    let mut s = ws(port, &format!("?access_token={owner}"), &[]).expect("events");
    assert_eq!(next(&mut s)["type"].as_str(), Some("snapshot"));
    ready_tx.send(()).unwrap();
    let seen = until(&mut s, "quota", |m| {
        m["quota"]["subscription"].as_str() == Some("local")
            && m["quota"]["reading"].as_str() == Some("ok")
    });
    let q = &seen.last().unwrap()["quota"];
    assert!(
        q["note"].as_str().unwrap_or("").contains("stand-in-model"),
        "{q}"
    );
    let (st, list) = post(port, "ListQuotas", Some(&owner), "{}");
    assert_eq!(st, 200);
    let rows = list["quotas"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "only the local reader: {list}");
    assert_eq!(rows[0]["reading"].as_str(), Some("ok"), "{list}");
    let state = hub.state();
    assert_eq!(
        state["quotas"][0]["reading"].as_str(),
        Some("ok"),
        "{state}"
    );
    #[cfg(debug_assertions)]
    eventually(
        "cached quota to expire without another poll",
        Duration::from_secs(8),
        || {
            let (_, list) = post(port, "ListQuotas", Some(&owner), "{}");
            (list["quotas"][0]["reading"].as_str() == Some("stale")).then_some(())
        },
    );
}

/// The owner's socket takes a caller without a token as the owner only when it can tell that
/// caller is not in a sleeve: a connection whose process has exited (its descriptor kept by
/// another) is refused, not taken for the owner. The sleeves' socket never takes one.
#[test]
fn unidentified_peer_is_refused() {
    if !on_path("python3") {
        eprintln!("SKIP: python3 is not installed");
        return;
    }
    let hub = Hub::start_with("peer", "pi_local", &[], &[]);
    // The connecting process exits; its child sends the request on the inherited socket.
    let script = r#"
import os, socket, sys, time
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
parent = os.getpid()
if os.fork():
    os._exit(0)
while os.path.exists("/proc/%d" % parent):
    time.sleep(0.01)
time.sleep(0.2)
s.sendall(b"POST /api/ListSleeves HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
print(s.recv(4096).split(b"\r\n")[0].decode())
"#;
    let n = std::cell::Cell::new(0);
    let status = |sock: &std::path::Path| {
        n.set(n.get() + 1);
        let out = hub.home.with_file_name(format!("orphan-{}.txt", n.get()));
        let mut p = Command::new("python3")
            .args(["-I", "-c", script])
            .arg(sock)
            .stdout(std::fs::File::create(&out).unwrap())
            .spawn()
            .expect("python3");
        // Reaped, so its pid is gone before the child sends.
        p.wait().unwrap();
        eventually("the orphan's answer", Duration::from_secs(10), || {
            let t = std::fs::read_to_string(&out).unwrap_or_default();
            (!t.trim().is_empty()).then(|| t.trim().to_string())
        })
    };
    let owner = status(&hub.home.join("run/api.sock"));
    assert!(owner.contains(" 403 "), "owner socket: {owner}");
    let sleeves = status(&hub.home.join("run/sleeves/api.sock"));
    assert!(sleeves.contains(" 401 "), "sleeves socket: {sleeves}");
    // The owner's own shell is still the owner.
    hub.cli(&["sleeve", "list"]).ok();
}
