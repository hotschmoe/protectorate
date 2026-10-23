//! Git-tree checkpoints of sleeve workspaces. Git runs in a sandbox that sees
//! only the workspace and a private store in its own helper home, so the
//! workspace's config, hooks and filters never run on the host, and the agent
//! can neither read nor rewrite its checkpoints. The store is a separate Git
//! directory, never the workspace repository, so pushes cannot carry it.
use super::{input, respond, string, Hub};
use protectorate_sandbox::{
    runtime::{self, LaunchOptions},
    Limits, Policy, Request, Workload,
};
use protectorate_sleeve_registry::{Mode, Record, State};
use protectorate_wire::{AuthContext, Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

const KEEP: usize = 20;
const MAX_OUTPUT: usize = 1 << 20;
const TIMEOUT: Duration = Duration::from_secs(300);

const SCRIPT: &str = r#"set -eu
export GIT_DIR="$HOME/store" GIT_INDEX_FILE="$HOME/index"
refs=refs/protectorate/checkpoints
format='%(refname:lstrip=3) %(objectname) %(creatordate:unix) %(trailers:key=Run-Id,valueonly,separator=) %(contents:subject)'
test -e .git || { echo "workspace is not a git repository" >&2; exit 3; }
if ! test -d "$GIT_DIR"; then
  git init -q --bare "$GIT_DIR"
  git config core.bare false
  git config gc.auto 0
  printf '.protectorate-token\n' > "$GIT_DIR/info/exclude"
fi
export GIT_WORK_TREE="$PWD"
git config core.excludesFile "$PWD/.git/info/exclude"
ids() { git for-each-ref --format='%(refname:lstrip=3)' "$refs" | sort -n; }
show() { git for-each-ref --format="$format" "$refs/$1"; }
exists() { git rev-parse -q --verify "$refs/$1^{commit}" >/dev/null || { echo "no checkpoint $1" >&2; exit 4; }; }
snapshot() {
  git add -A
  tree=$(git write-tree)
  last=$(ids | tail -n 1)
  if test -n "$last" && test "$(git rev-parse "$refs/$last^{tree}")" = "$tree"; then
    printf 'same '; show "$last"; return
  fi
  id=$(( ${last:-0} + 1 ))
  msg=$1
  test -z "$RUN_ID" || msg=$(printf '%s\n\nRun-Id: %s' "$1" "$RUN_ID")
  git update-ref "$refs/$id" "$(printf '%s\n' "$msg" | git commit-tree "$tree")"
  old=$(ids | head -n -"$KEEP")
  for gone in $old; do git update-ref -d "$refs/$gone"; done
  test -z "$old" || git prune --expire=now
  printf 'new '; show "$id"
}
case "$1" in
create) snapshot "$2" ;;
list) git for-each-ref --format="$format" "$refs" | sort -n ;;
diff)
  exists "$2"
  if test "$3" != 0; then
    exists "$3"
    git diff --no-ext-diff --no-textconv --no-color "$refs/$2" "$refs/$3"
  else
    rm -f "$HOME/index.diff"
    if test -f "$GIT_INDEX_FILE"; then cp "$GIT_INDEX_FILE" "$HOME/index.diff"; fi
    GIT_INDEX_FILE="$HOME/index.diff" git add -A
    GIT_INDEX_FILE="$HOME/index.diff" git diff --cached --no-ext-diff --no-textconv --no-color "$refs/$2"
    rm -f "$HOME/index.diff"
  fi ;;
restore) exists "$2"; snapshot "before restore of $2"; git read-tree -u --reset "$refs/$2" ;;
esac
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    pub id: u64,
    pub commit: String,
    pub created: i64,
    pub run_id: String,
    pub reason: String,
}

impl Checkpoint {
    fn parse(line: &str) -> Option<Self> {
        let mut parts = line.splitn(5, ' ');
        Some(Self {
            id: parts.next()?.parse().ok()?,
            commit: parts.next()?.to_owned(),
            created: parts.next()?.parse().ok()?,
            run_id: parts.next()?.to_owned(),
            reason: parts.next().unwrap_or_default().to_owned(),
        })
    }

    fn json(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "commit": self.commit,
            "createdAt": chrono::DateTime::from_timestamp(self.created, 0)
                .unwrap_or_default()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "reason": self.reason,
            "runId": self.run_id,
        })
    }
}

/// Everything needed to launch the checkpoint helper, taken from whichever
/// sleeve launcher the hub has.
#[derive(Clone, Debug)]
pub(crate) struct Store {
    state_root: PathBuf,
    user_home: PathBuf,
    runtime_dir: PathBuf,
    helper: PathBuf,
}

impl Store {
    pub(crate) fn for_hub(hub: &Hub) -> Option<Self> {
        if let Some(chat) = &hub.chat_runtime {
            return Some(Self::for_chat(chat.launcher()));
        }
        hub.terminal_launcher.as_ref().map(|launcher| Self {
            state_root: launcher.state_root.clone(),
            user_home: launcher.user_home.clone(),
            runtime_dir: launcher.runtime_dir.clone(),
            helper: launcher.helper.clone(),
        })
    }

    pub(crate) fn for_chat(launcher: &crate::chat::ChatLauncher) -> Self {
        Self {
            state_root: launcher.state_root.clone(),
            user_home: launcher.user_home.clone(),
            runtime_dir: launcher.runtime_dir.clone(),
            helper: launcher.helper.clone(),
        }
    }

    /// Snapshot the workspace. `None` means another checkpoint of this
    /// sleeve was running and `wait` was false.
    pub(crate) fn create(
        &self,
        row: &Record,
        reason: &str,
        wait: bool,
    ) -> std::result::Result<Option<(Checkpoint, bool)>, String> {
        let Some(output) = self.run(row, &["create", reason], wait)? else {
            return Ok(None);
        };
        snapshot_line(&output).map(Some)
    }

    fn list(&self, row: &Record) -> std::result::Result<Vec<Checkpoint>, String> {
        let output = self.run(row, &["list"], true)?.unwrap_or_default();
        Ok(output.lines().filter_map(Checkpoint::parse).collect())
    }

    fn run(
        &self,
        row: &Record,
        args: &[&str],
        wait: bool,
    ) -> std::result::Result<Option<String>, String> {
        self.run_bounded(row, args, wait)
            .map(|output| output.map(|(text, _)| text))
    }

    /// Run the script under a per-sleeve lock shared by every hub thread
    /// and process. Output beyond the limit is dropped and reported.
    fn run_bounded(
        &self,
        row: &Record,
        args: &[&str],
        wait: bool,
    ) -> std::result::Result<Option<(String, bool)>, String> {
        let (sleeve, workspace) = (row.name.as_str(), Path::new(&row.workspace));
        let root = self
            .state_root
            .join("helpers")
            .join(format!("checkpoint-{sleeve}"));
        let home = root.join("home");
        fs::create_dir_all(&home).map_err(|e| format!("checkpoint store: {e}"))?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("checkpoint store permissions: {e}"))?;
        let lock =
            fs::File::create(root.join("lock")).map_err(|e| format!("checkpoint lock: {e}"))?;
        if wait {
            lock.lock().map_err(|e| format!("checkpoint lock: {e}"))?;
        } else if lock.try_lock().is_err() {
            return Ok(None);
        }
        let policy = Policy {
            state_root: self.state_root.clone(),
            user_home: self.user_home.clone(),
            protected: Vec::new(),
            provider_credentials: BTreeMap::new(),
            trusted_tool_roots: Vec::new(),
            immutable_cache_roots: Vec::new(),
            nested_cgroups_available: false,
        };
        let mut command = vec![
            "/bin/sh".into(),
            "-c".into(),
            SCRIPT.into(),
            "checkpoint".into(),
        ];
        command.extend(args.iter().map(|arg| (*arg).to_owned()));
        let plan = policy
            .plan(Request {
                workload: Workload::CheckpointHelper,
                command,
                workspace: Some(workspace.to_path_buf()),
                managed_workspace_name: protectorate_sandbox::managed_workspace_name(
                    &self.state_root,
                    workspace,
                ),
                private_home: Some(home),
                grants: Vec::new(),
                tool_roots: Vec::new(),
                selected_executable: None,
                credential: None,
                overlays: Vec::new(),
                masks: Vec::new(),
                limits: Limits {
                    cpu_percent: 100,
                    memory_bytes: 512 << 20,
                    tasks_max: 64,
                },
                egress: None,
            })
            .map_err(|e| format!("checkpoint plan: {e}"))?;
        let mut options =
            LaunchOptions::new(&self.helper, &self.runtime_dir, &self.user_home, workspace);
        options.environment = BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("KEEP".into(), KEEP.to_string()),
            ("RUN_ID".into(), row.run_id.clone()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GIT_AUTHOR_NAME".into(), "protectorate".into()),
            ("GIT_AUTHOR_EMAIL".into(), "protectorate@localhost".into()),
            ("GIT_COMMITTER_NAME".into(), "protectorate".into()),
            (
                "GIT_COMMITTER_EMAIL".into(),
                "protectorate@localhost".into(),
            ),
        ]);
        let mut child =
            runtime::spawn(&plan, &options).map_err(|e| format!("checkpoint helper: {e}"))?;
        let stdout = child
            .take_stdout()
            .ok_or("checkpoint helper has no stdout")?;
        let stderr = child
            .take_stderr()
            .ok_or("checkpoint helper has no stderr")?;
        let out = thread::spawn(move || bounded(stdout, MAX_OUTPUT));
        let err = thread::spawn(move || bounded(stderr, 4096));
        let kill = child.kill_handle();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let _ = sender.send(child.wait());
        });
        let status = match receiver.recv_timeout(TIMEOUT) {
            Ok(status) => status.map_err(|e| format!("checkpoint helper: {e}"))?,
            Err(_) => {
                kill.kill();
                return Err("checkpoint helper timed out".into());
            }
        };
        let (stdout, truncated) = out.join().map_err(|_| "checkpoint output reader failed")?;
        let (stderr, _) = err.join().map_err(|_| "checkpoint error reader failed")?;
        if !status.success() {
            return Err(format!("checkpoint failed: {}", stderr.trim()));
        }
        Ok(Some((stdout, truncated)))
    }
}

fn bounded(mut reader: impl Read, limit: usize) -> (String, bool) {
    let mut bytes = Vec::new();
    let _ = (&mut reader).take(limit as u64).read_to_end(&mut bytes);
    let truncated = std::io::copy(&mut reader, &mut std::io::sink()).is_ok_and(|rest| rest > 0);
    (String::from_utf8_lossy(&bytes).into_owned(), truncated)
}

fn snapshot_line(output: &str) -> std::result::Result<(Checkpoint, bool), String> {
    let line = output.lines().last().unwrap_or_default();
    let (state, rest) = line.split_once(' ').unwrap_or_default();
    let checkpoint =
        Checkpoint::parse(rest).ok_or_else(|| format!("unexpected checkpoint output {line:?}"))?;
    Ok((checkpoint, state == "same"))
}

/// Checkpoint a chat sleeve after a turn settles, skipping if one is already
/// running. Failures are reported, never raised into the chat.
pub(crate) fn after_turn(store: Store, row: Record) {
    if !Path::new(&row.workspace).join(".git").exists() {
        return;
    }
    thread::spawn(move || {
        if let Err(error) = store.create(&row, "turn", false) {
            eprintln!("checkpoint {}: {error}", row.name);
        }
    });
}

pub(crate) fn before_resleeve(hub: &Hub, row: &Record) {
    if let Some(store) = Store::for_hub(hub) {
        if let Err(error) = store.create(row, "resleeve", true) {
            eprintln!("checkpoint {}: {error}", row.name);
        }
    }
}

fn authorize(hub: &Hub, auth: &AuthContext, name: &str) -> Result<(Store, Record)> {
    let row = hub
        .sleeves
        .get(name)
        .map_err(|e| RpcError::new(Code::NotFound, e.to_string()))?;
    if row.helper {
        return Err(RpcError::new(
            Code::NotFound,
            format!("sleeve not found: {name}"),
        ));
    }
    let own = auth.actor == format!("token:sleeve:{name}");
    if !(auth.owner || auth.device || own || crate::sleeves::spawned_by(auth, &row)) {
        return Err(RpcError::new(
            Code::PermissionDenied,
            "sleeve is owned by another identity",
        ));
    }
    let store = Store::for_hub(hub)
        .ok_or_else(|| RpcError::new(Code::Unavailable, "sleeve launcher is not configured"))?;
    Ok((store, row))
}

fn id(value: &Value, key: &str) -> Result<u64> {
    match value.get(key) {
        None | Some(Value::Null) => Some(0),
        Some(Value::String(s)) => s.parse().ok(),
        Some(Value::Number(n)) => n.as_u64(),
        _ => None,
    }
    .ok_or_else(|| RpcError::invalid(format!("invalid {key}")))
}

fn failed(error: String) -> RpcError {
    let code = if error.contains("no checkpoint") {
        Code::NotFound
    } else if error.contains("not a git repository") {
        Code::FailedPrecondition
    } else {
        Code::Internal
    };
    RpcError::new(code, error)
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    for procedure in [
        "/protectorate.v1.SleeveService/CreateCheckpoint",
        "/protectorate.v1.SleeveService/ListCheckpoints",
        "/protectorate.v1.SleeveService/DiffCheckpoint",
        "/protectorate.v1.SleeveService/RestoreCheckpoint",
    ] {
        let hub = Arc::clone(&hub);
        server.register(procedure, move |request| {
            let hub = Arc::clone(&hub);
            async move {
                let value = input(&request)?;
                let auth = request.auth.clone();
                let method = request
                    .method
                    .procedure
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                let result = tokio::task::spawn_blocking(move || {
                    let name = string(&value, "name");
                    let (store, row) = authorize(&hub, &auth, &name)?;
                    match method.as_str() {
                        "CreateCheckpoint" => {
                            let (checkpoint, unchanged) = store
                                .create(&row, "manual", true)
                                .map_err(failed)?
                                .ok_or_else(|| {
                                    RpcError::new(Code::Internal, "checkpoint lock was not taken")
                                })?;
                            Ok(json!({"checkpoint": checkpoint.json(), "unchanged": unchanged}))
                        }
                        "ListCheckpoints" => Ok(json!({
                            "checkpoints": store
                                .list(&row)
                                .map_err(failed)?
                                .iter()
                                .map(Checkpoint::json)
                                .collect::<Vec<_>>()
                        })),
                        "DiffCheckpoint" => {
                            let (from, to) = (id(&value, "fromId")?, id(&value, "toId")?);
                            if from == 0 {
                                return Err(RpcError::invalid("from_id is required"));
                            }
                            let (diff, truncated) = store
                                .run_bounded(
                                    &row,
                                    &["diff", &from.to_string(), &to.to_string()],
                                    true,
                                )
                                .map_err(failed)?
                                .unwrap_or_default();
                            Ok(json!({"diff": diff, "truncated": truncated}))
                        }
                        _ => {
                            let target = id(&value, "id")?;
                            if row.mode == Mode::Chat
                                && row.state == State::Running
                                && !crate::chat::chat_idle(&hub, &name)
                            {
                                return Err(RpcError::new(
                                    Code::FailedPrecondition,
                                    "the chat sleeve is busy; wait for it to be idle",
                                ));
                            }
                            let output = store
                                .run(&row, &["restore", &target.to_string()], true)
                                .map_err(failed)?
                                .unwrap_or_default();
                            let (before, _) = snapshot_line(&output).map_err(failed)?;
                            hub.emit(
                                "sleeve.restored",
                                &name,
                                json!({"checkpoint": target, "before": before.id}),
                            );
                            Ok(json!({"before": before.json()}))
                        }
                    }
                })
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                respond(&request, result)
            }
        })?;
    }
    Ok(())
}
