use protectorate_cli::{execute, parse, transport_timeout, Client, Code, Command, WaitUntil};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, RpcError, RpcResponse, UnaryServer,
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read},
    process::{Command as ProcessCommand, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::net::TcpListener;

const WAIT: &str = "/protectorate.v1.ChatService/Wait";

#[test]
fn chat_wait_parses_one_or_more_sleeves_and_deduplicates_them() {
    let invocation = parse(["chat", "wait", "sleeve-x"]).unwrap();
    assert!(
        matches!(&invocation.command, Command::ChatWait { sleeves, until, timeout }
            if sleeves == &["sleeve-x".to_owned()] && *until == WaitUntil::Idle && timeout.is_zero()),
        "{invocation:?}"
    );

    let invocation = parse(["chat", "wait", "a", "b", "c"]).unwrap();
    assert!(
        matches!(&invocation.command, Command::ChatWait { sleeves, .. }
            if sleeves == &["a".to_owned(), "b".to_owned(), "c".to_owned()]),
        "{invocation:?}"
    );

    // Repeated sleeves wait once, the way the Go CLI skips duplicates.
    let invocation = parse(["chat", "wait", "a", "b", "a", "b", "a"]).unwrap();
    assert!(
        matches!(&invocation.command, Command::ChatWait { sleeves, .. }
            if sleeves == &["a".to_owned(), "b".to_owned()]),
        "{invocation:?}"
    );

    // Flags may sit between sleeve names, like Go's persistent flags.
    let invocation = parse([
        "chat",
        "wait",
        "a",
        "--until",
        "report",
        "b",
        "--timeout",
        "30s",
    ])
    .unwrap();
    assert!(
        matches!(&invocation.command, Command::ChatWait { sleeves, until, timeout }
            if sleeves == &["a".to_owned(), "b".to_owned()]
                && *until == WaitUntil::Report
                && *timeout == Duration::from_secs(30)),
        "{invocation:?}"
    );
    let invocation = parse([
        "chat",
        "wait",
        "a",
        "b",
        "--until=report",
        "--timeout=1m30s",
    ])
    .unwrap();
    assert!(
        matches!(&invocation.command, Command::ChatWait { sleeves, until, timeout }
            if sleeves == &["a".to_owned(), "b".to_owned()]
                && *until == WaitUntil::Report
                && *timeout == Duration::from_secs(90)),
        "{invocation:?}"
    );

    for bad in &[
        vec!["chat", "wait"],
        vec!["chat", "wait", "--json"],
        vec!["chat", "wait", "a", "-b"],
        vec!["chat", "wait", "a", "b", "--until", "maybe"],
        vec!["chat", "wait", "a", "b", "--timeout", "soon"],
        vec!["chat", "wait", "a", "b", "--timeout", "30h"],
    ] {
        assert!(parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn chat_wait_transport_outlasts_the_longest_sleeve_deadline() {
    let invocation = parse(["chat", "wait", "a", "b", "--timeout", "5m"]).unwrap();
    assert_eq!(transport_timeout(&invocation), Duration::from_secs(305));
    let invocation = parse(["chat", "wait", "a", "b"]).unwrap();
    assert!(transport_timeout(&invocation) >= Duration::from_secs(24 * 3600));
}

struct FakeHub {
    url: String,
    calls: Arc<Mutex<Vec<Value>>>,
    peak: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeHub {
    async fn start() -> Self {
        let auth: Authorizer = Arc::new(|_, headers| {
            (headers
                .get("authorization")
                .is_some_and(|value| value == "Bearer owner-secret"))
            .then_some(AuthContext::default())
            .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
        });
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let inflight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
        let hub_calls = Arc::clone(&calls);
        let hub_inflight = Arc::clone(&inflight);
        let hub_peak = Arc::clone(&peak);
        server
            .register(WAIT, move |request| {
                let calls = Arc::clone(&hub_calls);
                let inflight = Arc::clone(&hub_inflight);
                let peak = Arc::clone(&hub_peak);
                async move {
                    let body = serde_json::to_value(&request.message).unwrap();
                    let sleeve = body["sleeve"].as_str().unwrap_or_default().to_owned();
                    calls.lock().unwrap().push(body);
                    let live = inflight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(live, Ordering::SeqCst);
                    let message = match sleeve.as_str() {
                        "busy" => {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            return Err(RpcError::new(Code::DeadlineExceeded, "wait timed out"));
                        }
                        "missing" => return Err(RpcError::new(Code::NotFound, "no such sleeve")),
                        "slow" => {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            json!({"sleeve": sleeve, "result": {
                                "status": "REPORT_STATUS_PARTIAL",
                                "summary": "still working",
                                "chatSeq": "12"
                            }})
                        }
                        "very-slow" => {
                            tokio::time::sleep(Duration::from_millis(1200)).await;
                            json!({"sleeve": sleeve})
                        }
                        "reported" => json!({"sleeve": sleeve, "result": {
                            "status": "REPORT_STATUS_COMPLETE",
                            "summary": "tests pass",
                            "chatSeq": "9"
                        }}),
                        _ => {
                            tokio::time::sleep(Duration::from_millis(150)).await;
                            json!({"sleeve": sleeve})
                        }
                    };
                    inflight.fetch_sub(1, Ordering::SeqCst);
                    let message =
                        decode_json(request.method.output(), message.to_string().as_bytes())?;
                    Ok(RpcResponse::from(message))
                }
            })
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task =
            tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
        Self {
            url,
            calls,
            peak,
            task,
        }
    }

    fn stop(self) {
        self.task.abort();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_sleeve_wait_runs_the_waits_concurrently_in_text_and_json() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let peak = Arc::clone(&hub.peak);
    let (text, machine, elapsed) = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10)).unwrap();
        let started = Instant::now();
        let text = execute(
            &client,
            &parse(["chat", "wait", "a", "reported", "b"]).unwrap(),
        )
        .unwrap();
        let machine = execute(
            &client,
            &parse(["--json", "chat", "wait", "slow", "reported"]).unwrap(),
        )
        .unwrap();
        (text, machine, started.elapsed())
    })
    .await
    .unwrap();
    hub.stop();

    // Text: one block per sleeve, idle sleeves as a bare status line.
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort();
    assert_eq!(
        lines,
        ["a\tidle", "b\tidle", "reported\tcomplete", "tests pass"],
        "{text}"
    );

    // JSON: one WaitResponse per sleeve, in completion order, not an array.
    let lines: Vec<Value> = machine
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "{machine}");
    let sleeves: Vec<&str> = lines
        .iter()
        .map(|line| line["sleeve"].as_str().unwrap())
        .collect();
    assert_eq!(sleeves, ["reported", "slow"], "{machine}");
    assert_eq!(lines[0]["result"]["status"], "REPORT_STATUS_COMPLETE");
    assert_eq!(lines[0]["result"]["chatSeq"], "9");
    assert_eq!(lines[1]["result"]["status"], "REPORT_STATUS_PARTIAL");
    assert_eq!(lines[1]["result"]["summary"], "still working");

    // Concurrency: the hub saw both waits in flight at once, and the whole
    // command finished in about the slowest single wait, not their sum.
    let peak = peak.load(Ordering::SeqCst);
    assert!(peak >= 2, "peak {peak}");
    assert!(elapsed < Duration::from_millis(550), "{elapsed:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_sleeve_gets_the_same_until_and_timeout_and_duplicates_wait_once() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let calls = Arc::clone(&hub.calls);
    let bodies = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10)).unwrap();
        execute(
            &client,
            &parse([
                "chat",
                "wait",
                "a",
                "b",
                "a",
                "--until",
                "report",
                "--timeout",
                "250ms",
            ])
            .unwrap(),
        )
        .unwrap();
        serde_json::to_value(&*calls.lock().unwrap()).unwrap()
    })
    .await
    .unwrap();
    hub.stop();

    let bodies = bodies.as_array().unwrap();
    assert_eq!(bodies.len(), 2, "{bodies:?}");
    let mut sleeves: Vec<&str> = bodies
        .iter()
        .map(|body| body["sleeve"].as_str().unwrap())
        .collect();
    sleeves.sort();
    assert_eq!(sleeves, ["a", "b"], "{bodies:?}");
    for body in bodies {
        assert_eq!(body["until"], "WAIT_UNTIL_REPORT", "{body}");
        assert_eq!(body["timeout"], "0.250s", "{body}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_sleeve_timing_out_fails_the_whole_wait_with_its_code() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let error = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10)).unwrap();
        execute(
            &client,
            &parse(["chat", "wait", "busy", "slow", "reported"]).unwrap(),
        )
        .unwrap_err()
    })
    .await
    .unwrap();
    hub.stop();
    assert_eq!(error.code, Code::DeadlineExceeded, "{error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_sleeve_fails_the_multi_wait_even_if_the_others_are_idle() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let error = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10)).unwrap();
        execute(&client, &parse(["chat", "wait", "a", "missing"]).unwrap()).unwrap_err()
    })
    .await
    .unwrap();
    hub.stop();
    assert_eq!(error.code, Code::NotFound, "{error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_error_does_not_wait_for_an_unrelated_sleeve() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let (error, elapsed) = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10)).unwrap();
        let started = Instant::now();
        let error = execute(
            &client,
            &parse(["chat", "wait", "missing", "very-slow"]).unwrap(),
        )
        .unwrap_err();
        (error, started.elapsed())
    })
    .await
    .unwrap();
    hub.stop();
    assert_eq!(error.code, Code::NotFound);
    assert!(elapsed < Duration::from_millis(800), "{elapsed:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn public_cli_prints_each_completed_wait_before_the_next_finishes() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let (first, rest, first_elapsed, success) = tokio::task::spawn_blocking(move || {
        let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_protectorate-cli"))
            .args(["--hub", &url, "chat", "wait", "reported", "very-slow"])
            .env("PROTECTORATE_TOKEN", "owner-secret")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let started = Instant::now();
        let mut first = String::new();
        stdout.read_line(&mut first).unwrap();
        let first_elapsed = started.elapsed();
        let mut rest = String::new();
        stdout.read_to_string(&mut rest).unwrap();
        let success = child.wait().unwrap().success();
        (first, rest, first_elapsed, success)
    })
    .await
    .unwrap();
    hub.stop();
    assert!(success);
    assert_eq!(first, "reported\tcomplete\n");
    assert!(
        first_elapsed < Duration::from_millis(800),
        "{first_elapsed:?}"
    );
    assert!(rest.contains("tests pass\n"), "{rest}");
    assert!(rest.contains("very-slow\tidle\n"), "{rest}");
}

#[tokio::test(flavor = "multi_thread")]
async fn single_sleeve_wait_keeps_its_one_shot_text_and_json_shape() {
    let hub = FakeHub::start().await;
    let url = hub.url.clone();
    let calls = Arc::clone(&hub.calls);
    let (text, machine) = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10)).unwrap();
        let text = execute(&client, &parse(["chat", "wait", "reported"]).unwrap()).unwrap();
        let machine = execute(&client, &parse(["--json", "chat", "wait", "a"]).unwrap()).unwrap();
        (text, machine)
    })
    .await
    .unwrap();
    hub.stop();
    assert_eq!(text, "reported\tcomplete\ntests pass\n");
    let response: Value = serde_json::from_str(machine.trim()).unwrap();
    assert_eq!(response["sleeve"], "a");
    assert!(response.get("result").is_none(), "{response}");
    assert_eq!(calls.lock().unwrap().len(), 2); // one Wait per execute
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .all(|body| body["sleeve"].is_string()));
}
