//! argv parsing. Global options are accepted before or after the command, the
//! way the Go CLI's persistent flags behave.
use crate::{
    AuditQuery, BacklogFilter, ChatSettings, Command, HarnessName, JobFilter, JobLogs, JobRun,
    JobState, LeaseAcquire, MessageKind, ModelChoice, NoteDraft, NoteEdit, NoteState, ReadOnlyBind,
    SessionNew, SleeveEnvoy, SleeveModeName, SleevePrune, SleeveResleeve, SleeveSpawn, WaitUntil,
    DEFAULT_ENVOY_TTL, DEFAULT_JOB_TIMEOUT, DEFAULT_TIMEOUT, MAX_JOB_TIMEOUT, MAX_WAIT,
};
use protectorate_wire::{Result, RpcError};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub command: Command,
    pub json: bool,
    /// `--hub`; unset means the environment or the default.
    pub hub: Option<String>,
    pub token_file: Option<PathBuf>,
    /// The state home holding this machine's saved device credential.
    pub home: Option<PathBuf>,
    pub timeout: Duration,
}

pub fn parse<A: AsRef<str>>(args: impl IntoIterator<Item = A>) -> Result<Invocation> {
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect();
    match parse_tokens(&args) {
        Ok(invocation) => Ok(invocation),
        Err(original) => {
            // A local option parser stops when it reaches a persistent flag.
            // Retry with those flags in front of the command, so local options
            // on both sides of a persistent flag remain visible to it.
            let reordered = move_global_options(&args);
            if reordered == args {
                Err(original)
            } else {
                parse_tokens(&reordered)
            }
        }
    }
}

fn parse_tokens(args: &[String]) -> Result<Invocation> {
    if let Some(command) = help_command(args) {
        return Ok(Invocation {
            command,
            json: false,
            hub: None,
            token_file: None,
            home: None,
            timeout: DEFAULT_TIMEOUT,
        });
    }
    let mut invocation = Invocation {
        command: Command::Help,
        json: false,
        hub: None,
        token_file: None,
        home: None,
        timeout: DEFAULT_TIMEOUT,
    };
    let mut command: Option<Command> = None;
    let mut index = 0;
    while index < args.len() {
        let (name, inline) = match args[index].split_once('=') {
            Some((head, tail)) => (head, Some(tail.to_owned())),
            None => (args[index].as_str(), None),
        };
        match name {
            "--help" | "-h" => set(&mut command, Command::Help)?,
            "--json" => invocation.json = true,
            "--hub" => invocation.hub = Some(value(args, &mut index, "--hub", inline)?),
            "--token-file" => {
                invocation.token_file = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--token-file",
                    inline,
                )?))
            }
            "--home" => {
                invocation.home = Some(PathBuf::from(value(args, &mut index, "--home", inline)?))
            }
            "--timeout" => {
                invocation.timeout =
                    Duration::from_secs(seconds(&value(args, &mut index, "--timeout", inline)?)?)
            }
            "status" => set(&mut command, Command::Status)?,
            "hub" => set(&mut command, hub(args, &mut index)?)?,
            "version" => set(&mut command, Command::Version)?,
            "api" => set(&mut command, api(args, &mut index)?)?,
            "tailnet" => {
                if next(args, index + 1) != Some("status") {
                    return Err(bad_argument("tailnet requires status"));
                }
                index += 1;
                set(&mut command, Command::TailnetStatus)?;
            }
            "quota" => set(&mut command, quota(args, &mut index)?)?,
            "token" => set(&mut command, token(args, &mut index)?)?,
            "grant" => set(&mut command, grant(args, &mut index)?)?,
            "audit" => set(&mut command, audit(args, &mut index)?)?,
            "devices" => set(&mut command, devices(args, &mut index)?)?,
            "login" => set(&mut command, login(args, &mut index)?)?,
            "pair" => set(&mut command, pair(args, &mut index)?)?,
            "who" => {
                let target = match next(args, index + 1) {
                    Some(next) if !next.starts_with('-') => {
                        index += 1;
                        Some(next.to_owned())
                    }
                    _ => None,
                };
                set(&mut command, Command::Who { target })?;
            }
            "events" => set(&mut command, Command::Events)?,
            "node" => set(&mut command, node(args, &mut index, &mut invocation.json)?)?,
            "backlog" => set(&mut command, backlog(args, &mut index)?)?,
            "lease" | "leases" => set(&mut command, lease(args, &mut index)?)?,
            "job" | "jobs" => set(&mut command, job(args, &mut index, &mut invocation.json)?)?,
            "chat" => set(&mut command, chat(args, &mut index)?)?,
            "report" => set(&mut command, report(args, &mut index)?)?,
            "sleeve" | "sleeves" => set(&mut command, sleeve(args, &mut index)?)?,
            "orchestrator" => set(&mut command, orchestrator(args, &mut index)?)?,
            "session" | "sessions" => set(
                &mut command,
                session(args, &mut index, &mut invocation.json)?,
            )?,
            "msg" | "message" => set(&mut command, msg(args, &mut index)?)?,
            other => return Err(bad_argument(other)),
        }
        index += 1;
    }
    invocation.command = command.ok_or_else(|| bad_argument("<none>"))?;
    Ok(invocation)
}

/// Move unambiguous persistent options ahead of the command. This is only
/// used after a normal parse fails: local flags with the same spelling (such
/// as `job run --timeout`) keep their established meaning on the first pass.
fn move_global_options(args: &[String]) -> Vec<String> {
    let mut globals = Vec::new();
    let mut rest = Vec::new();
    let mut at = 0;
    let mut command_path: Vec<String> = Vec::new();
    while at < args.len() {
        let token = &args[at];
        if token == "--" {
            rest.extend_from_slice(&args[at..]);
            break;
        }
        let (flag, inline) = option(token);
        let local_collision = (flag == "--home"
            && command_path.first().is_some_and(|s| s == "login"))
            || (flag == "--timeout"
                && matches!(command_path.as_slice(), [first, second, ..]
                    if matches!(first.as_str(), "job" | "jobs")
                        && matches!(second.as_str(), "run" | "test-cross")
                    || matches!(first.as_str(), "chat" | "msg" | "message")
                        && second == "wait"));
        if !local_collision
            && matches!(
                flag,
                "--json" | "--hub" | "--token-file" | "--home" | "--timeout"
            )
        {
            globals.push(token.clone());
            if flag != "--json" && inline.is_none() && at + 1 < args.len() {
                at += 1;
                globals.push(args[at].clone());
            }
        } else {
            rest.push(token.clone());
            if !token.starts_with('-') {
                command_path.push(token.clone());
            } else if inline.is_none() && !is_boolean_option(flag) && at + 1 < args.len() {
                // A flag's value may itself look like a persistent option.
                // Leave that pair to the local parser.
                at += 1;
                rest.push(args[at].clone());
            }
        }
        at += 1;
    }
    globals.extend(rest);
    globals
}

fn is_boolean_option(flag: &str) -> bool {
    matches!(
        flag,
        "--help"
            | "-h"
            | "--app"
            | "--refresh"
            | "--double"
            | "--lock-after"
            | "--off"
            | "--local"
            | "--uninstall"
            | "--all"
            | "--force"
            | "-f"
            | "--no-wait"
            | "--no-compress"
            | "--pin"
            | "--wait"
            | "--admin"
            | "--allow"
            | "--deny"
            | "--native-defaults"
            | "--readonly"
            | "--drop-caches"
            | "--supervised"
            | "--nested"
            | "--interrupt"
            | "--yes"
            | "--caches"
            | "--homes"
            | "--workspaces"
            | "--ignored"
            | "--running"
            | "--ack"
            | "--unread"
            | "--peek"
            | "--follow"
    )
}

fn help_command(args: &[String]) -> Option<Command> {
    let mut words = Vec::new();
    let mut requested = false;
    let mut at = 0;
    while at < args.len() {
        let arg = args[at].as_str();
        if arg == "--" {
            break;
        }
        if matches!(arg, "--help" | "-h") {
            requested = true;
            break;
        }
        if arg == "help" && words.is_empty() {
            requested = true;
            at += 1;
            while let Some(next) = args.get(at) {
                if next.starts_with('-') {
                    break;
                }
                words.push(next.as_str());
                at += 1;
            }
            break;
        }
        if matches!(arg, "--hub" | "--token-file" | "--home" | "--timeout") {
            at += 2;
            continue;
        }
        if arg.starts_with('-') {
            at += 1;
            continue;
        }
        words.push(arg);
        at += 1;
    }
    if !requested {
        return None;
    }
    if words.is_empty() {
        return Some(Command::Help);
    }
    // Positional arguments and flag values can follow a command path. The
    // longest prefix named in the command catalog is the requested topic.
    for len in (1..=words.len()).rev() {
        let path = words[..len].join(" ");
        if crate::usage().lines().any(|line| {
            let entry = line.trim_start();
            entry.starts_with(&path)
                && entry[path.len()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
        }) {
            return Some(Command::HelpPath(path));
        }
    }
    Some(Command::HelpPath(words.join(" ")))
}

fn orchestrator(args: &[String], index: &mut usize) -> Result<Command> {
    match next(args, *index + 1) {
        Some("status") => {
            *index += 1;
            Ok(Command::OrchestratorStatus)
        }
        Some("spawn") => {
            *index += 1;
            sleeve_envoy(args, index)
        }
        Some("scopes") => {
            *index += 1;
            let mut add = Vec::new();
            let mut remove = Vec::new();
            while let Some(item) = next(args, *index + 1) {
                let (flag, inline) = option(item);
                match flag {
                    "--add" | "--remove" => {
                        *index += 1;
                        let values = split_scopes(&value(args, index, flag, inline)?);
                        if values.is_empty() {
                            return Err(bad_argument(format!("{flag} needs scopes")));
                        }
                        if flag == "--add" {
                            add.extend(values);
                        } else {
                            remove.extend(values);
                        }
                    }
                    "--json" | "--hub" | "--token-file" | "--home" | "--timeout" => break,
                    _ => {
                        return Err(bad_argument(format!(
                            "unknown orchestrator scopes argument {item:?}"
                        )))
                    }
                }
            }
            if add.is_empty() && remove.is_empty() {
                return Err(bad_argument(
                    "orchestrator scopes requires --add or --remove",
                ));
            }
            Ok(Command::OrchestratorScopes { add, remove })
        }
        _ => Err(bad_argument("orchestrator needs spawn, status or scopes")),
    }
}

fn set(slot: &mut Option<Command>, command: Command) -> Result<()> {
    if slot.is_some() {
        return Err(bad_argument("two commands; run one at a time"));
    }
    *slot = Some(command);
    Ok(())
}

fn hub(args: &[String], index: &mut usize) -> Result<Command> {
    if next(args, *index + 1) != Some("update") {
        return Err(bad_argument("hub needs update"));
    }
    *index += 1;
    let mut channel = String::new();
    let mut version = String::new();
    while let Some(item) = next(args, *index + 1) {
        let (flag, inline) = option(item);
        match flag {
            "--channel" => {
                *index += 1;
                channel = value(args, index, flag, inline)?;
                if !matches!(channel.as_str(), "stable" | "dev") {
                    return Err(bad_argument("--channel must be stable or dev"));
                }
            }
            "--version" => {
                *index += 1;
                version = value(args, index, flag, inline)?;
            }
            "--json" | "--hub" | "--token-file" | "--home" | "--timeout" => break,
            _ => {
                return Err(bad_argument(format!(
                    "unknown hub update argument {item:?}"
                )))
            }
        }
    }
    Ok(Command::HubUpdate { channel, version })
}

/// `api call <Service/Method> [json]`: one raw unary call. An omitted body is
/// `{}`. A service without a package gets the `protectorate.v1.` prefix, the
/// way the Go CLI spells it.
fn api(args: &[String], index: &mut usize) -> Result<Command> {
    if next(args, *index + 1) != Some("call") {
        return Err(bad_argument("api needs call <Service/Method> [json]"));
    }
    *index += 1;
    let method = argument(args, index, "api call <Service/Method> [json]")?;
    let (service, name) = method
        .split_once('/')
        .ok_or_else(|| bad_argument("method must look like Service/Method"))?;
    let service = if service.contains('.') {
        service.to_owned()
    } else {
        format!("protectorate.v1.{service}")
    };
    if !valid_proto_name(&service) || !valid_proto_name(name) {
        return Err(bad_argument("method must look like Service/Method"));
    }
    let body = match next(args, *index + 1) {
        Some(token) if !token.starts_with('-') => {
            *index += 1;
            token.to_owned()
        }
        _ => "{}".to_owned(),
    };
    serde_json::from_str::<Value>(&body)
        .map_err(|error| bad_argument(format!("api call request JSON is invalid: {error}")))?;
    Ok(Command::ApiCall {
        procedure: format!("/{service}/{name}"),
        body,
    })
}

/// A protobuf service or method name: letters, digits and underscores, never
/// starting with a digit, each dotted service segment the same.
fn valid_proto_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.split('.').all(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) if first.is_ascii_alphabetic() || first == '_' => {
                    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                }
                _ => false,
            }
        })
}

fn quota(args: &[String], index: &mut usize) -> Result<Command> {
    let mut refresh = false;
    let mut suggest = false;
    let mut task_size = None;
    while let Some(token) = next(args, *index + 1) {
        let (flag, inline) = option(token);
        match flag {
            "suggest" if !suggest => {
                suggest = true;
                *index += 1;
            }
            "--refresh" => {
                refresh = bool_flag(flag, inline)?;
                *index += 1;
            }
            "--for" if suggest => {
                *index += 1;
                let size = value(args, index, "--for", inline)?;
                if !matches!(size.as_str(), "small" | "medium" | "large") {
                    return Err(bad_argument("--for must be small, medium or large"));
                }
                task_size = Some(size);
            }
            "--json" | "--hub" | "--token-file" | "--home" | "--timeout" => break,
            _ if token.starts_with('-') || suggest => return Err(bad_argument(token)),
            _ => break,
        }
    }
    if suggest {
        Ok(Command::QuotaSuggest { task_size, refresh })
    } else {
        Ok(Command::Quota { refresh })
    }
}

fn grant(args: &[String], index: &mut usize) -> Result<Command> {
    if next(args, *index + 1) != Some("admin") {
        return Err(bad_argument("grant requires admin"));
    }
    *index += 1;
    let mut node = String::new();
    let mut task = String::new();
    let mut ttl = Duration::from_secs(4 * 3600);
    let mut out = None;
    while let Some(item) = next(args, *index + 1) {
        let (flag, inline) = option(item);
        match flag {
            "--node" => {
                *index += 1;
                node = value(args, index, flag, inline)?;
            }
            "--task" => {
                *index += 1;
                task = value(args, index, flag, inline)?;
            }
            "--ttl" => {
                *index += 1;
                ttl = go_duration(flag, &value(args, index, flag, inline)?)?;
            }
            "--out" => {
                *index += 1;
                out = Some(PathBuf::from(value(args, index, flag, inline)?));
            }
            _ => break,
        }
    }
    if node.trim().is_empty() || task.trim().is_empty() {
        return Err(bad_argument("grant admin requires --node and --task"));
    }
    if ttl.is_zero() || ttl > Duration::from_secs(24 * 3600) {
        return Err(bad_argument("--ttl must be between 1s and 24h"));
    }
    Ok(Command::GrantAdmin {
        node,
        task,
        ttl,
        out,
    })
}

fn audit(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| bad_argument("audit needs ls or tail"))?;
    if !matches!(action, "ls" | "tail") {
        return Err(bad_argument("audit needs ls or tail"));
    }
    *index += 1;
    let mut query = AuditQuery {
        limit: 50,
        page_token: String::new(),
        actor: String::new(),
        procedure: String::new(),
        subject: String::new(),
        outcome: String::new(),
        chronological: action == "tail",
    };
    while let Some(item) = next(args, *index + 1) {
        let (flag, inline) = option(item);
        match flag {
            "--lines" | "-n" | "--limit" => {
                *index += 1;
                query.limit = value(args, index, flag, inline)?
                    .parse()
                    .map_err(|_| bad_argument("audit lines must be 1..500"))?;
            }
            "--page-token" => {
                *index += 1;
                query.page_token = value(args, index, flag, inline)?;
            }
            "--actor" => {
                *index += 1;
                query.actor = value(args, index, flag, inline)?;
            }
            "--procedure" => {
                *index += 1;
                query.procedure = value(args, index, flag, inline)?;
            }
            "--subject" => {
                *index += 1;
                query.subject = value(args, index, flag, inline)?;
            }
            "--outcome" => {
                *index += 1;
                query.outcome = value(args, index, flag, inline)?;
            }
            _ => break,
        }
    }
    if !(1..=500).contains(&query.limit) {
        return Err(bad_argument("audit lines must be 1..500"));
    }
    if !matches!(query.outcome.as_str(), "" | "ok" | "denied" | "error") {
        return Err(bad_argument("--outcome must be ok, denied or error"));
    }
    Ok(Command::AuditList(query))
}

/// Trusted-device management: `devices ls`, `pending`, `approve <code>`,
/// `deny <code>`, `revoke <id>` and `rename <id> <name>`.
fn devices(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument("devices needs ls, pending, approve <code>, deny <code>, revoke <id> or rename <id> <name>")
    })?;
    *index += 1;
    match action {
        "ls" | "list" => Ok(Command::DeviceList),
        "pending" => Ok(Command::DevicePendingList),
        "approve" => Ok(Command::DeviceApprove {
            code: argument(args, index, "devices approve <code>")?,
        }),
        "deny" => Ok(Command::DeviceDeny {
            code: argument(args, index, "devices deny <code>")?,
        }),
        "revoke" => Ok(Command::DeviceRevoke {
            id: argument(args, index, "devices revoke <id>")?,
        }),
        "rename" => {
            let id = argument(args, index, "devices rename <id> <name>")?;
            let name = argument(args, index, "devices rename <id> <name>")?;
            if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
                return Err(bad_argument(
                    "device name must be 1-100 characters without control characters",
                ));
            }
            Ok(Command::DeviceRename { id, name })
        }
        other => Err(bad_argument(format!(
            "unknown devices subcommand {other:?}; use ls, pending, approve <code>, deny <code>, revoke <id> or rename <id> <name>"
        ))),
    }
}

/// `login` with no harness: pair this machine with the hub through device
/// approval. Harness login runs on the hub machine, so a harness name here is
/// a usage error rather than a second implementation.
fn login(args: &[String], index: &mut usize) -> Result<Command> {
    let mut name = None;
    let mut platform = None;
    let mut home = None;
    while let Some(item) = next(args, *index + 1) {
        let (flag, inline) = option(item);
        match flag {
            "--name" => {
                *index += 1;
                name = Some(value(args, index, flag, inline)?);
            }
            "--platform" => {
                *index += 1;
                platform = Some(value(args, index, flag, inline)?);
            }
            "--home" => {
                *index += 1;
                home = Some(PathBuf::from(value(args, index, flag, inline)?));
            }
            "--json" | "--hub" | "--token-file" | "--timeout" => break,
            _ if item.starts_with('-') => {
                return Err(bad_argument(format!(
                    "unknown login flag {item:?}; use --name, --platform or --home"
                )))
            }
            _ => {
                return Err(bad_argument(format!(
                    "login {item} runs on the hub machine; login with no harness connects this device to the hub"
                )))
            }
        }
    }
    if name.as_deref().is_some_and(|name| !valid_label(name, 100)) {
        return Err(bad_argument(
            "--name must be 1-100 characters without control characters",
        ));
    }
    if platform
        .as_deref()
        .is_some_and(|platform| !valid_label(platform, 40))
    {
        return Err(bad_argument(
            "--platform must be 1-40 characters without control characters",
        ));
    }
    Ok(Command::DeviceLogin {
        name,
        platform,
        home,
    })
}

fn pair(args: &[String], index: &mut usize) -> Result<Command> {
    let mut url = None;
    let mut app = false;
    while let Some(item) = next(args, *index + 1) {
        let (flag, inline) = option(item);
        match flag {
            "--url" => {
                *index += 1;
                url = Some(value(args, index, flag, inline)?);
            }
            "--app" => {
                *index += 1;
                app = bool_flag(flag, inline)?;
            }
            "--json" | "--hub" | "--token-file" | "--home" | "--timeout" => break,
            _ => return Err(bad_argument(format!("unknown pair argument {item:?}"))),
        }
    }
    Ok(Command::Pair { url, app })
}

fn valid_label(text: &str, max: usize) -> bool {
    !text.is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

fn token(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1)
        .ok_or_else(|| bad_argument("token needs create, ls, revoke or scopes"))?;
    *index += 1;
    match action {
        "ls" | "list" => Ok(Command::TokenList),
        "create" => {
            let mut name = None;
            let mut scopes = Vec::new();
            let mut ttl = Duration::from_secs(720 * 3600);
            while let Some(item) = next(args, *index + 1) {
                let (flag, inline) = option(item);
                match flag {
                    "--scope" => {
                        *index += 1;
                        scopes.push(value(args, index, flag, inline)?);
                    }
                    "--ttl" => {
                        *index += 1;
                        ttl = go_duration(flag, &value(args, index, flag, inline)?)?;
                    }
                    _ if item.starts_with('-') => break,
                    _ if name.is_none() => {
                        name = Some(item.to_owned());
                        *index += 1;
                    }
                    _ => return Err(bad_argument("token create accepts one name")),
                }
            }
            let name = name.ok_or_else(|| bad_argument("token create <name>"))?;
            if scopes.is_empty() || scopes.iter().any(String::is_empty) {
                return Err(bad_argument("token create requires at least one --scope"));
            }
            Ok(Command::TokenCreate { name, scopes, ttl })
        }
        "revoke" => Ok(Command::TokenRevoke {
            id: argument(args, index, "token revoke <id>")?,
        }),
        "scopes" => {
            let mut id = None;
            let mut add = Vec::new();
            let mut remove = Vec::new();
            while let Some(item) = next(args, *index + 1) {
                let (flag, inline) = option(item);
                match flag {
                    "--add" | "--remove" => {
                        *index += 1;
                        let values = split_scopes(&value(args, index, flag, inline)?);
                        if values.is_empty() {
                            return Err(bad_argument(format!("{flag} needs scopes")));
                        }
                        if flag == "--add" {
                            add.extend(values);
                        } else {
                            remove.extend(values);
                        }
                    }
                    _ if item.starts_with('-') => break,
                    _ if id.is_none() => {
                        id = Some(item.to_owned());
                        *index += 1;
                    }
                    _ => return Err(bad_argument("token scopes accepts one id or name")),
                }
            }
            let id = id.ok_or_else(|| bad_argument("token scopes <id|name>"))?;
            if add.is_empty() && remove.is_empty() {
                return Err(bad_argument("token scopes requires --add or --remove"));
            }
            Ok(Command::TokenScopes { id, add, remove })
        }
        other => Err(bad_argument(format!(
            "unknown token subcommand {other:?}; use create, ls, revoke or scopes"
        ))),
    }
}

fn node(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument("node needs ls, show, metrics, add, rm, tier, update, invite, put or get")
    })?;
    *index += 1;
    match action {
        "ls" | "list" => Ok(Command::NodeList),
        "show" => Ok(Command::NodeShow {
            name: argument(args, index, "node show <name>")?,
        }),
        "metrics" => {
            let mut local = false;
            let mut watch = Vec::new();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--json" if inline.is_none() => {
                        *json = true;
                        *index += 1;
                    }
                    "--local" => {
                        local = bool_flag(flag, inline)?;
                        *index += 1;
                    }
                    "--watch" => {
                        *index += 1;
                        watch.push(PathBuf::from(value(args, index, flag, inline)?));
                    }
                    _ => break,
                }
            }
            if !local {
                return Err(bad_argument("node metrics requires --local"));
            }
            Ok(Command::NodeMetricsLocal { watch })
        }
        "add" => {
            let mut name = None;
            let mut address = String::new();
            let mut ssh_alias = String::new();
            let mut tier = "shared".to_owned();
            let mut tags = Vec::new();
            let mut notes = String::new();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                let slot = match flag {
                    "--addr" => &mut address,
                    "--ssh" => &mut ssh_alias,
                    "--tier" => &mut tier,
                    "--notes" => &mut notes,
                    "--tag" => {
                        *index += 1;
                        tags.extend(csv_flag_values(flag, &value(args, index, flag, inline)?)?);
                        continue;
                    }
                    _ if token.starts_with('-') => break,
                    _ if name.is_none() => { name = Some(token.to_owned()); *index += 1; continue; }
                    _ => return Err(bad_argument("node add accepts one name")),
                };
                *index += 1;
                *slot = value(args, index, flag, inline)?;
            }
            let name = name.ok_or_else(|| bad_argument("node add <name> --addr HOST"))?;
            if address.is_empty() { return Err(bad_argument("node add requires --addr HOST")); }
            if !matches!(tier.as_str(), "shared" | "care") { return Err(bad_argument("--tier must be shared or care")); }
            Ok(Command::NodeAdd { name, address, ssh_alias, tier, tags, notes })
        }
        "rm" => {
            let mut name = None;
            let mut jobs = "REMOVE_JOBS_UNSPECIFIED".to_owned();
            let mut uninstall = false;
            let mut wait_timeout = None;
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--jobs" => {
                        *index += 1;
                        jobs = match value(args, index, flag, inline)?.as_str() {
                            "wait" => "REMOVE_JOBS_WAIT",
                            "cancel" => "REMOVE_JOBS_CANCEL",
                            _ => return Err(bad_argument("--jobs must be wait or cancel")),
                        }.to_owned();
                    }
                    "--wait-timeout" => {
                        *index += 1;
                        let wait = go_duration(flag, &value(args, index, flag, inline)?)?;
                        if wait.is_zero() { return Err(bad_argument("--wait-timeout must be positive")); }
                        wait_timeout = Some(wait);
                    }
                    "--uninstall" => { uninstall = bool_flag(flag, inline)?; *index += 1; }
                    _ if token.starts_with('-') => break,
                    _ if name.is_none() => { name = Some(token.to_owned()); *index += 1; }
                    _ => return Err(bad_argument("node rm accepts one name")),
                }
            }
            let name = name.ok_or_else(|| bad_argument("node rm <name>"))?;
            Ok(Command::NodeRemove { name, jobs, uninstall, wait_timeout })
        }
        "tier" => {
            let name = argument(args, index, "node tier <name> shared|care")?;
            let tier = argument(args, index, "node tier <name> shared|care")?;
            if !matches!(tier.as_str(), "shared" | "care") { return Err(bad_argument("tier must be shared or care")); }
            Ok(Command::NodeTier { name, tier })
        }
        "update" => {
            let mut name = None;
            let mut all = false;
            let mut force = false;
            let mut no_wait = false;
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--all" => { all = bool_flag(flag, inline)?; *index += 1; }
                    "--force" => { force = bool_flag(flag, inline)?; *index += 1; }
                    "--no-wait" => { no_wait = bool_flag(flag, inline)?; *index += 1; }
                    _ if token.starts_with('-') => break,
                    _ if name.is_none() => { name = Some(token.to_owned()); *index += 1; }
                    _ => return Err(bad_argument("node update accepts one name or --all")),
                }
            }
            if all == name.is_some() { return Err(bad_argument("name one node, or pass --all")); }
            Ok(Command::NodeUpdate { name, all, force, no_wait })
        }
        "invite" => {
            let name = argument(args, index, "node invite <name>")?;
            let valid_name = name.len() <= 63
                && !name.is_empty()
                && !name.starts_with('-')
                && !name.ends_with('-')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
            if !valid_name {
                return Err(bad_argument(
                    "node invite name must be a lowercase DNS label of at most 63 characters",
                ));
            }
            let mut tier = "shared".to_owned();
            let mut pools = Vec::new();
            let mut ttl = Duration::from_secs(3600);
            let mut install_url = String::new();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match flag {
                    "--tier" => {
                        *index += 1;
                        tier = value(args, index, flag, inline)?;
                    }
                    "--pool" => {
                        *index += 1;
                        pools.extend(csv_flag_values(flag, &value(args, index, flag, inline)?)?);
                    }
                    "--expires" => {
                        *index += 1;
                        ttl = go_duration(flag, &value(args, index, flag, inline)?)?;
                    }
                    "--url" => {
                        *index += 1;
                        install_url = value(args, index, flag, inline)?;
                    }
                    _ => break,
                }
            }
            if ttl.is_zero() || ttl > Duration::from_secs(24 * 3600) {
                return Err(bad_argument("--expires must be between 1s and 24h"));
            }
            if ttl.subsec_nanos() != 0 {
                return Err(bad_argument("--expires must be whole seconds"));
            }
            Ok(Command::NodeInvite {
                name,
                tier,
                pools,
                ttl,
                install_url,
            })
        }
        "put" | "get" => {
            let mut force = false;
            let mut no_compress = false;
            let mut positional = Vec::with_capacity(3);
            while positional.len() < 3 {
                let token = value(
                    args,
                    index,
                    "node put/get <node> <source> <destination>",
                    None,
                )?;
                match token.as_str() {
                    "--force" | "-f" => force = true,
                    "--no-compress" => no_compress = true,
                    _ if token.starts_with('-') => return Err(bad_argument(token)),
                    _ => positional.push(token),
                }
            }
            while let Some(token) = next(args, *index + 1) {
                match token {
                    "--force" | "-f" => force = true,
                    "--no-compress" => no_compress = true,
                    _ => break,
                }
                *index += 1;
            }
            let mut positional = positional.into_iter();
            let name = positional.next().unwrap();
            let source = positional.next().unwrap();
            let destination = positional.next().unwrap();
            if action == "put" {
                Ok(Command::NodePut {
                    name,
                    local: source.into(),
                    remote: destination,
                    force,
                    no_compress,
                })
            } else {
                Ok(Command::NodeGet {
                    name,
                    remote: source,
                    local: destination.into(),
                    force,
                    no_compress,
                })
            }
        }
        other => Err(bad_argument(format!(
            "unknown node subcommand {other:?}; use ls, show, metrics, add, rm, tier, update, invite, put or get"
        ))),
    }
}

fn backlog(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument("backlog needs ls, show, add, edit, rm, pin, done or reopen")
    })?;
    *index += 1;
    match action {
        "ls" | "list" => {
            let mut filter = BacklogFilter::default();
            let mut all = false;
            let mut state_given = false;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match name {
                    "--project" => {
                        *index += 1;
                        filter.project = value(args, index, "--project", inline)?
                    }
                    "--query" => {
                        *index += 1;
                        filter.query = value(args, index, "--query", inline)?
                    }
                    "--state" => {
                        *index += 1;
                        state_given = true;
                        filter.state = match value(args, index, "--state", inline)?.as_str() {
                            "open" => NoteState::Open,
                            "done" => NoteState::Done,
                            "all" => NoteState::All,
                            other => {
                                return Err(bad_argument(format!(
                                    "state must be open, done or all, not {other:?}"
                                )))
                            }
                        }
                    }
                    "--all" => {
                        *index += 1;
                        all = true;
                    }
                    _ => break,
                }
            }
            if all {
                if state_given {
                    return Err(bad_argument("use --all or --state, not both"));
                }
                filter.state = NoteState::All;
            }
            Ok(Command::BacklogList(filter))
        }
        "show" => Ok(Command::BacklogShow {
            id: argument(args, index, "backlog show <id>")?,
        }),
        "add" => {
            let mut draft = NoteDraft::default();
            let mut title_given = false;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match name {
                    "--project" => {
                        *index += 1;
                        draft.project = value(args, index, "--project", inline)?
                    }
                    "--pin" => {
                        *index += 1;
                        draft.pinned = bool_flag("--pin", inline)?
                    }
                    "--body" => {
                        *index += 1;
                        draft.body = Some(value(args, index, "--body", inline)?)
                    }
                    "--body-file" => {
                        *index += 1;
                        draft.body_file = Some(value(args, index, "--body-file", inline)?)
                    }
                    _ if token.starts_with('-') => break,
                    _ if title_given => {
                        return Err(bad_argument("backlog add accepts at most one title"))
                    }
                    _ => {
                        *index += 1;
                        draft.title = token.to_owned();
                        title_given = true;
                    }
                }
            }
            if draft.body.is_some() && draft.body_file.is_some() {
                return Err(bad_argument("use --body or --body-file, not both"));
            }
            Ok(Command::BacklogAdd(draft))
        }
        "edit" => {
            let mut edit = NoteEdit {
                id: String::new(),
                title: None,
                project: None,
                body: None,
                body_file: None,
                append: None,
            };
            let mut ids = Vec::new();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                let slot = match flag {
                    "--title" => &mut edit.title,
                    "--project" => &mut edit.project,
                    "--body" => &mut edit.body,
                    "--body-file" => &mut edit.body_file,
                    "--append" => &mut edit.append,
                    _ if token.starts_with('-') => break,
                    _ => {
                        *index += 1;
                        ids.push(token.to_owned());
                        continue;
                    }
                };
                *index += 1;
                *slot = Some(value(args, index, flag, inline)?);
            }
            if ids.len() != 1 {
                return Err(bad_argument("backlog edit <id>"));
            }
            edit.id = ids.remove(0);
            if edit.body.is_some() && edit.body_file.is_some() {
                return Err(bad_argument("use --body or --body-file, not both"));
            }
            if edit.append.is_some() && (edit.body.is_some() || edit.body_file.is_some()) {
                return Err(bad_argument("use --body or --append, not both"));
            }
            Ok(Command::BacklogEdit(edit))
        }
        "rm" => Ok(Command::BacklogRemove {
            id: argument(args, index, "backlog rm <id>")?,
        }),
        "pin" => {
            let id = argument(args, index, "backlog pin <id>")?;
            let mut off = false;
            if let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                if flag == "--off" {
                    *index += 1;
                    off = bool_flag(flag, inline)?;
                }
            }
            Ok(Command::BacklogPin { id, pinned: !off })
        }
        "done" => {
            let id = argument(args, index, "backlog done <id>")?;
            let mut resolution = None;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match name {
                    "--resolution" => {
                        *index += 1;
                        resolution = Some(value(args, index, "--resolution", inline)?)
                    }
                    _ => break,
                }
            }
            Ok(Command::BacklogDone { id, resolution })
        }
        "reopen" => Ok(Command::BacklogReopen {
            id: argument(args, index, "backlog reopen <id>")?,
        }),
        other => Err(bad_argument(format!(
            "unknown backlog subcommand {other:?}; use ls, show, add, edit, rm, pin, done or reopen"
        ))),
    }
}

fn lease(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1)
        .ok_or_else(|| bad_argument("lease needs acquire, ls, renew or release"))?;
    *index += 1;
    match action {
        "acquire" => {
            let mut spec = LeaseAcquire {
                resource: String::new(),
                os: String::new(),
                arch: String::new(),
                min_free_memory_bytes: 0,
                gpu_vram_bytes: 0,
                gpu: String::new(),
                npu: String::new(),
                holder: default_holder(),
                ttl: Duration::from_secs(30 * 60),
                wait: false,
                priority: 0,
            };
            let mut selector_given = false;
            while let Some(item) = next(args, *index + 1) {
                let (flag, inline) = option(item);
                match flag {
                    "--node" => {
                        *index += 1;
                        spec.resource = value(args, index, flag, inline)?;
                    }
                    "--os" | "--arch" | "--gpu" | "--npu" | "--min-free-mem" | "--gpu-vram" => {
                        *index += 1;
                        let given = value(args, index, flag, inline)?;
                        selector_given = true;
                        match flag {
                            "--os" => spec.os = given,
                            "--arch" => spec.arch = given,
                            "--gpu" => spec.gpu = given,
                            "--npu" => spec.npu = given,
                            "--min-free-mem" => spec.min_free_memory_bytes = parse_size(&given)?,
                            "--gpu-vram" => spec.gpu_vram_bytes = parse_size(&given)?,
                            _ => unreachable!(),
                        }
                    }
                    "--holder" => {
                        *index += 1;
                        spec.holder = value(args, index, flag, inline)?;
                    }
                    "--ttl" => {
                        *index += 1;
                        spec.ttl = go_duration(flag, &value(args, index, flag, inline)?)?;
                    }
                    "--wait" => {
                        spec.wait = bool_flag(flag, inline)?;
                        *index += 1;
                    }
                    "--priority" => {
                        *index += 1;
                        spec.priority =
                            value(args, index, flag, inline)?.parse().map_err(|_| {
                                bad_argument("--priority needs a signed 32-bit integer")
                            })?;
                    }
                    _ => break,
                }
            }
            if !spec.resource.is_empty() && selector_given {
                return Err(bad_argument("--node and selector flags are exclusive"));
            }
            if spec.holder.trim().is_empty() {
                return Err(bad_argument("--holder must not be empty"));
            }
            Ok(Command::LeaseAcquire(spec))
        }
        "ls" | "list" => Ok(Command::LeaseList),
        "renew" => {
            let mut id = None;
            let mut ttl = None;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = match token.split_once('=') {
                    Some((name, value)) => (name, Some(value.to_owned())),
                    None => (token, None),
                };
                if name == "--ttl" {
                    *index += 1;
                    ttl = Some(wait_timeout(&value(args, index, "--ttl", inline)?)?);
                    continue;
                }
                if token.starts_with('-') {
                    break;
                }
                if id.is_some() {
                    return Err(bad_argument("lease renew <id>"));
                }
                *index += 1;
                id = Some(token.to_owned());
            }
            let id = id.ok_or_else(|| bad_argument("lease renew <id>"))?;
            Ok(Command::LeaseRenew { id, ttl })
        }
        "release" => {
            let mut ids = Vec::new();
            while let Some(token) = next(args, *index + 1) {
                if token.starts_with('-') {
                    break;
                }
                *index += 1;
                ids.push(token.to_owned());
            }
            if ids.is_empty() {
                return Err(bad_argument("lease release needs at least one id"));
            }
            Ok(Command::LeaseRelease { ids })
        }
        other => Err(bad_argument(format!(
            "unknown lease subcommand {other:?}; use acquire, ls, renew or release"
        ))),
    }
}

fn default_holder() -> String {
    let user = std::env::var("USER")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("USERNAME")
                .ok()
                .filter(|value| !value.is_empty())
        });
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|value| value.trim().to_owned())
        })
        .unwrap_or_default();
    match user {
        Some(user) => format!("{user}@{host}"),
        None => host,
    }
}

fn job(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let action = next(args, *index + 1)
        .ok_or_else(|| bad_argument("job needs ls, show, cancel, run, test-cross or logs"))?;
    *index += 1;
    match action {
        "ls" | "list" => {
            let mut filter = JobFilter::default();
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match name {
                    "--state" => {
                        *index += 1;
                        let given = value(args, index, "--state", inline)?;
                        filter.state = Some(match JobState::parse(&given) {
                            Some(state) => state,
                            None => {
                                return Err(bad_argument(format!(
                                    "state must be {}, not {given:?}",
                                    JobState::names()
                                )))
                            }
                        })
                    }
                    "--limit" => {
                        *index += 1;
                        filter.limit = limit(&value(args, index, "--limit", inline)?, "jobs")?
                    }
                    _ => break,
                }
            }
            Ok(Command::JobList(filter))
        }
        "show" => Ok(Command::JobShow {
            id: argument(args, index, "job show <id>")?,
        }),
        "cancel" => Ok(Command::JobCancel {
            id: argument(args, index, "job cancel <id>")?,
        }),
        "run" => job_run(args, index, json),
        "test-cross" => job_test_cross(args, index, json),
        "logs" => job_logs(args, index, json),
        other => Err(bad_argument(format!(
            "unknown job subcommand {other:?}; use ls, show, cancel, run, test-cross or logs"
        ))),
    }
}

/// The node runs Cargo using its own native toolchain. A Git source keeps the
/// node's target cache warm; --artifact sends an existing directory instead.
fn job_test_cross(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let mut on = String::new();
    let mut os = String::new();
    let mut arch = String::new();
    let mut git = String::new();
    let mut artifact = String::new();
    let mut workdir = String::new();
    let mut cargo = "cargo".to_owned();
    let mut packages = Vec::new();
    let mut artifact_globs = Vec::new();
    let mut timeout = DEFAULT_JOB_TIMEOUT;
    let mut extra = Vec::new();
    while let Some(token) = next(args, *index + 1) {
        *index += 1;
        if token == "--" {
            extra.extend_from_slice(&args[*index + 1..]);
            *index = args.len() - 1;
            break;
        }
        let (flag, inline) = option(token);
        match flag {
            "--json" if inline.is_none() => *json = true,
            "--on" => on = value(args, index, flag, inline)?,
            "--os" => os = value(args, index, flag, inline)?,
            "--arch" => arch = value(args, index, flag, inline)?,
            "--git" => git = value(args, index, flag, inline)?,
            "--artifact" => artifact = value(args, index, flag, inline)?,
            "--workdir" => workdir = value(args, index, flag, inline)?,
            "--cargo" => cargo = value(args, index, flag, inline)?,
            "--package" | "-p" => packages.push(value(args, index, flag, inline)?),
            "--artifact-glob" => artifact_globs.push(value(args, index, flag, inline)?),
            "--timeout" => timeout = go_duration(flag, &value(args, index, flag, inline)?)?,
            _ => {
                return Err(bad_argument(format!(
                    "unknown job test-cross flag {token:?}"
                )))
            }
        }
    }
    if !on.is_empty() && (!os.is_empty() || !arch.is_empty()) {
        return Err(bad_argument("use --on or both --os and --arch"));
    }
    if on.is_empty() {
        if os.is_empty() || arch.is_empty() {
            return Err(bad_argument(
                "job test-cross needs --on or both --os and --arch",
            ));
        }
        let arch = match arch.as_str() {
            "amd64" => "x86_64",
            "arm64" => "aarch64",
            other => other,
        };
        on = format!("os={os},arch={arch}");
    }
    if !git.is_empty() && !artifact.is_empty() {
        return Err(bad_argument("--git and --artifact are exclusive"));
    }
    if !workdir.is_empty() && artifact.is_empty() {
        return Err(bad_argument("--workdir needs --artifact DIR"));
    }
    if git.is_empty() && artifact.is_empty() {
        git = ".".to_owned();
    }
    if timeout.is_zero() || timeout > MAX_JOB_TIMEOUT {
        return Err(bad_argument("--timeout must be between 1s and 24h"));
    }
    if cargo.is_empty() || packages.iter().any(String::is_empty) {
        return Err(bad_argument("--cargo and --package values cannot be empty"));
    }
    let mut command = vec![cargo, "test".to_owned()];
    if packages.is_empty() {
        command.push("--workspace".to_owned());
    } else {
        for package in packages {
            command.extend(["--package".to_owned(), package]);
        }
    }
    command.extend(extra);
    Ok(Command::JobTestCross(JobRun {
        on,
        git,
        artifact,
        workdir,
        artifact_globs,
        env: Vec::new(),
        timeout,
        cpu_percent: 0,
        memory_bytes: 0,
        admin: false,
        command,
    }))
}

fn job_run(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let mut spec = JobRun {
        on: String::new(),
        git: String::new(),
        artifact: String::new(),
        workdir: String::new(),
        artifact_globs: Vec::new(),
        env: Vec::new(),
        timeout: DEFAULT_JOB_TIMEOUT,
        cpu_percent: 0,
        memory_bytes: 0,
        admin: false,
        command: Vec::new(),
    };
    let mut separator = false;
    while let Some(token) = next(args, *index + 1) {
        *index += 1;
        if token == "--" {
            separator = true;
            break;
        }
        let (name, inline) = token
            .split_once('=')
            .map_or((token, None), |(name, value)| {
                (name, Some(value.to_owned()))
            });
        match name {
            "--json" => *json = true,
            "--on" => spec.on = value(args, index, name, inline)?,
            "--git" => spec.git = value(args, index, name, inline)?,
            "--artifact" => spec.artifact = value(args, index, name, inline)?,
            "--workdir" => spec.workdir = value(args, index, name, inline)?,
            "--artifact-glob" => spec.artifact_globs.push(value(args, index, name, inline)?),
            "--env" | "-e" => {
                let pair = value(args, index, name, inline)?;
                let (key, val) = pair
                    .split_once('=')
                    .ok_or_else(|| bad_argument("--env needs KEY=VALUE"))?;
                if key.is_empty() || key.contains(['\0', '=']) || val.contains('\0') {
                    return Err(bad_argument("--env needs a nonempty KEY=VALUE"));
                }
                spec.env.push((key.to_owned(), val.to_owned()));
            }
            "--timeout" => spec.timeout = go_duration(name, &value(args, index, name, inline)?)?,
            "--cpu" => spec.cpu_percent = cpu_percent_flag(&value(args, index, name, inline)?)?,
            "--mem" => spec.memory_bytes = parse_size(&value(args, index, name, inline)?)?,
            "--admin" => spec.admin = bool_flag(name, inline)?,
            _ => return Err(bad_argument(format!("unknown job run flag {token:?}"))),
        }
    }
    if !separator || *index + 1 >= args.len() {
        return Err(bad_argument(
            "usage: job run --on <target> [flags] -- <cmd...>",
        ));
    }
    spec.command.extend_from_slice(&args[*index + 1..]);
    *index = args.len() - 1;
    if spec.on.is_empty() {
        return Err(bad_argument("job run needs --on <node|lease|selector>"));
    }
    if !spec.git.is_empty() && !spec.artifact.is_empty() {
        return Err(bad_argument("--git and --artifact are exclusive"));
    }
    if !spec.workdir.is_empty() && spec.artifact.is_empty() {
        return Err(bad_argument("--workdir needs --artifact <dir>"));
    }
    if spec.timeout > MAX_JOB_TIMEOUT {
        return Err(bad_argument("job timeout cannot exceed 24h"));
    }
    if spec.memory_bytes > 0 && spec.memory_bytes < 64 << 20 {
        return Err(bad_argument("--mem must be at least 64M"));
    }
    Ok(Command::JobRun(spec))
}

fn job_logs(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let mut id = None;
    let mut follow = false;
    while let Some(token) = next(args, *index + 1) {
        match token {
            "--follow" | "-f" => {
                follow = true;
                *index += 1;
            }
            "--json" => {
                *json = true;
                *index += 1;
            }
            value if !value.starts_with('-') && id.is_none() => {
                id = Some(value.to_owned());
                *index += 1;
            }
            _ => break,
        }
    }
    Ok(Command::JobLogs(JobLogs {
        id: id.ok_or_else(|| bad_argument("job logs <id>"))?,
        follow,
    }))
}

/// The hub's own ceiling on a model id and an effort value. Longer values are
/// rejected here so the caller learns before the hub does.
const MAX_MODEL_LEN: usize = 256;
const MAX_EFFORT_LEN: usize = 64;

fn selection_value(flag: &str, text: &str, limit: usize) -> Result<String> {
    if text.len() > limit {
        return Err(bad_argument(format!(
            "{flag} must be at most {limit} characters"
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(bad_argument(format!(
            "{flag} cannot contain control characters"
        )));
    }
    Ok(text.to_owned())
}

/// Chat commands. For `send`, the message is every
/// token after the sleeve name, joined with spaces.
fn chat(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument(
            "chat needs watch, send, wait, respond, interrupt, compact, models or settings",
        )
    })?;
    *index += 1;
    match action {
        "interrupt" | "compact" => {
            let sleeve = argument(args, index, &format!("chat {action} <sleeve>"))?;
            if action == "interrupt" {
                Ok(Command::ChatInterrupt { sleeve })
            } else {
                Ok(Command::ChatCompact { sleeve })
            }
        }
        "models" => {
            let mut sleeve = None;
            let mut harness = "codex".to_owned();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--harness" => {
                        *index += 1;
                        harness = value(args, index, flag, inline)?;
                        if harness.is_empty() {
                            return Err(bad_argument("--harness needs a harness name"));
                        }
                    }
                    _ if !token.starts_with('-') && sleeve.is_none() => {
                        *index += 1;
                        sleeve = Some(token.to_owned());
                    }
                    _ => break,
                }
            }
            Ok(Command::ChatModels { sleeve, harness })
        }
        "watch" => {
            let mut sleeve = None;
            let mut after = 0;
            let mut literal = false;
            while let Some(token) = next(args, *index + 1) {
                if token == "--" && !literal {
                    *index += 1;
                    literal = true;
                    continue;
                }
                let (flag, inline) = option(token);
                match flag {
                    "--after" if !literal => {
                        *index += 1;
                        after = value(args, index, flag, inline)?.parse().map_err(|_| {
                            bad_argument("--after must be a nonnegative sequence number")
                        })?;
                    }
                    _ if sleeve.is_none() && (literal || !token.starts_with('-')) => {
                        *index += 1;
                        sleeve = Some(token.to_owned());
                    }
                    _ => break,
                }
            }
            let sleeve = sleeve.ok_or_else(|| bad_argument("chat watch <sleeve>"))?;
            Ok(Command::ChatWatch { sleeve, after })
        }
        "respond" => {
            let mut sleeve = None;
            let mut approval_id = None;
            let mut allow = false;
            let mut deny = false;
            let mut note = String::new();
            let mut literal = false;
            while let Some(token) = next(args, *index + 1) {
                if token == "--" && !literal {
                    *index += 1;
                    literal = true;
                    continue;
                }
                let (flag, inline) = option(token);
                match flag {
                    "--allow" if !literal => {
                        *index += 1;
                        allow = bool_flag(flag, inline)?;
                    }
                    "--deny" if !literal => {
                        *index += 1;
                        deny = bool_flag(flag, inline)?;
                    }
                    "--note" if !literal => {
                        *index += 1;
                        note = value(args, index, flag, inline)?;
                    }
                    _ if sleeve.is_none() && (literal || !token.starts_with('-')) => {
                        *index += 1;
                        sleeve = Some(token.to_owned());
                    }
                    _ if approval_id.is_none() && (literal || !token.starts_with('-')) => {
                        *index += 1;
                        approval_id = Some(token.to_owned());
                    }
                    _ => break,
                }
            }
            let sleeve = sleeve.ok_or_else(|| bad_argument("chat respond <sleeve> <approval-id>"))?;
            let approval_id =
                approval_id.ok_or_else(|| bad_argument("chat respond <sleeve> <approval-id>"))?;
            if allow == deny {
                return Err(bad_argument("pass exactly one of --allow and --deny"));
            }
            Ok(Command::ChatRespond {
                sleeve,
                approval_id,
                allow,
                note,
            })
        }
        "send" => {
            let mut sleeve = None;
            let mut text: Vec<String> = Vec::new();
            let mut request_id = String::new();
            let mut literal = false;
            while let Some(token) = next(args, *index + 1) {
                if token == "--" && !literal {
                    *index += 1;
                    literal = true;
                    continue;
                }
                if literal {
                    *index += 1;
                    if sleeve.is_none() {
                        sleeve = Some(token.to_owned());
                    } else {
                        text.push(token.to_owned());
                    }
                    continue;
                }
                let (name, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match name {
                    "--request-id" => {
                        *index += 1;
                        request_id = value(args, index, "--request-id", inline)?
                    }
                    "-" => {
                        *index += 1;
                        if sleeve.is_none() {
                            sleeve = Some("-".into());
                        } else {
                            text.push("-".into());
                        }
                    }
                    _ if token.starts_with('-') => {
                        return Err(bad_argument(format!(
                            "unknown chat send flag {token:?}; use --request-id"
                        )))
                    }
                    _ => {
                        *index += 1;
                        if sleeve.is_none() {
                            sleeve = Some(token.to_owned());
                        } else {
                            text.push(token.to_owned());
                        }
                    }
                }
            }
            let sleeve = sleeve.ok_or_else(|| bad_argument("chat send <sleeve> <text>"))?;
            if text.is_empty() {
                return Err(bad_argument(
                    "chat send <sleeve> <text> needs a message (use - to read stdin)",
                ));
            }
            Ok(Command::ChatSend {
                sleeve,
                text: text.join(" "),
                request_id,
            })
        }
        "wait" => {
            let mut sleeves = vec![argument(args, index, "chat wait <sleeve> [sleeve...]")?];
            let mut until = WaitUntil::Idle;
            let mut timeout = Duration::ZERO;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = match token.split_once('=') {
                    Some((head, tail)) => (head, Some(tail.to_owned())),
                    None => (token, None),
                };
                match name {
                    "--until" => {
                        *index += 1;
                        let given = value(args, index, "--until", inline)?;
                        until = match WaitUntil::parse(&given) {
                            Some(until) => until,
                            None => {
                                return Err(bad_argument(format!(
                                    "--until must be idle or report, not {given:?}"
                                )))
                            }
                        }
                    }
                    "--timeout" => {
                        *index += 1;
                        timeout = wait_timeout(&value(args, index, "--timeout", inline)?)?
                    }
                    _ if !token.starts_with('-') => {
                        *index += 1;
                        let sleeve = token.to_owned();
                        if !sleeves.contains(&sleeve) {
                            sleeves.push(sleeve);
                        }
                    }
                    _ => break,
                }
            }
            Ok(Command::ChatWait {
                sleeves,
                until,
                timeout,
            })
        }
        "settings" => {
            let sleeve = argument(args, index, "chat settings <sleeve>")?;
            let mut model = None;
            let mut effort = None;
            let mut native_defaults = false;
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--model" => {
                        *index += 1;
                        model = Some(selection_value(flag, &value(args, index, flag, inline)?, MAX_MODEL_LEN)?);
                    }
                    "--effort" => {
                        *index += 1;
                        effort = Some(selection_value(flag, &value(args, index, flag, inline)?, MAX_EFFORT_LEN)?);
                    }
                    "--native-defaults" => {
                        *index += 1;
                        native_defaults = bool_flag(flag, inline)?;
                    }
                    _ => break,
                }
            }
            if native_defaults && (model.is_some() || effort.is_some()) {
                return Err(bad_argument(
                    "--native-defaults cannot be combined with --model or --effort",
                ));
            }
            Ok(Command::ChatSettings(ChatSettings {
                sleeve,
                model,
                effort,
                native_defaults,
            }))
        }
        other => Err(bad_argument(format!(
            "unknown chat subcommand {other:?}; use watch, send, wait, respond, interrupt, compact, models or settings"
        ))),
    }
}

fn report(args: &[String], index: &mut usize) -> Result<Command> {
    let mut status = None;
    let mut summary_file = None;
    while let Some(token) = next(args, *index + 1) {
        if token == "-" {
            *index += 1;
            if summary_file.is_some() {
                return Err(bad_argument("pass --summary-file FILE or - to read stdin"));
            }
            summary_file = Some("-".to_owned());
            continue;
        }
        let (flag, inline) = option(token);
        match flag {
            "--status" => {
                *index += 1;
                status = Some(value(args, index, flag, inline)?);
            }
            "--summary-file" => {
                *index += 1;
                if summary_file.is_some() {
                    return Err(bad_argument("pass --summary-file FILE or - to read stdin"));
                }
                summary_file = Some(value(args, index, flag, inline)?);
            }
            _ => break,
        }
    }
    let status =
        status.ok_or_else(|| bad_argument("--status must be complete, partial or blocked"))?;
    if !matches!(status.as_str(), "complete" | "partial" | "blocked") {
        return Err(bad_argument(
            "--status must be complete, partial or blocked",
        ));
    }
    Ok(Command::Report {
        status,
        summary_file,
    })
}

/// Sleeve inventory, workspace discovery, and lifecycle commands.
fn sleeve(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument(
            "sleeve needs ls, workspaces, browse, show, kill, spawn, envoy, resleeve or prune",
        )
    })?;
    *index += 1;
    match action {
        "ls" | "list" => Ok(Command::SleeveList),
        "workspaces" | "ws" => Ok(Command::SleeveWorkspaces),
        "browse" => {
            let path = match next(args, *index + 1) {
                Some(token) if !token.starts_with('-') => {
                    *index += 1;
                    token.to_owned()
                }
                _ => String::new(),
            };
            Ok(Command::SleeveBrowse { path })
        }
        "show" => {
            let mut names = Vec::new();
            while let Some(token) = next(args, *index + 1) {
                if token.starts_with('-') {
                    break;
                }
                *index += 1;
                names.push(token.to_owned());
            }
            if names.len() != 1 {
                return Err(bad_argument("sleeve show <name>"));
            }
            Ok(Command::SleeveShow {
                name: names.remove(0),
            })
        }
        "kill" => sleeve_kill(args, index),
        "attach" => {
            let name = argument(args, index, "sleeve attach <name>")?;
            let mut read_only = false;
            let mut escape = "^]".to_owned();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--readonly" => { *index += 1; read_only = bool_flag(flag, inline)?; }
                    "--escape" => {
                        *index += 1;
                        escape = escape_flag(args, index, inline)?;
                    }
                    _ => break,
                }
            }
            Ok(Command::SleeveAttach { name, read_only, escape })
        }
        "spawn" => sleeve_spawn(args, index),
        "envoy" => sleeve_envoy(args, index),
        "resleeve" => sleeve_resleeve(args, index),
        "prune" => sleeve_prune(args, index),
        other => Err(bad_argument(format!(
            "unknown sleeve subcommand {other:?}; use ls, workspaces, browse [path], show <name>, kill <name>, spawn <name>, envoy, resleeve <name> or prune"
        ))),
    }
}

fn sleeve_kill(args: &[String], index: &mut usize) -> Result<Command> {
    let name = argument(args, index, "sleeve kill <name>")?;
    let mut drop_caches = false;
    while let Some(token) = next(args, *index + 1) {
        let (flag, inline) = option(token);
        match flag {
            "--drop-caches" => {
                *index += 1;
                drop_caches = bool_flag("--drop-caches", inline)?;
            }
            _ => break,
        }
    }
    Ok(Command::SleeveKill { name, drop_caches })
}

fn sleeve_spawn(args: &[String], index: &mut usize) -> Result<Command> {
    let mut name = None;
    let mut workspace = String::new();
    let mut harness = String::new();
    let mut extra = Vec::new();
    let mut cpu_percent = 0_u32;
    let mut memory_bytes = 0_u64;
    let mut env = Vec::new();
    let mut mode = SleeveModeName::Default;
    let mut supervised = false;
    let mut nested = false;
    let mut selection = ModelChoice::default();
    let mut read_only = Vec::new();
    let mut exit_after_idle = Duration::ZERO;
    let mut branch = String::new();
    let mut new_branch = String::new();
    let mut brief_file = String::new();
    while let Some(token) = next(args, *index + 1) {
        if token == "--" {
            *index += 1;
            if name.is_none() {
                return Err(bad_argument(
                    "usage: sleeve spawn <name> [flags] -- <args...>",
                ));
            }
            while let Some(arg) = next(args, *index + 1) {
                *index += 1;
                extra.push(arg.to_owned());
            }
            break;
        }
        if token.starts_with('-') {
            let (flag, inline) = option(token);
            match flag {
                "--workspace" => {
                    *index += 1;
                    workspace = value(args, index, "--workspace", inline)?;
                }
                "--branch" => {
                    *index += 1;
                    branch = value(args, index, "--branch", inline)?;
                }
                "--new-branch" => {
                    *index += 1;
                    new_branch = value(args, index, "--new-branch", inline)?;
                }
                "--brief" => {
                    *index += 1;
                    brief_file = value(args, index, "--brief", inline)?;
                }
                "--ro" => {
                    *index += 1;
                    read_only.push(read_only_bind(&value(args, index, "--ro", inline)?)?);
                }
                "--exit-after-idle" => {
                    *index += 1;
                    exit_after_idle = go_duration(
                        "--exit-after-idle",
                        &value(args, index, "--exit-after-idle", inline)?,
                    )?;
                }
                "--model" => {
                    *index += 1;
                    selection.model = value(args, index, "--model", inline)?;
                }
                "--effort" => {
                    *index += 1;
                    selection.effort = value(args, index, "--effort", inline)?;
                }
                "--harness" => {
                    *index += 1;
                    harness = value(args, index, "--harness", inline)?;
                }
                "--cpu" => {
                    *index += 1;
                    cpu_percent = cpu_percent_flag(&value(args, index, "--cpu", inline)?)?;
                }
                "--mem" => {
                    *index += 1;
                    memory_bytes = parse_size(&value(args, index, "--mem", inline)?)?;
                }
                "--env" => {
                    *index += 1;
                    env.push(env_pair(&value(args, index, "--env", inline)?)?);
                }
                "--mode" => {
                    *index += 1;
                    mode = spawn_mode(&value(args, index, "--mode", inline)?)?;
                }
                "--supervised" => {
                    *index += 1;
                    supervised = bool_flag("--supervised", inline)?;
                }
                "--nested" => {
                    *index += 1;
                    nested = bool_flag("--nested", inline)?;
                }
                _ => break,
            }
        } else if name.is_some() {
            return Err(bad_argument(
                "separate harness arguments from the name with --",
            ));
        } else {
            *index += 1;
            name = Some(token.to_owned());
        }
    }
    let name = name.ok_or_else(|| bad_argument("sleeve spawn <name> needs a name"))?;
    let harness = spawn_harness(&harness)?;
    if !brief_file.is_empty() && (mode == SleeveModeName::Terminal || harness == HarnessName::Shell)
    {
        return Err(bad_argument("--brief requires chat mode"));
    }
    if workspace.is_empty() {
        return Err(bad_argument("--workspace is required"));
    }
    Ok(Command::SleeveSpawn(SleeveSpawn {
        name,
        workspace,
        harness,
        args: extra,
        cpu_percent,
        memory_bytes,
        env,
        mode,
        supervised,
        selection,
        read_only,
        exit_after_idle,
        branch,
        new_branch,
        brief_file,
        nested,
    }))
}

fn sleeve_envoy(args: &[String], index: &mut usize) -> Result<Command> {
    let mut harness = "auto".to_owned();
    let mut scopes = Vec::new();
    let mut add_scopes = Vec::new();
    let mut ttl = DEFAULT_ENVOY_TTL;
    let mut selection = ModelChoice::default();
    while let Some(token) = next(args, *index + 1) {
        if !token.starts_with('-') {
            return Err(bad_argument("sleeve envoy takes no arguments"));
        }
        let (flag, inline) = option(token);
        match flag {
            "--harness" => {
                *index += 1;
                harness = value(args, index, "--harness", inline)?;
            }
            "--scopes" => {
                *index += 1;
                scopes.extend(split_scopes(&value(args, index, "--scopes", inline)?));
            }
            "--add-scopes" => {
                *index += 1;
                add_scopes.extend(split_scopes(&value(args, index, "--add-scopes", inline)?));
            }
            "--ttl" => {
                *index += 1;
                ttl = go_duration("--ttl", &value(args, index, "--ttl", inline)?)?;
            }
            "--model" => {
                *index += 1;
                selection.model = value(args, index, "--model", inline)?;
            }
            "--effort" => {
                *index += 1;
                selection.effort = value(args, index, "--effort", inline)?;
            }
            _ => break,
        }
    }
    let harness = if harness.is_empty() || harness == "auto" {
        None
    } else {
        Some(agent_harness(&harness)?)
    };
    Ok(Command::SleeveEnvoy(SleeveEnvoy {
        harness,
        scopes,
        add_scopes,
        ttl,
        selection,
    }))
}

fn sleeve_resleeve(args: &[String], index: &mut usize) -> Result<Command> {
    let mut name = None;
    let mut to = "auto".to_owned();
    let mut interrupt = false;
    let mut wait = Duration::ZERO;
    let mut selection = ModelChoice::default();
    let mut native_defaults = false;
    while let Some(token) = next(args, *index + 1) {
        if token.starts_with('-') {
            let (flag, inline) = option(token);
            match flag {
                "--to" => {
                    *index += 1;
                    to = value(args, index, "--to", inline)?;
                }
                "--interrupt" => {
                    *index += 1;
                    interrupt = bool_flag("--interrupt", inline)?;
                }
                "--wait" => {
                    *index += 1;
                    wait = go_duration("--wait", &value(args, index, "--wait", inline)?)?;
                }
                "--model" => {
                    *index += 1;
                    selection.model = value(args, index, "--model", inline)?;
                }
                "--effort" => {
                    *index += 1;
                    selection.effort = value(args, index, "--effort", inline)?;
                }
                "--native-defaults" => {
                    *index += 1;
                    native_defaults = bool_flag("--native-defaults", inline)?;
                }
                _ => break,
            }
        } else if name.is_some() {
            return Err(bad_argument("sleeve resleeve takes one sleeve name"));
        } else {
            *index += 1;
            name = Some(token.to_owned());
        }
    }
    let name = name.ok_or_else(|| bad_argument("sleeve resleeve <name> needs a name"))?;
    if native_defaults && selection.is_set() {
        return Err(bad_argument(
            "--native-defaults cannot be combined with --model or --effort",
        ));
    }
    let to = if to.is_empty() || to == "auto" {
        None
    } else {
        Some(agent_harness(&to)?)
    };
    Ok(Command::SleeveResleeve(SleeveResleeve {
        name,
        to,
        interrupt,
        wait,
        selection,
        native_defaults,
    }))
}

fn sleeve_prune(args: &[String], index: &mut usize) -> Result<Command> {
    let mut prune = SleevePrune::default();
    while let Some(token) = next(args, *index + 1) {
        if !token.starts_with('-') {
            return Err(bad_argument("sleeve prune takes no arguments"));
        }
        let (flag, inline) = option(token);
        match flag {
            "--yes" => {
                *index += 1;
                prune.yes = bool_flag("--yes", inline)?;
            }
            "--caches" => {
                *index += 1;
                prune.caches = bool_flag("--caches", inline)?;
            }
            "--homes" => {
                *index += 1;
                prune.homes = bool_flag("--homes", inline)?;
            }
            "--workspaces" => {
                *index += 1;
                prune.workspaces = bool_flag("--workspaces", inline)?;
            }
            "--ignored" => {
                *index += 1;
                prune.ignored = bool_flag("--ignored", inline)?;
            }
            "--running" => {
                *index += 1;
                prune.running = bool_flag("--running", inline)?;
            }
            _ => break,
        }
    }
    Ok(Command::SleevePrune(prune))
}

/// Tmux sessions on the hub: `session ls`, `new`, `kill`, `respawn`,
/// `capture` and `attach`.
fn session(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument("session needs ls, new <name>, kill <name>, respawn <name>, capture <name> or attach <name>")
    })?;
    *index += 1;
    match action {
        "ls" | "list" => Ok(Command::SessionList),
        "new" => session_new(args, index, json),
        "kill" => Ok(Command::SessionKill {
            name: session_name(args, index, "session kill <name>")?,
        }),
        "respawn" => Ok(Command::SessionRespawn {
            name: session_name(args, index, "session respawn <name>")?,
        }),
        "capture" => {
            let name = session_name(args, index, "session capture <name>")?;
            let mut lines = 200_i32;
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--lines" => {
                        *index += 1;
                        let given = value(args, index, flag, inline)?;
                        lines = given.parse().map_err(|_| {
                            bad_argument(format!(
                                "--lines needs a whole number of lines, not {given:?}"
                            ))
                        })?;
                        if lines < 0 {
                            return Err(bad_argument("--lines must be zero or more"));
                        }
                    }
                    _ => break,
                }
            }
            Ok(Command::SessionCapture { name, lines })
        }
        "attach" => {
            let name = session_name(args, index, "session attach <name>")?;
            let mut read_only = false;
            let mut escape = "^]".to_owned();
            while let Some(token) = next(args, *index + 1) {
                let (flag, inline) = option(token);
                match flag {
                    "--readonly" => {
                        *index += 1;
                        read_only = bool_flag(flag, inline)?;
                    }
                    "--escape" => {
                        *index += 1;
                        escape = escape_flag(args, index, inline)?;
                    }
                    _ => break,
                }
            }
            Ok(Command::SessionAttach { name, read_only, escape })
        }
        other => Err(bad_argument(format!(
            "unknown session subcommand {other:?}; use ls, new <name>, kill <name>, respawn <name>, capture <name> or attach <name>"
        ))),
    }
}

fn session_new(args: &[String], index: &mut usize, json: &mut bool) -> Result<Command> {
    let mut spec = SessionNew {
        name: session_name(args, index, "session new <name>")?,
        ..Default::default()
    };
    let mut separator = false;
    while let Some(token) = next(args, *index + 1) {
        *index += 1;
        if token == "--" {
            separator = true;
            break;
        }
        let (flag, inline) = option(token);
        match flag {
            "--json" => *json = true,
            "--hub" | "--token-file" | "--home" | "--timeout" => {
                *index -= 1;
                break;
            }
            "--cwd" => spec.cwd = value(args, index, flag, inline)?,
            "--env" => {
                let pair = value(args, index, flag, inline)?;
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| bad_argument("--env needs KEY=VALUE"))?;
                if key.is_empty() || key.contains(['\0', '=']) || value.contains('\0') {
                    return Err(bad_argument("--env needs a nonempty KEY=VALUE"));
                }
                spec.env.push((key.to_owned(), value.to_owned()));
            }
            _ if token.starts_with('-') => {
                return Err(bad_argument(format!("unknown session new flag {token:?}")))
            }
            _ => return Err(bad_argument("separate the command from the name with --")),
        }
    }
    if separator {
        spec.command.extend_from_slice(&args[*index + 1..]);
        *index = args.len() - 1;
    }
    Ok(Command::SessionNew(spec))
}

/// A session name, checked against the same rule the hub's session manager
/// applies, so a name that cannot be a tmux target never reaches the hub or
/// the terminal URL.
fn session_name(args: &[String], index: &mut usize, form: &str) -> Result<String> {
    let name = argument(args, index, form)?;
    if !(1..=64).contains(&name.len())
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(bad_argument(format!(
            "{form} needs 1-64 of A-Z a-z 0-9 _ -, not {name:?}"
        )));
    }
    Ok(name)
}

/// `--escape` for `sleeve attach` and `session attach`: a control key as ^X,
/// or `none` to detach only when the connection closes.
fn escape_flag(args: &[String], index: &mut usize, inline: Option<String>) -> Result<String> {
    let escape = value(args, index, "--escape", inline)?;
    if escape != "none"
        && (escape.len() != 2 || !escape.starts_with('^') || !escape.as_bytes()[1].is_ascii())
    {
        return Err(bad_argument("--escape must be ^X or none"));
    }
    Ok(escape)
}

fn spawn_harness(text: &str) -> Result<HarnessName> {
    HarnessName::parse(text).ok_or_else(|| {
        bad_argument(format!(
            "unknown harness {text:?}, want claude, codex, grok, pi or shell"
        ))
    })
}

fn agent_harness(text: &str) -> Result<HarnessName> {
    match HarnessName::parse(text) {
        Some(harness) if harness != HarnessName::Shell => Ok(harness),
        _ => Err(bad_argument(format!(
            "unknown harness {text:?}, want auto, claude, codex, grok or pi"
        ))),
    }
}

fn spawn_mode(text: &str) -> Result<SleeveModeName> {
    match text {
        "" => Ok(SleeveModeName::Default),
        "chat" => Ok(SleeveModeName::Chat),
        "terminal" => Ok(SleeveModeName::Terminal),
        other => Err(bad_argument(format!(
            "unknown mode {other:?}, want chat or terminal"
        ))),
    }
}

fn read_only_bind(text: &str) -> Result<ReadOnlyBind> {
    let (source, target) = match text.split_once(':') {
        Some((source, target)) => (source.to_owned(), target.to_owned()),
        None => (text.to_owned(), String::new()),
    };
    if source.is_empty() {
        return Err(bad_argument(format!(
            "invalid --ro {text:?}, want host-path[:sandbox-path]"
        )));
    }
    Ok(ReadOnlyBind { source, target })
}

fn env_pair(text: &str) -> Result<(String, String)> {
    match text.split_once('=') {
        Some((key, value)) if !key.is_empty() => Ok((key.to_owned(), value.to_owned())),
        _ => Err(bad_argument(format!("invalid --env {text:?}, want K=V"))),
    }
}

fn cpu_percent_flag(text: &str) -> Result<u32> {
    text.parse().map_err(|_| {
        bad_argument(format!(
            "--cpu needs a whole number of percent of one core, not {text:?}"
        ))
    })
}

/// 512, 512M, 8G or 8GiB, in powers of 1024, the way the Go CLI reads `--mem`.
fn parse_size(text: &str) -> Result<u64> {
    let mut body = text.trim().to_ascii_uppercase();
    if let Some(stripped) = body.strip_suffix('B') {
        body = stripped.to_owned();
    }
    if let Some(stripped) = body.strip_suffix('I') {
        body = stripped.to_owned();
    }
    let mut mult = 1_u64;
    for (index, suffix) in ["K", "M", "G", "T"].into_iter().enumerate() {
        if let Some(stripped) = body.strip_suffix(suffix) {
            mult = 1_u64 << (10 * (index + 1));
            body = stripped.to_owned();
            break;
        }
    }
    let magnitude: f64 = body.trim().parse().map_err(|_| bad_size(text))?;
    if !magnitude.is_finite() || magnitude < 0.0 {
        return Err(bad_size(text));
    }
    let bytes = magnitude * mult as f64;
    if bytes > u64::MAX as f64 {
        return Err(bad_size(text));
    }
    Ok(bytes as u64)
}

fn bad_size(text: &str) -> RpcError {
    bad_argument(format!("invalid size {text:?}, want e.g. 512M or 8G"))
}

fn split_scopes(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A Go duration: 0, 250ms, 1.5s, 5m, 1h30m, 720h. Unlike `chat wait`, zero
/// is a real value and there is no 24h cap.
fn go_duration(flag: &str, text: &str) -> Result<Duration> {
    parse_go_duration(text).map_err(|_| {
        bad_argument(format!(
            "{flag} needs a duration like 90s, 5m, 1h30m or 720h, not {text:?}"
        ))
    })
}

fn parse_go_duration(text: &str) -> std::result::Result<Duration, ()> {
    let text = text.trim();
    if text.is_empty() || text.starts_with(['+', '-']) {
        return Err(());
    }
    // Go's ParseDuration accepts a bare 0 and no other unitless value.
    if text == "0" {
        return Ok(Duration::ZERO);
    }
    let mut total = 0_u128;
    let mut rest = text;
    let mut parts = 0_u32;
    while !rest.is_empty() {
        let digits = leading(rest, |c| c.is_ascii_digit());
        let (whole, tail) = if digits == 0 {
            if !rest.starts_with('.') {
                return Err(());
            }
            ("0", rest)
        } else {
            rest.split_at(digits)
        };
        let (fraction, tail) = match tail.strip_prefix('.') {
            Some(tail) => {
                let places = leading(tail, |c| c.is_ascii_digit());
                let (fraction, tail) = tail.split_at(places);
                (fraction, tail)
            }
            None => ("", tail),
        };
        let letters = leading(tail, |c| c.is_ascii_alphabetic() || matches!(c, 'µ' | 'μ'));
        if letters == 0 {
            return Err(());
        }
        let unit = match &tail[..letters] {
            "ns" => 1_u128,
            "us" | "µs" | "μs" => 1_000,
            "ms" => 1_000_000,
            "s" => 1_000_000_000,
            "m" => 60 * 1_000_000_000,
            "h" => 3_600 * 1_000_000_000,
            _ => return Err(()),
        };
        total = total
            .checked_add(scaled_nanos(whole, fraction, unit)?)
            .ok_or(())?;
        rest = &tail[letters..];
        parts += 1;
    }
    if parts == 0 {
        return Err(());
    }
    let nanos = u64::try_from(total).map_err(|_| ())?;
    Ok(Duration::from_nanos(nanos))
}

fn scaled_nanos(whole: &str, fraction: &str, unit: u128) -> std::result::Result<u128, ()> {
    let whole: u128 = whole.parse().map_err(|_| ())?;
    let mut total = whole.checked_mul(unit).ok_or(())?;
    if !fraction.is_empty() {
        let digits: u128 = fraction.parse().map_err(|_| ())?;
        let scale = 10_u128
            .checked_pow(u32::try_from(fraction.len()).map_err(|_| ())?)
            .ok_or(())?;
        total = total
            .checked_add(
                digits
                    .checked_mul(unit)
                    .ok_or(())?
                    .checked_div(scale)
                    .ok_or(())?,
            )
            .ok_or(())?;
    }
    Ok(total)
}

/// `msg send`, `inbox`, `reply`, `ack`, `thread`, `threads` and `wait`,
/// matching the Go CLI's mailbox commands: a body is every token after the
/// address or id, joined with spaces, and a bare `-` reads stdin.
fn msg(args: &[String], index: &mut usize) -> Result<Command> {
    let action = next(args, *index + 1).ok_or_else(|| {
        bad_argument("msg needs send <to> <body>, inbox, reply <id> <body>, ack <id>, thread <thread-id>, threads or wait <message-id|thread>")
    })?;
    *index += 1;
    match action {
        "send" => {
            let to = argument(args, index, "msg send <to> <body...>")?;
            let mut body: Vec<String> = Vec::new();
            let mut thread = String::new();
            let mut kind = MessageKind::Note;
            let mut ack = None;
            let mut request_id = String::new();
            while let Some(token) = next(args, *index + 1) {
                if token == "--" {
                    *index += 1;
                    body.extend_from_slice(&args[*index + 1..]);
                    *index = args.len() - 1;
                    break;
                }
                let (name, inline) = option(token);
                match name {
                    "--thread" => {
                        *index += 1;
                        thread = value(args, index, "--thread", inline)?
                    }
                    "--kind" => {
                        *index += 1;
                        let given = value(args, index, "--kind", inline)?;
                        kind = match MessageKind::parse(&given) {
                            Some(kind) => kind,
                            None => {
                                return Err(bad_argument(format!(
                                    "--kind must be {}, not {given:?}",
                                    MessageKind::names()
                                )))
                            }
                        }
                    }
                    "--ack" => {
                        *index += 1;
                        ack = Some(bool_flag("--ack", inline)?)
                    }
                    "--request-id" => {
                        *index += 1;
                        request_id = value(args, index, "--request-id", inline)?
                    }
                    "-" => {
                        *index += 1;
                        body.push("-".to_owned())
                    }
                    _ if token.starts_with('-') => {
                        return Err(bad_argument(format!(
                            "unknown msg send flag {token:?}; use --thread, --kind, --ack or --request-id"
                        )))
                    }
                    _ => {
                        *index += 1;
                        body.push(token.to_owned())
                    }
                }
            }
            if body.is_empty() {
                return Err(bad_argument(
                    "msg send <to> <body...> needs a message (use - to read stdin)",
                ));
            }
            Ok(Command::MsgSend {
                to,
                body: body.join(" "),
                thread,
                kind,
                ack,
                request_id,
            })
        }
        "inbox" => {
            let mut unread = false;
            let mut peek = false;
            let mut act_as = String::new();
            let mut thread = String::new();
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = option(token);
                match name {
                    "--unread" => {
                        *index += 1;
                        unread = bool_flag("--unread", inline)?
                    }
                    "--peek" => {
                        *index += 1;
                        peek = bool_flag("--peek", inline)?
                    }
                    "--as" => {
                        *index += 1;
                        act_as = value(args, index, "--as", inline)?
                    }
                    "--thread" => {
                        *index += 1;
                        thread = value(args, index, "--thread", inline)?
                    }
                    _ => break,
                }
            }
            Ok(Command::MsgInbox {
                unread,
                peek,
                act_as,
                thread,
            })
        }
        "reply" => {
            let id = argument(args, index, "msg reply <id> <body...>")?;
            let mut body: Vec<String> = Vec::new();
            let mut request_id = String::new();
            while let Some(token) = next(args, *index + 1) {
                if token == "--" {
                    *index += 1;
                    body.extend_from_slice(&args[*index + 1..]);
                    *index = args.len() - 1;
                    break;
                }
                let (name, inline) = option(token);
                match name {
                    "--request-id" => {
                        *index += 1;
                        request_id = value(args, index, "--request-id", inline)?
                    }
                    "-" => {
                        *index += 1;
                        body.push("-".to_owned())
                    }
                    _ if token.starts_with('-') => {
                        return Err(bad_argument(format!(
                            "unknown msg reply flag {token:?}; use --request-id"
                        )))
                    }
                    _ => {
                        *index += 1;
                        body.push(token.to_owned())
                    }
                }
            }
            if body.is_empty() {
                return Err(bad_argument(
                    "msg reply <id> <body...> needs an answer (use - to read stdin)",
                ));
            }
            Ok(Command::MsgReply {
                id,
                body: body.join(" "),
                request_id,
            })
        }
        "ack" => {
            let id = argument(args, index, "msg ack <id>")?;
            let mut act_as = String::new();
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = option(token);
                match name {
                    "--as" => {
                        *index += 1;
                        act_as = value(args, index, "--as", inline)?
                    }
                    _ => break,
                }
            }
            Ok(Command::MsgAck { id, act_as })
        }
        "thread" => Ok(Command::MsgThread {
            thread: argument(args, index, "msg thread <thread-id>")?,
        }),
        "threads" => {
            let mut sleeve = String::new();
            let mut project = String::new();
            let mut limit = 0;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = option(token);
                match name {
                    "--sleeve" => {
                        *index += 1;
                        sleeve = value(args, index, "--sleeve", inline)?
                    }
                    "--project" => {
                        *index += 1;
                        project = value(args, index, "--project", inline)?
                    }
                    "--limit" => {
                        *index += 1;
                        limit = self::limit(&value(args, index, "--limit", inline)?, "threads")?
                    }
                    _ => break,
                }
            }
            Ok(Command::MsgThreads {
                sleeve,
                project,
                limit,
            })
        }
        "wait" => {
            let target = argument(args, index, "msg wait <message-id|thread>")?;
            let mut timeout = crate::DEFAULT_MSG_WAIT;
            while let Some(token) = next(args, *index + 1) {
                let (name, inline) = option(token);
                match name {
                    "--timeout" => {
                        *index += 1;
                        timeout = wait_timeout(&value(args, index, "--timeout", inline)?)?
                    }
                    _ => break,
                }
            }
            if timeout.is_zero() {
                return Err(bad_argument(
                    "msg wait --timeout must be positive and at most 24h",
                ));
            }
            Ok(Command::MsgWait { target, timeout })
        }
        other => Err(bad_argument(format!(
            "unknown msg subcommand {other:?}; use send <to> <body>, inbox, reply <id> <body>, ack <id>, thread <thread-id>, threads or wait <message-id|thread>"
        ))),
    }
}

/// A flag and the value written after `=`, if any.
fn option(token: &str) -> (&str, Option<String>) {
    match token.split_once('=') {
        Some((head, tail)) => (head, Some(tail.to_owned())),
        None => (token, None),
    }
}

/// A Go-style duration: 90s, 1.5s, 5m, 1h30m, 250ms. Zero means no deadline.
fn wait_timeout(text: &str) -> Result<Duration> {
    let text = text.trim();
    let mut total = Duration::ZERO;
    let mut rest = text;
    let mut parts = 0;
    while !rest.is_empty() {
        let digits = leading(rest, |c| c.is_ascii_digit());
        if digits == 0 {
            return Err(bad_timeout(text));
        }
        let (whole, tail) = rest.split_at(digits);
        let mut fraction = "";
        let tail = match tail.strip_prefix('.') {
            Some(tail) => {
                let places = leading(tail, |c| c.is_ascii_digit());
                let (digits, tail) = tail.split_at(places);
                fraction = digits;
                tail
            }
            None => tail,
        };
        let letters = leading(tail, |c| c.is_ascii_alphabetic());
        let unit_ms: u64 = match tail.split_at(letters).0 {
            // A bare 0 is the only unitless duration: it means no deadline,
            // the way Go reads it.
            "" if whole == "0" && tail.is_empty() => 1_000,
            "ms" => 1,
            "s" => 1_000,
            "m" => 60_000,
            "h" => 3_600_000,
            _ => return Err(bad_timeout(text)),
        };
        let tail = &tail[letters..];
        let Some(millis) =
            unit_millis(whole, fraction).and_then(|units| units.checked_mul(unit_ms))
        else {
            return Err(bad_timeout(text));
        };
        total = total.saturating_add(Duration::from_millis(millis / 1_000));
        rest = tail;
        parts += 1;
    }
    if parts == 0 {
        return Err(bad_timeout(text));
    }
    if total > MAX_WAIT {
        return Err(bad_argument("--timeout cannot be longer than 24h"));
    }
    Ok(total)
}

fn leading(text: &str, want: impl Fn(char) -> bool) -> usize {
    text.len() - text.trim_start_matches(want).len()
}

/// "12" or "1.25", as thousandths of the unit that follows it.
fn unit_millis(whole: &str, fraction: &str) -> Option<u64> {
    let whole: u64 = whole.parse().ok()?;
    let mut milliunits = whole.checked_mul(1_000)?;
    if !fraction.is_empty() {
        let digits: u64 = fraction.parse().ok()?;
        let scale = 10_u64.checked_pow(fraction.len() as u32)?;
        milliunits += digits.checked_mul(1_000)?.checked_div(scale)?;
    }
    Some(milliunits)
}

fn bad_timeout(text: &str) -> RpcError {
    bad_argument(format!(
        "--timeout needs a duration like 90s, 5m or 1h30m, not {text:?}"
    ))
}

fn limit(text: &str, noun: &str) -> Result<i32> {
    let given: i32 = text.parse().map_err(|_| {
        bad_argument(format!(
            "--limit needs a whole number of {noun}, not {text:?}"
        ))
    })?;
    if given < 0 {
        return Err(bad_argument("--limit cannot be negative"));
    }
    Ok(given)
}

fn next(args: &[String], at: usize) -> Option<&str> {
    args.get(at).map(String::as_str)
}

/// A boolean flag: bare means true, `=true` or `=false` say it outright, the
/// way Go's pflag reads one.
fn bool_flag(flag: &str, inline: Option<String>) -> Result<bool> {
    match inline {
        None => Ok(true),
        Some(value) => match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(bad_argument(format!(
                "{flag} must be true or false, not {other:?}"
            ))),
        },
    }
}

/// The value of a flag: inline after `=`, or the token after the flag. `index`
/// ends up on the token holding the value.
fn value(args: &[String], index: &mut usize, flag: &str, inline: Option<String>) -> Result<String> {
    if let Some(value) = inline {
        return Ok(value);
    }
    match args.get(*index + 1) {
        Some(value) => {
            *index += 1;
            Ok(value.clone())
        }
        None => Err(bad_argument(format!("{flag} needs a value"))),
    }
}

fn argument(args: &[String], index: &mut usize, form: &str) -> Result<String> {
    let given = value(args, index, form, None)?;
    if given.starts_with('-') {
        return Err(bad_argument(format!("{form} needs a value")));
    }
    Ok(given)
}

fn seconds(text: &str) -> Result<u64> {
    let digits = text.strip_suffix('s').unwrap_or(text);
    let seconds: u64 = digits
        .parse()
        .map_err(|_| bad_argument(format!("--timeout needs seconds, not {text:?}")))?;
    if seconds == 0 {
        return Err(bad_argument("--timeout must be at least one second"));
    }
    Ok(seconds)
}

fn csv_flag_values(flag: &str, raw: &str) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let mut field = String::new();
    let mut chars = raw.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut start = true;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
        } else if closed {
            if ch != ',' {
                return Err(bad_argument(format!("{flag} has invalid CSV quoting")));
            }
            values.push(std::mem::take(&mut field));
            closed = false;
            start = true;
        } else if ch == ',' {
            values.push(std::mem::take(&mut field));
            start = true;
        } else if ch == '"' && start {
            quoted = true;
            start = false;
        } else if ch == '"' || ch == '\n' || ch == '\r' {
            return Err(bad_argument(format!("{flag} has invalid CSV quoting")));
        } else {
            field.push(ch);
            start = false;
        }
    }
    if quoted {
        return Err(bad_argument(format!(
            "{flag} has an unterminated CSV value"
        )));
    }
    values.push(field);
    Ok(values)
}

fn bad_argument(why: impl AsRef<str>) -> RpcError {
    RpcError::invalid(format!("{}\n\n{}", why.as_ref(), crate::usage()))
}
