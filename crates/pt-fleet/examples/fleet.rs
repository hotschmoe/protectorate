//! A stand-in hub for checking real machines with pt-fleet before the hub binary embeds it.
//!
//! ```text
//! fleet serve --home DIR --listen IP:PORT --headscale PATH --member PATH [--hub-udp-port N]
//! fleet ctl --home DIR nodes | hs | status | pending | enrolled
//! fleet ctl --home DIR approve AUTH_ID NAME FINGERPRINT
//! fleet ctl --home DIR job NODE ARGV...      (NODE is a name or headscale id)
//! fleet ctl --home DIR snapshot LABEL
//! ```
//!
//! `serve` runs the fleet and the poller, logs each node's online changes to stderr, and answers
//! `ctl` on `DIR/ctl-<pid>.sock` (a new name per run, since nothing removes an old socket): one
//! JSON array of arguments per connection, one JSON answer back.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pt_fleet::{Config, Fleet, FleetEvent, PollConfig, Poller};
use pt_frame::msg::{OnPresent, SubmitJob};
use pt_json::{Map, ToJson, Value, obj};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("serve") => serve(&args[1..]),
        Some("ctl") => ctl(&args[1..]),
        _ => Err("usage: fleet serve ... | fleet ctl --home DIR COMMAND...".into()),
    };
    if let Err(e) = r {
        eprintln!("fleet: {e}");
        std::process::exit(1);
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn need(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("{name} is required"))
}

fn ts() -> String {
    format!("{:.3}", pt_api::EpochMs::now().0 as f64 / 1000.0)
}

fn serve(args: &[String]) -> Result<(), String> {
    let home = PathBuf::from(need(args, "--home")?);
    let listen: SocketAddr = need(args, "--listen")?
        .parse()
        .map_err(|e| format!("--listen: {e}"))?;
    let mut cfg = Config::lan_direct(
        home.clone(),
        PathBuf::from(need(args, "--headscale")?),
        PathBuf::from(need(args, "--member")?),
        listen,
    );
    if let Some(p) = flag(args, "--hub-udp-port") {
        cfg.hub_udp_port = p.parse().map_err(|e| format!("--hub-udp-port: {e}"))?;
    }
    let fleet = Fleet::start(cfg).map_err(|e| e.to_string())?;
    eprintln!(
        "{} fleet up: control {} fingerprint {}",
        ts(),
        fleet.config().server_url,
        fleet.control_fingerprint().map_err(|e| e.to_string())?
    );
    let last: Arc<Mutex<BTreeMap<String, bool>>> = Arc::default();
    let poller = fleet
        .poll(PollConfig::default(), move |e| match e {
            FleetEvent::Node(n) => {
                let mut l = last.lock().unwrap();
                if l.insert(n.id.clone(), n.online) != Some(n.online) {
                    eprintln!("{} node {} ({}) online={}", ts(), n.id, n.name, n.online);
                }
            }
            FleetEvent::Removed(id) => eprintln!("{} node {id} removed", ts()),
        })
        .map_err(|e| e.to_string())?;
    let sock = home.join(format!("ctl-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&sock).map_err(|e| format!("{}: {e}", sock.display()))?;
    for conn in listener.incoming() {
        let Ok(mut conn) = conn else { continue };
        let mut line = String::new();
        if BufReader::new(&conn).read_line(&mut line).is_err() {
            continue;
        }
        let answer = match pt_json::parse(line.trim()) {
            Ok(Value::Array(a)) => {
                let words: Vec<String> = a
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                command(&fleet, &poller, &words).unwrap_or_else(|e| obj! { "error" => e })
            }
            _ => obj! { "error" => "send a JSON array of strings" },
        };
        let _ = conn.write_all(format!("{answer}\n").as_bytes());
    }
    Ok(())
}

fn view_json(v: &pt_fleet::NodeView) -> Value {
    let mut m = Map::new();
    m.insert("node", v.node.to_json());
    m.insert("agent_node_id", v.hello.as_ref().map(|h| h.node_id.clone()));
    m.insert(
        "journal_epoch",
        v.hello.as_ref().map(|h| h.journal_epoch.clone()),
    );
    m.insert("ip", v.ip.clone());
    m.insert("error", v.error.clone());
    Value::Object(m)
}

fn command(fleet: &Fleet, poller: &Poller, w: &[String]) -> Result<Value, String> {
    let e = |e: pt_fleet::Error| e.to_string();
    Ok(match w.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["nodes"] => Value::Array(poller.nodes().iter().map(view_json).collect()),
        ["hs"] => Value::Array(
            fleet
                .admin()
                .nodes()
                .map_err(e)?
                .iter()
                .map(|n| obj! { "id" => n.id, "name" => n.name.as_str(), "hostname" => n.hostname.as_str(),
                    "ips" => Value::Array(n.ips.iter().map(|s| Value::from(s.as_str())).collect()),
                    "tags" => Value::Array(n.tags.iter().map(|s| Value::from(s.as_str())).collect()), "online" => n.online })
                .collect(),
        ),
        ["status"] => {
            let st = fleet.status().map_err(e)?;
            Value::Array(
                st.peers
                    .iter()
                    .map(|p| obj! { "name" => p.name.as_str(), "id" => p.stable_id.as_str(), "online" => p.online,
                        "cur_addr" => p.cur_addr.clone(), "relay" => p.relay.clone(), "last_handshake" => p.last_handshake.clone() })
                    .collect(),
            )
        }
        ["pending"] => Value::Array(fleet.pending().map_err(e)?.iter().map(ToJson::to_json).collect()),
        ["enrolled"] => Value::Array(fleet.enrolled().map_err(e)?.iter().map(ToJson::to_json).collect()),
        ["approve", auth, name, fp] => {
            let n = fleet.approve(auth, name, fp).map_err(e)?;
            obj! { "id" => n.id, "name" => n.name.as_str() }
        }
        ["snapshot", label] => {
            obj! { "dir" => fleet.snapshot_headscale(label).map_err(e)?.display().to_string() }
        }
        ["job", node, argv @ ..] if !argv.is_empty() => {
            let v = poller
                .nodes()
                .into_iter()
                .find(|v| v.node.id == *node || v.node.name == *node)
                .ok_or_else(|| format!("no node {node}"))?;
            let ip = v.ip.clone().ok_or("no address")?;
            let mut c = fleet.connect(&ip).map_err(e)?;
            let job_id = format!("fleet-{}", pt_api::EpochMs::now().0);
            c.submit_job(SubmitJob {
                job_id: job_id.clone(),
                attempt: 1,
                argv: Some(argv.iter().map(|s| s.to_string()).collect()),
                script: None,
                cwd: None,
                env: Default::default(),
                timeout_secs: 120,
                log_cap: 1 << 20,
                even_if_present: false,
                on_present: OnPresent::Lower,
            })
            .map_err(e)?;
            let deadline = Instant::now() + Duration::from_secs(150);
            let job = loop {
                let j = c.job_status(&job_id, Some(1)).map_err(e)?.job.ok_or("job vanished")?;
                if j.state.is_terminal() {
                    break j;
                }
                if Instant::now() > deadline {
                    return Err(format!("job {job_id} still {} after 150 s", j.state));
                }
                std::thread::sleep(Duration::from_millis(300));
            };
            let (log, end) = c.fetch_log(&job_id, 1, 0, 1 << 20).map_err(e)?;
            c.ack_collected(&job_id, 1).map_err(e)?;
            obj! { "node" => v.node.name.as_str(), "job" => job.to_json(), "log" => String::from_utf8_lossy(&log).into_owned(), "log_end" => end.to_json() }
        }
        _ => return Err(format!("unknown command {w:?}")),
    })
}

fn ctl(args: &[String]) -> Result<(), String> {
    let home = PathBuf::from(need(args, "--home")?);
    let words: Vec<Value> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| *a != "--home" && (*i == 0 || args[i - 1] != "--home"))
        .map(|(_, a)| Value::from(a.as_str()))
        .collect();
    let mut s = std::fs::read_dir(&home)
        .map_err(|e| format!("{}: {e}", home.display()))?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("ctl-"))
        .find_map(|e| UnixStream::connect(e.path()).ok())
        .ok_or_else(|| format!("no fleet serve answers in {}", home.display()))?;
    s.write_all(format!("{}\n", Value::Array(words)).as_bytes())
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    println!("{}", out.trim());
    Ok(())
}
