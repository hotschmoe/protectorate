//! Shell completion for the installed CLI. Keep this command tree in step with
//! `args.rs` and the local companion aliases in `main.rs`.
use serde_json::json;
use std::{collections::HashSet, time::Duration};

type Candidate = (&'static str, &'static str);

const ROOT: &[Candidate] = &[
    ("api", "Raw API access"),
    ("audit", "Inspect the audit log"),
    ("backlog", "Manage notes"),
    ("chat", "Talk to a sleeve"),
    ("client", "Enroll a Tailscale client"),
    ("completion", "Generate shell completion"),
    ("devices", "Manage trusted devices"),
    ("events", "Stream hub events"),
    ("grant", "Mint task grants"),
    ("hub", "Manage the local hub"),
    ("job", "Run commands on nodes"),
    ("lease", "Reserve resources"),
    ("login", "Pair this device"),
    ("mcp", "Run the MCP bridge"),
    ("msg", "Use the mailbox"),
    ("node", "Manage nodes"),
    ("orchestrator", "Manage Envoy"),
    ("pair", "Show a pairing link"),
    ("quota", "Show subscription usage"),
    ("report", "Report a sleeve result"),
    ("service", "Generate service definitions"),
    ("session", "Manage sessions"),
    ("sleeve", "Manage sleeves"),
    ("status", "Show hub status"),
    ("tailnet", "Show the tailnet"),
    ("token", "Manage tokens"),
    ("version", "Show client version"),
    ("who", "Show project ownership"),
];

const GLOBAL_FLAGS: &[Candidate] = &[
    ("--hub", "Hub URL"),
    ("--token-file", "Bearer token file"),
    ("--home", "State directory"),
    ("--json", "JSON output"),
    ("--timeout", "Request timeout"),
    ("--help", "Show help"),
];

pub fn run_if_requested(args: &[String]) -> Option<Result<String, String>> {
    let mut at = 0;
    while let Some(arg) = args.get(at) {
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(flag, value)| (flag, Some(value)));
        if !matches!(
            flag,
            "--hub" | "--token-file" | "--home" | "--timeout" | "--json"
        ) {
            break;
        }
        at += 1;
        if takes_value(flag) && inline.is_none() {
            at += 1;
        }
    }
    match args.get(at).map(String::as_str)? {
        "completion" => Some(script_command(&args[at + 1..])),
        "__complete" => Some(Ok(complete(&args[at + 1..], &args[..at], true))),
        "__completeNoDesc" => Some(Ok(complete(&args[at + 1..], &args[..at], false))),
        _ => None,
    }
}

fn script_command(args: &[String]) -> Result<String, String> {
    let shell = args
        .first()
        .ok_or("completion needs bash, fish, zsh or powershell")?;
    let mut descriptions = true;
    for arg in &args[1..] {
        match arg.as_str() {
            "--no-descriptions" => descriptions = false,
            _ => return Err(format!("unknown completion argument {arg:?}")),
        }
    }
    let hook = if descriptions {
        "__complete"
    } else {
        "__completeNoDesc"
    };
    let script = match shell.as_str() {
        "bash" => format!(
            r#"_protectorate() {{
  local line
  COMPREPLY=()
  while IFS= read -r line; do
    [[ "$line" == :* ]] && continue
    COMPREPLY+=("${{line%%$'\t'*}}")
  done < <(protectorate {hook} "${{COMP_WORDS[@]:1:COMP_CWORD}}")
}}
complete -F _protectorate protectorate
"#
        ),
        "zsh" => format!(
            r#"#compdef protectorate
_protectorate() {{
  local -a candidates
  candidates=("${{(@f)$(protectorate {hook} "${{words[@]:1:CURRENT-2}}" "$PREFIX")}}")
  candidates=("${{(@)candidates:#:*}}")
  compadd -- "${{(@)candidates%%$'\t'*}}"
}}
compdef _protectorate protectorate
"#
        ),
        // `commandline -opc` excludes the current token; `-ct` supplies its
        // prefix once. Both forms also work on fish releases before `-xpc`.
        "fish" => format!(
            r#"function __protectorate_candidates
  set -l words (commandline -opc)
  protectorate {hook} $words[2..-1] (commandline -ct) | string match -rv '^:'
end
complete -c protectorate -f -a '(__protectorate_candidates)'
"#
        ),
        "powershell" => format!(
            r#"Register-ArgumentCompleter -Native -CommandName protectorate -ScriptBlock {{
  param($wordToComplete, $commandAst, $cursorPosition)
  $parts = @($commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object {{ $_.ToString() }})
  if (-not $wordToComplete) {{ $parts += '' }}
  & protectorate {hook} @parts | Where-Object {{ $_ -notmatch '^:' }} | ForEach-Object {{
    $candidate, $description = $_ -split "`t", 2
    if (-not $description) {{ $description = $candidate }}
    [System.Management.Automation.CompletionResult]::new($candidate, $candidate, 'ParameterValue', $description)
  }}
}}
"#
        ),
        _ => {
            return Err(format!(
                "unsupported completion shell {shell:?}; use bash, fish, zsh or powershell"
            ))
        }
    };
    Ok(script)
}

fn children(path: &str) -> &'static [Candidate] {
    match path {
        "" => ROOT,
        "api" => &[("call", "Call a unary RPC")],
        "audit" => &[("ls", "List entries"), ("tail", "Follow entries")],
        "backlog" => &[
            ("ls", "List notes"),
            ("show", "Show a note"),
            ("add", "Add a note"),
            ("edit", "Edit a note"),
            ("rm", "Remove a note"),
            ("pin", "Pin a note"),
            ("done", "Finish a note"),
            ("reopen", "Reopen a note"),
        ],
        "chat" => &[
            ("send", "Send text"),
            ("watch", "Watch events"),
            ("wait", "Wait for sleeves"),
            ("respond", "Answer approval"),
            ("interrupt", "Interrupt a turn"),
            ("compact", "Compact history"),
            ("models", "List models"),
            ("settings", "Show or change selection"),
        ],
        "client" => &[
            ("invite", "Issue a client key"),
            ("register", "Register a client"),
        ],
        "completion" => &[
            ("bash", "Bash script"),
            ("fish", "Fish script"),
            ("zsh", "Zsh script"),
            ("powershell", "PowerShell script"),
        ],
        "devices" => &[
            ("ls", "List devices"),
            ("pending", "Pending requests"),
            ("approve", "Approve a device"),
            ("deny", "Deny a request"),
            ("revoke", "Revoke a device"),
            ("rename", "Rename a device"),
        ],
        "grant" => &[("admin", "Mint an admin grant")],
        "hub" => &[
            ("serve", "Run the hub"),
            ("tailnet", "Manage local tailnet"),
            ("backup", "Create a backup"),
            ("restore", "Restore a backup"),
            ("publish-flutter-cache", "Publish Flutter SDK cache"),
            ("publish-flutter-tools", "Publish Flutter tool binaries"),
            ("publish-pub-cache", "Publish Pub package cache"),
            ("publish-go-mod-cache", "Publish Go module cache"),
            ("update", "Update the hub"),
        ],
        "hub tailnet" => &[
            ("install", "Install Headscale"),
            ("init", "Initialize tailnet"),
        ],
        "job" => &[
            ("ls", "List jobs"),
            ("show", "Show a job"),
            ("cancel", "Cancel a job"),
            ("run", "Run a command"),
            ("test-cross", "Run cross-node tests"),
            ("logs", "Show job output"),
        ],
        "lease" => &[
            ("ls", "List leases"),
            ("acquire", "Acquire a lease"),
            ("renew", "Renew a lease"),
            ("release", "Release leases"),
        ],
        "msg" => &[
            ("send", "Send a message"),
            ("reply", "Reply to a message"),
            ("ack", "Acknowledge a message"),
            ("inbox", "Read inbox"),
            ("thread", "Show thread"),
            ("threads", "List threads"),
            ("wait", "Wait for reply"),
        ],
        "node" => &[
            ("ls", "List nodes"),
            ("show", "Show a node"),
            ("metrics", "Local metrics"),
            ("add", "Add a node"),
            ("rm", "Remove a node"),
            ("tier", "Set tier"),
            ("update", "Update node"),
            ("invite", "Issue enrollment key"),
            ("put", "Upload a file"),
            ("get", "Download a file"),
            ("join", "Join tailnet"),
            ("renew", "Renew enrollment"),
            ("revoke", "Revoke enrollment"),
            ("install", "Install node"),
            ("uninstall", "Uninstall node"),
            ("doctor", "Inspect local node"),
            ("serve", "Run the node agent"),
        ],
        "orchestrator" => &[
            ("spawn", "Spawn Envoy"),
            ("status", "Show Envoy"),
            ("scopes", "Change Envoy token scopes"),
        ],
        "quota" => &[("suggest", "Suggest harness capacity")],
        "service" => &[
            ("systemd", "Systemd unit"),
            ("windows-task", "Task Scheduler XML"),
            ("install-windows", "Windows service commands"),
            ("termux", "Termux run and boot scripts"),
        ],
        "session" => &[
            ("ls", "List sessions"),
            ("new", "Start a session"),
            ("kill", "Stop a session"),
            ("respawn", "Restart a session"),
            ("capture", "Capture a pane"),
            ("attach", "Attach terminal"),
        ],
        "sleeve" => &[
            ("ls", "List sleeves"),
            ("workspaces", "List workspaces"),
            ("browse", "Browse workspaces"),
            ("show", "Show a sleeve"),
            ("kill", "Kill a sleeve"),
            ("attach", "Attach terminal"),
            ("spawn", "Spawn a sleeve"),
            ("envoy", "Spawn Envoy"),
            ("resleeve", "Move a sleeve"),
            ("prune", "Prune unused disk"),
        ],
        "tailnet" => &[("status", "Show tailnet status")],
        "token" => &[
            ("create", "Create token"),
            ("ls", "List tokens"),
            ("revoke", "Revoke token"),
            ("scopes", "Update scopes"),
        ],
        _ => &[],
    }
}

fn local_flags(path: &str) -> &'static [Candidate] {
    match path {
        "completion bash" | "completion fish" | "completion zsh" | "completion powershell" => {
            &[("--no-descriptions", "Omit descriptions")]
        }
        "mcp" => &[
            ("--list", "List MCP tools"),
            ("--allow", "Allow tool name glob"),
            ("--deny", "Deny tool name glob"),
            ("--http", "Listen over loopback HTTP"),
            ("--https", "Listen over HTTPS"),
            ("--tls-cert", "TLS certificate"),
            ("--tls-key", "TLS private key"),
        ],
        "hub update" => &[
            ("--channel", "Release channel"),
            ("--version", "Release version"),
        ],
        "hub restore" => &[("--home", "Empty destination directory")],
        "service systemd" => &[
            ("--binary", "Executable"),
            ("--home", "State directory"),
            ("--write", "Output path"),
            ("--install", "Install unit"),
            ("--user-name", "Service user"),
            ("--path", "PATH"),
            ("--tmux-socket", "Tmux socket"),
        ],
        "service windows-task" => &[
            ("--binary", "Executable"),
            ("--home", "State directory"),
            ("--write", "Output path"),
            ("--trigger", "Task trigger"),
            ("--elevated", "Run elevated"),
            ("--win-user", "Windows user"),
        ],
        "service install-windows" => &[
            ("--binary", "Executable"),
            ("--home", "State directory"),
            ("--write", "Output path"),
        ],
        "service termux" => &[
            ("--binary", "Executable"),
            ("--home", "State directory"),
            ("--write", "Output path"),
            ("--install", "Install into Termux"),
        ],
        "client invite" => &[
            ("--name", "Client name"),
            ("--expires", "Key lifetime"),
            ("--home", "State directory"),
        ],
        "client register" => &[("--home", "State directory")],
        "node join" | "node install" => &[
            ("--control", "Control URL"),
            ("--name", "Node name"),
            ("--key", "Enrollment key"),
            ("--key-file", "Enrollment key file"),
            ("--ca-cert", "CA certificate"),
            ("--home", "State directory"),
        ],
        "node doctor" => &[
            ("--home", "State directory"),
            ("--hub", "Hub URL"),
            ("--ca-cert", "CA certificate"),
        ],
        "node renew" | "node revoke" | "node uninstall" => &[("--home", "State directory")],
        "node serve" => &[
            ("--agent-token-file", "Node bearer token file"),
            ("--home", "State directory"),
            ("--file-root", "File transfer root"),
            ("--file-unconfined", "Unrestricted file transfers"),
            ("--job-concurrency", "Jobs at once"),
            ("--sandbox-helper", "Sandbox helper binary"),
            ("--allow-plain-jobs", "Run jobs without the sandbox"),
            ("--watch-dir", "Watched directory"),
            ("--docker-host", "Docker host URL"),
            ("--listen", "Listen address"),
            ("--tls-cert", "TLS certificate"),
            ("--tls-key", "TLS private key"),
        ],
        "job run" => &[
            ("--on", "Node, lease or selector"),
            ("--git", "Git source"),
            ("--artifact", "Local source"),
            ("--artifact-glob", "Collect artifacts"),
            ("--workdir", "Working directory"),
            ("--env", "Environment entry"),
            ("--cpu", "CPU limit"),
            ("--mem", "Memory limit"),
            ("--admin", "Run with admin grant"),
        ],
        "job test-cross" => &[
            ("--os", "Target OS"),
            ("--arch", "Target architecture"),
            ("--on", "Node, lease or selector"),
            ("--git", "Git source"),
            ("--artifact", "Local source"),
            ("--artifact-glob", "Collect artifacts"),
            ("--package", "Cargo package"),
        ],
        "node invite" => &[
            ("--tier", "Node tier"),
            ("--pool", "Network pool"),
            ("--expires", "Key lifetime"),
            ("--url", "Install URL"),
        ],
        "node add" => &[
            ("--addr", "Node address"),
            ("--ssh", "SSH alias"),
            ("--tier", "Node tier"),
            ("--tag", "Node tag"),
            ("--notes", "Notes"),
        ],
        "node rm" => &[
            ("--jobs", "Wait or cancel jobs"),
            ("--wait-timeout", "Drain deadline"),
            ("--uninstall", "Uninstall service"),
        ],
        "node update" => &[
            ("--all", "All nodes"),
            ("--force", "Force update"),
            ("--no-wait", "Do not wait"),
        ],
        "node metrics" => &[
            ("--local", "Collect locally"),
            ("--watch", "Watch directory"),
        ],
        "node put" | "node get" => &[
            ("--force", "Replace target"),
            ("--no-compress", "Disable compression"),
        ],
        "sleeve spawn" => &[
            ("--workspace", "Workspace"),
            ("--harness", "Harness"),
            ("--cpu", "CPU limit"),
            ("--mem", "Memory limit"),
            ("--mode", "Chat or terminal"),
            ("--supervised", "Supervise"),
            ("--nested", "Nested sleeve"),
            ("--env", "Environment entry"),
            ("--ro", "Read-only mount"),
            ("--brief", "Brief file"),
            ("--branch", "Git branch"),
            ("--new-branch", "Create branch"),
            ("--exit-after-idle", "Exit after idle"),
            ("--model", "Model"),
            ("--effort", "Reasoning effort"),
        ],
        "sleeve resleeve" => &[
            ("--to", "Target harness"),
            ("--interrupt", "Interrupt current turn"),
            ("--wait", "Wait duration"),
            ("--model", "Model"),
            ("--effort", "Reasoning effort"),
            ("--native-defaults", "Reset selection"),
        ],
        "sleeve prune" => &[
            ("--yes", "Remove selected data"),
            ("--caches", "Caches"),
            ("--homes", "Homes"),
            ("--workspaces", "Workspaces"),
            ("--ignored", "Ignored files"),
            ("--running", "Include running"),
        ],
        "chat wait" => &[
            ("--until", "Idle or report"),
            ("--timeout", "Wait duration"),
        ],
        "chat send" => &[("--request-id", "Idempotency key")],
        "chat watch" => &[("--after", "Replay offset")],
        "chat respond" => &[
            ("--allow", "Allow request"),
            ("--deny", "Deny request"),
            ("--note", "Reply note"),
        ],
        "chat models" => &[("--harness", "Harness")],
        "chat settings" => &[
            ("--model", "Model"),
            ("--effort", "Reasoning effort"),
            ("--native-defaults", "Reset selection"),
        ],
        "sleeve envoy" | "orchestrator spawn" => &[
            ("--harness", "Harness"),
            ("--scopes", "Grant scopes"),
            ("--add-scopes", "Additional scopes"),
            ("--ttl", "Grant lifetime"),
            ("--model", "Model"),
            ("--effort", "Reasoning effort"),
        ],
        "sleeve attach" | "session attach" => {
            &[("--readonly", "Read only"), ("--escape", "Detach key")]
        }
        "sleeve kill" => &[("--drop-caches", "Remove private caches")],
        "session new" => &[
            ("--cwd", "Working directory"),
            ("--env", "Environment entry"),
        ],
        "session capture" => &[("--lines", "Line count")],
        "backlog ls" => &[
            ("--project", "Project"),
            ("--query", "Search text"),
            ("--state", "Open, done or all"),
            ("--all", "All notes"),
        ],
        "backlog add" => &[
            ("--project", "Project"),
            ("--pin", "Pin note"),
            ("--body", "Body"),
            ("--body-file", "Body file"),
        ],
        "backlog edit" => &[
            ("--title", "Title"),
            ("--project", "Project"),
            ("--body", "Body"),
            ("--body-file", "Body file"),
            ("--append", "Append body"),
        ],
        "lease acquire" => &[
            ("--node", "Node"),
            ("--os", "OS"),
            ("--arch", "Architecture"),
            ("--min-free-mem", "Free memory"),
            ("--gpu-vram", "GPU memory"),
            ("--gpu", "GPU"),
            ("--npu", "NPU"),
            ("--holder", "Holder"),
            ("--ttl", "Lifetime"),
            ("--wait", "Wait for resource"),
            ("--priority", "Priority"),
        ],
        "msg send" => &[
            ("--thread", "Thread"),
            ("--kind", "Message kind"),
            ("--ack", "Require acknowledgment"),
            ("--request-id", "Idempotency key"),
        ],
        "msg inbox" => &[
            ("--unread", "Unread only"),
            ("--peek", "Do not mark read"),
            ("--as", "Mailbox"),
            ("--thread", "Thread"),
        ],
        "msg wait" => &[("--timeout", "Wait duration")],
        "msg reply" => &[("--request-id", "Idempotency key")],
        "msg ack" => &[("--as", "Mailbox")],
        "report" => &[
            ("--status", "Result status"),
            ("--summary-file", "Summary file"),
        ],
        "msg threads" => &[
            ("--sleeve", "Sleeve"),
            ("--project", "Project"),
            ("--limit", "Limit"),
        ],
        "hub backup" => &[("--to", "Backup directory")],
        "hub publish-flutter-cache" | "hub publish-flutter-tools" => &[
            ("--flutter-sdk-root", "Flutter SDK root"),
            ("--max-bytes", "Maximum archive bytes"),
        ],
        "hub publish-pub-cache" => &[
            ("--pub-cache-root", "Pub cache root"),
            ("--max-bytes", "Maximum archive bytes"),
        ],
        "hub publish-go-mod-cache" => &[
            ("--go-mod-cache-root", "Go module cache root"),
            ("--max-bytes", "Maximum archive bytes"),
        ],
        "hub tailnet init" => &[
            ("--server-url", "Public URL"),
            ("--no-embedded-derp", "Disable private relay"),
            ("--public-derp", "Use public relay"),
            ("--pool", "Network pool"),
        ],
        "token create" => &[("--scope", "Scope"), ("--ttl", "Lifetime")],
        "token scopes" | "orchestrator scopes" => {
            &[("--add", "Add scope"), ("--remove", "Remove scope")]
        }
        "audit ls" | "audit tail" => &[
            ("--lines", "Line count"),
            ("--actor", "Actor"),
            ("--procedure", "Procedure"),
            ("--subject", "Subject"),
            ("--outcome", "Outcome"),
            ("--page-token", "Page token"),
        ],
        "grant admin" => &[
            ("--node", "Node"),
            ("--task", "Task"),
            ("--ttl", "Lifetime"),
            ("--out", "Output file"),
        ],
        "quota" => &[("--refresh", "Refresh usage")],
        "quota suggest" => &[("--for", "Task size"), ("--refresh", "Refresh usage")],
        _ => &[],
    }
}

fn takes_value(flag: &str) -> bool {
    !matches!(
        flag,
        "--json"
            | "--help"
            | "--list"
            | "--no-descriptions"
            | "--all"
            | "--force"
            | "--no-wait"
            | "--uninstall"
            | "--local"
            | "--no-compress"
            | "--install"
            | "--elevated"
            | "--readonly"
            | "--drop-caches"
            | "--lock-after"
            | "--off"
            | "--admin"
            | "--supervised"
            | "--nested"
            | "--new-branch"
            | "--exit-after-idle"
            | "--native-defaults"
            | "--interrupt"
            | "--yes"
            | "--caches"
            | "--homes"
            | "--workspaces"
            | "--ignored"
            | "--running"
            | "--allow"
            | "--deny"
            | "--pin"
            | "--append"
            | "--wait"
            | "--unread"
            | "--peek"
            | "--ack"
            | "--refresh"
            | "--no-embedded-derp"
            | "--public-derp"
            | "--file-unconfined"
            | "--allow-plain-jobs"
            | "--double"
    )
}

fn takes_value_at(path: &str, flag: &str) -> bool {
    if flag == "--wait" {
        return path == "sleeve resleeve";
    }
    if matches!(flag, "--allow" | "--deny") && path == "mcp" {
        return true;
    }
    takes_value(flag)
}

fn complete(args: &[String], leading: &[String], descriptions: bool) -> String {
    let (prefix, words) = args
        .split_last()
        .map(|(last, rest)| (last.as_str(), rest))
        .unwrap_or(("", &[]));
    let mut path = String::new();
    let mut pending_flag = None;
    let mut positionals = 0;
    for word in words {
        if pending_flag.take().is_some() {
            continue;
        }
        if word == "--" {
            break;
        }
        if word.starts_with('-') {
            if !word.contains('=') && takes_value_at(&path, word) {
                pending_flag = Some(word.as_str());
            }
            continue;
        }
        if let Some((name, _)) = children(&path).iter().find(|(name, _)| *name == word) {
            if !path.is_empty() {
                path.push(' ');
            }
            path.push_str(name);
        } else if !path.is_empty() && children(&path).is_empty() {
            positionals += 1;
        }
    }
    let mut output = String::new();
    let value_target = match (path.as_str(), pending_flag) {
        ("job run" | "job test-cross", Some("--on")) => Some(ResourceKind::JobTarget),
        ("lease acquire", Some("--node")) => Some(ResourceKind::Node),
        _ => None,
    };
    let (inline_target, lookup_prefix) = match prefix.split_once('=') {
        Some(("--on", value)) if matches!(path.as_str(), "job run" | "job test-cross") => {
            (Some(ResourceKind::JobTarget), value)
        }
        Some(("--node", value)) if path == "lease acquire" => (Some(ResourceKind::Node), value),
        _ => (None, prefix),
    };
    if let Some(kind) = value_target.or(inline_target).or_else(|| {
        (!prefix.starts_with('-') && pending_flag.is_none())
            .then(|| resource_kind(&path, positionals))
            .flatten()
    }) {
        let inline_flag = inline_target.map(|_| prefix.split_once('=').unwrap().0);
        let mut seen = HashSet::new();
        for (name, label) in resource_names(kind, leading, words, lookup_prefix).unwrap_or_default()
        {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(flag) = inline_flag {
                output.push_str(flag);
                output.push('=');
            }
            output.push_str(&name);
            if descriptions {
                output.push('\t');
                output.push_str(label);
            }
            output.push('\n');
        }
        output.push_str(":4\n");
        return output;
    }
    let candidates = if prefix.starts_with('-') {
        GLOBAL_FLAGS
            .iter()
            .chain(local_flags(&path))
            .filter(|(name, _)| name.starts_with(prefix))
            .collect::<Vec<_>>()
    } else {
        children(&path)
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .collect()
    };
    let mut seen = HashSet::new();
    for (name, description) in candidates {
        if !seen.insert(name) {
            continue;
        }
        output.push_str(name);
        if descriptions {
            output.push('\t');
            output.push_str(description);
        }
        output.push('\n');
    }
    output.push_str(":4\n");
    output
}

#[derive(Clone, Copy)]
enum ResourceKind {
    Node,
    Sleeve,
    Lease,
    JobTarget,
}

impl ResourceKind {
    fn label(self) -> &'static str {
        match self {
            Self::Node => "Node",
            Self::Sleeve => "Sleeve",
            Self::Lease => "Lease",
            Self::JobTarget => "Job target",
        }
    }

    fn procedure(self) -> Option<&'static str> {
        match self {
            Self::Node => Some("/protectorate.v1.NodeService/ListNodes"),
            Self::Sleeve => Some("/protectorate.v1.SleeveService/ListSleeves"),
            Self::Lease => Some("/protectorate.v1.LeaseService/ListLeases"),
            Self::JobTarget => None,
        }
    }
}

fn resource_kind(path: &str, positionals: usize) -> Option<ResourceKind> {
    match path {
        "node show" | "node rm" | "node tier" | "node update" | "node put" | "node get"
            if positionals == 0 =>
        {
            Some(ResourceKind::Node)
        }
        "sleeve show" | "sleeve kill" | "sleeve attach" | "sleeve resleeve" if positionals == 0 => {
            Some(ResourceKind::Sleeve)
        }
        "chat send" | "chat watch" | "chat respond" | "chat interrupt" | "chat compact"
        | "chat settings" | "chat models"
            if positionals == 0 =>
        {
            Some(ResourceKind::Sleeve)
        }
        "chat wait" => Some(ResourceKind::Sleeve),
        _ => None,
    }
}

/// Completion must stay quiet and fast when the hub is down or credentials are
/// unavailable. The same auth resolution as ordinary CLI calls is used here.
fn resource_names(
    kind: ResourceKind,
    leading: &[String],
    words: &[String],
    prefix: &str,
) -> Option<Vec<(String, &'static str)>> {
    let mut globals = Vec::new();
    for tokens in [leading, words] {
        let mut at = 0;
        while at < tokens.len() {
            let (flag, inline) = tokens[at]
                .split_once('=')
                .map_or((tokens[at].as_str(), None), |(flag, value)| {
                    (flag, Some(value))
                });
            if matches!(flag, "--hub" | "--token-file" | "--home") {
                globals.push(tokens[at].clone());
                if inline.is_none() {
                    at += 1;
                    let value = tokens.get(at)?;
                    globals.push(value.clone());
                }
            }
            at += 1;
        }
    }
    globals.extend(["node".to_owned(), "ls".to_owned()]);
    let mut invocation = protectorate_cli::parse(&globals).ok()?;
    invocation.timeout = Duration::from_millis(750);
    let client = protectorate_cli::client_for(&invocation).ok()?;
    let mut names = if matches!(kind, ResourceKind::JobTarget) {
        let mut names = fetch_names(&client, ResourceKind::Node, prefix);
        names.extend(fetch_names(&client, ResourceKind::Lease, prefix));
        names
    } else {
        fetch_names(&client, kind, prefix)
    };
    names.sort();
    names.dedup_by(|left, right| left.0 == right.0);
    names.truncate(1024);
    Some(names)
}

fn fetch_names(
    client: &protectorate_cli::Client,
    kind: ResourceKind,
    prefix: &str,
) -> Vec<(String, &'static str)> {
    let Some(procedure) = kind.procedure() else {
        return Vec::new();
    };
    let Ok(response) = client.call(procedure, &json!({})) else {
        return Vec::new();
    };
    let rows = match kind {
        ResourceKind::Node => response.get("nodes").and_then(|v| v.as_array()),
        ResourceKind::Sleeve => response.get("sleeves").and_then(|v| v.as_array()),
        ResourceKind::Lease => response.get("leases").and_then(|v| v.as_array()),
        ResourceKind::JobTarget => None,
    };
    let Some(rows) = rows else { return Vec::new() };
    rows.iter()
        .filter_map(|row| match kind {
            ResourceKind::Node => row.get("node")?.get("name")?.as_str(),
            ResourceKind::Sleeve => row.get("name")?.as_str(),
            ResourceKind::Lease if row["state"] != "LEASE_STATE_QUEUED" => row.get("id")?.as_str(),
            ResourceKind::Lease | ResourceKind::JobTarget => None,
        })
        .filter(|name| {
            !name.is_empty() && name.starts_with(prefix) && !name.chars().any(char::is_control)
        })
        .map(|name| (name.to_owned(), kind.label()))
        .collect()
}
