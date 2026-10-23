//! Local developer entry point for the four official headless protocols.
use std::{
    cell::Cell,
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use protectorate_chatlog::Log;
use protectorate_sandbox::{runtime::LaunchOptions, Limits, Policy, Request, Workload};
use serde_json::Value;

pub struct Command {
    harness: Harness,
    cwd: PathBuf,
    resume: Option<String>,
    prompt: String,
}

#[derive(Clone, Copy)]
enum Harness {
    Claude,
    Codex,
    Grok,
    Pi,
}

impl Harness {
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Pi => "pi",
        }
    }
}

pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    if args.next().as_deref() != Some("run") {
        return Err(
            "usage: driver run --harness claude|codex|grok|pi --cwd DIR [--session ID] <prompt>"
                .into(),
        );
    }
    let (mut harness, mut cwd, mut resume) = (None, None, None);
    let mut words = Vec::new();
    while let Some(arg) = args.next() {
        if !words.is_empty() {
            words.push(arg);
            continue;
        }
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
        match flag {
            "--harness" => {
                let value = inline
                    .map(str::to_owned)
                    .or_else(|| args.next())
                    .ok_or("--harness requires a value")?;
                harness = Some(match value.as_str() {
                    "claude" => Harness::Claude,
                    "codex" => Harness::Codex,
                    "grok" => Harness::Grok,
                    "pi" => Harness::Pi,
                    _ => return Err(format!("unknown harness {value:?}")),
                });
            }
            "--cwd" => {
                cwd = Some(PathBuf::from(
                    inline
                        .map(str::to_owned)
                        .or_else(|| args.next())
                        .ok_or("--cwd requires a directory")?,
                ))
            }
            "--session" => {
                resume = Some(
                    inline
                        .map(str::to_owned)
                        .or_else(|| args.next())
                        .ok_or("--session requires an ID")?,
                )
            }
            "--" => {
                words.extend(args);
                break;
            }
            _ if arg.starts_with('-') => return Err(format!("unknown driver run option {arg:?}")),
            _ => words.push(arg),
        }
    }
    if words.is_empty() {
        return Err("driver run requires a prompt".into());
    }
    let cwd =
        fs::canonicalize(cwd.ok_or("driver run requires --cwd")?).map_err(|e| e.to_string())?;
    if !cwd.is_dir() {
        return Err(format!("{} is not a directory", cwd.display()));
    }
    Ok(Command {
        harness: harness.ok_or("driver run requires --harness")?,
        cwd,
        resume,
        prompt: words.join(" "),
    })
}

enum Session {
    Claude(Arc<protectorate_claude_driver::Session>),
    Codex(Arc<protectorate_codex_driver::Session>),
    Grok(Arc<protectorate_grok_driver::Session>),
    Pi(Arc<protectorate_pi_driver::Session>),
}
impl Session {
    fn status(&self) -> Value {
        match self {
            Self::Claude(s) => s.status(),
            Self::Codex(s) => s.status(),
            Self::Grok(s) => s.status(),
            Self::Pi(s) => s.status(),
        }
    }
    fn send(&self, text: &str) -> Result<u64, String> {
        match self {
            Self::Claude(s) => s.send(text, "driver", ""),
            Self::Codex(s) => s.send(text, "driver", ""),
            Self::Grok(s) => s.send(text, "driver", ""),
            Self::Pi(s) => s.send(text, "driver", ""),
        }
    }
    fn interrupt(&self) -> Result<(), String> {
        match self {
            Self::Claude(s) => s.interrupt(),
            Self::Codex(s) => s.interrupt(),
            Self::Grok(s) => s.interrupt(),
            Self::Pi(s) => s.interrupt(),
        }
    }
    fn stop(&self) {
        match self {
            Self::Claude(s) => s.stop(),
            Self::Codex(s) => s.stop(),
            Self::Grok(s) => s.stop(),
            Self::Pi(s) => s.stop(),
        }
    }
    fn respond(&self, id: &str, allow: bool) -> Result<(), String> {
        match self {
            Self::Claude(s) => s.respond(id, allow, "", "driver"),
            Self::Codex(s) => s.respond(id, allow, "", "driver"),
            Self::Grok(s) => s.respond(id, allow, "", "driver"),
            Self::Pi(_) => Err("Pi does not expose approval responses".into()),
        }
    }
}

fn find_executable(name: &str) -> Result<PathBuf, String> {
    let path = env::var_os("PATH").ok_or("PATH is unset")?;
    for dir in env::split_paths(&path) {
        #[cfg(windows)]
        let names = [
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
        ];
        #[cfg(not(windows))]
        let names = [name.to_owned()];
        for executable in names {
            let candidate = dir.join(executable);
            if candidate.is_file() {
                return fs::canonicalize(candidate).map_err(|e| e.to_string());
            }
        }
    }
    Err(format!("{name} is not on PATH"))
}

fn ask_approval(value: &Value) -> Result<bool, String> {
    let kind = value["kind"].as_str().unwrap_or("action");
    let summary = value["summary"].as_str().unwrap_or("");
    let mut input = io::stdin().lock();
    loop {
        eprint!("approve {kind}: {summary} [y/n] ");
        io::stderr().flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("no approval answer".into());
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => {}
        }
    }
}

pub async fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"));
    #[cfg(not(windows))]
    let home = env::var_os("HOME");
    let home = home.map(PathBuf::from).ok_or("user home is unset")?;
    let temp = tempfile::tempdir()?;
    let executable = find_executable(command.harness.name())?;
    let mut argv = vec![executable.to_string_lossy().into_owned()];
    let mut claude_session_id = None;
    match command.harness {
        Harness::Pi => {
            argv.extend(["--mode".into(), "rpc".into()]);
            if let Some(id) = &command.resume {
                argv.extend(["--session".into(), id.clone()]);
            }
        }
        Harness::Codex => argv.push("app-server".into()),
        Harness::Grok => argv.extend(protectorate_grok_driver::stdio_args().map(str::to_owned)),
        Harness::Claude => {
            argv.extend(
                [
                    "-p",
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--permission-prompts",
                    "host",
                    "--permission-prompt-tool",
                    "stdio",
                    "--permission-mode",
                    "manual",
                ]
                .map(str::to_owned),
            );
            let id = if let Some(id) = &command.resume {
                argv.extend(["--resume".into(), id.clone()]);
                id.clone()
            } else {
                let id = protectorate_claude_driver::new_session_id()?;
                argv.extend(["--session-id".into(), id.clone()]);
                id
            };
            claude_session_id = Some(id);
        }
    }
    let policy = Policy {
        state_root: temp.path().to_path_buf(),
        user_home: home.clone(),
        protected: vec![],
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: vec![],
        immutable_cache_roots: vec![],
        nested_cgroups_available: false,
    };
    let plan = policy.plan(Request {
        workload: Workload::DirectHarnessDriver,
        command: argv,
        workspace: None,
        managed_workspace_name: None,
        private_home: None,
        grants: vec![],
        tool_roots: vec![],
        selected_executable: None,
        credential: None,
        overlays: vec![],
        masks: vec![],
        egress: None,
        limits: Limits {
            cpu_percent: 0,
            memory_bytes: 0,
            tasks_max: 0,
        },
    })?;
    let options = LaunchOptions::new(Path::new(""), Path::new(""), &home, &command.cwd);
    let log = Arc::new(Log::open(temp.path().join("chat.jsonl"))?);
    let session = Arc::new(match command.harness {
        Harness::Claude => Session::Claude(protectorate_claude_driver::Session::start(
            &plan,
            options,
            Arc::clone(&log),
            claude_session_id.expect("Claude session ID"),
            false,
        )?),
        Harness::Codex => Session::Codex(protectorate_codex_driver::Session::start(
            &plan,
            options,
            Arc::clone(&log),
            command.resume.as_deref(),
            false,
        )?),
        Harness::Grok => Session::Grok(protectorate_grok_driver::Session::start(
            &plan,
            options,
            Arc::clone(&log),
            command.resume.as_deref(),
            false,
        )?),
        Harness::Pi => Session::Pi(protectorate_pi_driver::Session::start(
            &plan,
            options,
            Arc::clone(&log),
            command.resume.as_deref(),
        )?),
    });
    eprintln!(
        "session {}/{}",
        command.harness.name(),
        session.status()["harnessSessionId"]
            .as_str()
            .unwrap_or("pending")
    );
    let signals = Arc::clone(&session);
    #[cfg(unix)]
    let signal_task = tokio::spawn(async move {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        let mut count = 0;
        loop {
            tokio::select! {
                result = tokio::signal::ctrl_c() => { if result.is_err() { break; } }
                _ = async { if let Some(signal)=term.as_mut() { signal.recv().await; } else { std::future::pending::<()>().await; } } => {}
            }
            count += 1;
            if count == 1 {
                let _ = signals.interrupt();
            } else {
                signals.stop();
                break;
            }
        }
    });
    #[cfg(windows)]
    let signal_task = tokio::spawn(async move {
        let mut break_signal = tokio::signal::windows::ctrl_break().ok();
        let mut count = 0;
        loop {
            tokio::select! {
                result = tokio::signal::ctrl_c() => { if result.is_err() { break; } }
                _ = async { if let Some(signal)=break_signal.as_mut() { signal.recv().await; } else { std::future::pending::<()>().await; } } => {}
            }
            count += 1;
            if count == 1 {
                let _ = signals.interrupt();
            } else {
                signals.stop();
                break;
            }
        }
    });
    let result = (|| -> Result<(), String> {
        let after = session.send(&command.prompt)?;
        let cancelled = AtomicBool::new(false);
        let last_seen = Cell::new(after);
        let mut settled = false;
        let mut exit_error = None;
        let mut consume = |event: &protectorate_chatlog::Event| {
            last_seen.set(event.seq);
            let mut out = io::stdout().lock();
            serde_json::to_writer(&mut out,&serde_json::json!({"seq":event.seq,"timeMs":event.time_ms,"payload":event.payload})).map_err(|e|e.to_string())?;
            writeln!(out)
                .and_then(|_| out.flush())
                .map_err(|e| e.to_string())?;
            if let Some(approval) = event.payload.get("approval") {
                let id = approval["id"].as_str().ok_or("approval event has no ID")?;
                session.respond(id, ask_approval(approval)?)?;
            }
            if event.payload.get("settled").is_some() {
                settled = true;
                return Err("settled".into());
            }
            if let Some(text) = event.payload["error"]["text"].as_str() {
                exit_error = Some(text.to_owned());
            }
            if event.payload["state"]["state"] == "CHAT_STATE_EXITED" {
                return Err("exited".into());
            }
            Ok(())
        };
        let watched = log.watch(after, &cancelled, &mut consume);
        // A fast peer may settle and close its log before watch attaches.
        if matches!(watched, Err(protectorate_chatlog::Error::Closed)) {
            for event in protectorate_chatlog::read_all(temp.path().join("chat.jsonl"))
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|event| event.seq > last_seen.get())
            {
                if consume(&event).is_err() {
                    break;
                }
            }
        }
        if settled {
            eprintln!(
                "settled; session {}",
                session.status()["harnessSessionId"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            exit_error.map_or(Ok(()), Err)
        } else {
            Err(exit_error
                .or_else(|| watched.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "harness exited before settling".into()))
        }
    })();
    signal_task.abort();
    session.stop();
    let _ = std::thread::spawn({
        let session = Arc::clone(&session);
        move || match &*session {
            Session::Claude(s) => s.wait_exited(Duration::from_secs(2)),
            Session::Codex(s) => s.wait_exited(Duration::from_secs(2)),
            Session::Grok(s) => s.wait_exited(Duration::from_secs(2)),
            Session::Pi(s) => s.wait_exited(Duration::from_secs(2)),
        }
    })
    .join();
    result.map_err(|e| e.into())
}
