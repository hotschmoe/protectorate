//! `protectorate`: the hub (`serve`), its CLI and the stdio MCP for vendor agents. Every CLI
//! command maps onto one hub API method over the unix socket (`$PROTECTORATE_SOCKET`, else
//! `<home>/run/api.sock`), with `$PROTECTORATE_TOKEN` as the bearer credential when set.

#![forbid(unsafe_code)]

mod cli;
mod mcp;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use pt_api::{ApiError, ErrorBody, ErrorCode, Method};
use pt_json::{FromJson, ToJson, Value};

mod web {
    include!(concat!(env!("OUT_DIR"), "/web.rs"));
}

mod net {
    include!(concat!(env!("OUT_DIR"), "/net.rs"));
}

/// What `version` and `serve` say about the embedded headscale and member.
fn net_line() -> String {
    let n = &net::NET;
    let one = |e: &Option<pt_hub::Embedded>, name: &str| match e {
        Some(e) => format!(
            "{name} sha256:{} ({:.1} MB gzipped)",
            &e.sha256[..16],
            e.gz.len() as f64 / 1e6
        ),
        None => format!("{name} missing"),
    };
    let mut s = format!(
        "net: {}, {}",
        one(&n.headscale, "headscale"),
        one(&n.member, "member")
    );
    if !n.missing.is_empty() {
        s.push_str(&format!(" ({})", n.missing));
    }
    s
}

const USAGE: &str = "usage: protectorate [--home DIR] [--socket PATH] [--json] <noun> <verb> ...

  serve [--listen ADDR|off] [--proxy-listen ADDR --trusted-proxy IP] [--web-dir DIR]
        [--fleet-listen ADDR|off] [--fleet-dir DIR] [--fleet-udp-port N] [--fleet-server-url URL]
        [--log-level error|warn|info|debug]
  version | --version
  hub info | hub state
  hub update [--check] [--channel stable|dev] [--version TAG] | hub update --rollback
  hub probation                     run by the unit before each start, without the API
  doctor [--refresh]
  harness update [PROGRAM...]       claude, codex, grok, pi; none means every installed one
  harness login <claude|codex|grok|pi> [-- ARGS...]
                                    the sleeves' shared login, on this terminal, without the API
  sleeve list | sleeve show <sleeve>
  sleeve spawn <name> (--path DIR | --parent SLEEVE [--readonly]) [--harness H]
        [--provider P] [--model M] [--effort E] [--resleeve-to H,H] [--budget TOKENS] [--message TEXT]
  sleeve send <sleeve> <text...> [--steer]
  sleeve interrupt|stop|rm|takeover <sleeve>
  sleeve start <sleeve> [--harness H] [--provider P] [--model M] [--effort E]
  sleeve resleeve <sleeve> --harness H [--provider P] [--model M] [--effort E]
  sleeve transcript <sleeve> [--from SEQ] [--limit N]
  sleeve wait <sleeve> [--until idle|done|blocked|parked|ended] [--timeout SECS]
  approval list [--pending] | approval decide <id> allow|allow-session|deny
  node ls | node show <node> | node add [REGISTRATION] | node approve <auth_id>
        [--name NAME] [--fingerprint FP]
  node rename <node> <new-name> | node rm <node> [--force]
  node update (<node>... | --all) [--version TAG] [--channel stable|dev] [--probation SECS]
        [--no-wait]
  job run (--node ID | --arch ARCH) [--even-if-present] [--on-present idle|kill]
        [--maintenance] [--wait] [--timeout SECS] [--cwd DIR] [--artifact PATH]... -- <argv...>
  job ls [--node N] [--limit N] | job show|cancel <job> | job log <job> [--offset N] [--limit N]
  job artifact <job> <path> [--out FILE]
  quota list
  event list [--from SEQ] [--limit N] | event read <seq>
  envoy send <text...> [--as NAME] | envoy wait [REQUEST_ID] [--timeout SECS]
  envoy read [--from SEQ] | envoy token
  history search <query...> [--mode all|any|phrase] [--sleeve S]... [--harness H]
        [--session ID] [--role user|assistant|tool-call|tool-error|compaction]...
        [--since 7d|MS] [--until 7d|MS] [--limit N]
  history read --doc ID | history read <session> [--from TURN] [--to TURN]
  device list | device add <name> | device pair <code> | device revoke <id>
  mcp --envoy [--token-file PATH] [--as NAME]

Harnesses: claude, codex, grok, pi_vendor, pi_local. --json prints the API's answer.

job run --wait exits with the job's code, or 3 interrupted, 5 lost, and 6 when the
hub stays unreachable for about 60 seconds.";

pub struct Args {
    pub words: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// Flags that take no value; every other `--flag` takes the next word.
const SWITCHES: &[&str] = &[
    "--json",
    "--steer",
    "--help",
    "--pending",
    "--readonly",
    "--even-if-present",
    "--maintenance",
    "--envoy",
    "--check",
    "--rollback",
    "--refresh",
    "--all",
    "--no-wait",
    "--wait",
    "--force",
];

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

    pub fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_deref())
    }

    pub fn all(&self, name: &str) -> Vec<String> {
        self.flags
            .iter()
            .filter(|(k, _)| k == name)
            .filter_map(|(_, v)| v.clone())
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }

    pub fn word(&self, i: usize, what: &str) -> Result<&str, String> {
        self.words
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {what}"))
    }

    pub fn rest(&self, from: usize, what: &str) -> Result<String, String> {
        let t = self.words.get(from..).unwrap_or(&[]).join(" ");
        if t.is_empty() {
            return Err(format!("missing {what}"));
        }
        Ok(t)
    }

    pub fn num(&self, name: &str) -> Result<Option<u64>, String> {
        self.flag(name)
            .map(|v| {
                v.parse()
                    .map_err(|_| format!("{name} {v:?} is not a number"))
            })
            .transpose()
    }
}

pub fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn home(a: &Args) -> PathBuf {
    a.flag("--home")
        .map(PathBuf::from)
        .unwrap_or_else(pt_hub::default_home)
}

#[derive(Debug)]
pub enum Fail {
    Usage(String),
    Api(ApiError),
    Other(String),
    Exit(u8),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(e) | Self::Other(e) => f.write_str(e),
            Self::Api(e) => write!(f, "{e}"),
            Self::Exit(code) => write!(f, "exit {code}"),
        }
    }
}

impl From<String> for Fail {
    fn from(s: String) -> Fail {
        Fail::Usage(s)
    }
}

/// The hub over its unix socket.
pub struct Client {
    pub socket: PathBuf,
    pub token: Option<String>,
}

impl Client {
    fn raw(&self, method: &str, body: &Value, wait: Duration) -> Result<Value, Fail> {
        let client =
            pt_http::Client::unix(&self.socket).read_timeout(Some(wait + Duration::from_secs(30)));
        let auth = self.token.as_ref().map(|t| format!("Bearer {t}"));
        let mut headers = vec![("Content-Type", "application/json")];
        if let Some(a) = &auth {
            headers.push(("Authorization", a.as_str()));
        }
        let path = format!("{}{method}", pt_api::API_PREFIX);
        let r = client
            .request("POST", &path, &headers, body.to_string().as_bytes())
            .map_err(|e| Fail::Other(format!("{}: {e}", self.socket.display())))?;
        let v = pt_json::parse_bytes(&r.body).map_err(|e| {
            Fail::Other(format!(
                "{method}: hub answered {} with bad JSON: {e}",
                r.status
            ))
        })?;
        if r.status != 200 {
            return Err(match ErrorBody::from_json(&v) {
                Ok(b) => Fail::Api(b.error),
                Err(_) => Fail::Other(format!("{method}: hub answered {}: {v}", r.status)),
            });
        }
        Ok(v)
    }

    /// Calls `M`, allowing the hub `wait` before the answer (Wait, EnvoyWait, D9 approvals).
    pub fn call_for<M: Method>(
        &self,
        req: &M::Request,
        wait: Duration,
    ) -> Result<M::Response, Fail> {
        let v = self.raw(M::NAME, &req.to_json(), wait)?;
        M::Response::from_json(&v)
            .map_err(|e| Fail::Other(format!("{}: the hub's answer does not parse: {e}", M::NAME)))
    }

    pub fn call<M: Method>(&self, req: &M::Request) -> Result<M::Response, Fail> {
        self.call_for::<M>(req, Duration::from_secs(30))
    }
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
    let web = match a.flag("--web-dir") {
        Some(d) => pt_hub::Web::Dir(PathBuf::from(d)),
        None if web::FILES.is_empty() => pt_hub::Web::None,
        None => pt_hub::Web::Embedded(web::FILES),
    };
    let fleet = pt_hub::FleetFlags {
        dir: a.flag("--fleet-dir").map(PathBuf::from),
        listen: a.flag("--fleet-listen").map(String::from),
        udp_port: a
            .flag("--fleet-udp-port")
            .map(|p| {
                p.parse()
                    .map_err(|_| format!("--fleet-udp-port {p:?} is not a port"))
            })
            .transpose()?,
        server_url: a.flag("--fleet-server-url").map(String::from),
    };
    eprintln!("{}", net_line());
    let exit = pt_hub::serve(pt_hub::ServeOptions {
        home: home(a),
        listen,
        proxy_listen: addr("--proxy-listen")?,
        trusted_proxy: trusted,
        web,
        fleet,
        net: net::NET,
    })
    .map_err(Fail::Other)?;
    match exit {
        pt_hub::Exit::Stopped => Ok(()),
        pt_hub::Exit::Restart => std::process::exit(i32::from(pt_hub::RESTART_CODE)),
    }
}

fn run(a: &Args) -> Result<(), Fail> {
    let noun = a.word(0, "a command")?;
    match noun {
        "serve" => return serve(a),
        "version" => {
            println!(
                "protectorate {}",
                option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or("dev")
            );
            println!("{}", net_line());
            return Ok(());
        }
        "hub" if a.words.get(1).map(String::as_str) == Some("probation") => {
            let line = pt_hub::probation_prestart(&home(a)).map_err(Fail::Other)?;
            println!("protectorate hub probation: {line}");
            return Ok(());
        }
        "harness" if a.words.get(1).map(String::as_str) == Some("login") => {
            let harness = a.word(2, "a harness (claude, codex, grok or pi)")?;
            let args = a.words.get(3..).unwrap_or(&[]);
            let line = pt_hub::harness_login(&home(a), harness, args).map_err(Fail::Other)?;
            println!("{line}");
            return Ok(());
        }
        _ => {}
    }
    let socket = a
        .flag("--socket")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PROTECTORATE_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| home(a).join("run/api.sock"));
    let env_token = std::env::var("PROTECTORATE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    if noun == "mcp" {
        if !a.has("--envoy") {
            return Err(Fail::Usage("mcp serves only --envoy".into()));
        }
        let token = match a.flag("--token-file") {
            Some(f) => std::fs::read_to_string(f)
                .map_err(|e| Fail::Other(format!("--token-file {f}: {e}")))?
                .trim()
                .to_string(),
            None => env_token.ok_or_else(|| {
                Fail::Usage("mcp --envoy needs --token-file or PROTECTORATE_TOKEN".into())
            })?,
        };
        let client = Client {
            socket,
            token: Some(token),
        };
        return mcp::serve(&client, a.flag("--as")).map_err(Fail::Other);
    }
    let client = Client {
        socket,
        token: env_token,
    };
    cli::run(&client, a, a.has("--json"))
}

fn main() -> ExitCode {
    let mut raw: Vec<String> = std::env::args().skip(1).collect();
    // `--version` right after the global flags is the version command; after a command it is
    // the flag that takes a tag (`hub update --version TAG`).
    let mut first = 0;
    while let Some(a) = raw.get(first) {
        match a.as_str() {
            "--home" | "--socket" => first += 2,
            a if a == "--json" || a.starts_with("--home=") || a.starts_with("--socket=") => {
                first += 1
            }
            _ => break,
        }
    }
    if raw.get(first).is_some_and(|a| a == "--version") {
        raw[first] = "version".into();
    }
    let args = match Args::parse(raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.words.first().is_none_or(|w| w != "serve") {
        let _ = pt_sys::default_sigpipe();
    }
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
        Err(Fail::Api(e)) => {
            eprintln!("error ({}): {e}", e.code.http_status());
            let denied = matches!(e.code, ErrorCode::Unauthenticated | ErrorCode::Forbidden);
            ExitCode::from(if denied { 4 } else { 1 })
        }
        Err(Fail::Other(e)) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
        Err(Fail::Exit(code)) => ExitCode::from(code),
    }
}
