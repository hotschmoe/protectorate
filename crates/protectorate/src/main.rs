//! `protectorate`: the hub (`serve`), its CLI and the stdio MCP for vendor agents. Every CLI
//! command maps onto one hub API method over the unix socket (`$PROTECTORATE_SOCKET`, else
//! `<home>/run/api.sock`), with `$PROTECTORATE_TOKEN` as the bearer credential when set.

#![forbid(unsafe_code)]

mod args;
mod cli;
mod client;
mod fail;
mod mcp;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;

use pt_api::ErrorCode;

use crate::args::Args;
use crate::client::Client;
use crate::fail::Fail;

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
  hub probation [--retry-rollback]  run by the unit before each start, without the API
  doctor [--refresh]
  harness update [PROGRAM...]       claude, codex, grok, pi; none means every installed one
  harness login <claude|codex|grok|pi> [-- ARGS...]
                                    the sleeves' shared login, on this terminal, without the API
  sleeve list | sleeve show <sleeve>
  sleeve spawn <name> (--path DIR | --new-path DIR | --parent SLEEVE [--readonly]) [--harness H]
        [--provider P] [--model M] [--effort E] [--resleeve-to H,H] [--budget TOKENS] [--message TEXT]
  sleeve send <sleeve> <text...> [--steer]
        a sleeve may send to itself, its children, its parent, or envoy
  sleeve interrupt|stop|rm|takeover <sleeve>
  sleeve start <sleeve> [--harness H] [--provider P] [--model M] [--effort E]
  sleeve resleeve <sleeve> --harness H [--provider P] [--model M] [--effort E]
  sleeve transcript <sleeve> [--from SEQ] [--limit N]
  sleeve wait <sleeve> [--until idle|done|blocked|parked|ended|report]
        [--after-seq N] [--timeout SECS]
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
  history read --doc ID | history read <session> [--from TURN] [--to TURN] [--after DOC]
  device list | device add <name> | device pair <code> | device revoke <id>
  mcp --envoy [--token-file PATH] [--as NAME]

Harnesses: claude, codex, grok, pi_vendor, pi_local. --json prints the API's answer.

job run --wait exits with the job's code, or 3 interrupted, 5 lost, and 6 when the
hub stays unreachable for about 60 seconds.";

fn home(a: &Args) -> PathBuf {
    a.flag("--home")
        .map(PathBuf::from)
        .unwrap_or_else(pt_hub::default_home)
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
            println!("hub-rollback: 1");
            return Ok(());
        }
        "hub" if a.words.get(1).map(String::as_str) == Some("probation") => {
            let line = if a.has("--retry-rollback") {
                pt_hub::retry_probation_restore(&home(a))
            } else {
                pt_hub::probation_prestart(&home(a))
            }
            .map_err(Fail::Other)?;
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
