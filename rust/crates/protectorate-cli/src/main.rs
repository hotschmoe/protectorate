//! Thin binary over the dispatcher: argv in, text or JSON out.
mod completion;
mod harness_login;
mod service;
use protectorate_cli::{run_with_status, RpcError, RunError};
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
    if let Some(result) = service::run_if_requested(&args) {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = doctor_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = install_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = uninstall_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = install_runner_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = node_tailnet_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = node_serve_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_server_companion(&companion_executable(), &forwarded),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(forwarded) = client_alias_argv(&args) {
        return match forwarded {
            Ok(forwarded) => run_companion(&companion_executable(), &forwarded),
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
    match run_with_status(&args) {
        Ok(text) => finish_stdout(&text),
        Err(RunError::Rpc(error)) => {
            eprintln!("error: {}", short_message(&error));
            ExitCode::FAILURE
        }
        Err(RunError::Status { code, message }) => {
            eprintln!("error: {message}");
            ExitCode::from(
                u8::try_from(code)
                    .ok()
                    .filter(|code| *code != 0)
                    .unwrap_or(1),
            )
        }
    }
}

/// The diagnostic belongs to protectorate-rs; the installed CLI forwards to
/// its companion binary so there is one implementation of each check.
fn doctor_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let at = args
        .windows(2)
        .position(|pair| pair == ["node", "doctor"])?;
    let mut forwarded = vec!["node".to_owned(), "doctor".to_owned()];
    let mut index = 0;
    while index < at {
        match args[index].as_str() {
            "--json" => forwarded.push("--json".into()),
            "--hub" | "--ca-cert" => {
                let flag = args[index].clone();
                index += 1;
                if index >= at {
                    return Some(Err(format!("{flag} requires a value")));
                }
                forwarded.extend([flag, args[index].clone()]);
            }
            flag if flag.starts_with("--hub=") || flag.starts_with("--ca-cert=") => {
                forwarded.push(flag.into())
            }
            "--token-file" | "--timeout" => {
                index += 1;
                if index >= at {
                    return Some(Err(format!("{} requires a value", args[index - 1])));
                }
            }
            flag if flag.starts_with("--token-file=") || flag.starts_with("--timeout=") => {}
            other => return Some(Err(format!("unknown option before node doctor: {other}"))),
        }
        index += 1;
    }
    index = at + 2;
    while index < args.len() {
        match args[index].as_str() {
            "--token-file" | "--timeout" => {
                index += 1;
                if index >= args.len() {
                    return Some(Err(format!("{} requires a value", args[index - 1])));
                }
            }
            flag if flag.starts_with("--token-file=") || flag.starts_with("--timeout=") => {}
            _ => forwarded.push(args[index].clone()),
        }
        index += 1;
    }
    Some(Ok(forwarded))
}

/// The node install and doctor diagnostics belong to the Rust node binary;
/// the installed CLI forwards to the companion beside it, or to the bare
/// name on PATH. On Windows the node-only companion is the Windows build.
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
    if cfg!(windows) {
        if let Some(development_binary) = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.parent()
                    .map(|dir| dir.join("protectorate-rs-windows.exe"))
            })
            .filter(|path| path.is_file())
        {
            return development_binary;
        }
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
        // saved device or local hub token that ordinary commands use.
        let token = match protectorate_cli::mcp_token(
            forwarded.hub.as_deref(),
            forwarded.token_file.as_deref(),
            forwarded.home.as_deref(),
        ) {
            Ok(token) => token,
            Err(error) => {
                eprintln!("error: {}", short_message(&error));
                return ExitCode::FAILURE;
            }
        };
        command.env("PROTECTORATE_TOKEN", token);
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

/// `protectorate node install ...` forwards to the companion's node install.
/// Install flags after the subcommand are passed through verbatim; the
/// global RPC options are ignored. The hub API URL can differ from the
/// Headscale control URL, so enrollment requires an explicit --control.
fn install_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let at = args
        .windows(2)
        .position(|pair| pair == ["node", "install"])?;
    let mut forwarded = vec!["node".to_owned(), "install".to_owned()];
    let mut index = 0;
    while index < at {
        let token = args[index].as_str();
        if !token.starts_with("--") {
            // A bare word before node install belongs to another command,
            // for example `backlog add node install`; let it dispatch.
            return None;
        }
        match token {
            "--json" => return Some(Err("node install does not support --json".into())),
            "--hub" | "--token-file" | "--timeout" => {
                let flag = args[index].clone();
                index += 1;
                if index >= at {
                    return Some(Err(format!("{flag} requires a value")));
                }
            }
            flag if flag.starts_with("--hub=")
                || flag.starts_with("--token-file=")
                || flag.starts_with("--timeout=") => {}
            other => return Some(Err(format!("unknown option before node install: {other}"))),
        }
        index += 1;
    }
    forwarded.extend_from_slice(&args[at + 2..]);
    Some(Ok(forwarded))
}

/// `protectorate node uninstall ...` forwards to the companion's node
/// uninstall. Both the Linux and the Windows companion implement it locally:
/// the machine stops its own node, which is not a hub call, so the RPC options
/// are dropped. Uninstall flags after the subcommand pass through verbatim.
fn uninstall_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    node_local_alias_argv(args, "uninstall", UNINSTALL_LOCAL_OPTIONS, None)
}

/// `protectorate node install-runner ...` provisions the Windows runner
/// account for shared-node jobs. Only the Windows companion has a runner
/// account, so the Linux CLI says so instead of forwarding.
fn install_runner_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let unsupported = if cfg!(windows) {
        None
    } else {
        Some("node install-runner is supported on Windows only; the Linux node has no runner account")
    };
    node_local_alias_argv(
        args,
        "install-runner",
        INSTALL_RUNNER_LOCAL_OPTIONS,
        unsupported,
    )
}

/// `protectorate node serve ...` forwards to the companion's node agent.
/// The agent serves this machine locally, so the RPC options are dropped and
/// `--json` is refused; the agent's own flags after the subcommand pass
/// through verbatim.
fn node_serve_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    node_local_alias_argv(args, "serve", NODE_SERVE_LOCAL_OPTIONS, None)
}

/// Node enrollment and local Headscale administration live in the companion.
/// The Windows companion supports joining a node; a Rust hub is Unix-only.
fn node_tailnet_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let at = local_root_at(args, "node")?;
    let action = args.get(at + 1)?.as_str();
    if !matches!(action, "join" | "renew" | "revoke") {
        return None;
    }
    let leading = local_alias_leading(args, at, &format!("node {action}"), &[("--home", true)])?;
    if cfg!(windows) && matches!(action, "renew" | "revoke") {
        return Some(Err(format!(
            "node {action} administers the local Rust hub and is unavailable in the Windows node companion"
        )));
    }
    Some(leading.map(|leading| {
        let mut forwarded = vec!["node".to_owned(), action.to_owned()];
        forwarded.extend(leading);
        forwarded.extend_from_slice(&args[at + 2..]);
        forwarded
    }))
}

/// Client enrollment administers the local Headscale instance, so the public
/// CLI forwards it to the installed Rust hub companion instead of the hub API.
fn client_alias_argv(args: &[String]) -> Option<Result<Vec<String>, String>> {
    let at = local_root_at(args, "client")?;
    if !matches!(
        args.get(at + 1).map(String::as_str),
        Some("invite" | "register")
    ) {
        return None;
    }
    let action = &args[at + 1];
    let leading = local_alias_leading(args, at, &format!("client {action}"), &[("--home", true)])?;
    Some(leading.map(|leading| {
        let mut forwarded = vec!["client".to_owned(), action.clone()];
        forwarded.extend(leading);
        forwarded.extend_from_slice(&args[at + 2..]);
        forwarded
    }))
}

/// Hub startup, tailnet setup, backup/restore and cache publication are local
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
        Some("tailnet") => match args.get(at + 2).map(String::as_str) {
            Some("install") => (3, &["hub", "tailnet", "install"]),
            Some("init") => (3, &["hub", "tailnet", "init"]),
            _ => return None,
        },
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

/// Local node options accepted before the subcommand: (flag, takes a value).
const UNINSTALL_LOCAL_OPTIONS: &[(&str, bool)] = &[("--home", true), ("--dry-run", false)];
const NODE_SERVE_LOCAL_OPTIONS: &[(&str, bool)] = &[("--home", true)];
const INSTALL_RUNNER_LOCAL_OPTIONS: &[(&str, bool)] = &[("--home", true), ("--job-root", true)];

fn node_local_alias_argv(
    args: &[String],
    subcommand: &str,
    local_options: &[(&str, bool)],
    unsupported: Option<&str>,
) -> Option<Result<Vec<String>, String>> {
    let at = args
        .windows(2)
        .position(|pair| pair == ["node", subcommand])?;
    let mut forwarded = vec!["node".to_owned(), subcommand.to_owned()];
    let leading = match node_alias_leading(args, at, subcommand, local_options) {
        None => return None,
        Some(Ok(leading)) => leading,
        Some(Err(message)) => return Some(Err(message)),
    };
    if let Some(message) = unsupported {
        return Some(Err(message.to_owned()));
    }
    forwarded.extend(leading);
    forwarded.extend_from_slice(&args[at + 2..]);
    Some(Ok(forwarded))
}

/// Options written before a local node subcommand. The companion does not
/// speak the hub API, so RPC credentials are dropped and `--json` is refused;
/// the node's own options are forwarded. A bare word before the subcommand
/// means the words belong to another command, for example `backlog add node
/// uninstall`, and must keep dispatching normally.
fn node_alias_leading(
    args: &[String],
    at: usize,
    subcommand: &str,
    local_options: &[(&str, bool)],
) -> Option<Result<Vec<String>, String>> {
    local_alias_leading(args, at, &format!("node {subcommand}"), local_options)
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

#[cfg(test)]
mod tests {
    use super::{
        client_alias_argv, companion_name, doctor_alias_argv, driver_alias_argv, hub_alias_argv,
        install_alias_argv, install_runner_alias_argv, mcp_alias_argv, node_serve_alias_argv,
        node_tailnet_alias_argv, run_companion, uninstall_alias_argv,
    };
    use std::path::Path;

    #[test]
    fn driver_run_forwards_to_local_companion() {
        let args =
            ["driver", "run", "--harness", "pi", "--cwd", "/tmp", "hello"].map(str::to_owned);
        assert_eq!(driver_alias_argv(&args).unwrap().unwrap(), args);
        assert!(driver_alias_argv(&["driver", "status"].map(str::to_owned)).is_none());
    }

    #[test]
    fn node_tailnet_alias_forwards_local_join_and_admin_actions() {
        let join = [
            "--hub",
            "https://api",
            "--home",
            "/tmp/node",
            "node",
            "join",
            "--control",
            "https://hs",
            "--key-file",
            "/tmp/key",
        ]
        .map(str::to_owned);
        assert_eq!(
            node_tailnet_alias_argv(&join).unwrap().unwrap(),
            [
                "node",
                "join",
                "--home",
                "/tmp/node",
                "--control",
                "https://hs",
                "--key-file",
                "/tmp/key"
            ]
        );
        if !cfg!(windows) {
            for action in ["renew", "revoke"] {
                let args = ["--home", "/tmp/hub", "node", action, "gpu1"].map(str::to_owned);
                assert_eq!(
                    node_tailnet_alias_argv(&args).unwrap().unwrap(),
                    ["node", action, "--home", "/tmp/hub", "gpu1"]
                );
            }
        }
    }

    #[test]
    fn node_tailnet_alias_does_not_capture_rpc_or_payload_commands() {
        for input in [
            vec!["node", "invite", "gpu1"],
            vec!["node", "update", "gpu1"],
            vec!["node", "install", "--control", "https://hs"],
            vec!["backlog", "add", "node", "join"],
            vec!["--hub", "node", "join"],
        ] {
            assert!(node_tailnet_alias_argv(
                &input.into_iter().map(str::to_owned).collect::<Vec<_>>()
            )
            .is_none());
        }
        assert!(
            node_tailnet_alias_argv(&["--json", "node", "join"].map(str::to_owned))
                .unwrap()
                .unwrap_err()
                .contains("does not support --json")
        );
    }

    #[test]
    fn hub_alias_maps_local_operations_but_not_update() {
        for (input, expected) in [
            (
                vec!["hub", "serve", "--listen", "127.0.0.1:0"],
                vec!["serve", "--listen", "127.0.0.1:0"],
            ),
            (
                vec!["hub", "tailnet", "install"],
                vec!["hub", "tailnet", "install"],
            ),
            (
                vec![
                    "hub",
                    "tailnet",
                    "init",
                    "--server-url",
                    "https://hs.example",
                ],
                vec![
                    "hub",
                    "tailnet",
                    "init",
                    "--server-url",
                    "https://hs.example",
                ],
            ),
            (
                vec!["hub", "backup", "--to", "/tmp/backups"],
                vec!["hub", "backup", "--to", "/tmp/backups"],
            ),
            (
                vec![
                    "hub",
                    "restore",
                    "/tmp/archive.tar.zst",
                    "--home",
                    "/tmp/recovered",
                ],
                vec![
                    "hub",
                    "restore",
                    "/tmp/archive.tar.zst",
                    "--home",
                    "/tmp/recovered",
                ],
            ),
            (
                vec![
                    "hub",
                    "publish-flutter-cache",
                    "--flutter-sdk-root",
                    "/opt/flutter",
                    "--max-bytes",
                    "3221225472",
                ],
                vec![
                    "hub",
                    "publish-flutter-cache",
                    "--flutter-sdk-root",
                    "/opt/flutter",
                    "--max-bytes",
                    "3221225472",
                ],
            ),
            (
                vec![
                    "hub",
                    "publish-flutter-tools",
                    "--flutter-sdk-root",
                    "/opt/flutter",
                ],
                vec![
                    "hub",
                    "publish-flutter-tools",
                    "--flutter-sdk-root",
                    "/opt/flutter",
                ],
            ),
            (
                vec![
                    "hub",
                    "publish-pub-cache",
                    "--pub-cache-root",
                    "/tmp/pub-cache",
                ],
                vec![
                    "hub",
                    "publish-pub-cache",
                    "--pub-cache-root",
                    "/tmp/pub-cache",
                ],
            ),
            (
                vec![
                    "hub",
                    "publish-go-mod-cache",
                    "--go-mod-cache-root",
                    "/tmp/go-mod",
                ],
                vec![
                    "hub",
                    "publish-go-mod-cache",
                    "--go-mod-cache-root",
                    "/tmp/go-mod",
                ],
            ),
        ] {
            assert_eq!(
                hub_alias_argv(&input.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .unwrap()
                    .unwrap(),
                expected
            );
        }
        assert!(hub_alias_argv(&["hub", "update"].map(str::to_owned)).is_none());
        assert!(hub_alias_argv(&["hub", "tailnet", "status"].map(str::to_owned)).is_none());
        assert!(hub_alias_argv(&["hub", "publish-flutter"].map(str::to_owned)).is_none());
    }

    #[test]
    fn hub_alias_preserves_local_home_and_rejects_unrelated_global_modes() {
        let args = [
            "--hub",
            "https://other-hub",
            "--token-file",
            "/tmp/owner",
            "--home",
            "/tmp/state",
            "hub",
            "backup",
            "--to",
            "/tmp/archives",
        ]
        .map(str::to_owned);
        assert_eq!(
            hub_alias_argv(&args).unwrap().unwrap(),
            [
                "hub",
                "backup",
                "--home",
                "/tmp/state",
                "--to",
                "/tmp/archives"
            ]
        );
        assert!(
            hub_alias_argv(&["--json", "hub", "backup"].map(str::to_owned))
                .unwrap()
                .unwrap_err()
                .contains("does not support --json")
        );
        assert!(hub_alias_argv(&["backlog", "add", "hub", "backup"].map(str::to_owned)).is_none());
        assert!(hub_alias_argv(&["--hub", "hub", "backup"].map(str::to_owned)).is_none());
        let publish = [
            "--hub",
            "https://other-hub",
            "--token-file",
            "/tmp/owner",
            "--home",
            "/tmp/state",
            "hub",
            "publish-pub-cache",
            "--pub-cache-root",
            "/tmp/pub-cache",
        ]
        .map(str::to_owned);
        assert_eq!(
            hub_alias_argv(&publish).unwrap().unwrap(),
            [
                "hub",
                "publish-pub-cache",
                "--home",
                "/tmp/state",
                "--pub-cache-root",
                "/tmp/pub-cache"
            ]
        );
        assert!(
            hub_alias_argv(&["--json", "hub", "publish-flutter-cache"].map(str::to_owned))
                .unwrap()
                .unwrap_err()
                .contains("does not support --json")
        );
        assert!(
            hub_alias_argv(&["backlog", "add", "hub", "publish-pub-cache"].map(str::to_owned))
                .is_none()
        );
    }

    #[test]
    fn client_alias_forwards_local_home_and_drops_rpc_flags() {
        let args = [
            "--hub=https://hub",
            "--token-file",
            "private",
            "--home",
            "/tmp/hub",
            "client",
            "invite",
            "--name",
            "phone",
            "--expires",
            "2h",
        ]
        .map(str::to_owned);
        assert_eq!(
            client_alias_argv(&args).unwrap().unwrap(),
            [
                "client",
                "invite",
                "--home",
                "/tmp/hub",
                "--name",
                "phone",
                "--expires",
                "2h"
            ]
        );
        assert_eq!(
            client_alias_argv(
                &[
                    "client",
                    "register",
                    "registration-key",
                    "--home",
                    "/tmp/hub"
                ]
                .map(str::to_owned)
            )
            .unwrap()
            .unwrap(),
            [
                "client",
                "register",
                "registration-key",
                "--home",
                "/tmp/hub"
            ]
        );
    }

    #[test]
    fn client_alias_does_not_capture_other_commands_or_option_values() {
        assert!(
            client_alias_argv(&["backlog", "add", "client", "invite"].map(str::to_owned)).is_none()
        );
        assert!(client_alias_argv(&["--hub", "client", "invite"].map(str::to_owned)).is_none());
        assert!(client_alias_argv(&["client", "unknown"].map(str::to_owned)).is_none());
        assert!(
            client_alias_argv(&["--json", "client", "invite"].map(str::to_owned))
                .unwrap()
                .unwrap_err()
                .contains("does not support --json")
        );
    }

    #[test]
    fn mcp_alias_forwards_flags_and_preserves_protocol_stdout() {
        let args = [
            "--hub",
            "https://hub",
            "--home",
            "/state",
            "--json",
            "mcp",
            "--allow",
            "BacklogService_CreateNote",
            "--list",
        ]
        .map(str::to_owned);
        let forward = mcp_alias_argv(&args).unwrap().unwrap();
        assert_eq!(
            forward.argv,
            [
                "--hub",
                "https://hub",
                "--allow",
                "BacklogService_CreateNote",
                "--list"
            ]
        );
        assert_eq!(forward.hub.as_deref(), Some("https://hub"));
        assert_eq!(forward.home.as_deref(), Some(Path::new("/state")));
        assert!(!forward.needs_token);

        let after = [
            "mcp",
            "--token-file=secret",
            "--hub=https://other",
            "--http",
            "127.0.0.1:7471",
        ]
        .map(str::to_owned);
        let forward = mcp_alias_argv(&after).unwrap().unwrap();
        assert_eq!(forward.hub.as_deref(), Some("https://other"));
        assert_eq!(forward.token_file.as_deref(), Some(Path::new("secret")));
        assert!(forward.needs_token);
    }

    #[test]
    fn mcp_alias_only_matches_the_command_position() {
        assert!(mcp_alias_argv(&["backlog", "add", "mcp"].map(str::to_owned)).is_none());
        assert!(mcp_alias_argv(&["--hub", "mcp", "--list"].map(str::to_owned)).is_none());
        assert!(mcp_alias_argv(&["--unknown", "mcp"].map(str::to_owned)).is_none());
    }

    #[test]
    fn mcp_alias_preserves_tls_options_and_reports_missing_values() {
        let args = [
            "mcp",
            "--https",
            "127.0.0.1:7471",
            "--tls-cert",
            "cert.pem",
            "--tls-key=key.pem",
        ]
        .map(str::to_owned);
        let forwarded = mcp_alias_argv(&args).unwrap().unwrap();
        assert_eq!(
            forwarded.argv,
            [
                "--https",
                "127.0.0.1:7471",
                "--tls-cert",
                "cert.pem",
                "--tls-key",
                "key.pem"
            ]
        );
        assert!(forwarded.needs_token);
        for flag in ["--https", "--tls-cert", "--tls-key"] {
            let args = ["mcp".to_owned(), flag.to_owned()];
            assert!(mcp_alias_argv(&args)
                .unwrap()
                .unwrap_err()
                .contains("requires a value"));
        }
    }

    #[test]
    fn doctor_alias_forwards_diagnostic_options_without_rpc_credentials() {
        let args = [
            "--token-file",
            "private",
            "--hub",
            "https://hub",
            "--ca-cert",
            "/tmp/ca.pem",
            "--json",
            "node",
            "doctor",
            "--home",
            "/tmp/node",
        ]
        .map(str::to_owned);
        assert_eq!(
            doctor_alias_argv(&args).unwrap().unwrap(),
            [
                "node",
                "doctor",
                "--hub",
                "https://hub",
                "--ca-cert",
                "/tmp/ca.pem",
                "--json",
                "--home",
                "/tmp/node"
            ]
        );
    }

    #[test]
    fn install_alias_forwards_install_flags_without_global_rpc_options() {
        let args = [
            "--token-file",
            "private",
            "--hub",
            "https://hub",
            "node",
            "install",
            "--control",
            "https://hub",
            "--name",
            "gpu1",
            "--home",
            "/tmp/node",
            "--key-file",
            "/tmp/enroll.key",
            "--ca-cert",
            "/tmp/ca.pem",
            "--dry-run",
        ]
        .map(str::to_owned);
        assert_eq!(
            install_alias_argv(&args).unwrap().unwrap(),
            [
                "node",
                "install",
                "--control",
                "https://hub",
                "--name",
                "gpu1",
                "--home",
                "/tmp/node",
                "--key-file",
                "/tmp/enroll.key",
                "--ca-cert",
                "/tmp/ca.pem",
                "--dry-run",
            ]
        );
    }

    #[test]
    fn install_alias_does_not_confuse_hub_api_with_control_url() {
        let args = ["--hub=https://hub", "node", "install", "--name", "gpu1"].map(str::to_owned);
        assert_eq!(
            install_alias_argv(&args).unwrap().unwrap(),
            ["node", "install", "--name", "gpu1"]
        );
    }

    #[test]
    fn install_alias_rejects_missing_values_and_unknown_leading_options() {
        let missing = ["--hub", "node", "install"].map(str::to_owned);
        assert!(install_alias_argv(&missing)
            .unwrap()
            .unwrap_err()
            .contains("--hub requires a value"));
        let unknown = ["--home", "/tmp/cli", "node", "install"].map(str::to_owned);
        assert!(install_alias_argv(&unknown)
            .unwrap()
            .unwrap_err()
            .contains("unknown option before node install"));
        assert!(install_alias_argv(&["node".to_owned(), "doctor".to_owned()]).is_none());
        assert!(
            install_alias_argv(&["--json", "node", "install"].map(str::to_owned))
                .unwrap()
                .unwrap_err()
                .contains("does not support --json")
        );
        let positional = [
            "backlog".to_owned(),
            "add".to_owned(),
            "node".to_owned(),
            "install".to_owned(),
        ];
        assert!(install_alias_argv(&positional).is_none());
    }

    #[test]
    fn uninstall_alias_forwards_local_options_without_rpc_credentials() {
        let args = [
            "--token-file",
            "private",
            "--hub",
            "https://hub",
            "--timeout",
            "30",
            "node",
            "uninstall",
            "--home",
            "/tmp/node",
            "--dry-run",
        ]
        .map(str::to_owned);
        assert_eq!(
            uninstall_alias_argv(&args).unwrap().unwrap(),
            ["node", "uninstall", "--home", "/tmp/node", "--dry-run"]
        );
    }

    #[test]
    fn uninstall_alias_forwards_a_leading_home_and_dry_run() {
        let args = [
            "--home=/tmp/node",
            "--dry-run",
            "--token-file=private",
            "node",
            "uninstall",
        ]
        .map(str::to_owned);
        assert_eq!(
            uninstall_alias_argv(&args).unwrap().unwrap(),
            ["node", "uninstall", "--home", "/tmp/node", "--dry-run"]
        );
    }

    #[test]
    fn uninstall_alias_rejects_missing_values_and_unknown_leading_options() {
        let cases: &[(&[&str], &str)] = &[
            (&["--hub", "node", "uninstall"], "--hub requires a value"),
            (&["--home", "node", "uninstall"], "--home requires a value"),
            (
                &["--json", "node", "uninstall"],
                "node uninstall does not support --json",
            ),
            (
                &["--ca-cert", "/tmp/ca.pem", "node", "uninstall"],
                "unknown option before node uninstall: --ca-cert",
            ),
            (
                &["--dry-run=1", "node", "uninstall"],
                "--dry-run does not take a value",
            ),
        ];
        for (args, expected) in cases {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            let message = uninstall_alias_argv(&args).unwrap().unwrap_err();
            assert!(message.contains(expected), "{args:?}: {message}");
        }
    }

    #[test]
    fn install_runner_alias_routes_only_where_the_backend_exists() {
        let args = [
            "--token-file",
            "private",
            "node",
            "install-runner",
            "--home",
            "C:\\State",
            "--job-root",
            "C:\\ProgramData\\Protectorate-Rust\\runner-jobs",
        ]
        .map(str::to_owned);
        if cfg!(windows) {
            assert_eq!(
                install_runner_alias_argv(&args).unwrap().unwrap(),
                [
                    "node",
                    "install-runner",
                    "--home",
                    "C:\\State",
                    "--job-root",
                    "C:\\ProgramData\\Protectorate-Rust\\runner-jobs",
                ]
            );
        } else {
            let message = install_runner_alias_argv(&args).unwrap().unwrap_err();
            assert!(message.contains("supported on Windows only"), "{message}");
        }
    }

    #[test]
    fn install_runner_alias_accepts_leading_job_root() {
        let args = [
            "--job-root",
            "C:\\jobs",
            "node",
            "install-runner",
            "--home",
            "C:\\state",
        ]
        .map(str::to_owned);
        if cfg!(windows) {
            assert_eq!(
                install_runner_alias_argv(&args).unwrap().unwrap(),
                [
                    "node",
                    "install-runner",
                    "--job-root",
                    "C:\\jobs",
                    "--home",
                    "C:\\state"
                ]
            );
        } else {
            assert!(install_runner_alias_argv(&args)
                .unwrap()
                .unwrap_err()
                .contains("supported on Windows only"));
        }
    }

    #[test]
    fn node_aliases_do_not_intercept_other_commands() {
        let positional = [
            "backlog".to_owned(),
            "add".to_owned(),
            "node".to_owned(),
            "uninstall".to_owned(),
        ];
        assert!(uninstall_alias_argv(&positional).is_none());
        assert!(install_runner_alias_argv(&positional).is_none());
        let cases: &[&[&str]] = &[
            &["node", "rm", "uninstall"],
            &["node", "show", "uninstall"],
            &["node", "install", "--name", "gpu1"],
            &["node", "doctor", "--home", "/tmp/node"],
        ];
        for case in cases {
            let args: Vec<String> = case.iter().map(|arg| arg.to_string()).collect();
            assert!(uninstall_alias_argv(&args).is_none(), "{args:?}");
            assert!(install_runner_alias_argv(&args).is_none(), "{args:?}");
        }
    }

    #[test]
    fn node_serve_alias_forwards_agent_flags_without_rpc_credentials() {
        let args = [
            "--hub",
            "https://hub",
            "--token-file",
            "private",
            "--timeout",
            "30",
            "--home",
            "/tmp/node",
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/agent.token",
            "--job-concurrency",
            "2",
            "--file-unconfined",
            "--watch-dir",
            "/tmp/build-cache",
        ]
        .map(str::to_owned);
        assert_eq!(
            node_serve_alias_argv(&args).unwrap().unwrap(),
            [
                "node",
                "serve",
                "--home",
                "/tmp/node",
                "--agent-token-file",
                "/tmp/agent.token",
                "--job-concurrency",
                "2",
                "--file-unconfined",
                "--watch-dir",
                "/tmp/build-cache"
            ]
        );
        let inline = [
            "--home=/tmp/node",
            "--token-file=private",
            "node",
            "serve",
            "--listen",
            "127.0.0.1:7475",
            "--tls-cert",
            "node.pem",
            "--tls-key=key.pem",
            "--allow-plain-jobs",
            "--sandbox-helper",
            "helper",
            "--docker-host",
            "http://127.0.0.1:2375",
            "--file-root",
            "/srv/files",
        ]
        .map(str::to_owned);
        assert_eq!(
            node_serve_alias_argv(&inline).unwrap().unwrap(),
            [
                "node",
                "serve",
                "--home",
                "/tmp/node",
                "--listen",
                "127.0.0.1:7475",
                "--tls-cert",
                "node.pem",
                "--tls-key=key.pem",
                "--allow-plain-jobs",
                "--sandbox-helper",
                "helper",
                "--docker-host",
                "http://127.0.0.1:2375",
                "--file-root",
                "/srv/files"
            ]
        );
    }

    #[test]
    fn node_serve_alias_rejects_missing_values_and_unknown_leading_options() {
        let cases: &[(&[&str], &str)] = &[
            (&["--hub", "node", "serve"], "--hub requires a value"),
            (&["--home", "node", "serve"], "--home requires a value"),
            (
                &["--json", "node", "serve"],
                "node serve does not support --json",
            ),
            (
                &["--control", "https://hs", "node", "serve"],
                "unknown option before node serve: --control",
            ),
            (
                &["--home=/tmp/node", "--dry-run", "node", "serve"],
                "unknown option before node serve: --dry-run",
            ),
        ];
        for (args, expected) in cases {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            let message = node_serve_alias_argv(&args).unwrap().unwrap_err();
            assert!(message.contains(expected), "{args:?}: {message}");
        }
    }

    #[test]
    fn node_serve_alias_does_not_intercept_other_commands() {
        let cases: &[&[&str]] = &[
            &["backlog", "add", "node", "serve"],
            &["node", "install", "serve"],
            &["node", "show", "serve"],
            &["node", "put", "serve", "/local", "/remote"],
            &["node", "uninstall"],
        ];
        for case in cases {
            let args: Vec<String> = case.iter().map(|arg| arg.to_string()).collect();
            assert!(node_serve_alias_argv(&args).is_none(), "{args:?}");
        }
    }

    #[test]
    fn companion_name_follows_the_platform() {
        let expected = if cfg!(windows) {
            "protectorate-rs.exe"
        } else {
            "protectorate-rs"
        };
        assert_eq!(companion_name(), expected);
    }

    #[test]
    fn run_companion_reports_a_missing_companion_as_failure() {
        let missing = Path::new("protectorate-rs-missing-companion-does-not-exist");
        let code = run_companion(missing, &["node".to_owned(), "install".to_owned()]);
        assert_eq!(code, std::process::ExitCode::FAILURE);
    }
}
