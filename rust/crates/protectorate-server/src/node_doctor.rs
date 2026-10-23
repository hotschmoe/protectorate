//! Read-only diagnostics for a Rust node installation.
use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) struct Command {
    home: PathBuf,
    hub: Option<String>,
    ca_cert: Option<PathBuf>,
    json: bool,
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut home = None;
    let mut hub = std::env::var("PROTECTORATE_HUB")
        .ok()
        .filter(|v| !v.is_empty());
    let mut ca_cert = None;
    let mut json = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            "--hub" => hub = Some(args.next().ok_or("--hub requires URL")?),
            "--ca-cert" => {
                ca_cert = Some(PathBuf::from(args.next().ok_or("--ca-cert requires FILE")?))
            }
            flag if flag.starts_with("--home=") => {
                home = Some(PathBuf::from(&flag[7..]));
            }
            flag if flag.starts_with("--hub=") => hub = Some(flag[6..].to_owned()),
            flag if flag.starts_with("--ca-cert=") => {
                ca_cert = Some(PathBuf::from(&flag[10..]));
            }
            "--json" => json = true,
            _ => return Err(format!("unknown node doctor argument {flag:?}")),
        }
    }
    Ok(Command {
        home: home.map_or_else(crate::default_home, Ok)?,
        hub,
        ca_cert,
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
                "protectorate-node.service is not enabled ({enabled}); run `protectorate-rs node install --control URL`"
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
        let script = "$ErrorActionPreference='Stop'; $t=Get-ScheduledTask -TaskName 'Protectorate-Rust-Node' -ErrorAction Stop; @{state=[string]$t.State; action=[string]$t.Actions[0].Execute; arguments=[string]$t.Actions[0].Arguments} | ConvertTo-Json -Compress";
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
                    "cannot inspect Protectorate-Rust-Node task (exit {}); run `protectorate-rs node install --control URL`",
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

#[cfg(any(windows, test))]
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
            "Protectorate-Rust-Node task does not launch the Rust node",
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

fn enrollment(home: &Path) -> Check {
    let dir = home.join("tsnet");
    let join = dir.join("join.json");
    let valid_file = |path: &Path, limit: u64| -> io::Result<bool> {
        let metadata = fs::symlink_metadata(path)?;
        Ok(metadata.file_type().is_file() && metadata.len() > 0 && metadata.len() <= limit)
    };
    if fs::symlink_metadata(&join).is_err_and(|error| error.kind() == io::ErrorKind::NotFound) {
        return Check::new(
            "enrollment",
            Level::Warn,
            "not enrolled in the Rust tailnet; run `protectorate-rs node join`",
        );
    }
    if !valid_file(&join, 64 * 1024).unwrap_or(false) {
        return Check::new(
            "enrollment",
            Level::Fail,
            "tailnet registration file is invalid",
        );
    }
    if !valid_file(&dir.join("machine.key"), 4096).unwrap_or(false)
        || !valid_file(&dir.join("node.key"), 4096).unwrap_or(false)
    {
        return Check::new(
            "enrollment",
            Level::Fail,
            "tailnet registration exists but a private identity key is missing or invalid",
        );
    }
    let Ok(bytes) = fs::read(&join) else {
        return Check::new(
            "enrollment",
            Level::Fail,
            "cannot read tailnet registration",
        );
    };
    let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Check::new(
            "enrollment",
            Level::Fail,
            "tailnet registration JSON is invalid",
        );
    };
    if ["controlUrl", "name", "nodeKey", "discoKey"]
        .iter()
        .any(|field| {
            state
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_none_or(str::is_empty)
        })
    {
        return Check::new(
            "enrollment",
            Level::Fail,
            "tailnet registration is missing identity fields",
        );
    }
    Check::new(
        "enrollment",
        Level::Ok,
        format!(
            "registered in {}; running connection not checked",
            dir.display()
        ),
    )
}

async fn hub_check(url: Option<String>, ca_cert: Option<PathBuf>) -> Check {
    let Some(url) = url else {
        return Check::new(
            "hub",
            Level::Info,
            "not checked; pass --hub URL to test reachability",
        );
    };
    let detail = tokio::task::spawn_blocking(move || {
        let mut config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(Duration::from_secs(5)));
        if let Some(path) = ca_cert {
            let bytes =
                fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
            if bytes.is_empty() || bytes.len() > 1024 * 1024 {
                return Err("hub CA bundle must be between 1 byte and 1 MiB".to_owned());
            }
            let mut certs = Vec::new();
            for item in ureq::tls::parse_pem(&bytes) {
                match item.map_err(|error| format!("invalid hub CA PEM: {error}"))? {
                    ureq::tls::PemItem::Certificate(cert) => certs.push(cert),
                    _ => return Err("hub CA bundle must contain certificates only".to_owned()),
                }
            }
            if certs.is_empty() {
                return Err("hub CA bundle has no certificates".to_owned());
            }
            config = config.tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::new_with_certs(&certs))
                    .build(),
            );
        }
        let config = config.build();
        let agent: ureq::Agent = config.into();
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

#[cfg(windows)]
fn desktop_checks(home: &Path) -> Vec<Check> {
    let controller = match protectorate_desk_node::Controller::open(home.to_path_buf()) {
        Ok(controller) => controller,
        Err(error) => {
            return vec![Check::new(
                "desk policy",
                Level::Fail,
                format!("cannot load local desktop policy: {error}"),
            )];
        }
    };
    let state = match controller.inspect_state() {
        Ok(state) => state,
        Err(error) => {
            return vec![Check::new(
                "desk policy",
                Level::Fail,
                format!("cannot inspect local desktop state: {error}"),
            )];
        }
    };
    desktop_checks_from_state(&state)
}

#[cfg(any(windows, test))]
fn desktop_checks_from_state(state: &serde_json::Value) -> Vec<Check> {
    let ready = state["sessionReady"] == true;
    let screen = state["screenState"].as_str().unwrap_or("unknown");
    vec![
        Check::new(
            "session",
            if ready { Level::Ok } else { Level::Fail },
            if ready {
                "interactive desktop session (desk commands can run)"
            } else {
                "not in an interactive desktop session; desk commands cannot run"
            },
        ),
        Check::new(
            "screen",
            if !ready {
                Level::Info
            } else if screen == "unlocked" {
                Level::Ok
            } else if screen == "locked" {
                Level::Fail
            } else {
                Level::Warn
            },
            if !ready {
                "not checked: this process is not in an interactive session"
            } else if screen == "locked" {
                "locked or showing the logon screen; desk input fails until it is unlocked"
            } else {
                screen
            },
        ),
        Check::new(
            "desk policy",
            Level::Info,
            state["detail"].as_str().unwrap_or("unavailable"),
        ),
    ]
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
    #[cfg(windows)]
    checks.extend(desktop_checks(&options.home));
    #[cfg(not(windows))]
    {
        checks.push(Check::new(
            "session",
            Level::Info,
            "desk commands are Windows only",
        ));
        checks.push(Check::new("screen", Level::Info, "not applicable"));
        checks.push(Check::new("desk policy", Level::Info, "not applicable"));
    }
    checks.push(startup_check().await);
    checks.push(enrollment(&options.home));
    checks.push(hub_check(options.hub, options.ca_cert).await);
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
        #[cfg(windows)]
        println!("\nUnattended use: desk input fails on a locked screen and on the logon screen. A machine agents will drive must stay logged in and unlocked. This command changes none of those settings.");
    }
    if failed > 0 {
        return Err(io::Error::other(format!("{failed} node doctor check(s) failed")).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_doctor_fails_for_locked_or_noninteractive_desktop() {
        let locked = desktop_checks_from_state(&serde_json::json!({
            "sessionReady": true,
            "screenState": "locked",
            "available": false,
            "detail": "screen locked"
        }));
        assert_eq!(locked[0].level, Level::Ok);
        assert_eq!(locked[1].level, Level::Fail);
        assert_eq!(locked[2].level, Level::Info);

        let not_interactive = desktop_checks_from_state(&serde_json::json!({
            "sessionReady": false,
            "screenState": "locked",
            "available": false,
            "detail": "no interactive desktop"
        }));
        assert_eq!(not_interactive[0].level, Level::Fail);
        assert_eq!(not_interactive[1].level, Level::Info);
        assert_eq!(not_interactive[2].level, Level::Info);
    }
    #[test]
    fn equals_options_forwarded_by_installed_cli_are_accepted() {
        let command = parse(
            [
                "--home=/tmp/node",
                "--hub=https://hub.test",
                "--ca-cert=/tmp/ca.pem",
                "--json",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(command.home, Path::new("/tmp/node"));
        assert_eq!(command.hub.as_deref(), Some("https://hub.test"));
        assert_eq!(command.ca_cert.as_deref(), Some(Path::new("/tmp/ca.pem")));
        assert!(command.json);
    }
    #[cfg(unix)]
    #[test]
    fn startup_distinguishes_masked_failed_and_linger() {
        assert_eq!(
            startup("masked", "inactive", "Linger=yes").level,
            Level::Fail
        );
        assert!(startup("masked", "inactive", "Linger=yes")
            .detail
            .contains("masked"));
        assert!(startup("enabled", "failed", "Linger=yes")
            .detail
            .contains("failed"));
        assert_eq!(startup("enabled", "active", "Linger=no").level, Level::Warn);
        assert_eq!(startup("enabled", "active", "Linger=yes").level, Level::Ok);
    }
    #[test]
    fn windows_task_status_checks_the_installed_rust_action() {
        let task = serde_json::json!({
            "state":"Running",
            "action":"C:\\Windows\\System32\\conhost.exe",
            "arguments":"--headless \"C:\\Program Files\\Protectorate\\protectorate-rs-guardian.exe\" node serve --home C:\\state",
        });
        assert_eq!(windows_startup(&task).level, Level::Ok);
        let mut stopped = task.clone();
        stopped["state"] = "Ready".into();
        assert_eq!(windows_startup(&stopped).level, Level::Warn);
        stopped["state"] = "Disabled".into();
        assert_eq!(windows_startup(&stopped).level, Level::Fail);
        stopped["action"] = "C:\\Windows\\System32\\cmd.exe".into();
        assert_eq!(windows_startup(&stopped).level, Level::Fail);
    }
    #[test]
    fn enrollment_is_read_only_and_requires_both_keys() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(enrollment(temp.path()).level, Level::Warn);
        assert!(!temp.path().join("tsnet").exists());
        fs::create_dir(temp.path().join("tsnet")).unwrap();
        fs::write(temp.path().join("tsnet/join.json"), b"{}").unwrap();
        assert_eq!(enrollment(temp.path()).level, Level::Fail);
        fs::write(temp.path().join("tsnet/machine.key"), b"key").unwrap();
        fs::write(temp.path().join("tsnet/node.key"), b"key").unwrap();
        assert_eq!(enrollment(temp.path()).level, Level::Fail);
        fs::write(temp.path().join("tsnet/join.json"), br#"{"controlUrl":"https://hub","name":"node","nodeKey":"nodekey:test","discoKey":"discokey:test"}"#).unwrap();
        assert_eq!(enrollment(temp.path()).level, Level::Ok);
    }

    #[tokio::test]
    async fn hub_probe_accepts_an_http_error_as_reachable() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 256];
            let _ = stream.read(&mut request).await.unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let check = hub_check(Some(url), None).await;
        assert_eq!(check.level, Level::Ok);
        assert!(check.detail.contains("HTTP 401"));
        server.await.unwrap();
    }
}
