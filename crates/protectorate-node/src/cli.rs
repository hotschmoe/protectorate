use crate::flags::number;
use pt_json::ToJson;
use pt_node::presence::Config;
use pt_node::{Detector, Sampler, inventory};
use std::io::Write;
use std::time::Duration;

fn usage() -> String {
    format!(
        "usage: protectorate-node <command>
{}
{}
{}
{}
{}
{}
  inventory                         print the machine inventory as JSON
  sample                            print one sample as JSON
  presence [--away-after SECS]      print presence as JSON
  watch-presence [--away-after SECS] [--interval SECS] [--limit N]
                                    print a JSON line each time presence changes
  lower-idle PID                    drop a running process tree to idle priority
  idle-probe                        Windows: print this session's idle time as one JSON line
                                    (the service starts it inside the console session)
  --version                         print the version, whether member is embedded, and the
                                    supervisor contract it keeps",
        crate::join::USAGE,
        crate::serve::USAGE,
        crate::supervise::USAGE,
        crate::service::USAGE,
        crate::rpc::USAGE,
        crate::update::USAGE
    )
}

pub fn run(args: Vec<String>) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("protectorate-node {}", pt_version::VERSION);
            println!("{}", crate::member::embedded_line());
            println!("{}", crate::update::SUPERVISED);
            Ok(())
        }
        Some("inventory") => {
            no_args(&args[1..]).map(|()| println!("{}", inventory::collect().to_json()))
        }
        Some("sample") => {
            no_args(&args[1..]).map(|()| println!("{}", Sampler::new().sample().to_json()))
        }
        Some("presence") => Opts::parse(&args[1..]).map(|o| {
            println!("{}", Detector::new(o.config()).detect().to_json());
        }),
        Some("watch-presence") => Opts::parse(&args[1..]).and_then(watch),
        Some("lower-idle") => lower_idle(&args[1..]),
        Some("idle-probe") => no_args(&args[1..])
            .and_then(|()| pt_node::presence::idle_probe())
            .map(|line| println!("{line}")),
        Some("join") => crate::join::run(&args[1..]),
        Some("serve") => crate::serve::run(&args[1..]),
        Some("supervise") => crate::supervise::run(&args[1..]),
        Some("install-service") => crate::service::install(&args[1..]),
        Some("uninstall") => crate::service::uninstall(&args[1..]),
        Some("probation") => crate::update::probation(&args[1..]),
        Some("rpc") => return crate::rpc::run(&args[1..]),
        Some("-h") | Some("--help") | Some("help") => {
            println!("{}", usage());
            Ok(())
        }
        Some(other) => Err(format!("unknown command '{other}'\n{}", usage())),
        None => Err(usage()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("protectorate-node: {e}");
            2
        }
    }
}

fn no_args(rest: &[String]) -> Result<(), String> {
    match rest.first() {
        None => Ok(()),
        Some(a) => Err(format!("unexpected argument '{a}'")),
    }
}

struct Opts {
    away_after: u64,
    interval: u64,
    limit: Option<u64>,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut o = Opts {
            away_after: Config::default().away_after_secs,
            interval: 2,
            limit: None,
        };
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let slot = match flag.as_str() {
                "--away-after" => &mut o.away_after,
                "--interval" => &mut o.interval,
                "--limit" => {
                    let v = number(flag, it.next())?;
                    o.limit = Some(v);
                    continue;
                }
                other => return Err(format!("unknown option '{other}'")),
            };
            *slot = number(flag, it.next())?;
        }
        if o.interval == 0 {
            return Err("--interval must be at least 1".into());
        }
        Ok(o)
    }

    fn config(&self) -> Config {
        Config {
            away_after_secs: self.away_after,
            ..Config::default()
        }
    }
}

fn watch(o: Opts) -> Result<(), String> {
    let mut det = Detector::new(o.config());
    let mut last = None;
    let mut printed = 0u64;
    loop {
        let p = det.detect();
        if last != Some(p.state) {
            last = Some(p.state);
            let mut out = std::io::stdout().lock();
            writeln!(out, "{}", p.to_json())
                .and_then(|()| out.flush())
                .map_err(|e| format!("writing stdout: {e}"))?;
            printed += 1;
            if o.limit.is_some_and(|l| printed >= l) {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_secs(o.interval));
    }
}

fn lower_idle(rest: &[String]) -> Result<(), String> {
    let pid = rest.first().ok_or("lower-idle needs a PID")?;
    let pid: u32 = pid.parse().map_err(|_| format!("'{pid}' is not a PID"))?;
    pt_node::lower_to_idle(pid).map_err(|e| e.to_string())
}
