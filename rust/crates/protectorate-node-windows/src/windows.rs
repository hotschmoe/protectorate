#[path = "../../protectorate-server/src/node_doctor.rs"]
mod node_doctor;
#[path = "../../protectorate-server/src/driver.rs"]
mod driver;
mod node_install;
#[path = "../../protectorate-server/src/node_join.rs"]
mod node_join;
#[path = "../../protectorate-server/src/transport.rs"]
mod transport;

use std::{
    env,
    error::Error,
    fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const GUARD_CHILD: &str = "PROTECTORATE_NODE_GUARD_CHILD";

fn build_version() -> &'static str {
    option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn default_home() -> Result<PathBuf, String> {
    if let Some(home) = env::var_os("PROTECTORATE_HOME") {
        return Ok(PathBuf::from(home));
    }
    env::var_os("LOCALAPPDATA")
        .map(|local| PathBuf::from(local).join("Protectorate-Node"))
        .or_else(|| {
            env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".protectorate"))
        })
        .ok_or_else(|| "LOCALAPPDATA is unset; pass --home".into())
}

struct Serve {
    home: PathBuf,
    listen: Option<SocketAddr>,
    file_root: Option<PathBuf>,
    file_unconfined: bool,
    job_concurrency: usize,
    allow_plain_jobs: bool,
    watch_paths: Vec<PathBuf>,
    docker_host: Option<String>,
}

struct InstallRunner {
    home: PathBuf,
    job_root: PathBuf,
}

enum Command {
    Version,
    Join(node_join::Command),
    Install(node_install::Command),
    Uninstall(node_install::Uninstall),
    Doctor(node_doctor::Command),
    Driver(driver::Command),
    InstallRunner(InstallRunner),
    Serve(Serve),
}

fn usage() -> &'static str {
    "Usage:\n  protectorate-rs version --json\n  protectorate-rs driver run --harness claude|codex|grok|pi --cwd DIR [--session ID] PROMPT\n  protectorate-rs node join --hub URL --ca-cert FILE [--code CODE | --code-file FILE] [--name NAME] [--listen IP:PORT] [--home DIR]\n  protectorate-rs node install --hub URL --ca-cert FILE [--code CODE | --code-file FILE] [--name NAME] [--listen IP:PORT] [--home DIR] [--dry-run] [--elevated] [-- serve-flags]\n  protectorate-rs node uninstall [--home DIR] [--dry-run]\n  protectorate-rs node doctor [--home DIR] [--json]\n  protectorate-rs node install-runner --home DIR --job-root ABSOLUTE_DIR\n  protectorate-rs node serve [--home DIR] [--listen IP:PORT] [--file-root DIR] [--file-unconfined] [--job-concurrency 2] [--allow-plain-jobs] [--watch-dir DIR] [--docker-host URL]\n"
}

fn parse_install_runner(mut args: impl Iterator<Item = String>) -> Result<InstallRunner, String> {
    let mut home = None;
    let mut job_root = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?));
            }
            "--job-root" if job_root.is_none() => {
                job_root = Some(PathBuf::from(
                    args.next().ok_or("--job-root requires ABSOLUTE_DIR")?,
                ));
            }
            _ => {
                return Err(format!(
                    "unknown or duplicate node install-runner argument {flag:?}"
                ));
            }
        }
    }
    let home = home.ok_or("--home is required")?;
    let job_root = job_root.ok_or("--job-root is required")?;
    if !job_root.is_absolute() {
        return Err("--job-root must be an absolute directory".into());
    }
    Ok(InstallRunner { home, job_root })
}

fn parse_serve(mut args: impl Iterator<Item = String>) -> Result<Serve, String> {
    let mut home = None;
    let mut listen = None;
    let mut file_root = None;
    let mut file_unconfined = false;
    let mut job_concurrency = 2;
    let mut allow_plain_jobs = false;
    let mut watch_paths = Vec::new();
    let mut docker_host = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            "--listen" => {
                listen = Some(
                    args.next()
                        .ok_or("--listen requires IP:PORT")?
                        .parse()
                        .map_err(|_| "--listen requires IP:PORT")?,
                )
            }
            "--file-root" => {
                file_root = Some(PathBuf::from(
                    args.next().ok_or("--file-root requires DIR")?,
                ))
            }
            "--file-unconfined" => file_unconfined = true,
            "--job-concurrency" => {
                job_concurrency = args
                    .next()
                    .ok_or("--job-concurrency requires a number")?
                    .parse()
                    .map_err(|_| "--job-concurrency requires a number")?;
                if !(1..=32).contains(&job_concurrency) {
                    return Err("--job-concurrency must be between 1 and 32".into());
                }
            }
            "--allow-plain-jobs" => allow_plain_jobs = true,
            "--watch-dir" => watch_paths.push(PathBuf::from(
                args.next().ok_or("--watch-dir requires DIR")?,
            )),
            "--docker-host" => docker_host = Some(args.next().ok_or("--docker-host requires URL")?),
            "--sandbox-helper" => {
                let _ = args.next().ok_or("--sandbox-helper requires FILE")?;
                return Err("--sandbox-helper is unavailable on Windows".into());
            }
            "--help" | "-h" => return Err(usage().into()),
            _ => return Err(format!("unknown node serve argument {flag:?}")),
        }
    }
    Ok(Serve {
        home: home.map_or_else(default_home, Ok)?,
        listen,
        file_root,
        file_unconfined,
        job_concurrency,
        allow_plain_jobs,
        watch_paths,
        docker_host,
    })
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Command>, String> {
    match args.next().as_deref() {
        None | Some("--help" | "-h") => Ok(None),
        Some("version") if args.next().as_deref() == Some("--json") && args.next().is_none() => {
            Ok(Some(Command::Version))
        }
        Some("driver") => driver::parse(args).map(Command::Driver).map(Some),
        Some("node") => match args.next().as_deref() {
            Some("join") => node_join::parse(args).map(Command::Join).map(Some),
            Some("install") => node_install::parse(args).map(Command::Install).map(Some),
            Some("uninstall") => node_install::parse_uninstall(args)
                .map(Command::Uninstall)
                .map(Some),
            Some("doctor") => node_doctor::parse(args).map(Command::Doctor).map(Some),
            Some("install-runner") => parse_install_runner(args)
                .map(Command::InstallRunner)
                .map(Some),
            Some("serve") => parse_serve(args).map(Command::Serve).map(Some),
            _ => Err(usage().into()),
        },
        _ => Err(usage().into()),
    }
}

async fn serve_node(options: Serve) -> Result<bool, Box<dyn Error>> {
    let home = fs::canonicalize(&options.home)?;
    let (address, tls, renew) = node_join::serve_tls(&home, options.listen)?;
    let collector: Arc<dyn protectorate_node::MetricsCollector> = Arc::new(
        protectorate_node_metrics::Collector::windows(protectorate_node_metrics::Options {
            version: build_version().to_owned(),
            home: home.clone(),
            watch_paths: options.watch_paths,
            docker_host: options.docker_host,
            ..Default::default()
        }),
    );
    let file_root = options.file_root.unwrap_or_else(|| home.join("node-files"));
    let mut jobs = protectorate_node::Jobs::open_windows(&home, options.job_concurrency)?;
    if options.allow_plain_jobs {
        jobs = jobs.with_plain_mode();
    }
    let restart_requested = Arc::new(AtomicBool::new(false));
    let restart_notify = Arc::new(tokio::sync::Notify::new());
    let restart_flag = Arc::clone(&restart_requested);
    let restart_wake = Arc::clone(&restart_notify);
    let restart: protectorate_node::UpdateRestart = Arc::new(move || {
        restart_flag.store(true, Ordering::SeqCst);
        restart_wake.notify_one();
    });
    let agent = protectorate_node::Agent::new(collector)
        .with_certificate_renewal(renew)
        .with_files_durable(file_root, options.file_unconfined, &home)?
        .with_jobs(jobs)
        .with_update(env::current_exe()?, build_version(), restart)?;
    if agent.start_update_trial()? {
        return Ok(true);
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("protectorate-rs node serving on {}", listener.local_addr()?);
    let restart = Arc::clone(&restart_requested);
    let wake = Arc::clone(&restart_notify);
    axum::serve(
        transport::TlsListener::new(listener, tls)?,
        transport::peer_router(agent.router()?)
            .into_make_service_with_connect_info::<transport::Peer>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = async {
                while !restart.load(Ordering::SeqCst) {
                    wake.notified().await;
                }
            } => {},
        }
    })
    .await?;
    Ok(restart_requested.load(Ordering::SeqCst))
}

fn update_marker(exe: &Path) -> io::Result<(String, u64)> {
    let path = PathBuf::from(format!("{}.update", exe.display()));
    let body = fs::read(path)?;
    let marker: serde_json::Value = serde_json::from_slice(&body)?;
    Ok((
        marker["phase"].as_str().unwrap_or_default().to_owned(),
        marker["deadline_ms"].as_u64().unwrap_or_default(),
    ))
}

fn spawn_guard_child(exe: &Path, args: &[std::ffi::OsString]) -> io::Result<tokio::process::Child> {
    tokio::process::Command::new(exe)
        .args(args)
        .env(GUARD_CHILD, "1")
        .spawn()
}

fn guarded_binary(guardian: &Path) -> PathBuf {
    if guardian
        .file_name()
        .is_some_and(|name| name == "protectorate-rs-guardian.exe")
    {
        guardian.with_file_name("protectorate-rs.exe")
    } else {
        guardian.to_path_buf()
    }
}

async fn supervise_node_restart(exe: &Path) -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut child = spawn_guard_child(exe, &args)?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = child.kill().await;
                return Ok(());
            }
            status = child.wait() => {
                let status = status?;
                let phase = match update_marker(exe) {
                    Ok((phase, _)) => phase,
                    Err(error) if error.kind() == io::ErrorKind::NotFound && status.success() => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                if status.code() == Some(75) {
                    child = spawn_guard_child(exe, &args)?;
                    continue;
                }
                if phase == "trial" {
                    protectorate_node::recover_failed_trial(exe, &format!("new node binary exited {status} before confirmation"))?;
                    child = spawn_guard_child(exe, &args)?;
                    continue;
                }
                if status.success() {
                    return Ok(());
                }
                if phase == "confirmed" || phase == "rolledback" {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    child = spawn_guard_child(exe, &args)?;
                    continue;
                }
                return Err(format!("node child exited {status} with unknown update state").into());
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                let (phase, deadline_ms) = match update_marker(exe) {
                    Ok(marker) => marker,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
                if phase == "trial" && now_ms >= deadline_ms.saturating_add(15_000) {
                    let _ = child.kill().await;
                    protectorate_node::recover_failed_trial(exe, "new node binary did not confirm before deadline")?;
                    child = spawn_guard_child(exe, &args)?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match parse(env::args().skip(1)).map_err(io::Error::other)? {
        None => print!("{}", usage()),
        Some(Command::Version) => println!(
            "{}",
            serde_json::json!({
                "version": build_version(), "os": "windows",
                "arch": match std::env::consts::ARCH {
                    "x86_64" => "amd64",
                    "aarch64" => "arm64",
                    other => other,
                }
            })
        ),
        Some(Command::Join(command)) => node_join::run(command).await?,
        Some(Command::Install(command)) => node_install::run(command).await?,
        Some(Command::Uninstall(command)) => node_install::run_uninstall(command)?,
        Some(Command::Doctor(command)) => node_doctor::run(command).await?,
        Some(Command::Driver(command)) => driver::run(command).await?,
        Some(Command::InstallRunner(command)) => {
            protectorate_node::install_windows_runner(&command.home, &command.job_root)?;
            println!("Windows job runner configured. Restart the node service to use it.");
        }
        Some(Command::Serve(options)) => {
            if env::var_os(GUARD_CHILD).is_some() {
                if serve_node(options).await? {
                    std::process::exit(75);
                }
            } else {
                supervise_node_restart(&guarded_binary(&env::current_exe()?)).await?;
            }
        }
    }
    Ok(())
}
