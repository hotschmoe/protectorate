//! `protectorate`: the hub (`serve`) and its CLI. Every CLI command maps onto one hub API method
//! over the unix socket (`$PROTECTORATE_SOCKET`, else `<home>/run/api.sock`).

#![forbid(unsafe_code)]

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use pt_json::{Value, obj};

const USAGE: &str = "usage: protectorate [--home DIR] [--socket PATH] [--json] <command>

  serve [--listen ADDR|off] [--proxy-listen ADDR --trusted-proxy IP] [--web-dir DIR]
        [--log-level error|warn|info|debug]
  state
  sleeve list
  sleeve spawn <name> [--path DIR] [--parent SLEEVE] [--layout root|worktree|view]
        [--harness H] [--provider P] [--model M] [--resleeve-to H,H] [--budget TOKENS]
  sleeve send <sleeve> <text...> [--steer]
  sleeve stop|rm|start|interrupt <sleeve>
  sleeve resleeve <sleeve> [--harness H] [--provider P] [--model M]
  transcript <sleeve> [--after SEQ] [--raw]
  wait <sleeve> [--until idle|done|blocked|parked|ended] [--timeout SECS]
  envoy send <text...> [--as NAME]
  envoy wait [REQUEST_ID] [--timeout SECS]
  envoy read [--after SEQ]
  envoy token
  version";

struct Args {
    words: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// Flags that take no value; every other `--flag` takes the next word.
const SWITCHES: &[&str] = &["--json", "--steer", "--raw", "--help"];

impl Args {
    fn parse(raw: Vec<String>) -> Result<Args, String> {
        let mut words = Vec::new();
        let mut flags = Vec::new();
        let mut it = raw.into_iter();
        while let Some(a) = it.next() {
            if a == "--" {
                words.extend(it.by_ref());
                break;
            }
            if let Some(f) = a.strip_prefix("--") {
                if let Some((k, v)) = f.split_once('=') {
                    flags.push((format!("--{k}"), Some(v.to_string())));
                } else if SWITCHES.contains(&a.as_str()) {
                    flags.push((a, None));
                } else {
                    let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                    flags.push((a, Some(v)));
                }
            } else {
                words.push(a);
            }
        }
        Ok(Args { words, flags })
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_deref())
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }

    fn word(&self, i: usize, what: &str) -> Result<&str, String> {
        self.words
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {what}"))
    }

    fn rest(&self, from: usize, what: &str) -> Result<String, String> {
        let t = self.words.get(from..).unwrap_or(&[]).join(" ");
        if t.is_empty() {
            return Err(format!("missing {what}"));
        }
        Ok(t)
    }
}

fn home(a: &Args) -> PathBuf {
    a.flag("--home")
        .map(PathBuf::from)
        .unwrap_or_else(pt_hub::default_home)
}

struct Cli {
    socket: PathBuf,
    token: Option<String>,
    json: bool,
}

#[derive(Debug)]
enum Fail {
    Usage(String),
    Api(u16, String),
    Other(String),
}

impl From<String> for Fail {
    fn from(s: String) -> Fail {
        Fail::Usage(s)
    }
}

impl Cli {
    fn call(&self, method: &str, req: Value, wait: Duration) -> Result<Value, Fail> {
        let client =
            pt_http::Client::unix(&self.socket).read_timeout(Some(wait + Duration::from_secs(30)));
        let auth = self.token.as_ref().map(|t| format!("Bearer {t}"));
        let mut headers = vec![("Content-Type", "application/json")];
        if let Some(a) = &auth {
            headers.push(("Authorization", a.as_str()));
        }
        let body = req.to_string();
        let r = client
            .request("POST", &format!("/api/{method}"), &headers, body.as_bytes())
            .map_err(|e| Fail::Other(format!("{}: {e}", self.socket.display())))?;
        let v = pt_json::parse_bytes(&r.body)
            .map_err(|e| Fail::Other(format!("hub answered {} with bad JSON: {e}", r.status)))?;
        if r.status != 200 {
            let msg = format!(
                "{}: {}",
                v["error"]["code"].as_str().unwrap_or("error"),
                v["error"]["message"].as_str().unwrap_or("")
            );
            return Err(Fail::Api(r.status, msg));
        }
        Ok(v)
    }

    fn quick(&self, method: &str, req: Value) -> Result<Value, Fail> {
        self.call(method, req, Duration::from_secs(30))
    }

    /// Prints the raw answer under `--json`, else what `human` makes of it.
    fn show(&self, v: &Value, human: impl FnOnce(&Value)) {
        if self.json {
            println!("{v}");
        } else {
            human(v);
        }
    }
}

fn secs(a: &Args, default: u64) -> Result<u64, String> {
    match a.flag("--timeout") {
        None => Ok(default),
        Some(t) => t
            .parse()
            .map_err(|_| format!("--timeout {t:?} is not seconds")),
    }
}

fn sleeve_line(s: &Value) -> String {
    let inc = &s["incarnation"];
    format!(
        "{:<4} {:<16} {:<12} {:<12} {:<10} g{:<3} {}",
        s["id"].as_str().unwrap_or(""),
        s["name"].as_str().unwrap_or(""),
        s["lifecycle"].as_str().unwrap_or(""),
        inc["activity"]["state"].as_str().unwrap_or("-"),
        inc["harness"].as_str().unwrap_or("-"),
        inc["g"].as_u64().unwrap_or(0),
        s["path"].as_str().unwrap_or(""),
    )
}

fn entry_line(e: &Value) -> String {
    let seq = e["seq"].as_u64().unwrap_or(0);
    let text = e["text"].as_str().unwrap_or("");
    let who = match e["kind"].as_str().unwrap_or("") {
        "user" => e["from"].as_str().unwrap_or("user").to_string(),
        "assistant" => "assistant".to_string(),
        "tool" => {
            let status = e["status"].as_str().unwrap_or("");
            let name = e["tool"].as_str().unwrap_or("");
            return format!(
                "[{seq}] tool {name} {status}: {}",
                if text.is_empty() {
                    e["output"].as_str().unwrap_or("")
                } else {
                    text
                }
            );
        }
        k => k.to_string(),
    };
    format!("[{seq}] {who}: {text}")
}

fn target_req(a: &Args, mut req: pt_json::Map) -> Value {
    for (flag, key) in [
        ("--harness", "harness"),
        ("--provider", "provider"),
        ("--model", "model"),
    ] {
        if let Some(v) = a.flag(flag) {
            req.insert(key, v);
        }
    }
    Value::Object(req)
}

fn sleeve_cmd(cli: &Cli, a: &Args) -> Result<(), Fail> {
    match a.word(1, "a sleeve command")? {
        "list" => {
            let v = cli.quick("GetState", obj! {})?;
            cli.show(&v, |v| {
                for s in v["sleeves"].as_array().unwrap_or(&[]) {
                    println!("{}", sleeve_line(s));
                }
            });
        }
        "spawn" => {
            let mut req = pt_json::Map::new();
            req.insert("name", a.word(2, "a sleeve name")?);
            for (flag, key) in [
                ("--path", "path"),
                ("--parent", "parent"),
                ("--layout", "layout"),
            ] {
                if let Some(v) = a.flag(flag) {
                    let v = if key == "path" {
                        std::fs::canonicalize(v)
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|_| v.to_string())
                    } else {
                        v.to_string()
                    };
                    req.insert(key, v);
                }
            }
            if let Some(list) = a.flag("--resleeve-to") {
                let hs: Vec<Value> = list.split(',').map(Value::from).collect();
                req.insert("resleeve_to", hs);
            }
            if let Some(b) = a.flag("--budget") {
                let n: u64 = b
                    .parse()
                    .map_err(|_| format!("--budget {b:?} is not a number"))?;
                req.insert("budget", n);
            }
            let v = cli.quick("SpawnSleeve", target_req(a, req))?;
            cli.show(&v, |v| {
                let s = &v["sleeve"];
                println!(
                    "spawned {} (id {})",
                    s["name"].as_str().unwrap_or(""),
                    s["id"].as_str().unwrap_or("")
                );
            });
        }
        "send" => {
            let mode = if a.has("--steer") { "steer" } else { "queue" };
            let req = obj! {
                "sleeve" => a.word(2, "a sleeve")?,
                "text" => a.rest(3, "the message text")?,
                "mode" => mode,
            };
            let v = cli.quick("SendMessage", req)?;
            cli.show(&v, |v| {
                println!(
                    "{} {}",
                    v["result"].as_str().unwrap_or(""),
                    v["request_id"].as_str().unwrap_or("")
                );
            });
        }
        verb @ ("stop" | "rm" | "start" | "interrupt" | "resleeve") => {
            let method = match verb {
                "stop" => "StopSleeve",
                "rm" => "RemoveSleeve",
                "start" => "StartSleeve",
                "interrupt" => "Interrupt",
                _ => "Resleeve",
            };
            let mut req = pt_json::Map::new();
            req.insert("sleeve", a.word(2, "a sleeve")?);
            let v = cli.quick(method, target_req(a, req))?;
            cli.show(&v, |v| println!("{}", sleeve_line(&v["sleeve"])));
        }
        other => return Err(Fail::Usage(format!("unknown sleeve command {other:?}"))),
    }
    Ok(())
}

fn envoy_cmd(cli: &Cli, a: &Args) -> Result<(), Fail> {
    match a.word(1, "an envoy command")? {
        "send" => {
            let mut req = pt_json::Map::new();
            req.insert("text", a.rest(2, "the message text")?);
            if let Some(name) = a.flag("--as") {
                req.insert("agent", name);
            }
            let v = cli.quick("EnvoySend", Value::Object(req))?;
            cli.show(&v, |v| {
                println!("{}", v["request_id"].as_str().unwrap_or(""))
            });
        }
        "wait" => {
            let t = secs(a, 600)?;
            let mut req = pt_json::Map::new();
            req.insert("timeout_ms", t * 1000);
            if let Some(r) = a.words.get(2) {
                req.insert("request_id", r.as_str());
            }
            let v = cli.call("EnvoyWait", Value::Object(req), Duration::from_secs(t))?;
            cli.show(&v, |v| {
                if v["settled"].as_bool() == Some(true) {
                    println!("{}", v["text"].as_str().unwrap_or(""));
                } else {
                    eprintln!("not settled: {}", v["state"].as_str().unwrap_or(""));
                }
            });
            if v["settled"].as_bool() != Some(true) {
                return Err(Fail::Other(format!(
                    "envoy wait: {}",
                    v["state"].as_str().unwrap_or("")
                )));
            }
        }
        "read" => {
            let after = a
                .flag("--after")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let v = cli.quick("EnvoyRead", obj! { "after_seq" => after })?;
            cli.show(&v, |v| {
                for e in v["entries"].as_array().unwrap_or(&[]) {
                    println!("{}", entry_line(e));
                }
            });
        }
        "token" => {
            let v = cli.quick("EnvoyToken", obj! {})?;
            cli.show(&v, |v| println!("{}", v["token"].as_str().unwrap_or("")));
        }
        other => return Err(Fail::Usage(format!("unknown envoy command {other:?}"))),
    }
    Ok(())
}

fn serve(a: &Args) -> Result<(), Fail> {
    if let Some(l) = a.flag("--log-level") {
        pt_hub::set_log_level(l)?;
    }
    let addr = |flag: &str| -> Result<Option<SocketAddr>, String> {
        match a.flag(flag) {
            None | Some("off") => Ok(None),
            Some(s) => s
                .parse()
                .map(Some)
                .map_err(|_| format!("{flag} {s:?} is not an address")),
        }
    };
    let listen = match a.flag("--listen") {
        None => Some(SocketAddr::from(([127, 0, 0, 1], 7470))),
        Some(_) => addr("--listen")?,
    };
    let trusted: Option<IpAddr> = match a.flag("--trusted-proxy") {
        None => None,
        Some(s) => Some(
            s.parse()
                .map_err(|_| format!("--trusted-proxy {s:?} is not an IP"))?,
        ),
    };
    pt_hub::serve(pt_hub::ServeOptions {
        home: home(a),
        listen,
        proxy_listen: addr("--proxy-listen")?,
        trusted_proxy: trusted,
        web_dir: a.flag("--web-dir").map(PathBuf::from),
    })
    .map_err(Fail::Other)
}

fn run(a: &Args) -> Result<(), Fail> {
    let cmd = a.word(0, "a command")?;
    if cmd == "serve" {
        return serve(a);
    }
    if cmd == "version" {
        println!(
            "protectorate {}",
            option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or("dev")
        );
        return Ok(());
    }
    let socket = a
        .flag("--socket")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PROTECTORATE_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| home(a).join("run/api.sock"));
    let cli = Cli {
        socket,
        token: std::env::var("PROTECTORATE_TOKEN")
            .ok()
            .filter(|t| !t.is_empty()),
        json: a.has("--json"),
    };
    match cmd {
        "state" => {
            let v = cli.quick("GetState", obj! {})?;
            println!("{v}");
        }
        "sleeve" => sleeve_cmd(&cli, a)?,
        "envoy" => envoy_cmd(&cli, a)?,
        "transcript" => {
            let after = a
                .flag("--after")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let req = obj! {
                "sleeve" => a.word(1, "a sleeve")?,
                "after_seq" => after,
                "raw" => a.has("--raw"),
            };
            let v = cli.quick("GetTranscript", req)?;
            cli.show(&v, |v| {
                for e in v["entries"].as_array().unwrap_or(&[]) {
                    println!("{}", entry_line(e));
                }
            });
        }
        "wait" => {
            let t = secs(a, 600)?;
            let req = obj! {
                "sleeve" => a.word(1, "a sleeve")?,
                "until" => a.flag("--until").unwrap_or("idle,done,blocked"),
                "timeout_ms" => t * 1000,
            };
            let v = cli.call("Wait", req, Duration::from_secs(t))?;
            cli.show(&v, |v| println!("{}", v["state"].as_str().unwrap_or("")));
            if v["reached"].as_bool() != Some(true) {
                return Err(Fail::Other(format!(
                    "wait: {}",
                    v["state"].as_str().unwrap_or("")
                )));
            }
        }
        other => return Err(Fail::Usage(format!("unknown command {other:?}"))),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = match Args::parse(std::env::args().skip(1).collect()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.has("--help") || args.words.is_empty() {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fail::Usage(e)) => {
            eprintln!("{e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Fail::Api(status, e)) => {
            eprintln!("error ({status}): {e}");
            ExitCode::from(if status == 403 || status == 401 { 4 } else { 1 })
        }
        Err(Fail::Other(e)) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
