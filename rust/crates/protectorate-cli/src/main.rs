//! Thin binary over the dispatcher: argv in, text or JSON out.
mod completion;
mod harness_login;
use protectorate_cli::{run, RpcError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn write_stdout(text: &str) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|_| stdout.flush())
    {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(format!("write stdout: {error}")),
    }
}

fn finish_stdout(text: &str) -> ExitCode {
    match write_stdout(text) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = completion::run_if_requested(&args) {
        return match result {
            Ok(output) => finish_stdout(&output),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(result) = harness_login::run_if_requested(&args) {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = hub_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) if forwarded.first().is_some_and(|arg| arg == "serve") => {
                run_server_companion(&companion_executable(), &forwarded)
            }
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = driver_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = mcp_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_mcp_companion(&forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    match run(&args) {
        Ok(text) => finish_stdout(&text),
        Err(error) => {
            eprintln!("error: {}", short_message(&error));
            ExitCode::FAILURE
        }
    }
}

/// Hub and driver commands belong to the Rust hub binary; the installed CLI
/// forwards to the companion beside it, or to the bare name on PATH.
fn companion_name() -> &'static str {
    if cfg!(windows) {
        "protectorate-rs.exe"
    } else {
        "protectorate-rs"
    }
}

fn companion_executable() -> PathBuf {
    let name = companion_name();
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join(name)))
        .filter(|path| path.is_file());
    if let Some(sibling) = sibling {
        return sibling;
    }
    name.into()
}

fn mcp_executable() -> PathBuf {
    let name = if cfg!(windows) {
        "protectorate-mcpbridge.exe"
    } else {
        "protectorate-mcpbridge"
    };
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join(name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| name.into())
}

/// Forward MCP's protocol stream directly to the dedicated Rust bridge.
/// Only a leading `mcp` command is recognized: an argument to another command
/// that happens to say `mcp` must keep its ordinary meaning.
#[derive(Debug, PartialEq, Eq)]
struct McpForward {
    argv: Vec<String>,
    hub: Option<String>,
    token_file: Option<PathBuf>,
    home: Option<PathBuf>,
    needs_token: bool,
}

fn mcp_alias_argv(args: &[String]) -> Option<Result<McpForward, String>> {
    let mut forwarded = Vec::new();
    let mut hub = None;
    let mut token_file = None;
    let mut home = None;
    let mut at = 0;
    while at < args.len() {
        let token = &args[at];
        if token == "mcp" {
            break;
        }
        let (flag, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(flag, value)| (flag, Some(value)));
        match flag {
            "--json" if inline.is_none() => {
                // MCP already owns stdout: --list is JSON and serving is JSON-RPC.
            }
            "--hub" | "--token-file" | "--timeout" => {
                let value = if let Some(inline) = inline {
                    inline.to_owned()
                } else {
                    at += 1;
                    if at >= args.len() {
                        return None;
                    }
                    args[at].clone()
                };
                if flag == "--hub" {
                    hub = Some(value.clone());
                } else if flag == "--token-file" {
                    token_file = Some(PathBuf::from(&value));
                }
                forwarded.extend([flag.to_owned(), value]);
            }
            "--home" => {
                let value = if let Some(inline) = inline {
                    inline.to_owned()
                } else {
                    at += 1;
                    if at >= args.len() {
                        return None;
                    }
                    args[at].clone()
                };
                home = Some(PathBuf::from(value));
            }
            _ => return None,
        }
        at += 1;
    }
    if at == args.len() {
        return None;
    }
    at += 1;
    let mut needs_token = true;
    while at < args.len() {
        let token = &args[at];
        let (flag, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(flag, value)| (flag, Some(value)));
        match flag {
            "--json" if inline.is_none() => {}
            "--list" | "--help" | "-h" if inline.is_none() => {
                needs_token = false;
                forwarded.push(token.clone());
            }
            "--home" | "--hub" | "--token-file" | "--timeout" | "--allow" | "--deny" | "--http"
            | "--https" | "--tls-cert" | "--tls-key" => {
                let value = if let Some(inline) = inline {
                    inline.to_owned()
                } else {
                    at += 1;
                    if at >= args.len() {
                        return Some(Err(format!("{flag} requires a value")));
                    }
                    args[at].clone()
                };
                match flag {
                    "--hub" => hub = Some(value.clone()),
                    "--token-file" => token_file = Some(PathBuf::from(&value)),
                    "--home" => home = Some(PathBuf::from(&value)),
                    _ => {}
                }
                if flag != "--home" {
                    forwarded.extend([flag.to_owned(), value]);
                }
            }
            _ => forwarded.push(token.clone()),
        }
        at += 1;
    }
    Some(Ok(McpForward {
        argv: forwarded,
        hub,
        token_file,
        home,
        needs_token,
    }))
}

fn run_mcp_companion(forwarded: &McpForward) -> ExitCode {
    let mut command = Command::new(mcp_executable());
    command.args(&forwarded.argv);
    if forwarded.needs_token {
        // Resolve credentials in the CLI so the bridge receives the same
        // capability file, saved device key or local hub token that ordinary
        // commands use.
        let (name, value) = match protectorate_cli::mcp_credential(
            forwarded.hub.as_deref(),
            forwarded.token_file.as_deref(),
            forwarded.home.as_deref(),
        ) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("error: {}", short_message(&error));
                return ExitCode::FAILURE;
            }
        };
        command.env_remove("PROTECTORATE_TOKEN_FILE");
        command.env_remove("PROTECTORATE_TOKEN");
        command.env(name, value);
    }
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(
            status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .filter(|code| *code != 0)
                .unwrap_or(1),
        ),
        Err(error) => {
            eprintln!("error: run {}: {error}", mcp_executable().display());
            ExitCode::FAILURE
        }
    }
}

/// Hub startup, backup/restore and cache publication are local
/// operations owned by the Rust hub companion. `hub update` remains an
/// authenticated hub RPC.
fn hub_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let at = local_root_at(args, "hub")?;
    let (consumed, prefix): (usize, &[&str]) = match args.get(at + 1).map(String::as_str) {
        Some("serve") => (2, &["serve"]),
        Some("backup") => (2, &["hub", "backup"]),
        Some("restore") => (2, &["hub", "restore"]),
        Some("publish-flutter-cache") => (2, &["hub", "publish-flutter-cache"]),
        Some("publish-flutter-tools") => (2, &["hub", "publish-flutter-tools"]),
        Some("publish-pub-cache") => (2, &["hub", "publish-pub-cache"]),
        Some("publish-go-mod-cache") => (2, &["hub", "publish-go-mod-cache"]),
        _ => return None,
    };
    let label = args[at..at + consumed].join(" ");
    let leading = local_alias_leading(args, at, &label, &[("--home", true)])?;
    Some(leading.map(|leading| {
        let mut forwarded = prefix
            .iter()
            .map(|part| (*part).to_owned())
            .collect::<Vec<_>>();
        forwarded.extend(leading);
        forwarded.extend_from_slice(&args[at + consumed..]);
        forwarded
    }))
}

fn driver_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let at = local_root_at(args, "driver")?;
    if args.get(at + 1).map(String::as_str) != Some("run") {
        return None;
    }
    let leading = local_alias_leading(args, at, "driver run", &[])?;
    Some(leading.map(|_| args[at..].to_vec()))
}

/// Find the first positional command after known global options. An option
/// value or a later payload word must not be mistaken for a command.
fn local_root_at(args: &[String], root: &str) -> Option<usize> {
    let mut at = 0;
    while let Some(token) = args.get(at) {
        if token == root {
            return Some(at);
        }
        if !token.starts_with("--") {
            return None;
        }
        let (flag, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(flag, value)| (flag, Some(value)));
        at += 1;
        if inline.is_none() && matches!(flag, "--hub" | "--token-file" | "--timeout" | "--home") {
            at += 1;
        }
    }
    None
}

fn local_alias_leading(
    args: &[String],
    at: usize,
    command: &str,
    local_options: &[(&str, bool)],
) -> Option<Result<Vec<String>, String>> {
    let mut leading = Vec::new();
    let mut index = 0;
    while index < at {
        let token = args[index].as_str();
        if !token.starts_with("--") {
            return None;
        }
        let (flag, inline) = match token.split_once('=') {
            Some((head, tail)) => (head, Some(tail)),
            None => (token, None),
        };
        if flag == "--json" {
            return Some(Err(format!("{command} does not support --json")));
        }
        if let Some((_, takes_value)) = local_options.iter().find(|(name, _)| *name == flag) {
            match (takes_value, inline) {
                (false, Some(_)) => return Some(Err(format!("{flag} does not take a value"))),
                (false, None) => leading.push(flag.to_owned()),
                (true, Some(value)) => leading.extend([flag.to_owned(), value.to_owned()]),
                (true, None) => {
                    index += 1;
                    if index >= at {
                        return Some(Err(format!("{flag} requires a value")));
                    }
                    leading.extend([flag.to_owned(), args[index].clone()]);
                }
            }
            index += 1;
            continue;
        }
        match flag {
            "--hub" | "--token-file" | "--timeout" => {
                if inline.is_none() {
                    index += 1;
                    if index >= at {
                        return Some(Err(format!("{flag} requires a value")));
                    }
                }
            }
            _ => return Some(Err(format!("unknown option before {command}: {token}"))),
        }
        index += 1;
    }
    Some(Ok(leading))
}

fn run_companion(executable: &Path, args: &[String]) -> ExitCode {
    match Command::new(executable).args(args).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(
            status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .filter(|code| *code != 0)
                .unwrap_or(1),
        ),
        Err(error) => {
            eprintln!("error: run {}: {error}", executable.display());
            ExitCode::FAILURE
        }
    }
}

fn run_server_companion(executable: &Path, args: &[String]) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = Command::new(executable).args(args).exec();
        eprintln!("error: run {}: {error}", executable.display());
        ExitCode::FAILURE
    }
    #[cfg(not(unix))]
    {
        run_companion(executable, args)
    }
}

/// Usage errors carry the whole usage text; on stderr only the first line of
/// them is the point.
fn short_message(error: &RpcError) -> String {
    error
        .message
        .lines()
        .next()
        .unwrap_or(&error.message)
        .to_owned()
}
