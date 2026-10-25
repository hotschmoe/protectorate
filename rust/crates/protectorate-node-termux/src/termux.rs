#[path = "../../protectorate-server/src/node_join.rs"]
mod node_join;
#[path = "../../protectorate-server/src/transport.rs"]
mod transport;

use std::{
    env,
    error::Error,
    fs, io,
    net::SocketAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const GUARD_CHILD: &str = "PROTECTORATE_NODE_GUARD_CHILD";

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

enum Command {
    Version,
    Join(node_join::Command),
    Serve(Serve),
    Install(Install),
    Doctor(Doctor),
    Uninstall(Uninstall),
}

struct Install {
    paths: lifecycle::Paths,
    hub: String,
    name: Option<String>,
    code: Option<String>,
    code_file: Option<PathBuf>,
    ca_cert: PathBuf,
    listen: Option<SocketAddr>,
    allow_plain_jobs: bool,
    dry_run: bool,
    no_start: bool,
    serve_args: Vec<String>,
}
struct Doctor {
    paths: lifecycle::Paths,
    json: bool,
}
struct Uninstall {
    paths: lifecycle::Paths,
    dry_run: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ServeExit {
    Stopped,
    Restart,
    Uninstalled,
}

fn build_version() -> &'static str {
    option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn host_arch() -> &'static str {
    match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    }
}

fn default_home() -> Result<PathBuf, String> {
    if let Some(home) = env::var_os("PROTECTORATE_HOME") {
        return Ok(PathBuf::from(home));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".protectorate"))
        .ok_or_else(|| "HOME is unset; pass --home".into())
}

fn usage() -> &'static str {
    "Usage:\n  protectorate-rs version --json\n  protectorate-rs node join --hub URL --ca-cert FILE [--code CODE | --code-file FILE] [--name NAME] [--listen IP:PORT] [--home DIR]\n  protectorate-rs node install --hub URL --ca-cert FILE [--code CODE | --code-file FILE] [--name NAME] [--listen IP:PORT] [--home DIR] [--allow-plain-jobs] [--dry-run] [--no-start] [-- serve-flags]\n  protectorate-rs node doctor [--home DIR] [--json]\n  protectorate-rs node uninstall [--home DIR] [--dry-run]\n  protectorate-rs node serve [--home DIR] [--listen IP:PORT] [--file-root DIR] [--file-unconfined] [--job-concurrency 2] [--allow-plain-jobs] [--watch-dir DIR] [--docker-host URL]\n"
}

fn parse_listen(value: Option<String>) -> Result<SocketAddr, String> {
    value
        .ok_or("--listen requires IP:PORT")?
        .parse()
        .map_err(|_| "--listen requires IP:PORT".into())
}

fn parse_install(mut args: impl Iterator<Item = String>) -> Result<Install, String> {
    let mut home = None;
    let mut hub = None;
    let mut name = None;
    let mut code = None;
    let mut code_file = None;
    let mut ca_cert = None;
    let mut listen = None;
    let mut allow_plain_jobs = false;
    let mut dry_run = false;
    let mut no_start = false;
    let mut serve_args = Vec::new();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--" => {
                serve_args.extend(args);
                break;
            }
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
            }
            "--hub" if hub.is_none() => hub = Some(args.next().ok_or("--hub requires URL")?),
            "--name" if name.is_none() => name = Some(args.next().ok_or("--name requires NAME")?),
            "--code" if code.is_none() => code = Some(args.next().ok_or("--code requires CODE")?),
            "--code-file" if code_file.is_none() => {
                code_file = Some(PathBuf::from(
                    args.next().ok_or("--code-file requires FILE")?,
                ))
            }
            "--ca-cert" if ca_cert.is_none() => {
                ca_cert = Some(PathBuf::from(args.next().ok_or("--ca-cert requires FILE")?))
            }
            "--listen" => listen = Some(parse_listen(args.next())?),
            "--allow-plain-jobs" => allow_plain_jobs = true,
            "--dry-run" => dry_run = true,
            "--no-start" => no_start = true,
            _ => {
                return Err(format!(
                    "unknown or duplicate node install argument {flag:?}"
                ))
            }
        }
    }
    let hub = hub.ok_or("node install requires --hub URL")?;
    if !hub.starts_with("https://") {
        return Err("--hub requires the hub's HTTPS URL".into());
    }
    if code.is_some() && code_file.is_some() {
        return Err("--code and --code-file are exclusive".into());
    }
    for arg in &serve_args {
        if arg.contains(['\n', '\r'])
            || ["--home", "--listen", "--sandbox-helper"]
                .iter()
                .any(|reserved| arg == reserved || arg.starts_with(&format!("{reserved}=")))
        {
            return Err("node serve argument conflicts with installer-managed options".into());
        }
    }
    let paths = lifecycle::Paths::current(home.map_or_else(default_home, Ok)?)
        .map_err(|error| error.to_string())?;
    Ok(Install {
        paths,
        hub: hub.trim_end_matches('/').to_owned(),
        name,
        code,
        code_file,
        ca_cert: ca_cert.ok_or("node install requires --ca-cert FILE (the hub CA)")?,
        listen,
        allow_plain_jobs,
        dry_run,
        no_start,
        serve_args,
    })
}

fn parse_doctor(mut args: impl Iterator<Item = String>) -> Result<Doctor, String> {
    let mut home = None;
    let mut json = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
            }
            "--json" => json = true,
            _ => return Err(format!("unknown node doctor argument {flag:?}")),
        }
    }
    let paths = lifecycle::Paths::current(home.map_or_else(default_home, Ok)?)
        .map_err(|error| error.to_string())?;
    Ok(Doctor { paths, json })
}

fn parse_uninstall(mut args: impl Iterator<Item = String>) -> Result<Uninstall, String> {
    let mut home = None;
    let mut dry_run = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
            }
            "--dry-run" => dry_run = true,
            _ => return Err(format!("unknown node uninstall argument {flag:?}")),
        }
    }
    let paths = lifecycle::Paths::current(home.map_or_else(default_home, Ok)?)
        .map_err(|error| error.to_string())?;
    Ok(Uninstall { paths, dry_run })
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
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
            }
            "--listen" => listen = Some(parse_listen(args.next())?),
            "--file-root" if file_root.is_none() => {
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
            "--docker-host" if docker_host.is_none() => {
                docker_host = Some(args.next().ok_or("--docker-host requires URL")?)
            }
            "--sandbox-helper" => {
                let _ = args.next().ok_or("--sandbox-helper requires FILE")?;
                return Err("namespace sandbox helper is unavailable in Termux".into());
            }
            "--help" | "-h" => return Err(usage().into()),
            _ => return Err(format!("unknown or duplicate node serve argument {flag:?}")),
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
        Some("node") => match args.next().as_deref() {
            Some("join") => node_join::parse(args).map(Command::Join).map(Some),
            Some("serve") => parse_serve(args).map(Command::Serve).map(Some),
            Some("install") => parse_install(args).map(Command::Install).map(Some),
            Some("doctor") => parse_doctor(args).map(Command::Doctor).map(Some),
            Some("uninstall") => parse_uninstall(args).map(Command::Uninstall).map(Some),
            _ => Err(usage().into()),
        },
        _ => Err(usage().into()),
    }
}

async fn shutdown_signal() {
    if let Ok(mut term) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    } else {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn serve_args(command: &Install) -> Vec<String> {
    let mut args = vec!["--home".into(), command.paths.home.display().to_string()];
    if command.allow_plain_jobs {
        args.push("--allow-plain-jobs".into());
    }
    args.extend(command.serve_args.iter().cloned());
    args
}

/// A node already joined to this hub under this name is not joined again.
fn existing_join(command: &Install) -> io::Result<bool> {
    let joined = match node_join::Joined::load(&command.paths.home) {
        Ok(joined) => joined,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if joined.hub != command.hub
        || command
            .name
            .as_deref()
            .is_some_and(|name| name != joined.name)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "already joined {} as {}; uninstall first to join another hub or name",
                joined.hub, joined.name
            ),
        ));
    }
    Ok(true)
}

fn publish_binary(source: &Path, target: &Path) -> io::Result<()> {
    if source == target {
        return Ok(());
    }
    let stage = target.with_extension(format!("install-{}", std::process::id()));
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(&stage)?;
    io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    drop(output);
    protectorate_fsutil::rename(&stage, target)
}

async fn install_node(command: Install) -> Result<(), Box<dyn Error>> {
    let source = fs::canonicalize(env::current_exe()?)?;
    let args = serve_args(&command);
    parse_serve(args.clone().into_iter()).map_err(io::Error::other)?;
    let joined = existing_join(&command)?;
    let (run, boot) = lifecycle::service_files(&command.paths, args)?;
    lifecycle::validate_managed(&command.paths.run())?;
    lifecycle::validate_managed(&command.paths.boot())?;
    println!(
        "node binary: {} -> {}",
        source.display(),
        command.paths.binary().display()
    );
    println!("service run: {}", command.paths.run().display());
    println!("boot script: {}", command.paths.boot().display());
    println!(
        "enrollment: {}",
        if joined {
            "keep existing"
        } else {
            "join the hub"
        }
    );
    if command.dry_run {
        println!("dry run: no files or services changed");
        return Ok(());
    }
    if !joined
        && command.code.is_none()
        && command.code_file.is_none()
        && env::var_os("PROTECTORATE_JOIN_CODE").is_none()
    {
        return Err("joining requires --code, --code-file or PROTECTORATE_JOIN_CODE".into());
    }
    fs::create_dir_all(&command.paths.home)?;
    fs::set_permissions(&command.paths.home, fs::Permissions::from_mode(0o700))?;
    if !joined {
        let mut join = vec![
            "--home".into(),
            command.paths.home.display().to_string(),
            "--hub".into(),
            command.hub.clone(),
            "--ca-cert".into(),
            command.ca_cert.display().to_string(),
        ];
        if let Some(name) = &command.name {
            join.extend(["--name".into(), name.clone()]);
        }
        if let Some(code) = &command.code {
            join.extend(["--code".into(), code.clone()]);
        }
        if let Some(path) = &command.code_file {
            join.extend(["--code-file".into(), path.display().to_string()]);
        }
        if let Some(listen) = command.listen {
            join.extend(["--listen".into(), listen.to_string()]);
        }
        node_join::run(node_join::parse(join.into_iter()).map_err(io::Error::other)?).await?;
    }
    fs::create_dir_all(command.paths.binary().parent().unwrap())?;
    publish_binary(&source, &command.paths.binary())?;
    lifecycle::write_service_files(&command.paths, &run, &boot)?;
    if command.no_start {
        println!(
            "service staged; run sv-enable protectorate-node after installing termux-services"
        );
    } else {
        if let Err(error) = lifecycle::command(
            &command.paths.prefix.join("bin/sv-enable"),
            &["protectorate-node"],
        ) {
            eprintln!("node install: cannot enable service yet: {error}");
        } else if let Err(error) = lifecycle::command(
            &command.paths.prefix.join("bin/sv"),
            &["restart", "protectorate-node"],
        ) {
            eprintln!("node install: cannot restart service yet: {error}");
        }
    }
    println!("installed Termux node; open Termux:Boot once to permit startup after reboot");
    Ok(())
}

async fn doctor_node(command: Doctor) -> Result<(), Box<dyn Error>> {
    let mut checks = lifecycle::doctor(&command.paths, build_version());
    match node_join::Joined::load(&command.paths.home)
        .and_then(|joined| node_join::Joined::certificate(&command.paths.home).map(|_| joined))
    {
        Ok(joined) => checks.push((
            "enrollment",
            "ok",
            format!("joined {} as {}; serves {}", joined.hub, joined.name, joined.address),
        )),
        Err(error) => checks.push(("enrollment", "FAIL", error.to_string())),
    }
    let failed = checks
        .iter()
        .filter(|(_, level, _)| *level == "FAIL")
        .count();
    if command.json {
        println!(
            "{}",
            serde_json::json!({"checks": checks.iter().map(|(name, level, detail)| serde_json::json!({"name":name,"level":level,"detail":detail})).collect::<Vec<_>>(), "failed": failed})
        );
    } else {
        for (name, level, detail) in checks {
            println!("{level:<5} {name:<14} {detail}");
        }
    }
    if failed > 0 {
        return Err(format!("{failed} node doctor check(s) failed").into());
    }
    Ok(())
}

fn uninstall_node(command: Uninstall) -> Result<(), Box<dyn Error>> {
    println!("disable Termux service: sv-disable protectorate-node");
    println!(
        "left in place: {}, {}, {}, {}",
        command.paths.binary().display(),
        command.paths.run().display(),
        command.paths.boot().display(),
        command.paths.home.display()
    );
    if !command.dry_run {
        lifecycle::validate_managed(&command.paths.run())?;
        lifecycle::command(
            &command.paths.prefix.join("bin/sv-disable"),
            &["protectorate-node"],
        )?;
    }
    Ok(())
}

async fn serve_node(options: Serve) -> Result<ServeExit, Box<dyn Error>> {
    let home = fs::canonicalize(&options.home)?;
    let (tls, renew) = node_join::serve_tls(&home)?;
    let collector: Arc<dyn protectorate_node::MetricsCollector> = Arc::new(
        protectorate_node_metrics::Collector::linux(protectorate_node_metrics::Options {
            version: build_version().to_owned(),
            home: home.clone(),
            watch_paths: options.watch_paths,
            docker_host: options.docker_host,
            ..Default::default()
        }),
    );
    let file_root = options.file_root.unwrap_or_else(|| home.join("node-files"));
    let mut jobs = protectorate_node::Jobs::new(&home, options.job_concurrency)?;
    if options.allow_plain_jobs {
        jobs = jobs.with_plain_mode();
    }
    let restart_requested = Arc::new(AtomicBool::new(false));
    let restart_notify = Arc::new(tokio::sync::Notify::new());
    let uninstall_requested = Arc::new(AtomicBool::new(false));
    let uninstall_notify = Arc::new(tokio::sync::Notify::new());
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
    let paths = lifecycle::Paths::current(home.clone())?;
    let plan = protectorate_node::UninstallPlan::termux(
        &home,
        &paths.user_home,
        env::current_exe()?,
        &paths.prefix,
        true,
    )?;
    let uninstall_flag = Arc::clone(&uninstall_requested);
    let uninstall_wake = Arc::clone(&uninstall_notify);
    let agent = agent.with_uninstall(
        plan,
        paths.prefix.join("bin/sv-disable"),
        Arc::new(move || {
            uninstall_flag.store(true, Ordering::SeqCst);
            uninstall_wake.notify_one();
        }),
    );
    if agent.start_update_trial()? {
        return Ok(ServeExit::Restart);
    }
    let (listener, follow) = node_join::bind(&home, options.listen).await?;
    let (swap, rebind) = tokio::sync::mpsc::channel(1);
    eprintln!("protectorate-rs node serving on {}", listener.local_addr()?);
    follow.spawn(swap);
    let shutdown_requested = Arc::clone(&restart_requested);
    let uninstall_shutdown = Arc::clone(&uninstall_requested);
    axum::serve(
        transport::TlsListener::new(listener, tls, Some(rebind))?,
        transport::peer_router(agent.router()?)
            .into_make_service_with_connect_info::<transport::Peer>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = shutdown_signal() => {},
            _ = async { while !shutdown_requested.load(Ordering::SeqCst) { restart_notify.notified().await; } } => {},
            _ = async { while !uninstall_shutdown.load(Ordering::SeqCst) { uninstall_notify.notified().await; } } => {},
        }
    })
    .await?;
    Ok(if uninstall_requested.load(Ordering::SeqCst) {
        ServeExit::Uninstalled
    } else if restart_requested.load(Ordering::SeqCst) {
        ServeExit::Restart
    } else {
        ServeExit::Stopped
    })
}

fn update_marker(exe: &Path) -> io::Result<(String, u64)> {
    let bytes = fs::read(format!("{}.update", exe.display()))?;
    let marker: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok((
        marker["phase"].as_str().unwrap_or_default().to_owned(),
        marker["deadline_ms"].as_u64().unwrap_or_default(),
    ))
}

async fn spawn_guard_child(
    exe: &Path,
    args: &[std::ffi::OsString],
) -> io::Result<tokio::process::Child> {
    for attempt in 0..5 {
        match tokio::process::Command::new(exe)
            .args(args)
            .env(GUARD_CHILD, "1")
            .spawn()
        {
            Err(error) if error.kind() == io::ErrorKind::ExecutableFileBusy && attempt < 4 => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            result => return result,
        }
    }
    unreachable!("the final spawn attempt always returns")
}

async fn supervise_node(exe: &Path) -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut child = spawn_guard_child(exe, &args).await?;
    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                if let Some(pid) = child.id() {
                    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
                    if tokio::time::timeout(Duration::from_secs(8), child.wait()).await.is_err() {
                        let _ = child.kill().await;
                    }
                }
                return Ok(());
            }
            status = child.wait() => {
                let status = status?;
                if status.code() == Some(76) { return Ok(()); }
                let phase = match update_marker(exe) {
                    Ok((phase, _)) => phase,
                    Err(error) if error.kind() == io::ErrorKind::NotFound && status.success() => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                if status.code() == Some(75) {
                    child = spawn_guard_child(exe, &args).await?;
                    continue;
                }
                if phase == "trial" {
                    protectorate_node::recover_failed_trial(exe, &format!("new node binary exited {status} before confirmation"))?;
                    child = spawn_guard_child(exe, &args).await?;
                    continue;
                }
                if status.success() { return Ok(()); }
                if phase == "confirmed" || phase == "rolledback" {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    child = spawn_guard_child(exe, &args).await?;
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
                    child = spawn_guard_child(exe, &args).await?;
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
                "version": build_version(), "os": "android", "arch": host_arch()
            })
        ),
        Some(Command::Join(command)) => node_join::run(command).await?,
        Some(Command::Install(command)) => install_node(command).await?,
        Some(Command::Doctor(command)) => doctor_node(command).await?,
        Some(Command::Uninstall(command)) => uninstall_node(command)?,
        Some(Command::Serve(options)) => {
            if env::var_os(GUARD_CHILD).is_some() {
                match serve_node(options).await? {
                    ServeExit::Restart => std::process::exit(75),
                    ServeExit::Uninstalled => std::process::exit(76),
                    ServeExit::Stopped => std::process::exit(0),
                }
            }
            let exe = fs::canonicalize(env::current_exe()?)?;
            supervise_node(&exe).await?;
        }
    }
    Ok(())
}
