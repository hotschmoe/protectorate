//! Read-only diagnostics for a Rust node installation.
use crate::node_join::Joined;
use std::{
    error::Error,
    io,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) struct Command {
    home: PathBuf,
    json: bool,
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut home = None;
    let mut json = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            flag if flag.starts_with("--home=") => {
                home = Some(PathBuf::from(&flag[7..]));
            }
            "--json" => json = true,
            _ => return Err(format!("unknown node doctor argument {flag:?}")),
        }
    }
    Ok(Command {
        home: home.map_or_else(crate::default_home, Ok)?,
        json,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Fail => "FAIL",
        }
    }
}

struct Check {
    name: &'static str,
    level: Level,
    detail: String,
}
impl Check {
    fn new(name: &'static str, level: Level, detail: impl Into<String>) -> Self {
        Self {
            name,
            level,
            detail: detail.into(),
        }
    }
}

#[cfg(unix)]
fn startup(enabled: &str, active: &str, linger: &str) -> Check {
    let enabled = enabled.trim();
    let active = active.trim();
    let linger = linger.trim();
    if enabled == "masked" {
        Check::new(
            "startup",
            Level::Fail,
            "protectorate-node.service is masked; unmask and reinstall the Rust node service",
        )
    } else if enabled != "enabled" {
        Check::new(
            "startup",
            Level::Fail,
            format!(
                "protectorate-node.service is not enabled ({enabled}); run `protectorate-rs node install --hub URL`"
            ),
        )
    } else if active == "failed" {
        Check::new(
            "startup",
            Level::Fail,
            "protectorate-node.service failed; inspect `systemctl --user status protectorate-node.service`",
        )
    } else if active != "active" {
        Check::new(
            "startup",
            Level::Warn,
            format!(
                "unit enabled but {active}; inspect `systemctl --user status protectorate-node.service`"
            ),
        )
    } else if linger != "Linger=yes" {
        Check::new(
            "startup",
            Level::Warn,
            "unit active but linger is off; enable linger to keep it running after logout",
        )
    } else {
        Check::new(
            "startup",
            Level::Ok,
            "systemd user unit enabled and active, linger on",
        )
    }
}

#[cfg(unix)]
async fn command_output(program: &str, args: &[&str]) -> String {
    let mut command = tokio::process::Command::new(program);
    command.args(args).kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(3), command.output()).await {
        Ok(Ok(output)) => String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        _ => String::new(),
    }
}

async fn startup_check() -> Check {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() }.to_string();
        let enabled_args = ["--user", "is-enabled", "protectorate-node.service"];
        let active_args = ["--user", "is-active", "protectorate-node.service"];
        let linger_args = ["show-user", uid.as_str(), "-p", "Linger"];
        let (enabled, active, linger) = tokio::join!(
            command_output("systemctl", &enabled_args),
            command_output("systemctl", &active_args),
            command_output("loginctl", &linger_args),
        );
        startup(&enabled, &active, &linger)
    }
    #[cfg(windows)]
    {
        let script = "$ErrorActionPreference='Stop'; $t=Get-ScheduledTask -TaskName 'Protectorate-Node' -ErrorAction Stop; @{state=[string]$t.State; action=[string]$t.Actions[0].Execute; arguments=[string]$t.Actions[0].Arguments} | ConvertTo-Json -Compress";
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .kill_on_drop(true)
                .output(),
        )
        .await;
        match output {
            Ok(Ok(output)) if output.status.success() => {
                match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                    Ok(task) => windows_startup(&task),
                    Err(error) => Check::new(
                        "startup",
                        Level::Fail,
                        format!("cannot parse Rust node scheduled task: {error}"),
                    ),
                }
            }
            Ok(Ok(output)) => Check::new(
                "startup",
                Level::Fail,
                format!(
                    "cannot inspect Protectorate-Node task (exit {}); run `protectorate-rs node install --hub URL`",
                    output.status
                ),
            ),
            Ok(Err(error)) => Check::new(
                "startup",
                Level::Fail,
                format!("cannot inspect Rust node scheduled task: {error}"),
            ),
            Err(_) => Check::new(
                "startup",
                Level::Fail,
                "timed out inspecting Rust node scheduled task",
            ),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        Check::new("startup", Level::Info, "startup check is unavailable")
    }
}

#[cfg(windows)]
fn windows_startup(task: &serde_json::Value) -> Check {
    let state = task["state"].as_str().unwrap_or("");
    let action = task["action"].as_str().unwrap_or("").to_ascii_lowercase();
    let arguments = task["arguments"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !action.ends_with("\\conhost.exe")
        || !arguments.contains("protectorate-rs-guardian.exe")
        || !arguments.contains(" node serve ")
    {
        return Check::new(
            "startup",
            Level::Fail,
            "Protectorate-Node task does not launch the Rust node",
        );
    }
    match state.to_ascii_lowercase().as_str() {
        "running" => Check::new("startup", Level::Ok, "Rust node logon task is running"),
        "disabled" => Check::new(
            "startup",
            Level::Fail,
            "Rust node logon task is disabled; enable it in Task Scheduler",
        ),
        "ready" => Check::new(
            "startup",
            Level::Warn,
            "Rust node logon task is ready but not running; inspect Task Scheduler",
        ),
        _ => Check::new(
            "startup",
            Level::Warn,
            format!("Rust node logon task state is {state:?}; inspect Task Scheduler"),
        ),
    }
}

fn enrollment(home: &Path) -> (Check, Option<Joined>) {
    let joined = match Joined::load(home) {
        Ok(joined) => joined,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (
                Check::new(
                    "enrollment",
                    Level::Warn,
                    "not joined to a hub; run `protectorate-rs node join` with an invite code",
                ),
                None,
            )
        }
        Err(error) => {
            return (
                Check::new(
                    "enrollment",
                    Level::Fail,
                    format!("join state is invalid: {error}"),
                ),
                None,
            )
        }
    };
    let check = match Joined::certificate(home) {
        Ok(_) => Check::new(
            "enrollment",
            Level::Ok,
            format!(
                "joined {} as {}; serves {}",
                joined.hub, joined.name, joined.address
            ),
        ),
        Err(error) => Check::new(
            "enrollment",
            Level::Fail,
            format!("node certificate is unusable: {error}"),
        ),
    };
    (check, Some(joined))
}

async fn hub_check(home: &Path, joined: Option<Joined>) -> Check {
    let Some(joined) = joined else {
        return Check::new("hub", Level::Info, "not checked: the node has not joined");
    };
    let ca = home.join("tls/ca.pem");
    let detail = tokio::task::spawn_blocking(move || {
        let pem = protectorate_tls::read_pem(&ca).map_err(|error| format!("hub CA: {error}"))?;
        let certs = ureq::tls::parse_pem(&pem)
            .filter_map(|item| match item {
                Ok(ureq::tls::PemItem::Certificate(cert)) => Some(cert),
                _ => None,
            })
            .collect::<Vec<_>>();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(5)))
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::new_with_certs(&certs))
                    .build(),
            )
            .build()
            .into();
        let url = format!("{}/ca.pem", joined.hub);
        agent
            .get(&url)
            .call()
            .map(|response| format!("{url} answered HTTP {}", response.status().as_u16()))
            .map_err(|error| format!("cannot reach {url}: {error}"))
    })
    .await;
    match detail {
        Ok(Ok(detail)) => Check::new("hub", Level::Ok, detail),
        Ok(Err(detail)) => Check::new("hub", Level::Fail, detail),
        Err(error) => Check::new("hub", Level::Fail, format!("hub check failed: {error}")),
    }
}

fn age(seconds: i64) -> String {
    match seconds.max(0) {
        s if s < 90 => format!("{s}s ago"),
        s if s < 5400 => format!("{}m ago", s / 60),
        s => format!("{}h ago", s / 3600),
    }
}

/// The recorded address, whether this machine still has it, the address its
/// route to the hub leaves from, and how the last report to the hub went.
async fn address_check(home: &Path, joined: Option<Joined>) -> Check {
    let Some(joined) = joined else {
        return Check::new(
            "address",
            Level::Info,
            "not checked: the node has not joined",
        );
    };
    let recorded = joined.address;
    let hub = joined.hub.clone();
    let (here, route) = tokio::task::spawn_blocking(move || {
        (
            crate::node_join::assigned(recorded.ip()),
            crate::node_join::route_ip(&hub),
        )
    })
    .await
    .unwrap_or_else(|error| (false, Err(io::Error::other(error))));
    let report = crate::node_join::Report::load(home);
    let last = match &report {
        Some(report) => format!(
            "last report of {} {} ({}): {}",
            report.address,
            if report.ok { "ok" } else { "FAILED" },
            age(protectorate_fsutil::unix_seconds() - report.time),
            report.message
        ),
        None => "no report to the hub yet (the running node sends one within a minute)".into(),
    };
    let route_text = match &route {
        Ok(ip) => format!("the route to {} leaves from {ip}", joined.hub),
        Err(error) => format!("no route to {} ({error})", joined.hub),
    };
    let base = format!(
        "recorded {recorded}, {}; {route_text}; {last}",
        if here {
            "assigned to this machine"
        } else {
            "NOT assigned to this machine"
        }
    );
    if !here {
        return match route {
            Ok(ip) => Check::new(
                "address",
                Level::Warn,
                format!("{base}. The machine's IP changed: a running node moves to {ip}:{} within a minute and tells the hub; restart the node to do it now", recorded.port()),
            ),
            Err(_) => Check::new(
                "address",
                Level::Fail,
                format!("{base}. The node cannot follow its machine's address without a route to the hub: restore the network, then restart the node"),
            ),
        };
    }
    if report.as_ref().is_some_and(|report| !report.ok) {
        return Check::new(
            "address",
            Level::Warn,
            format!("{base}. The hub accepts a report only when its source IP is the reported IP, over this node's own certificate: serve on the address the route to the hub leaves from (omit --listen), and check the hub is up"),
        );
    }
    Check::new("address", Level::Ok, base)
}

pub(crate) async fn run(options: Command) -> Result<(), Box<dyn Error>> {
    let mut checks = vec![Check::new(
        "platform",
        Level::Info,
        format!(
            "{}/{}, protectorate {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            crate::build_version()
        ),
    )];
    checks.push(startup_check().await);
    let (check, joined) = enrollment(&options.home);
    checks.push(check);
    checks.push(address_check(&options.home, joined.clone()).await);
    checks.push(hub_check(&options.home, joined).await);
    let failed = checks.iter().filter(|c| c.level == Level::Fail).count();
    if options.json {
        let rows: Vec<_> = checks
            .iter()
            .map(|c| serde_json::json!({"name":c.name,"level":c.level.label(),"detail":c.detail}))
            .collect();
        println!("{}", serde_json::json!({"checks":rows,"failed":failed}));
    } else {
        for check in &checks {
            println!(
                "{:<5} {:<14} {}",
                check.level.label(),
                check.name,
                check.detail
            );
        }
    }
    if failed > 0 {
        return Err(io::Error::other(format!("{failed} node doctor check(s) failed")).into());
    }
    Ok(())
}
