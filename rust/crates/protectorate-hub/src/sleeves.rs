//! Read projection of the durable sleeve registry. Process launch and live
//! usage/chat updates are deliberately separate from these read routes.
pub(super) mod branch;
mod prune_git;
use super::{input, respond, string, token_json, Hub};
use chrono::Utc;
use protectorate_audit::Entry as AuditEntry;
use protectorate_sandbox::{
    runtime::LaunchOptions, Helper, Limits as SandboxLimits, Policy, Request, Workload,
};
use protectorate_session::{CreateOptions, Error as SessionError, State as SessionState};
use protectorate_sleeve_registry::{
    DiskUsage, Error, Harness, Limits, Mode, Pin, ReadOnlyMount, Record, State,
};
use protectorate_sleeve_runner::{Config, ReadOnly};
use protectorate_wire::{AuthContext, Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CString, OsStr},
    fs,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The tool scopes of every sleeve's hub token.
pub(super) const SLEEVE_SCOPES: [&str; 6] = [
    "read",
    "backlog_*",
    "directory_*",
    "lease_*",
    "message_*",
    "chat_report",
];

/// The sleeve a parent label names: Envoy or another sleeve.
pub(super) fn parent_sleeve(label: &str) -> Option<&str> {
    if label == "envoy" {
        Some(label)
    } else {
        label.strip_prefix("sleeve:")
    }
}

/// Pins are kept per mode and harness: a resleeve onto another harness is
/// compared with that harness's own last launch.
pub(super) fn pin_key(mode: Mode, harness: Harness) -> String {
    format!(
        "{}/{}",
        match mode {
            Mode::Chat => "chat",
            Mode::Terminal => "terminal",
        },
        harness_name(harness)
    )
}

struct ResleeveReservation<'a> {
    hub: &'a Hub,
    name: String,
}

pub(super) struct SpawnParent {
    pub label: String,
    pub token_id: String,
}

impl SpawnParent {
    fn from_auth(auth: &AuthContext) -> Self {
        let label = if auth.owner {
            "owner".to_owned()
        } else if auth.actor == "token:orchestrator-envoy" {
            "envoy".to_owned()
        } else if auth.actor.starts_with("token:sleeve:") {
            auth.actor.trim_start_matches("token:").to_owned()
        } else {
            auth.actor.clone()
        };
        Self {
            label,
            token_id: auth.token_id.clone(),
        }
    }

    pub(super) fn owner() -> Self {
        Self {
            label: "owner".into(),
            token_id: String::new(),
        }
    }
}

/// Whether the caller is the identity that spawned this sleeve.
pub(super) fn spawned_by(auth: &AuthContext, row: &Record) -> bool {
    let same_token = !row.parent_token_id.is_empty() && row.parent_token_id == auth.token_id;
    let same_sleeve = auth.actor.starts_with("token:sleeve:")
        && auth.actor.strip_prefix("token:") == Some(row.parent.as_str());
    let same_envoy = row.parent == "envoy" && auth.actor == "token:orchestrator-envoy";
    same_token || same_sleeve || same_envoy
}

fn has_kill_scope(auth: &AuthContext) -> bool {
    super::auth::scopes_allow(
        &auth.scopes.iter().map(String::as_str).collect::<Vec<_>>(),
        &super::ScopeMethod {
            tool: "sleeve_kill_sleeve",
            service: "SleeveService",
            read_only: false,
            owner_only: false,
        },
    )
}

impl Drop for ResleeveReservation<'_> {
    fn drop(&mut self) {
        self.hub.resleeving.lock().unwrap().remove(&self.name);
    }
}

fn reserve_resleeve<'a>(hub: &'a Hub, name: &str) -> Result<ResleeveReservation<'a>> {
    let mut names = hub.resleeving.lock().unwrap();
    if !names.insert(name.to_owned()) {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "sleeve is already being resleeved",
        ));
    }
    Ok(ResleeveReservation {
        hub,
        name: name.to_owned(),
    })
}

fn resleeve_sleeve(hub: &Hub, value: &Value) -> Result<Record> {
    let name = string(value, "name");
    protectorate_sleeve_registry::valid_name(&name).map_err(registry_error)?;
    let _reservation = reserve_resleeve(hub, &name)?;
    let _envoy_guard = (name == "envoy").then(|| hub.envoy_spawn_lock.lock().unwrap());
    let old = hub.sleeves.get(&name).map_err(registry_error)?;
    if old.helper || old.mode != Mode::Chat || refresh_record(hub, &old).state != State::Running {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "resleeve needs a running chat sleeve",
        ));
    }
    let to = resleeve_target(hub, old.harness, &string(value, "to"))?;
    let selection = value.get("selection").filter(|v| v.is_object());
    let native_defaults = value["nativeDefaults"] == true;
    if native_defaults && selection.is_some_and(|s| s.as_object().is_some_and(|o| !o.is_empty())) {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "native_defaults cannot be combined with explicit selection",
        ));
    }
    let previous_selection = old
        .chat
        .as_ref()
        .map(|chat| chat["settings"]["requested"].clone())
        .unwrap_or(Value::Null);
    let previous_custom = !string(&previous_selection, "model").is_empty()
        || !string(&previous_selection, "effort").is_empty();
    if to != old.harness && previous_custom && selection.is_none() && !native_defaults {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "choose target selection or explicitly use native defaults when changing harnesses",
        ));
    }
    let chosen_selection = if native_defaults {
        Value::Null
    } else if let Some(selection) = selection {
        selection.clone()
    } else if to == old.harness {
        previous_selection.clone()
    } else {
        Value::Null
    };
    let wait = match value.get("wait") {
        Some(raw) => {
            let parsed = super::parse_duration(Some(raw))?;
            if parsed.is_zero() {
                Duration::from_secs(300)
            } else {
                parsed
            }
        }
        None => Duration::from_secs(300),
    };
    let mut launch_request = resleeve_launch_request(&old, to, &chosen_selection);
    launch_request["requester"] = value["requester"].clone();
    if let Some(pin) = crate::chat::chat_pin(hub, &launch_request, to)? {
        let pin_key = pin_key(Mode::Chat, to);
        crate::approvals::check_pin(
            hub,
            &launch_request,
            &name,
            &old.run_id,
            &pin_key,
            old.pins.get(&pin_key),
            &pin,
        )?;
        hub.sleeves
            .set_pin(&name, &pin_key, pin)
            .map_err(registry_error)?;
    }
    hub.emit(
        "resleeve.started",
        &name,
        json!({"from":harness_name(old.harness),"to":harness_name(to)}),
    );
    let status = crate::chat::quiesce_for_resleeve(hub, &name, value["interrupt"] == true, wait)?;
    let mut facts = resleeve_facts(hub, &old, to, &status);
    facts.at = Some(Utc::now());
    let summary = crate::chat::summarize_handoff(
        hub,
        &name,
        &protectorate_handoff::summary_prompt(
            &facts,
            &facts.markdown(protectorate_handoff::DEFAULT_BUDGET),
        ),
    );
    let workspace = Path::new(&old.workspace);
    protectorate_handoff::exclude(workspace).map_err(handoff_error)?;
    let document =
        protectorate_handoff::document(&facts, &summary, protectorate_handoff::DEFAULT_BUDGET);
    let path =
        protectorate_handoff::write(workspace, document.as_bytes()).map_err(handoff_error)?;
    let archive = protectorate_handoff::archive(
        hub.workspaces.state_root().join("handoffs"),
        &name,
        harness_name(old.harness),
        harness_name(to),
        facts.at.unwrap(),
        document.as_bytes(),
    )
    .ok();
    crate::checkpoints::before_resleeve(hub, &old);
    {
        let _guard = hub.sleeve_launch_lock.lock().unwrap();
        crate::chat::kill_pi(hub, &name)?;
    }
    match crate::chat::spawn_resleeve(hub, &launch_request, to) {
        Ok(_) => {}
        Err(target_error) => {
            let mut rollback = resleeve_launch_request(&old, old.harness, &previous_selection);
            rollback["requester"] = value["requester"].clone();
            match crate::chat::spawn_resleeve(hub, &rollback, old.harness) {
                Ok(_) => {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        format!(
                            "start {}: {}; {} is back on its own session",
                            harness_name(to),
                            target_error.message,
                            harness_name(old.harness)
                        ),
                    ))
                }
                Err(restore_error) => {
                    return Err(RpcError::new(
                        Code::Internal,
                        format!(
                            "start {}: {}; restoring {} also failed: {}",
                            harness_name(to),
                            target_error.message,
                            harness_name(old.harness),
                            restore_error.message
                        ),
                    ))
                }
            }
        }
    }
    let handoff = json!({
        "fromHarness": harness_name(old.harness),
        "fromSessionId": string(&status, "harnessSessionId"),
        "toHarness": harness_name(to),
        "summarizerHarness": summary.harness,
        "summarizerModel": summary.model,
        "summarizerProvider": summary.provider,
        "summarizerLocal": summary.local,
        "summaryError": summary.err,
        "path": path,
        "archivePath": archive.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        "at": facts.at.unwrap().to_rfc3339(),
    });
    let row = hub
        .sleeves
        .set_handoff(&name, handoff.clone())
        .map_err(registry_error)?;
    if let Err(error) = crate::chat::append_handoff(hub, &name, &handoff) {
        eprintln!("resleeve {name}: append handoff event: {error}");
    }
    hub.emit("sleeve.resleeved", &name, json!({"from":harness_name(old.harness),"to":harness_name(to),"handoff":path,"archive":archive,
        "summarizer":summary.harness,"summarizer_model":summary.model,"summarizer_local":summary.local,"summary_error":summary.err}));
    if !summary.err.is_empty() {
        hub.emit("resleeve.warning", &name, json!({"text":format!("Summary unavailable: {}; the handoff contains hub facts and conversation excerpt.",summary.err),"summarizer_local":summary.local}));
    }
    let prompt = format!("Continue the task in this workspace. Read {} first: the hub wrote it when this sleeve moved from {} to {}. It has the task state, what is unfinished, and the end of the previous conversation.",protectorate_handoff::FILE,harness_name(old.harness),harness_name(to));
    if let Err(error) = crate::chat::send_resleeve_prompt(hub, &name, &prompt) {
        eprintln!("resleeve {name}: first prompt: {error}");
    }
    hub.directory_kick.notify_one();
    Ok(row)
}

fn harness_name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
        Harness::Grok => "grok",
        Harness::Pi => "pi",
        Harness::Shell => "shell",
    }
}

fn resleeve_target(hub: &Hub, from: Harness, requested: &str) -> Result<Harness> {
    let explicit = match requested {
        "HARNESS_CLAUDE" => Some(Harness::Claude),
        "HARNESS_CODEX" => Some(Harness::Codex),
        "HARNESS_GROK" => Some(Harness::Grok),
        "HARNESS_PI" => Some(Harness::Pi),
        "" | "HARNESS_UNSPECIFIED" => None,
        _ => {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "resleeve onto claude, codex, grok or pi",
            ))
        }
    };
    if let Some(harness) = explicit {
        return Ok(harness);
    }
    let suggestions = protectorate_quota::rank_harnesses(
        &hub.quota.snapshot(Utc::now()),
        protectorate_quota::TaskSize::Medium,
        Utc::now(),
    );
    suggestions
        .into_iter()
        .find_map(|suggestion| {
            if !suggestion.recommended {
                return None;
            }
            let harness = match suggestion.harness {
                protectorate_quota::Harness::Claude => Harness::Claude,
                protectorate_quota::Harness::Codex => Harness::Codex,
                protectorate_quota::Harness::Grok => Harness::Grok,
                protectorate_quota::Harness::Pi => Harness::Pi,
            };
            (harness != from).then_some(harness)
        })
        .ok_or_else(|| {
            RpcError::new(
                Code::FailedPrecondition,
                format!(
                    "quota suggest recommends no harness other than {}; pass --to",
                    harness_name(from)
                ),
            )
        })
}

fn resleeve_launch_request(old: &Record, harness: Harness, selection: &Value) -> Value {
    let mut request = json!({
        "name":old.name,"workspace":old.workspace,"harness":harness.proto(),
        "mode":"SLEEVE_MODE_CHAT","supervised":old.supervised,
        "limits":old.limits,"readOnly":old.read_only,"nested":old.nested,
        "env":old.env,
    });
    if selection.is_object() {
        request["selection"] = selection.clone();
    }
    request
}

fn resleeve_facts(
    hub: &Hub,
    old: &Record,
    to: Harness,
    status: &Value,
) -> protectorate_handoff::Facts {
    let session_id = string(status, "harnessSessionId");
    let mut facts = protectorate_handoff::Facts {
        sleeve: old.name.clone(),
        harness: harness_name(old.harness).into(),
        session_id: session_id.clone(),
        model: string(status, "model"),
        provider: string(status, "provider"),
        workspace: PathBuf::from(&old.workspace),
        to: harness_name(to).into(),
        notes_file: protectorate_handoff::has_notes(&old.workspace),
        ..Default::default()
    };
    let home = hub
        .workspaces
        .state_root()
        .join("sleeves")
        .join(&old.name)
        .join("home");
    if !session_id.is_empty() {
        match protectorate_handoff::read_transcript(&home, harness_name(old.harness), &session_id) {
            Ok(transcript) => facts.transcript = transcript,
            Err(error) => facts.transcript_err = error.to_string(),
        }
    } else {
        facts.transcript_err = "outgoing harness has no native session".into();
    }
    if facts.transcript.turns.is_empty() {
        facts.transcript = chatlog_tail(
            hub.workspaces
                .state_root()
                .join("sleeves")
                .join(&old.name)
                .join("chat.jsonl"),
        );
    }
    if let Some(result) = &old.result {
        let text = string(result, "summary");
        if !text.is_empty() {
            facts.hub_notes.push(format!(
                "Latest result: {}",
                protectorate_handoff::clip(&text, 1000)
            ));
        }
    }
    if let Ok(notes) = hub.backlog.list(Default::default()) {
        for note in notes
            .into_iter()
            .filter(|note| note.project == old.name || note.source == old.name)
            .take(10)
        {
            facts.hub_notes.push(format!(
                "{}: {} — {}",
                note.id,
                note.title,
                protectorate_handoff::clip(&note.body, 300)
            ));
        }
    }
    read_resleeve_git(hub, &mut facts);
    facts
}

// A single read-only namespace contains every Git subcommand. Repository
// filters can still execute, but cannot write the workspace or see host state.
const GIT_FACT_SCRIPT: &str = r#"
run() {
  name="$1"; shift
  printf '%s ' "$name"
  /usr/bin/timeout 2s /usr/bin/git --no-optional-locks -c core.fsmonitor=false -c core.hooksPath=/dev/null -c diff.external= "$@" 2>/dev/null | /usr/bin/head -c 16384 | /usr/bin/base64 -w0
  code=${PIPESTATUS[0]}
  printf ' %s\n' "$code"
}
run status status --short --branch --ignore-submodules=all --untracked-files=normal
run branch symbolic-ref --short -q HEAD
run head rev-parse --short HEAD
run diff diff --no-ext-diff --no-textconv --stat --ignore-submodules=all HEAD
run log log --no-show-signature --format='%h %ad %s' --date=short -n 10
"#;

fn read_resleeve_git(hub: &Hub, facts: &mut protectorate_handoff::Facts) {
    if let Err(error) = sandboxed_git_facts(hub, facts) {
        facts.git_err = error;
    }
}

fn sandboxed_git_facts(
    hub: &Hub,
    facts: &mut protectorate_handoff::Facts,
) -> std::result::Result<(), String> {
    use base64::Engine;
    let launcher = hub
        .chat_runtime
        .as_ref()
        .ok_or("Git helper sandbox is not configured")?
        .launcher();
    let home = launcher
        .state_root
        .join("helpers")
        .join(format!("git-{}", facts.sleeve))
        .join("home");
    fs::create_dir_all(&home).map_err(|error| format!("Git helper home: {error}"))?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Git helper home permissions: {error}"))?;
    let policy = Policy {
        state_root: launcher.state_root.clone(),
        user_home: launcher.user_home.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let plan = policy
        .plan(Request {
            workload: Workload::ReadOnlyHelper(Helper::Git),
            command: vec![
                "/bin/bash".into(),
                "--noprofile".into(),
                "--norc".into(),
                "-c".into(),
                GIT_FACT_SCRIPT.into(),
            ],
            workspace: Some(facts.workspace.clone()),
            managed_workspace_name: protectorate_sandbox::managed_workspace_name(
                &launcher.state_root,
                &facts.workspace,
            ),
            private_home: Some(home),
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            egress: None,
            limits: SandboxLimits {
                cpu_percent: 100,
                memory_bytes: 128 << 20,
                tasks_max: 24,
            },
        })
        .map_err(|error| format!("Git helper plan: {error}"))?;
    let mut options = LaunchOptions::new(
        &launcher.helper,
        &launcher.runtime_dir,
        &launcher.user_home,
        &facts.workspace,
    );
    options.setup_timeout = Duration::from_secs(5);
    options.environment = BTreeMap::from([
        (
            "HOME".into(),
            launcher.user_home.to_string_lossy().into_owned(),
        ),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
        ("GIT_ATTR_NOSYSTEM".into(), "1".into()),
    ]);
    let mut child = protectorate_sandbox::runtime::spawn(&plan, &options)
        .map_err(|error| format!("Git helper launch: {error}"))?;
    let stdout = child.take_stdout().ok_or("Git helper has no stdout")?;
    let stderr = child.take_stderr().ok_or("Git helper has no stderr")?;
    let out_reader = thread::spawn(move || read_git_output(stdout, 128 * 1024));
    let err_reader = thread::spawn(move || read_git_output(stderr, 16 * 1024));
    let kill = child.kill_handle();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    let status = match rx.recv_timeout(Duration::from_secs(12)) {
        Ok(status) => status.map_err(|error| format!("Git helper wait: {error}"))?,
        Err(_) => {
            kill.kill();
            return Err("Git helper timed out".into());
        }
    };
    let output = out_reader
        .join()
        .map_err(|_| "Git helper output reader panicked")?
        .map_err(|error| format!("Git helper output: {error}"))?;
    let _ = err_reader.join();
    if !status.success() {
        return Err(format!("Git helper exited {status}"));
    }
    let output = String::from_utf8(output).map_err(|_| "Git helper output is not UTF-8")?;
    let mut seen = BTreeSet::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, ' ');
        let name = parts.next().ok_or("Git helper output is malformed")?;
        if !seen.insert(name) {
            return Err("Git helper returned duplicate facts".into());
        }
        let encoded = parts.next().ok_or("Git helper output is malformed")?;
        let code = parts.next().ok_or("Git helper output is malformed")?;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "Git helper output has invalid base64")?;
        let value = String::from_utf8_lossy(&raw).trim_end().to_owned();
        if code != "0" {
            if name == "status" {
                return Err(format!("git {name} failed in sandbox ({code})"));
            }
            if matches!(name, "diff" | "log") && facts.git_err.is_empty() {
                facts.git_err = format!("git {name} failed in sandbox ({code})");
            }
        }
        match name {
            "status" => facts.status = value,
            "branch" => facts.branch = value,
            "head" => facts.head = value,
            "diff" => facts.diff_stat = value,
            "log" => facts.log = value,
            _ => return Err("Git helper returned an unknown fact".into()),
        }
    }
    if seen.len() != 5 {
        return Err("Git helper returned incomplete facts".into());
    }
    Ok(())
}

fn read_git_output(mut reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut overflow = false;
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let available = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..count.min(available)]);
        overflow |= count > available;
    }
    if overflow {
        Err(io::Error::other("Git helper output exceeded limit"))
    } else {
        Ok(bytes)
    }
}

fn chatlog_tail(path: PathBuf) -> protectorate_handoff::Transcript {
    use std::collections::VecDeque;
    let Ok(mut file) = fs::File::open(path) else {
        return Default::default();
    };
    let Ok(size) = file.metadata().map(|m| m.len()) else {
        return Default::default();
    };
    let partial = size > 2 << 20;
    if partial && file.seek(SeekFrom::Start(size - (2 << 20))).is_err() {
        return Default::default();
    }
    let mut turns = VecDeque::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let Ok(line) = line else { break };
        if partial && index == 0 {
            continue;
        }
        let Ok(event) = serde_json::from_str::<protectorate_chatlog::Event>(&line) else {
            continue;
        };
        let (role, text) = if let Some(text) = event.payload["user"]["text"].as_str() {
            ("user", text)
        } else if event.payload["text"]["thinking"] != true {
            (
                "assistant",
                event.payload["text"]["delta"].as_str().unwrap_or(""),
            )
        } else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if turns.len() == 24 {
            turns.pop_front();
        }
        turns.push_back(protectorate_handoff::Turn {
            role: role.into(),
            text: protectorate_handoff::clip(text, 1200),
        });
    }
    protectorate_handoff::Transcript {
        path: String::new(),
        turns: turns.into(),
    }
}

fn handoff_error(error: protectorate_handoff::Error) -> RpcError {
    RpcError::new(Code::FailedPrecondition, error.to_string())
}

#[derive(Clone, Debug)]
pub struct TerminalLauncher {
    pub state_root: PathBuf,
    pub user_home: PathBuf,
    pub runtime_dir: PathBuf,
    pub runner: PathBuf,
    pub helper: PathBuf,
    pub hub_url: String,
    pub cli_executable: Option<PathBuf>,
    pub tool_roots: Vec<PathBuf>,
    pub flutter_sdk_root: Option<PathBuf>,
    pub flutter_cache_lower: Option<PathBuf>,
    pub flutter_tools_lower: Option<PathBuf>,
    pub pub_cache_lower: Option<PathBuf>,
    pub go_mod_cache_lower: Option<PathBuf>,
    pub tasks_max: u32,
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    // A terminal may exit without another SleeveService call. Reconcile it in
    // the background so its scoped hub credential is not left active.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        branch::start_watcher(&runtime, &hub);
        crate::chat::start_idle_reaper(&runtime, &hub);
        crate::envoy::start_watcher(&runtime, &hub);
        let weak = Arc::downgrade(&hub);
        runtime.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let Some(hub) = weak.upgrade() else { break };
                let _ = tokio::task::spawn_blocking(move || {
                    for row in hub.sleeves.list() {
                        if row.state == State::Running && row.mode == Mode::Terminal {
                            refresh_record(&hub, &row);
                        }
                    }
                })
                .await;
            }
        });
        let weak = Arc::downgrade(&hub);
        // Running sleeves with a disk limit are sampled every tick; every
        // sleeve is sampled for status every 30th tick.
        runtime.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut warned = BTreeSet::new();
            for tick in 0_u64.. {
                interval.tick().await;
                let Some(hub) = weak.upgrade() else { break };
                match tokio::task::spawn_blocking(move || {
                    sample_sleeve_disks(&hub, tick % 30 == 0, &mut warned);
                    warned
                })
                .await
                {
                    Ok(kept) => warned = kept,
                    Err(_) => break,
                }
            }
        });
    }
    let list = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.SleeveService/ListSleeves",
        move |request| {
            let hub = Arc::clone(&list);
            async move {
                let sleeves = hub
                    .sleeves
                    .list()
                    .iter()
                    .map(|row| refresh_record(&hub, row))
                    .map(|row| sleeve_json(&row))
                    .collect::<Vec<_>>();
                respond(&request, json!({"sleeves": sleeves}))
            }
        },
    )?;
    let get = Arc::clone(&hub);
    server.register("/protectorate.v1.SleeveService/GetSleeve", move |request| {
        let hub = Arc::clone(&get);
        async move {
            let name = string(&input(&request)?, "name");
            let sleeve = hub.sleeves.get(&name).map_err(registry_error)?;
            if sleeve.helper {
                return Err(RpcError::new(
                    Code::NotFound,
                    format!("sleeve not found: {name}"),
                ));
            }
            respond(
                &request,
                json!({"sleeve": sleeve_json(&refresh_record(&hub, &sleeve))}),
            )
        }
    })?;
    let spawn = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.SleeveService/SpawnSleeve",
        move |request| {
            let hub = Arc::clone(&spawn);
            async move {
                let mut value = input(&request)?;
                value["requester"] = crate::approvals::requester(&hub, &request);
                let parent = SpawnParent::from_auth(&request.auth);
                request_env(&value)?;
                let exit_after_idle = crate::chat::parse_exit_after_idle(&value)?;
                if !exit_after_idle.is_zero()
                    && (string(&value, "mode") == "SLEEVE_MODE_TERMINAL"
                        || string(&value, "harness") == "HARNESS_SHELL")
                {
                    return Err(RpcError::new(
                        Code::InvalidArgument,
                        "exit-after-idle requires chat mode",
                    ));
                }
                if string(&value, "name") == "envoy" {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "envoy is a reserved sleeve name",
                    ));
                }
                if hub
                    .resleeving
                    .lock()
                    .unwrap()
                    .contains(&string(&value, "name"))
                {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "sleeve is being resleeved",
                    ));
                }
                let directory_kick = Arc::clone(&hub.directory_kick);
                let row = tokio::task::spawn_blocking(move || {
                    let mut prepared = branch::prepare(&hub, &value)?;
                    let mode = string(&prepared.request, "mode");
                    let harness = string(&prepared.request, "harness");
                    let result = match (mode.as_str(), harness.as_str()) {
                        ("SLEEVE_MODE_TERMINAL", _) | (_, "HARNESS_SHELL") => {
                            spawn_terminal(&hub, &prepared.request, &parent)
                        }
                        (_, "HARNESS_PI") => {
                            crate::chat::spawn_pi(&hub, &prepared.request, &parent)
                        }
                        (_, "HARNESS_CLAUDE") => {
                            crate::chat::spawn_claude(&hub, &prepared.request, &parent)
                        }
                        (_, "HARNESS_CODEX") => {
                            crate::chat::spawn_codex(&hub, &prepared.request, &parent)
                        }
                        (_, "HARNESS_GROK") => {
                            crate::chat::spawn_grok(&hub, &prepared.request, &parent)
                        }
                        _ => Err(RpcError::new(Code::InvalidArgument, "invalid harness")),
                    };
                    if result.is_ok() {
                        prepared.commit();
                    }
                    result
                })
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                directory_kick.notify_one();
                respond(&request, json!({"sleeve": sleeve_json(&row)}))
            }
        },
    )?;
    let kill = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.SleeveService/KillSleeve",
        move |request| {
            let hub = Arc::clone(&kill);
            async move {
                let value = input(&request)?;
                let name = string(&value, "name");
                let auth = request.auth.clone();
                if hub.resleeving.lock().unwrap().contains(&name) {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "sleeve is being resleeved",
                    ));
                }
                let directory_kick = Arc::clone(&hub.directory_kick);
                let drop_caches = value.get("dropCaches").and_then(Value::as_bool) == Some(true);
                if drop_caches && !request.auth.owner {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "dropping caches requires the owner token",
                    ));
                }
                tokio::task::spawn_blocking(move || {
                    let _guard = hub.sleeve_launch_lock.lock().unwrap();
                    if !auth.owner && !auth.device && !has_kill_scope(&auth) {
                        let row = hub.sleeves.get(&name).map_err(registry_error)?;
                        if name == "envoy" || !spawned_by(&auth, &row) {
                            return Err(RpcError::new(
                                Code::PermissionDenied,
                                "sleeve is owned by another identity",
                            ));
                        }
                    }
                    if hub.resleeving.lock().unwrap().contains(&name) {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "sleeve is being resleeved",
                        ));
                    }
                    if hub
                        .sleeves
                        .get(&name)
                        .is_ok_and(|row| row.mode == Mode::Chat)
                    {
                        crate::chat::kill_pi(&hub, &name)?;
                    } else {
                        kill_shell(&hub, &name)?;
                    }
                    if drop_caches {
                        // A respawn cannot start until both stop and deletion
                        // have finished under the same launch lock.
                        let row = hub.sleeves.get(&name).map_err(registry_error)?;
                        if row.state == State::Running {
                            return Err(RpcError::new(
                                Code::FailedPrecondition,
                                "sleeve is still running",
                            ));
                        }
                        drop_home_caches(&hub, &name)?;
                    }
                    Ok::<(), RpcError>(())
                })
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                directory_kick.notify_one();
                respond(&request, json!({}))
            }
        },
    )?;
    let prune = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.SleeveService/PruneSleeves",
        move |request| {
            let hub = Arc::clone(&prune);
            async move {
                let value = input(&request)?;
                let entries = tokio::task::spawn_blocking(move || prune_sleeves(&hub, &value))
                    .await
                    .map_err(|error| RpcError::new(Code::Internal, error.to_string()))??;
                respond(&request, json!({"entries": entries}))
            }
        },
    )?;
    let resleeve = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.SleeveService/ResleeveSleeve",
        move |request| {
            let hub = Arc::clone(&resleeve);
            async move {
                let mut value = input(&request)?;
                value["requester"] = crate::approvals::requester(&hub, &request);
                let row = tokio::task::spawn_blocking(move || resleeve_sleeve(&hub, &value))
                    .await
                    .map_err(|error| RpcError::new(Code::Internal, error.to_string()))??;
                respond(&request, json!({"sleeve":sleeve_json(&row)}))
            }
        },
    )?;
    server.register(
        "/protectorate.v1.SleeveService/SpawnEnvoy",
        move |request| {
            let hub = Arc::clone(&hub);
            async move {
                let value = input(&request)?;
                let work = Arc::clone(&hub);
                let result = tokio::task::spawn_blocking(move || spawn_envoy(&work, &value))
                    .await
                    .map_err(|error| RpcError::new(Code::Internal, error.to_string()))??;
                let harness = result["sleeve"]["harness"]
                    .as_str()
                    .unwrap_or_default()
                    .trim_start_matches("HARNESS_")
                    .to_ascii_lowercase();
                let replaced = result["replaced"].as_bool().unwrap_or(false);
                let response = respond(&request, result)?;
                hub.audit.write(AuditEntry {
                    actor: request.auth.actor.clone(),
                    token_id: request.auth.token_id.clone(),
                    task: request.auth.task.clone(),
                    procedure: request.method.procedure.clone(),
                    subject: "envoy".into(),
                    outcome: "ok".into(),
                    detail: format!("harness={harness} replaced={replaced}"),
                    ..AuditEntry::default()
                });
                Ok(response)
            }
        },
    )?;
    Ok(())
}

fn spawn_envoy(hub: &Hub, value: &Value) -> Result<Value> {
    let _envoy_guard = hub.envoy_spawn_lock.lock().unwrap();
    if hub.resleeving.lock().unwrap().contains("envoy") {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "envoy is being resleeved",
        ));
    }
    if hub.chat_runtime.is_none() {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "chat sleeves are not configured",
        ));
    }
    let catalog = protectorate_wire::Catalog::embedded()?;
    let methods = catalog.methods().cloned().collect::<Vec<_>>();
    let harness_name = string(value, "harness");
    let (harness, reason) = match harness_name.as_str() {
        "" | "HARNESS_UNSPECIFIED" => {
            let suggestions = protectorate_quota::rank_harnesses(
                &hub.quota.snapshot(Utc::now()),
                protectorate_quota::TaskSize::Medium,
                Utc::now(),
            );
            let suggestion = suggestions
                .into_iter()
                .find(|s| s.recommended)
                .ok_or_else(|| {
                    RpcError::new(
                        Code::FailedPrecondition,
                        "no harness is recommended; choose one explicitly",
                    )
                })?;
            let harness = match suggestion.harness {
                protectorate_quota::Harness::Claude => Harness::Claude,
                protectorate_quota::Harness::Codex => Harness::Codex,
                protectorate_quota::Harness::Grok => Harness::Grok,
                protectorate_quota::Harness::Pi => Harness::Pi,
            };
            (harness, suggestion.reason)
        }
        "HARNESS_CLAUDE" => (Harness::Claude, "chosen explicitly".into()),
        "HARNESS_CODEX" => (Harness::Codex, "chosen explicitly".into()),
        "HARNESS_GROK" => (Harness::Grok, "chosen explicitly".into()),
        "HARNESS_PI" => (Harness::Pi, "chosen explicitly".into()),
        "HARNESS_SHELL" => {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "shell cannot run Envoy",
            ))
        }
        _ => {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "invalid Envoy harness",
            ))
        }
    };
    let selection = &value["selection"];
    if selection.is_object() && selection.as_object().is_some_and(|o| !o.is_empty()) {
        if harness != Harness::Codex || harness_name != "HARNESS_CODEX" {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "choose Codex explicitly for model/effort selection",
            ));
        }
        if string(selection, "model").len() > 256 || string(selection, "effort").len() > 64 {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "selection is too long",
            ));
        }
    }
    let ttl = if value.get("ttl").is_some() {
        super::parse_duration(value.get("ttl"))?
    } else {
        Duration::from_secs(720 * 3600)
    };
    let mut scopes = value
        .get("scopes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|scopes| !scopes.is_empty())
        .unwrap_or_else(|| envoy_default_scopes(&methods));
    scopes.extend(
        value
            .get("addScopes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    scopes.sort();
    scopes.dedup();
    let scope_methods = super::scope_methods(&methods);
    protectorate_auth::validate_scopes(
        &scopes.iter().map(String::as_str).collect::<Vec<_>>(),
        &scope_methods,
    )
    .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))?;
    let hub_url = crate::chat::hub_url(hub)
        .ok_or_else(|| RpcError::new(Code::FailedPrecondition, "chat hub URL is not configured"))?;
    let workspace = hub.workspaces.state_root().join("workspaces/envoy");
    fs::create_dir_all(&workspace).map_err(envoy_io)?;
    if fs::canonicalize(&workspace).map_err(envoy_io)? != workspace {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "Envoy workspace path contains a symlink",
        ));
    }
    let metadata = fs::symlink_metadata(&workspace).map_err(envoy_io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "Envoy workspace is not a plain directory",
        ));
    }
    fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700)).map_err(envoy_io)?;
    let replaced = if let Ok(row) = hub.sleeves.get("envoy") {
        if refresh_record(hub, &row).state == State::Running {
            if row.mode != Mode::Chat {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "Envoy record is not a chat sleeve",
                ));
            }
            let _guard = hub.sleeve_launch_lock.lock().unwrap();
            crate::chat::kill_pi(hub, "envoy")?;
            true
        } else {
            false
        }
    } else {
        false
    };
    for token in hub.identity.list() {
        if token.name == "orchestrator-envoy" && token.revoked_at.is_none() {
            hub.identity
                .revoke(&token.id, super::now_unix())
                .map_err(super::token_error)?;
        }
    }
    let issued = hub
        .identity
        .create(
            "orchestrator-envoy",
            &scopes.iter().map(String::as_str).collect::<Vec<_>>(),
            ttl,
            super::now_unix(),
            &scope_methods,
        )
        .map_err(super::token_create_error)?;
    let token_path = workspace.join(".protectorate-token");
    let launch = || -> Result<Record> {
        envoy_write_if_changed(
            &token_path,
            format!("{}\n", issued.secret.expose()).as_bytes(),
            0o600,
        )?;
        let agents = envoy_agents(&hub_url, &scopes, &methods);
        envoy_write_if_changed(&workspace.join("AGENTS.md"), agents.as_bytes(), 0o644)?;
        let hosts = fs::read_to_string(hub.workspaces.state_root().join("hosts.md"))
            .unwrap_or_else(|_| {
                "# Hosts\n\nThe hub has not written an inventory yet. Run `protectorate node ls`.\n"
                    .into()
            });
        envoy_write_if_changed(&workspace.join("hosts.md"), hosts.as_bytes(), 0o644)?;
        envoy_claude_link(&workspace)?;
        crate::chat::spawn_envoy(
            hub,
            &json!({
                "name":"envoy", "workspace":workspace, "harness":harness.proto(),
                "mode":"SLEEVE_MODE_CHAT", "selection":selection,
                "requester": {"actor":"owner", "gated":false},
                "tokenBudget": value["tokenBudget"],
            }),
            harness,
        )
    };
    let sleeve = match launch() {
        Ok(row) => row,
        Err(error) => {
            let _ = hub.identity.revoke(&issued.token.id, super::now_unix());
            let _ = fs::remove_file(&token_path);
            return Err(error);
        }
    };
    hub.directory_kick.notify_one();
    Ok(
        json!({"sleeve":sleeve_json(&sleeve),"token":token_json(&issued.token),"workspace":workspace,"reason":reason,"replaced":replaced}),
    )
}

fn envoy_default_scopes(methods: &[protectorate_wire::Method]) -> Vec<String> {
    let mut scopes = BTreeSet::from(["read".to_owned(), "events".to_owned()]);
    for method in methods {
        if method.read_only || method.destructive || method.owner_only {
            continue;
        }
        let tool = method.tool.as_str();
        if tool.starts_with("backlog_")
            || tool.starts_with("lease_")
            || tool.starts_with("message_")
            || matches!(
                tool,
                "sleeve_spawn_sleeve" | "sleeve_resleeve_sleeve" | "chat_report" | "grant_admin"
            )
        {
            scopes.insert(tool.to_owned());
        }
    }
    scopes.into_iter().collect()
}

fn envoy_agents(hub_url: &str, scopes: &[String], methods: &[protectorate_wire::Method]) -> String {
    let mut out = format!("# Envoy, the Protectorate orchestrator\n\nGenerated by the hub; edits here are lost. Put standing notes in `NOTES.md` and read them at the start of a session.\n\n## Role\n\nYou run in the isolated `envoy` chat sleeve. Use `protectorate` to coordinate work through the hub at `{hub_url}`. The token is in `PROTECTORATE_TOKEN_FILE`; never print, copy or move it. Use `--json` when parsing output.\n\nRead `hosts.md` for host inventory. Start work through `protectorate sleeve spawn`, which runs in a sandbox. Raw host sessions are outside the sandbox and are not granted to you. Destructive calls, admin grants and admin jobs wait for the owner's approval: the CLI prints the request id and waits, and a denial or expiry fails the command. Do not repeat a denied call. Never widen your own permissions or work around a permission denial.\n\n## How to decide\n\n");
    const WORKFLOW: &[(&str, &str)] = &[
        (
            "quota suggest --for medium",
            "rank harnesses by fresh usable capacity",
        ),
        ("quota", "inspect usage windows, resets and freshness"),
        (
            "backlog ls",
            "read open notes, pinned first; --all includes done notes",
        ),
        ("backlog show", "read one note in full"),
        ("sleeve ls", "inspect running sleeves and resources"),
        ("sleeve spawn", "start a harness in an isolated workspace"),
        (
            "chat wait",
            "wait with --until report for results; partial and blocked mean work remains",
        ),
        (
            "sleeve resleeve",
            "move a chat sleeve with a hub-written handoff",
        ),
        ("session capture", "read a sleeve pane without attaching"),
        ("backlog edit", "update a note when the owner asks"),
        ("backlog done", "close completed work with --resolution"),
        ("backlog reopen", "return a note to open work"),
        ("events", "stream hub events"),
    ];
    for (command, purpose) in WORKFLOW {
        out.push_str(&format!("- `protectorate {command}`: {purpose}\n"));
    }
    out.push_str("\nTypical loop: read open notes, check fresh quota, spawn one sleeve for one piece of work, wait with `protectorate chat wait <name> --until report`, then report back. Leave partial or blocked notes open. Close completed work with `protectorate backlog done <id> --resolution TEXT`.\n\n## Coordination\n\n");
    out.push_str(protectorate_mailbox::CONCISE_GUIDANCE);
    out.push_str("\n\n");
    out.push_str(protectorate_mailbox::SUBAGENT_GUIDANCE);
    out.push_str("\n\nAfter sending a request, end your turn. Replies arrive as a new turn. Do not sleep-poll the inbox or send progress checks for work younger than about ten minutes. For an explicit blocking workflow, use `protectorate msg wait <message-id|thread> --timeout 10m`. Every brief should state a word limit and use a real path from `protectorate sleeve workspaces`; say 'no reply needed' when appropriate.\n\n## Granted scopes\n\n");
    let mut sorted_scopes = scopes.to_vec();
    sorted_scopes.sort();
    for scope in &sorted_scopes {
        out.push_str(&format!("- `{scope}`\n"));
    }
    out.push_str("\nA call outside these scopes fails with `permission_denied`. The hub renews the token file automatically.\n\n## Hub tools\n\nEach tool can also be called with `protectorate api call <Service>/<Method> '<json>'`.\n\n| Tool | Access | Granted | Description |\n|------|--------|---------|-------------|\n");
    let mut tools: Vec<_> = methods
        .iter()
        .filter(|method| !method.owner_only && !method.node_local && method.is_unary())
        .collect();
    tools.sort_by(|left, right| left.tool.cmp(&right.tool));
    let scope_refs = scopes.iter().map(String::as_str).collect::<Vec<_>>();
    for method in tools {
        let access = if method.read_only {
            "read-only"
        } else if method.destructive {
            "destructive"
        } else {
            "mutating"
        };
        let granted = super::auth::scopes_allow(
            &scope_refs,
            &super::ScopeMethod {
                tool: &method.tool,
                service: &method.service,
                read_only: method.read_only,
                owner_only: method.owner_only,
            },
        );
        let description = method.description().replace('|', "/").replace('\n', " ");
        out.push_str(&format!(
            "| `{}` | {access} | {} | {description} |\n",
            method.tool,
            if granted { "yes" } else { "no" }
        ));
    }
    out.push_str("\n## CLI reference\n\nGlobal flags: `--hub URL`, `--token-file FILE`, `--json`. Run `protectorate --help` for the full command tree.\n\n");
    const COMMANDS: &[(&str, &str)] = &[
        ("quota suggest --for medium", "Rank harnesses by capacity; use --refresh when data is stale."),
        ("backlog ls --all", "List open or all notes. Use --project, --query and --state to filter."),
        ("backlog show <id>", "Read one note and its body."),
        ("sleeve workspaces", "List usable workspace paths before writing a brief."),
        ("sleeve spawn <name> --workspace <path> --brief <file>", "Start an isolated worker. Optional --harness, --branch, --new-branch, --cpu, --mem, --model and --effort."),
        ("chat wait <name> --until report", "Block until a worker reports. Optional --timeout DURATION."),
        ("sleeve resleeve <name> --to auto", "Move a chat worker with a handoff. Optional --interrupt and --wait."),
        ("session capture <name> --lines N", "Read a pane and scrollback."),
        ("backlog edit <id> --append TEXT", "Update a note when requested; optional --body and --body-file."),
        ("backlog done <id> --resolution TEXT", "Close completed work."),
        ("backlog reopen <id>", "Return a note to open work."),
        ("msg send <to> <body>", "Send a request or notice; optional --thread, --kind, --ack and --request-id."),
        ("msg wait <message-id|thread> --timeout 10m", "Wait explicitly for a reply or acknowledgment."),
        ("report --status complete|partial|blocked --summary-file <file>", "Report your result; use partial honestly when work remains."),
    ];
    for (command, description) in COMMANDS {
        out.push_str(&format!("### protectorate {command}\n\n{description}\n\n"));
    }
    out
}

pub(super) fn envoy_write_if_changed(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            if fs::read(path).is_ok_and(|old| old == contents) {
                return Ok(());
            }
        }
        Ok(metadata) if metadata.is_dir() => {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("{} is a directory", path.display()),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(envoy_io(error)),
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("new-{}-{nonce}", std::process::id()));
    let write = || -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    };
    if let Err(error) = write() {
        let _ = fs::remove_file(&temporary);
        return Err(envoy_io(error));
    }
    Ok(())
}

fn envoy_claude_link(workspace: &Path) -> Result<()> {
    let link = workspace.join("CLAUDE.md");
    match fs::symlink_metadata(&link) {
        Ok(meta)
            if meta.file_type().is_symlink()
                && fs::read_link(&link).is_ok_and(|to| to == Path::new("AGENTS.md")) =>
        {
            return Ok(())
        }
        Ok(_) => {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            fs::rename(&link, workspace.join(format!("CLAUDE.md.orphaned-{nonce}")))
                .map_err(envoy_io)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(envoy_io(error)),
    }
    std::os::unix::fs::symlink("AGENTS.md", link).map_err(envoy_io)
}

fn envoy_io(error: io::Error) -> RpcError {
    RpcError::new(Code::Internal, format!("Envoy workspace: {error}"))
}

pub(super) fn refresh_record(hub: &Hub, row: &Record) -> Record {
    let on_exit = |record: Record| {
        if record.state == State::Exited {
            hub.directory_kick.notify_one();
        }
        record
    };
    if row.state == State::Running && row.mode == Mode::Chat {
        if let Some(runtime) = &hub.chat_runtime {
            if runtime.exited(&row.name) {
                let _ = crate::chat::revoke_identity(hub, &row.name);
                return on_exit(
                    hub.sleeves
                        .record_exit(&row.name, now(), None, 0, false, false)
                        .unwrap_or_else(|_| row.clone()),
                );
            }
            if let Some(status) = runtime.status(hub, &row.name) {
                return hub
                    .sleeves
                    .set_chat(&row.name, status)
                    .unwrap_or_else(|_| row.clone());
            }
        }
        return row.clone();
    }
    if row.state != State::Running || row.mode != Mode::Terminal {
        return row.clone();
    }
    match hub.sessions.get(&row.session) {
        Ok(session) if session.state == SessionState::Exited => {
            if let Err(error) = revoke_terminal_identity(hub, &row.name) {
                eprintln!("revoke exited sleeve {}: {error}", row.name);
            }
            on_exit(
                hub.sleeves
                    .record_exit(
                        &row.name,
                        now(),
                        Some(session.exit_code),
                        session.exit_signal,
                        false,
                        false,
                    )
                    .unwrap_or_else(|_| row.clone()),
            )
        }
        Err(SessionError::NotFound | SessionError::NoServer) => {
            if let Err(error) = revoke_terminal_identity(hub, &row.name) {
                eprintln!("revoke missing sleeve {}: {error}", row.name);
            }
            on_exit(
                hub.sleeves
                    .record_exit(&row.name, now(), None, 0, false, false)
                    .unwrap_or_else(|_| row.clone()),
            )
        }
        _ => row.clone(),
    }
}

fn spawn_terminal(hub: &Hub, value: &Value, parent: &SpawnParent) -> Result<Record> {
    let launcher = hub.terminal_launcher.as_ref().ok_or_else(|| {
        RpcError::new(
            Code::Unavailable,
            "terminal sleeve launcher is not configured",
        )
    })?;
    let _guard = hub.sleeve_launch_lock.lock().unwrap();
    let name = string(value, "name");
    protectorate_sleeve_registry::valid_name(&name).map_err(registry_error)?;
    let harness = match string(value, "harness").as_str() {
        "HARNESS_SHELL" => Harness::Shell,
        "HARNESS_PI" => Harness::Pi,
        "HARNESS_CLAUDE" => Harness::Claude,
        "HARNESS_CODEX" => Harness::Codex,
        "HARNESS_GROK" => Harness::Grok,
        _ => return Err(RpcError::new(Code::InvalidArgument, "invalid harness")),
    };
    let mode = string(value, "mode");
    if !mode.is_empty() && mode != "SLEEVE_MODE_UNSPECIFIED" && mode != "SLEEVE_MODE_TERMINAL" {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "terminal sleeves require terminal mode",
        ));
    }
    if value
        .get("supervised")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "supervised requires chat mode",
        ));
    }
    if value
        .get("brief")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "brief requires chat mode",
        ));
    }
    if number(value, "tokenBudget")? > 0 {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "a token budget requires chat mode, where the harness reports usage",
        ));
    }
    let workspace = PathBuf::from(string(value, "workspace"));
    if !workspace.is_absolute() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "workspace must be an absolute local directory",
        ));
    }
    let workspace = fs::canonicalize(&workspace)
        .map_err(|e| RpcError::new(Code::InvalidArgument, e.to_string()))?;
    if !workspace.is_dir() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "workspace is not a directory",
        ));
    }
    if let Ok(prev) = hub.sleeves.get(&name) {
        if refresh_record(hub, &prev).state == State::Running {
            return Err(RpcError::new(Code::AlreadyExists, "sleeve already running"));
        }
        match hub.sessions.kill(&prev.session) {
            Ok(()) | Err(SessionError::NotFound | SessionError::NoServer) => {}
            Err(e) => return Err(RpcError::new(Code::Internal, e.to_string())),
        }
    }
    let args = value
        .get("args")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut tool_roots = launcher.tool_roots.clone();
    let (command, provider, selected_executable) = if harness == Harness::Shell {
        let mut command = if args.is_empty() {
            vec!["/usr/bin/sh".into()]
        } else {
            args.clone()
        };
        command[0] = resolve_executable(&command[0], &tool_roots)?
            .to_string_lossy()
            .into_owned();
        (command, None, None)
    } else {
        let chat = hub.chat_runtime.as_ref().ok_or_else(|| {
            RpcError::new(Code::Unavailable, "agent harnesses are not configured")
        })?;
        let configured = chat.launcher();
        let (executable, extra_roots, provider) = match harness {
            Harness::Pi => (&configured.pi_executable, &configured.tool_roots, "pi"),
            Harness::Claude => (
                &configured.claude_executable,
                &configured.claude_tool_roots,
                "claude",
            ),
            Harness::Codex => (
                &configured.codex_executable,
                &configured.codex_tool_roots,
                "codex",
            ),
            Harness::Grok => (
                &configured.grok_executable,
                &configured.grok_tool_roots,
                "grok",
            ),
            Harness::Shell => unreachable!(),
        };
        let executable = executable.as_ref().ok_or_else(|| {
            RpcError::new(
                Code::Unavailable,
                format!("{provider} executable is not configured"),
            )
        })?;
        if !executable.is_file() {
            return Err(RpcError::new(
                Code::Unavailable,
                format!("{provider} executable is missing"),
            ));
        }
        let auth = launcher.state_root.join("auth").join(provider);
        if !auth.is_dir() {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("{provider} credential directory is missing"),
            ));
        }
        tool_roots.extend(
            extra_roots
                .iter()
                .filter(|root| {
                    !matches!(root.to_str(), Some("/usr/bin" | "/bin" | "/usr/local/bin"))
                })
                .cloned(),
        );
        let mut command = vec![executable.to_string_lossy().into_owned()];
        if harness == Harness::Codex {
            command.extend(["--disable".into(), "daemon_auto_start".into()]);
        }
        command.extend(args.iter().cloned());
        let selected = (!matches!(
            executable.parent().and_then(Path::to_str),
            Some("/usr/bin" | "/bin" | "/usr/local/bin")
        ))
        .then(|| executable.clone());
        (command, Some(provider.to_owned()), selected)
    };
    tool_roots.sort();
    tool_roots.dedup();
    let limits = value.get("limits").cloned().unwrap_or_else(|| json!({}));
    let cpu_percent = u32::try_from(number(&limits, "cpuPercent")?)
        .map_err(|_| RpcError::new(Code::InvalidArgument, "CPU percent exceeds u32"))?;
    let memory_bytes = number(&limits, "memoryBytes")?;
    let disk_bytes = number(&limits, "diskBytes")?;
    let nested = value
        .get("nested")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut read_only = value
        .get("readOnly")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|item| {
                    let source = string(item, "source");
                    let target = string(item, "target");
                    if source.is_empty() {
                        return Err(RpcError::new(
                            Code::InvalidArgument,
                            "read-only source is empty",
                        ));
                    }
                    Ok(ReadOnly {
                        source: PathBuf::from(&source),
                        target: PathBuf::from(if target.is_empty() { &source } else { &target }),
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let old_base = hub.sleeves.get(&name).ok();
    let base_repo = old_base
        .as_ref()
        .map_or_else(|| string(value, "baseRepo"), |row| row.base_repo.clone());
    let base_grants = branch::base_grants(&launcher.state_root, &name, &base_repo)?;
    read_only.extend(base_grants.into_iter().map(|grant| ReadOnly {
        source: grant.source,
        target: grant.target,
    }));
    let caller_env = request_env(value)?;
    let mut env = caller_env.clone();
    env.insert("PROTECTORATE_HUB".into(), launcher.hub_url.clone());
    env.insert(
        "PROTECTORATE_TOKEN_FILE".into(),
        launcher
            .user_home
            .join(".protectorate-token")
            .to_string_lossy()
            .into_owned(),
    );
    if let Some(toolchain) =
        protectorate_sleeve_runner::rustup_default_toolchain(&launcher.user_home, &tool_roots)
    {
        env.entry("RUSTUP_TOOLCHAIN".into()).or_insert(toolchain);
    }
    if !tool_roots.is_empty() {
        let paths = tool_roots
            .iter()
            .flat_map(|root| {
                let mut paths = vec![root.to_string_lossy().into_owned()];
                if root.join("bin").is_dir() {
                    paths.push(root.join("bin").to_string_lossy().into_owned());
                }
                paths
            })
            .collect::<Vec<_>>()
            .join(":");
        let existing = env
            .get("PATH")
            .cloned()
            .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
        env.insert("PATH".into(), format!("{paths}:{existing}"));
    }
    if launcher.cli_executable.is_some() {
        let existing = env
            .get("PATH")
            .cloned()
            .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
        env.insert("PATH".into(), format!("/opt/protectorate/bin:{existing}"));
    }
    let root = launcher.state_root.join("sleeves").join(&name);
    fs::create_dir_all(root.join("home")).map_err(internal)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    fs::set_permissions(root.join("home"), fs::Permissions::from_mode(0o700)).map_err(internal)?;
    if let Some(provider) = provider.as_deref() {
        prepare_terminal_credentials(&root.join("home"), &launcher.user_home, provider, &mut env)?;
    }
    env.extend(
        protectorate_sleeve_runner::prepare_private_caches(&root.join("home"), &launcher.user_home)
            .map_err(internal)?,
    );
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let ready = root.join(format!("ready-{}-{nonce}", std::process::id()));
    let run_id = new_run_id()?;
    let config_path = root.join(format!("launch-{}-{nonce}.json", std::process::id()));
    let config = Config {
        name: name.clone(),
        state_root: launcher.state_root.clone(),
        user_home: launcher.user_home.clone(),
        runtime_dir: launcher.runtime_dir.clone(),
        helper: launcher.helper.clone(),
        workspace: workspace.clone(),
        command,
        provider,
        selected_executable,
        read_only: read_only.clone(),
        tool_roots,
        flutter_sdk_root: launcher.flutter_sdk_root.clone(),
        flutter_cache_lower: launcher.flutter_cache_lower.clone(),
        flutter_tools_lower: launcher.flutter_tools_lower.clone(),
        pub_cache_lower: launcher.pub_cache_lower.clone(),
        go_mod_cache_lower: launcher.go_mod_cache_lower.clone(),
        cli_executable: launcher.cli_executable.clone(),
        mcpbridge_executable: launcher
            .cli_executable
            .as_deref()
            .and_then(mcpbridge_companion),
        environment: env,
        cpu_percent,
        memory_bytes,
        tasks_max: launcher.tasks_max,
        nested,
        ready_file: ready.clone(),
        egress: Some(sleeve_egress(hub, harness == Harness::Pi)),
        run_id: run_id.clone(),
    };
    protectorate_sleeve_runner::plan(&config)
        .map_err(|e| RpcError::new(Code::InvalidArgument, e.to_string()))?;
    let pin_key = pin_key(Mode::Terminal, harness);
    let pin = Pin {
        executable_sha256: crate::approvals::file_sha256(Path::new(&config.command[0]))?,
        profile_sha256: crate::approvals::json_sha256(&json!({
            "mode": "terminal",
            "command": config.command,
            "provider": config.provider,
            "toolRoots": config.tool_roots,
            "readOnly": config.read_only,
            "cpuPercent": cpu_percent,
            "memoryBytes": memory_bytes,
            "diskBytes": disk_bytes,
            "nested": nested,
            "env": caller_env,
            "egress": config.egress,
            "scopes": SLEEVE_SCOPES,
        })),
    };
    crate::approvals::check_pin(
        hub,
        value,
        &name,
        &run_id,
        &pin_key,
        old_base.as_ref().and_then(|row| row.pins.get(&pin_key)),
        &pin,
    )?;
    issue_terminal_identity(hub, &name)?;
    if let Err(error) = protectorate_sleeve_runner::write_config(&config_path, &config) {
        let _ = revoke_terminal_identity(hub, &name);
        return Err(internal(error));
    }
    let session_name = format!("sleeve-{name}");
    let create = hub.sessions.create(&CreateOptions {
        name: session_name.clone(),
        cwd: Some(workspace.clone()),
        command: vec![
            launcher.runner.to_string_lossy().into_owned(),
            config_path.to_string_lossy().into_owned(),
        ],
        env: BTreeMap::new(),
    });
    if let Err(e) = create {
        let _ = fs::remove_file(&config_path);
        let _ = revoke_terminal_identity(hub, &name);
        return Err(RpcError::new(Code::Unavailable, e.to_string()));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if ready.is_file() {
            break;
        }
        if Instant::now() >= deadline
            || hub
                .sessions
                .get(&session_name)
                .is_ok_and(|s| s.state == SessionState::Exited)
        {
            let tail = hub.sessions.capture(&session_name, 30).unwrap_or_default();
            let _ = hub.sessions.kill(&session_name);
            let _ = fs::remove_file(&config_path);
            let _ = revoke_terminal_identity(hub, &name);
            return Err(RpcError::new(
                Code::Unavailable,
                format!("sleeve sandbox did not start: {tail}"),
            ));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut row =
        Record::new(&name, harness, Mode::Terminal, &workspace, &now()).map_err(|error| {
            let _ = hub.sessions.kill(&session_name);
            let _ = revoke_terminal_identity(hub, &name);
            registry_error(error)
        })?;
    if let Ok(old) = hub.sleeves.get(&name) {
        row.pins = old.pins;
        row.base_repo = old.base_repo;
        row.base_branch = old.base_branch;
        row.base_tip = old.base_tip;
        row.managed_clone = old.managed_clone;
    } else {
        row.base_repo = string(value, "baseRepo");
        row.base_branch = string(value, "baseBranch");
        row.base_tip = string(value, "baseTip");
        row.managed_clone = value["managedClone"] == true;
    }
    row.args = args;
    row.env = caller_env;
    row.limits = Limits {
        cpu_percent,
        memory_bytes,
        disk_bytes,
    };
    row.nested = nested;
    row.run_id = run_id;
    row.pins.insert(pin_key, pin);
    row.parent = parent.label.clone();
    row.parent_token_id = parent.token_id.clone();
    row.read_only = read_only
        .iter()
        .map(|r| ReadOnlyMount {
            source: r.source.to_string_lossy().into_owned(),
            target: r.target.to_string_lossy().into_owned(),
        })
        .collect();
    if let Some(source) = &launcher.cli_executable {
        row.read_only.push(ReadOnlyMount {
            source: source.to_string_lossy().into_owned(),
            target: "/opt/protectorate/bin/protectorate".into(),
        });
    }
    if let Some(source) = &config.mcpbridge_executable {
        row.read_only.push(ReadOnlyMount {
            source: source.to_string_lossy().into_owned(),
            target: "/opt/protectorate/bin/protectorate-mcpbridge".into(),
        });
    }
    let row = hub.sleeves.record_spawned(row).map_err(|e| {
        let _ = hub.sessions.kill(&session_name);
        let _ = revoke_terminal_identity(hub, &name);
        registry_error(e)
    })?;
    hub.emit("sleeve.spawned", &name, json!({"harness":harness_name(harness), "workspace":workspace, "mode":"terminal", "session":session_name}));
    Ok(row)
}

fn mcpbridge_companion(cli: &Path) -> Option<PathBuf> {
    let directory = cli.parent()?;
    let plain = directory.join("protectorate-mcpbridge");
    if plain.is_file() {
        return Some(plain);
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return None,
    };
    let published = directory.join(format!(
        "protectorate-mcpbridge-{}-{arch}",
        std::env::consts::OS
    ));
    published.is_file().then_some(published)
}

fn prepare_terminal_credentials(
    home: &Path,
    user_home: &Path,
    provider: &str,
    env: &mut BTreeMap<String, String>,
) -> Result<()> {
    let (directory, files, variable) = match provider {
        "pi" => {
            fs::create_dir_all(home.join(".pi/sessions")).map_err(internal)?;
            env.insert(
                "PI_CODING_AGENT_DIR".into(),
                "/run/protectorate/auth/pi".into(),
            );
            env.insert(
                "PI_CODING_AGENT_SESSION_DIR".into(),
                user_home
                    .join(".pi/sessions")
                    .to_string_lossy()
                    .into_owned(),
            );
            return Ok(());
        }
        "claude" => (".claude", &[".credentials.json"][..], "CLAUDE_CONFIG_DIR"),
        "codex" => (".codex", &["auth.json"][..], "CODEX_HOME"),
        "grok" => (".grok", &["auth.json", "auth.json.lock"][..], "GROK_HOME"),
        _ => {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "invalid terminal provider",
            ))
        }
    };
    let config = home.join(directory);
    fs::create_dir_all(&config).map_err(internal)?;
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    for file in files {
        let target = format!("/run/protectorate/auth/{provider}/{file}");
        let link = config.join(file);
        match fs::symlink_metadata(&link) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    && fs::read_link(&link).is_ok_and(|actual| actual == Path::new(&target)) => {}
            Ok(_) => {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    format!("terminal credential link {} has changed", link.display()),
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                std::os::unix::fs::symlink(&target, &link).map_err(internal)?;
            }
            Err(error) => return Err(internal(error)),
        }
    }
    env.insert(
        variable.into(),
        user_home.join(directory).to_string_lossy().into_owned(),
    );
    Ok(())
}

fn kill_shell(hub: &Hub, name: &str) -> Result<()> {
    let row = hub.sleeves.get(name).map_err(registry_error)?;
    match hub.sessions.kill(&row.session) {
        Ok(()) | Err(SessionError::NotFound | SessionError::NoServer) => {}
        Err(e) => return Err(RpcError::new(Code::Internal, e.to_string())),
    }
    revoke_terminal_identity(hub, name)?;
    if row.state == State::Running {
        match hub.sleeves.record_exit(name, now(), None, 0, false, true) {
            Ok(_) => {}
            Err(Error::State(_)) => {
                // A read or reconciliation pass may have observed the same
                // process exit after SessionManager killed it.
                hub.sleeves
                    .mark_killed_after_exit(name)
                    .map_err(registry_error)?;
            }
            Err(error) => return Err(registry_error(error)),
        }
        hub.emit("sleeve.exited", name, json!({"reason":"killed"}));
    }
    Ok(())
}

// Only these private-home paths are disposable. The workspace, harness
// transcripts and credentials live outside them and are never traversed.
const HOME_CACHES: [&str; 5] = ["go-build", "go-mod", "pub", "flutter", "overlays"];

fn drop_home_caches(hub: &Hub, name: &str) -> Result<()> {
    protectorate_sleeve_registry::valid_name(name).map_err(registry_error)?;
    let state = hub.workspaces.state_root();
    let state = open_root_directory(state).map_err(cache_error)?;
    let Some(sleeves) = open_child_directory(&state, OsStr::new("sleeves")).map_err(cache_error)?
    else {
        return Ok(());
    };
    let Some(sleeve) = open_child_directory(&sleeves, OsStr::new(name)).map_err(cache_error)?
    else {
        return Ok(());
    };
    if let Some(home) = open_child_directory(&sleeve, OsStr::new("home")).map_err(cache_error)? {
        if let Some(cache) =
            open_child_directory(&home, OsStr::new(".cache")).map_err(cache_error)?
        {
            for name in HOME_CACHES {
                let Some(tree) =
                    open_child_directory(&cache, OsStr::new(name)).map_err(cache_error)?
                else {
                    continue;
                };
                remove_open_tree(&cache, OsStr::new(name), tree).map_err(cache_error)?;
            }
        }
    }
    if let Some(overlays) =
        open_child_directory(&sleeve, OsStr::new("overlays")).map_err(cache_error)?
    {
        remove_open_tree(&sleeve, OsStr::new("overlays"), overlays).map_err(cache_error)?;
    }
    Ok(())
}

/// Inventory private sleeve data while holding the launch lock. A respawn
/// cannot race a destructive prune and a newly started sleeve cannot acquire
/// a path that has already been classified as stopped.
fn prune_sleeves(hub: &Hub, options: &Value) -> Result<Vec<Value>> {
    let _guard = hub.sleeve_launch_lock.lock().unwrap();
    let yes = options.get("yes").and_then(Value::as_bool) == Some(true);
    let homes = options.get("homes").and_then(Value::as_bool) == Some(true);
    let workspaces = options.get("workspaces").and_then(Value::as_bool) == Some(true);
    let caches =
        options.get("caches").and_then(Value::as_bool) == Some(true) || (!homes && !workspaces);
    let show_running = options.get("running").and_then(Value::as_bool) == Some(true);
    let registered = hub
        .sleeves
        .list()
        .into_iter()
        .map(|row| refresh_record(hub, &row))
        .collect::<Vec<_>>();
    let running = registered
        .iter()
        .filter(|row| row.state == State::Running)
        .cloned()
        .collect::<Vec<_>>();
    let state = open_root_directory(hub.workspaces.state_root()).map_err(cache_error)?;
    let helpers = open_child_directory(&state, OsStr::new("helpers")).map_err(cache_error)?;
    let mut entries = Vec::new();
    if let Some(sleeves) =
        open_child_directory(&state, OsStr::new("sleeves")).map_err(cache_error)?
    {
        for dir in
            fs::read_dir(format!("/proc/self/fd/{}", sleeves.as_raw_fd())).map_err(cache_error)?
        {
            let dir = dir.map_err(cache_error)?;
            let name = dir.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if protectorate_sleeve_registry::valid_name(name_str).is_err() {
                continue;
            }
            let Some(sleeve) = open_child_or_skip(&sleeves, &name)? else {
                continue;
            };
            let home_path = hub
                .workspaces
                .state_root()
                .join("sleeves")
                .join(name_str)
                .join("home");
            let busy = prune_busy_reason(&running, name_str, &home_path);
            if busy.is_some() && !show_running {
                continue;
            }
            let home = open_child_or_skip(&sleeve, OsStr::new("home"))?;
            if !homes {
                if let Some(home) = &home {
                    if let Some(cache) = open_child_or_skip(home, OsStr::new(".cache"))? {
                        for cache_name in HOME_CACHES {
                            let path = home_path.join(".cache").join(cache_name);
                            prune_one(
                                &mut entries,
                                name_str,
                                &cache,
                                OsStr::new(cache_name),
                                &path,
                                "cache",
                                caches,
                                yes,
                                busy.as_deref(),
                                true,
                            )?;
                        }
                    }
                }
                let overlay_path = hub
                    .workspaces
                    .state_root()
                    .join("sleeves")
                    .join(name_str)
                    .join("overlays");
                prune_one(
                    &mut entries,
                    name_str,
                    &sleeve,
                    OsStr::new("overlays"),
                    &overlay_path,
                    "cache",
                    caches,
                    yes,
                    busy.as_deref(),
                    true,
                )?;
            }
            if home.is_some() {
                prune_one(
                    &mut entries,
                    name_str,
                    &sleeve,
                    OsStr::new("home"),
                    &home_path,
                    "home",
                    homes,
                    yes,
                    busy.as_deref(),
                    true,
                )?;
            }
            if let Some(helpers) = &helpers {
                let store = format!("checkpoint-{name_str}");
                prune_one(
                    &mut entries,
                    name_str,
                    helpers,
                    OsStr::new(&store),
                    &hub.workspaces.state_root().join("helpers").join(&store),
                    "checkpoints",
                    homes,
                    yes,
                    busy.as_deref(),
                    true,
                )?;
            }
        }
    }
    if let Some(clones) =
        open_child_directory(&state, OsStr::new("workspaces")).map_err(cache_error)?
    {
        for dir in
            fs::read_dir(format!("/proc/self/fd/{}", clones.as_raw_fd())).map_err(cache_error)?
        {
            let dir = dir.map_err(cache_error)?;
            let name = dir.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if protectorate_sleeve_registry::valid_name(name_str).is_err() {
                continue;
            }
            let path = hub
                .workspaces
                .state_root()
                .join("workspaces")
                .join(name_str);
            let running_busy = prune_busy_reason(&running, name_str, &path);
            if running_busy.is_some() && !show_running {
                continue;
            }
            let busy = running_busy
                .or_else(|| prune_registered_clone_reason(&registered, name_str, &path));
            let owner = hub
                .sleeves
                .get(name_str)
                .ok()
                .map(|row| refresh_record(hub, &row));
            prune_clone(
                &mut entries,
                name_str,
                &clones,
                &name,
                &path,
                hub.workspaces.state_root(),
                owner.as_ref(),
                workspaces,
                yes,
                busy.as_deref(),
            )?;
        }
    }
    entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(entries)
}

fn prune_registered_clone_reason(rows: &[Record], owner: &str, path: &Path) -> Option<String> {
    rows.iter().filter(|row| row.name != owner).find_map(|row| {
        if prune_path_intersects(Path::new(&row.workspace), path) {
            return Some(format!(
                "workspace retained by registered sleeve {}",
                row.name
            ));
        }
        row.read_only.iter().find_map(|mount| {
            prune_path_intersects(Path::new(&mount.source), path)
                .then(|| format!("mounted by registered sleeve {}", row.name))
        })
    })
}

fn prune_path_intersects(left: &Path, right: &Path) -> bool {
    if left.starts_with(right) || right.starts_with(left) {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left.starts_with(&right) || right.starts_with(&left),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn prune_clone(
    entries: &mut Vec<Value>,
    name: &str,
    parent: &fs::File,
    basename: &OsStr,
    path: &Path,
    state: &Path,
    owner: Option<&Record>,
    selected: bool,
    yes: bool,
    busy: Option<&str>,
) -> Result<()> {
    let directory = match open_child_directory(parent, basename) {
        Ok(Some(directory)) => directory,
        Ok(None) => return Ok(()),
        Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
            entries.push(json!({"sleeve":name,"path":path,"kind":"workspace","bytes":"0","eligible":false,"removed":false,"reason":"keep non-directory or symlink"}));
            return Ok(());
        }
        Err(error) => return Err(cache_error(error)),
    };
    let bytes = disk_size_open(&directory).map_err(cache_error)?;
    let provenance = owner.is_some_and(|row| {
        row.managed_clone
            && row.name == name
            && row.workspace == path.to_string_lossy()
            && row.state != State::Running
    });
    let mut reason = if let Some(busy) = busy {
        format!("{busy}; clone is kept")
    } else if !selected {
        "not selected".into()
    } else if !provenance {
        "no stopped hub-owned fresh-clone record".into()
    } else {
        String::new()
    };
    let mut eligible = false;
    let mut removed = false;
    if reason.is_empty() {
        let origin = owner.map_or("", |row| row.base_repo.as_str());
        let workspaces = state.join("workspaces");
        if yes {
            // Move the exact directory opened above out of the public clone
            // name before examining it. A new sleeve cannot acquire that name
            // while the launch lock is held, and renameat2 refuses overwrite.
            let mut nonce = [0_u8; 8];
            getrandom::getrandom(&mut nonce)
                .map_err(|error| RpcError::new(Code::Internal, format!("prune nonce: {error}")))?;
            let hidden = format!(".prune-{name}-{:016x}", u64::from_le_bytes(nonce));
            rename_child(parent, basename, OsStr::new(&hidden)).map_err(cache_error)?;
            let hidden_path = workspaces.join(&hidden);
            let pinned = open_child_directory(parent, OsStr::new(&hidden))
                .map_err(|error| quarantine_error(&hidden_path, error))?
                .ok_or_else(|| {
                    RpcError::new(
                        Code::Internal,
                        format!("quarantined clone disappeared: {}", hidden_path.display()),
                    )
                })?;
            if !same_directory(&directory, &pinned)
                .map_err(|error| quarantine_error(&hidden_path, error))?
            {
                // The public name changed between opening and renaming it.
                // Keep the moved directory rather than deleting an unverified
                // inode or overwriting the replacement at the public name.
                reason = restore_quarantine(
                    parent,
                    OsStr::new(&hidden),
                    basename,
                    &hidden_path,
                    "clone changed while entering prune quarantine",
                );
            } else if let Err(error) = prune_git::verify(&hidden_path, origin, &workspaces) {
                reason =
                    restore_quarantine(parent, OsStr::new(&hidden), basename, &hidden_path, &error);
            } else {
                eligible = true;
                remove_open_tree(parent, OsStr::new(&hidden), pinned)
                    .map_err(|error| quarantine_error(&hidden_path, error))?;
                removed = true;
                reason = "clean clone; all refs recoverable from origin".into();
            }
        } else {
            match prune_git::verify(path, origin, &workspaces) {
                Ok(()) => {
                    eligible = true;
                    reason = "clean clone; all refs recoverable from origin".into();
                }
                Err(error) => reason = error,
            }
        }
    }
    entries.push(json!({"sleeve":name,"path":path,"kind":"workspace","bytes":bytes.to_string(),"eligible":eligible,"removed":removed,"reason":reason}));
    Ok(())
}

fn restore_quarantine(
    parent: &fs::File,
    hidden: &OsStr,
    original: &OsStr,
    hidden_path: &Path,
    reason: &str,
) -> String {
    match rename_child(parent, hidden, original) {
        Ok(()) => reason.to_owned(),
        Err(error) => format!(
            "{reason}; clone retained at {} because restoration failed: {error}",
            hidden_path.display()
        ),
    }
}

fn quarantine_error(path: &Path, error: io::Error) -> RpcError {
    RpcError::new(
        Code::Internal,
        format!(
            "clone retained in prune quarantine {}: {error}",
            path.display()
        ),
    )
}

fn same_directory(left: &fs::File, right: &fs::File) -> io::Result<bool> {
    let mut a: libc::stat = unsafe { std::mem::zeroed() };
    let mut b: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(left.as_raw_fd(), &mut a) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fstat(right.as_raw_fd(), &mut b) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(a.st_dev == b.st_dev && a.st_ino == b.st_ino)
}

fn rename_child(parent: &fs::File, from: &OsStr, to: &OsStr) -> io::Result<()> {
    let from = CString::new(from.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in clone name"))?;
    let to = CString::new(to.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in clone name"))?;
    if unsafe {
        libc::renameat2(
            parent.as_raw_fd(),
            from.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn prune_busy_reason(running: &[Record], name: &str, path: &Path) -> Option<String> {
    running.iter().find_map(|row| {
        let workspace = Path::new(&row.workspace);
        if row.name == name || workspace.starts_with(path) || path.starts_with(workspace) {
            return Some(format!("running sleeve {}", row.name));
        }
        row.read_only.iter().find_map(|mount| {
            let source = Path::new(&mount.source);
            (source.starts_with(path) || path.starts_with(source))
                .then(|| format!("mounted by running sleeve {}", row.name))
        })
    })
}

fn open_child_or_skip(parent: &fs::File, name: &OsStr) -> Result<Option<fs::File>> {
    match open_child_directory(parent, name) {
        Ok(directory) => Ok(directory),
        Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => Ok(None),
        Err(error) => Err(cache_error(error)),
    }
}

#[allow(clippy::too_many_arguments)]
fn prune_one(
    entries: &mut Vec<Value>,
    sleeve: &str,
    parent: &fs::File,
    name: &OsStr,
    path: &Path,
    kind: &str,
    selected: bool,
    yes: bool,
    busy: Option<&str>,
    safe_to_remove: bool,
) -> Result<()> {
    let directory = match open_child_directory(parent, name) {
        Ok(Some(directory)) => directory,
        Ok(None) => return Ok(()),
        Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
            entries.push(json!({"sleeve":sleeve,"path":path,"kind":kind,"bytes":"0","eligible":false,"removed":false,"reason":"keep non-directory or symlink"}));
            return Ok(());
        }
        Err(error) => return Err(cache_error(error)),
    };
    let bytes = disk_size_open(&directory).map_err(cache_error)?;
    let reason = if let Some(busy) = busy {
        format!("{busy}; running data is always kept")
    } else if !selected {
        "not selected".to_owned()
    } else if !safe_to_remove {
        "clone requires Git history and working-tree verification".to_owned()
    } else if kind == "home" {
        "whole home, including auth, sessions, history and config".to_owned()
    } else if kind == "checkpoints" {
        "workspace checkpoints, pruned with the home".to_owned()
    } else {
        "regenerable cache".to_owned()
    };
    let eligible = busy.is_none() && selected && safe_to_remove;
    let removed = eligible && yes;
    if removed {
        remove_open_tree(parent, name, directory).map_err(cache_error)?;
    }
    entries.push(json!({"sleeve":sleeve,"path":path,"kind":kind,"bytes":bytes.to_string(),"eligible":eligible,"removed":removed,"reason":reason}));
    Ok(())
}

fn disk_size_open(directory: &fs::File) -> io::Result<u64> {
    let mut total = 0_u64;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        let entry = entry?;
        let child_name = entry.file_name();
        match open_child_directory(directory, &child_name) {
            Ok(Some(child)) => total = total.saturating_add(disk_size_open(&child)?),
            Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
                let metadata = entry.path().symlink_metadata()?;
                if metadata.is_file() {
                    total = total.saturating_add(metadata.len());
                }
            }
            Ok(None) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(total)
}

fn disk_size_path(path: &Path) -> io::Result<u64> {
    match open_root_directory(path) {
        Ok(directory) => disk_size_open(&directory),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn overlay_upper_size(path: &Path) -> io::Result<(u64, u64)> {
    let overlays = match open_root_directory(path) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(error) => return Err(error),
    };
    let mut total = 0_u64;
    let mut go_build = 0_u64;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", overlays.as_raw_fd()))? {
        let name = entry?.file_name();
        let overlay = match open_child_directory(&overlays, &name) {
            Ok(Some(directory)) => directory,
            Ok(None) => continue,
            Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
                continue;
            }
            Err(error) => return Err(error),
        };
        let Some(upper) = open_child_directory(&overlay, OsStr::new("upper"))? else {
            continue;
        };
        let bytes = disk_size_open(&upper)?;
        total = total.saturating_add(bytes);
        if name == OsStr::new("go-build") {
            go_build = go_build.saturating_add(bytes);
        }
    }
    Ok((total, go_build))
}

fn sample_sleeve_disks(hub: &Hub, all: bool, warned: &mut BTreeSet<String>) {
    for row in hub.sleeves.list() {
        let limited = row.state == State::Running && row.limits.disk_bytes > 0;
        if !all && !limited {
            continue;
        }
        let mut sample = DiskUsage::default();
        let workspace = Path::new(&row.workspace);
        match disk_size_path(workspace) {
            Ok(bytes) => sample.workspace_bytes = Some(bytes),
            Err(error) => eprintln!("sleeve {}: measure workspace disk: {error}", row.name),
        }
        let home = hub
            .workspaces
            .state_root()
            .join("sleeves")
            .join(&row.name)
            .join("home");
        match disk_size_path(&home) {
            Ok(bytes) => sample.home_bytes = Some(bytes),
            Err(error) => eprintln!("sleeve {}: measure home disk: {error}", row.name),
        }
        let cache = [".cache", ".cargo", ".rustup"]
            .into_iter()
            .try_fold(0_u64, |total, name| {
                disk_size_path(&home.join(name)).map(|bytes| total.saturating_add(bytes))
            });
        let overlays = overlay_upper_size(
            &hub.workspaces
                .state_root()
                .join("sleeves")
                .join(&row.name)
                .join("overlays"),
        );
        match (&cache, &overlays) {
            (Ok(cache), Ok((overlay, _))) => {
                sample.cache_bytes = Some(cache.saturating_add(*overlay));
            }
            (Err(error), _) | (_, Err(error)) => {
                eprintln!("sleeve {}: measure cache disk: {error}", row.name)
            }
        }
        match (
            disk_size_path(&home.join(".cache/go-build")),
            overlays.as_ref(),
        ) {
            (Ok(cache), Ok((_, overlay))) => {
                sample.go_build_cache_bytes = Some(cache.saturating_add(*overlay));
            }
            (Err(error), _) => {
                eprintln!("sleeve {}: measure Go cache disk: {error}", row.name)
            }
            (_, Err(error)) => {
                eprintln!("sleeve {}: measure Go cache disk: {error}", row.name)
            }
        }
        let used = match (sample.workspace_bytes, sample.home_bytes, &overlays) {
            (Some(workspace), Some(home), Ok((overlay, _))) => Some(
                workspace
                    .saturating_add(home)
                    .saturating_add(*overlay)
                    .saturating_add(
                        disk_size_path(
                            &hub.workspaces
                                .state_root()
                                .join("helpers")
                                .join(format!("checkpoint-{}", row.name)),
                        )
                        .unwrap_or(0),
                    ),
            ),
            _ => None,
        };
        if let Err(error) = hub
            .sleeves
            .set_disk_usage(&row.name, &row.created_at, sample, now())
        {
            eprintln!("sleeve {}: save disk usage: {error}", row.name);
        }
        if let (true, Some(used)) = (limited, used) {
            enforce_disk_limit(hub, &row, used, warned);
        }
    }
}

/// Warn once at 90% of a sleeve's disk limit and stop its harness at 100%.
fn enforce_disk_limit(hub: &Hub, row: &Record, used: u64, warned: &mut BTreeSet<String>) {
    let limit = row.limits.disk_bytes;
    let name = row.name.as_str();
    if used < limit {
        if used.saturating_mul(10) >= limit.saturating_mul(9) {
            if warned.insert(name.to_owned()) {
                hub.emit(
                    "sleeve.disk_warning",
                    name,
                    json!({"usedBytes": used, "limitBytes": limit}),
                );
            }
        } else {
            warned.remove(name);
        }
        return;
    }
    warned.remove(name);
    let _guard = hub.sleeve_launch_lock.lock().unwrap();
    let current = hub.sleeves.get(name).ok();
    if hub.resleeving.lock().unwrap().contains(name)
        || current.is_none_or(|current| {
            current.created_at != row.created_at || current.state != State::Running
        })
    {
        return;
    }
    let text = format!("disk limit reached: {used} of {limit} bytes used; the harness was stopped");
    let stopped = if row.mode == Mode::Chat {
        let _ = crate::chat::append_error(hub, name, &text);
        crate::chat::kill_pi(hub, name)
    } else {
        kill_shell(hub, name)
    };
    hub.emit(
        "sleeve.disk_limit",
        name,
        json!({"usedBytes": used, "limitBytes": limit, "stopped": stopped.is_ok()}),
    );
    hub.audit.write(AuditEntry {
        actor: "hub".into(),
        procedure: "disk-limit".into(),
        subject: name.into(),
        outcome: if stopped.is_ok() { "denied" } else { "error" }.into(),
        detail: match stopped {
            Ok(()) => text,
            Err(error) => format!("{text}; stop failed: {}", error.message),
        },
        ..AuditEntry::default()
    });
}

fn cache_error(error: io::Error) -> RpcError {
    let code = match error.raw_os_error() {
        Some(libc::ELOOP | libc::ENOTDIR) => Code::FailedPrecondition,
        _ => Code::Internal,
    };
    RpcError::new(code, format!("remove private sleeve cache: {error}"))
}

fn open_root_directory(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

fn open_child_directory(parent: &fs::File, name: &OsStr) -> io::Result<Option<fs::File>> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in cache path"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd >= 0 {
        Ok(Some(unsafe { fs::File::from_raw_fd(fd) }))
    } else {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        }
    }
}

// Overlayfs creates `work/work` with mode 000. After the sleeve has stopped,
// pin that directory without following links, grant its owner traversal, then
// reopen it for descriptor-anchored deletion. An older kernel without
// fchmodat2 fails closed and leaves the cache for a later cleanup.
fn open_inaccessible_overlay_directory(parent: &fs::File, name: &OsStr) -> io::Result<fs::File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in overlay path"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let pinned = unsafe { fs::File::from_raw_fd(fd) };
    let empty = c"";
    // Linux fchmodat2 is syscall 452 on supported amd64 and arm64 hosts.
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    let changed = unsafe {
        libc::syscall(
            452 as libc::c_long,
            pinned.as_raw_fd(),
            empty.as_ptr(),
            0o700,
            libc::AT_EMPTY_PATH,
        )
    };
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let changed = -1;
    if changed != 0 {
        return Err(io::Error::last_os_error());
    }
    open_child_directory(parent, OsStr::from_bytes(name.as_bytes()))?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "overlay work directory disappeared",
        )
    })
}

fn unlink_child(parent: &fs::File, name: &OsStr, flags: libc::c_int) -> io::Result<()> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in cache path"))?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), flags) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn remove_open_tree(parent: &fs::File, name: &OsStr, tree: fs::File) -> io::Result<()> {
    if unsafe { libc::fchmod(tree.as_raw_fd(), 0o700) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // /proc/self/fd points at the directory already opened with O_NOFOLLOW.
    // Every mutation below is anchored to that descriptor with openat/unlinkat.
    for entry in fs::read_dir(format!("/proc/self/fd/{}", tree.as_raw_fd()))? {
        let entry = entry?;
        let child_name = entry.file_name();
        match open_child_directory(&tree, &child_name).or_else(|error| {
            if error.kind() == io::ErrorKind::PermissionDenied {
                open_inaccessible_overlay_directory(&tree, &child_name).map(Some)
            } else {
                Err(error)
            }
        }) {
            Ok(Some(child)) => remove_open_tree(&tree, &child_name, child).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("{}: {error}", child_name.to_string_lossy()),
                )
            })?,
            Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
                unlink_child(&tree, &child_name, 0)?;
            }
            Ok(None) => {} // A concurrent removal is harmless.
            Err(error) => return Err(error),
        }
    }
    unlink_child(parent, name, libc::AT_REMOVEDIR)
}

fn terminal_token_path(hub: &Hub, name: &str) -> Result<PathBuf> {
    protectorate_sleeve_registry::valid_name(name).map_err(registry_error)?;
    let launcher = hub
        .terminal_launcher
        .as_ref()
        .ok_or_else(|| RpcError::new(Code::Unavailable, "terminal launcher is not configured"))?;
    Ok(launcher
        .state_root
        .join("sleeves")
        .join(name)
        .join("home/.protectorate-token"))
}

fn revoke_terminal_identity(hub: &Hub, name: &str) -> Result<()> {
    let now = super::now_unix();
    for token in hub.identity.list() {
        if token.name == format!("sleeve:{name}") && token.is_active(now) {
            hub.identity
                .revoke(&token.id, now)
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
        }
    }
    match fs::remove_file(terminal_token_path(hub, name)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(internal(error)),
    }
}

fn issue_terminal_identity(hub: &Hub, name: &str) -> Result<()> {
    revoke_terminal_identity(hub, name)?;
    let catalog = protectorate_wire::Catalog::embedded()?;
    let methods = catalog.methods().cloned().collect::<Vec<_>>();
    let scope_methods = super::scope_methods(&methods);
    let issued = hub
        .identity
        .create(
            &format!("sleeve:{name}"),
            &SLEEVE_SCOPES,
            Duration::ZERO,
            super::now_unix(),
            &scope_methods,
        )
        .map_err(|e| RpcError::new(Code::Internal, format!("issue sleeve token: {e}")))?;
    let path = terminal_token_path(hub, name)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("token-{}-{nonce}", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(issued.secret.expose().as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, &path)
    };
    if let Err(error) = write() {
        let _ = fs::remove_file(&temporary);
        let _ = hub.identity.revoke(&issued.token.id, super::now_unix());
        return Err(internal(error));
    }
    Ok(())
}

fn number(value: &Value, key: &str) -> Result<u64> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::String(s)) => s
            .parse()
            .map_err(|_| RpcError::new(Code::InvalidArgument, format!("invalid {key}"))),
        Some(Value::Number(n)) => n
            .as_u64()
            .ok_or_else(|| RpcError::new(Code::InvalidArgument, format!("invalid {key}"))),
        _ => Err(RpcError::new(
            Code::InvalidArgument,
            format!("invalid {key}"),
        )),
    }
}

fn resolve_executable(executable: &str, tool_roots: &[PathBuf]) -> Result<PathBuf> {
    let path = Path::new(executable);
    let found = if path.is_absolute() {
        path.to_path_buf()
    } else if path.components().count() == 1 {
        let system = ["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"];
        let mut dirs = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|dir| {
                system.iter().any(|path| dir == Path::new(path))
                    || tool_roots.iter().any(|root| dir.starts_with(root))
            })
            .collect::<Vec<_>>();
        for root in tool_roots {
            dirs.push(root.clone());
            dirs.push(root.join("bin"));
        }
        dirs.extend(system.iter().map(PathBuf::from));
        dirs.into_iter()
            .map(|dir| dir.join(path))
            .find(|p| p.is_file())
            .ok_or_else(|| RpcError::new(Code::InvalidArgument, "shell executable was not found"))?
    } else {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "shell executable must be absolute or on PATH",
        ));
    };
    if !found.is_file() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "shell executable is not a file",
        ));
    }
    Ok(found)
}

pub(super) fn new_run_id() -> Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::getrandom(&mut bytes).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn internal(error: std::io::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

/// The egress a sleeve or harness helper gets: its provider's API hosts
/// (added by the sandbox policy), the hub, the operator's allowlist and, for
/// Pi, the configured local model servers.
pub(super) fn sleeve_egress(hub: &Hub, pi: bool) -> protectorate_sandbox::Egress {
    let url = hub
        .terminal_launcher
        .as_ref()
        .map(|launcher| launcher.hub_url.clone())
        .or_else(|| crate::chat::hub_url(hub));
    let authority = url.and_then(|url| {
        let (default_port, rest) = if let Some(rest) = url.strip_prefix("https://") {
            (443, rest.to_owned())
        } else {
            (80, url.strip_prefix("http://")?.to_owned())
        };
        let authority = rest.split('/').next().unwrap_or_default();
        let has_port = if authority.starts_with('[') {
            authority.contains("]:")
        } else {
            authority.contains(':')
        };
        Some(if has_port {
            authority.to_owned()
        } else {
            format!("{authority}:{default_port}")
        })
    });
    protectorate_sandbox::Egress {
        hub: authority,
        allow: hub.egress_allow.clone(),
        private_hosts: if pi {
            hub.local_model_hosts.clone()
        } else {
            Vec::new()
        },
    }
}

pub(super) fn egress_log(hub: &Hub, name: &str, run_id: &str) -> protectorate_sandbox::egress::Log {
    protectorate_sleeve_runner::egress_audit(
        Arc::clone(&hub.audit),
        format!("sleeve:{name}"),
        run_id.to_owned(),
    )
}

pub(super) fn sleeve_json(sleeve: &Record) -> Value {
    let mut value = serde_json::to_value(sleeve).expect("valid sleeve record serializes");
    let object = value.as_object_mut().expect("record is object");
    object.remove("parentTokenId");
    object.remove("helper");
    object.remove("managedClone");
    object.remove("env");
    object.remove("pins");
    object.remove("parentRunId");
    if let Some(pin) = sleeve.pins.get(&pin_key(sleeve.mode, sleeve.harness)) {
        object.insert("harnessSha256".into(), json!(pin.executable_sha256));
        object.insert("profileSha256".into(), json!(pin.profile_sha256));
    }
    object.insert("harness".into(), json!(sleeve.harness.proto()));
    object.insert("mode".into(), json!(sleeve.mode.proto()));
    object.insert("state".into(), json!(sleeve.state.proto()));
    object.insert(
        "gitBranch".into(),
        json!(crate::workspaces::git_details(Path::new(&sleeve.workspace)).1),
    );
    value
}

pub(super) fn request_env(value: &Value) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    if let Some(raw) = value.get("env") {
        let entries = raw
            .as_object()
            .ok_or_else(|| RpcError::invalid("env must be a string map"))?;
        for (key, value) in entries {
            let value = value
                .as_str()
                .ok_or_else(|| RpcError::invalid(format!("env {key} must be a string")))?;
            env.insert(key.clone(), value.to_owned());
        }
    }
    protectorate_sleeve_registry::validate_env(&env).map_err(registry_error)?;
    Ok(env)
}

fn registry_error(error: Error) -> RpcError {
    let code = match error {
        Error::NotFound(_) => Code::NotFound,
        Error::Invalid(_) => Code::InvalidArgument,
        Error::Exists(_) => Code::AlreadyExists,
        Error::State(_) => Code::FailedPrecondition,
        Error::Io(_) | Error::Json(_) => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}
