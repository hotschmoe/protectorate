//! Prints this machine's doctor report as one JSON line. The hub and node binaries will run the
//! same checks as their `doctor` subcommand; this is the stand-alone probe.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "usage: pt-doctor-probe [--node] [--headscale PATH] [--member PATH] [--disk PATH]...
                        [--save FILE]
  --node       skip the harness checks (a node runs no harnesses)
  --save FILE  also write the report there by rename, as doctor.json";

fn main() -> ExitCode {
    let mut cfg = pt_doctor::Config::from_env();
    let mut save = None;
    let mut disks = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || {
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{a} needs a value"))
        };
        let r = match a.as_str() {
            "--node" => {
                cfg.harnesses = false;
                Ok(())
            }
            "--headscale" => value().map(|v| cfg.headscale = Some(v)),
            "--member" => value().map(|v| cfg.member = Some(v)),
            "--disk" => value().map(|v| disks.push(v)),
            "--save" => value().map(|v| save = Some(v)),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => Err(format!("unknown option '{other}'\n{USAGE}")),
        };
        if let Err(e) = r {
            eprintln!("pt-doctor-probe: {e}");
            return ExitCode::from(2);
        }
    }
    if !disks.is_empty() {
        cfg.disk_paths = disks;
    }
    let report = pt_doctor::run(&cfg);
    println!("{}", pt_json::ToJson::to_json(&report));
    if let Some(path) = save
        && let Err(e) = report.save(&path)
    {
        eprintln!("pt-doctor-probe: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
