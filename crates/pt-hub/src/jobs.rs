//! Node jobs on the hub (SPEC 3.5, 6, 12a, 12b): the request a job carries through the core, its
//! API view, and the hub's copy of each job's log and artifacts. `job_exec.rs` runs them.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use pt_api::{self as api, EpochMs};
use pt_core::{Job, JobId, JobSpec, JobState};
use pt_frame::msg;
use pt_json::ToJson;

use crate::lock::lock;

pt_json::record! {
    /// What `RunJob` asked for. It rides through the core as the job spec's `command`, so the
    /// journal holds it, with its SHA-256 as the spec's `digest`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct JobRequest {
        pub argv: Vec<String>,
        pub target: api::JobTarget,
        pub cwd: Option<String>,
        pub artifacts: Vec<String> = Vec::new(),
        pub on_present: api::OnPresent = api::OnPresent::Idle,
        pub timeout_secs: u64 = msg::DEFAULT_TIMEOUT_SECS,
        pub maintenance: bool = false,
        pub requested_by: String,
        /// The requesting sleeve; none for the owner.
        pub sleeve: Option<u64>,
        pub approval_id: Option<String>,
        pub queued_at_ms: u64,
    }

    /// One artifact as the hub fetched it, in `artifacts.json`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Fetched {
        pub path: String,
        /// The file under the attempt's `artifacts/` dir.
        pub file: String,
        pub size_bytes: u64,
        pub sha256: String,
    }
}

impl JobRequest {
    pub fn spec(&self, even_if_present: bool) -> JobSpec {
        let command = self.to_json().to_string();
        JobSpec {
            digest: pt_hash::to_hex(&pt_hash::sha256(command.as_bytes())),
            arch: match &self.target {
                api::JobTarget::Arch { arch } => arch.clone(),
                api::JobTarget::Node { .. } => String::new(),
            },
            command,
            even_if_present,
        }
    }

    pub fn of(spec: &JobSpec) -> Option<JobRequest> {
        pt_json::from_str(&spec.command).ok()
    }
}

pub fn api_state(s: JobState) -> api::JobState {
    use api::JobState as A;
    match s {
        JobState::Queued => A::Queued,
        JobState::Submitting => A::Submitting,
        JobState::SubmitUncertain { .. } => A::SubmitUncertain,
        JobState::Running => A::Running,
        JobState::Collecting { .. } => A::Collecting,
        JobState::Succeeded => A::Succeeded,
        JobState::Failed(_) => A::Failed,
        JobState::Interrupted => A::Interrupted,
        JobState::Lost { .. } => A::Lost,
        JobState::Cancelling => A::Cancelling,
        JobState::Cancelled => A::Cancelled,
        JobState::Abandoned => A::Abandoned,
    }
}

pub fn ended(s: JobState) -> bool {
    matches!(
        s,
        JobState::Succeeded
            | JobState::Failed(_)
            | JobState::Interrupted
            | JobState::Cancelled
            | JobState::Abandoned
    )
}

// ---- the hub's copy ------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Meta {
    /// A failed append whose rollback could not be synced.
    log_error: Option<String>,
    started_at_ms: Option<u64>,
    fetched: Option<Vec<Fetched>>,
}

/// `<home>/jobs/<job>/<attempt>/`: `log`, `artifacts.json` and `artifacts/<n>`.
pub struct JobStore {
    root: PathBuf,
    meta: Mutex<BTreeMap<(JobId, u32), Meta>>,
}

impl JobStore {
    pub fn new(root: PathBuf) -> JobStore {
        JobStore {
            root,
            meta: Mutex::new(BTreeMap::new()),
        }
    }

    fn dir(&self, job: JobId, attempt: u32) -> PathBuf {
        self.root.join(job.0.to_string()).join(attempt.to_string())
    }

    fn log_path(&self, job: JobId, attempt: u32) -> PathBuf {
        self.dir(job, attempt).join("log")
    }

    pub fn log_len(&self, job: JobId, attempt: u32) -> u64 {
        fs::metadata(self.log_path(job, attempt)).map_or(0, |m| m.len())
    }

    fn check_log(&self, job: JobId, attempt: u32) -> io::Result<()> {
        if let Some(error) = lock(&self.meta)
            .get(&(job, attempt))
            .and_then(|m| m.log_error.clone())
        {
            return Err(io::Error::other(error));
        }
        Ok(())
    }

    /// Syncs the existing copy before using its length as the next fetch offset.
    pub fn log_offset(&self, job: JobId, attempt: u32) -> io::Result<u64> {
        self.check_log(job, attempt)?;
        let path = self.log_path(job, attempt);
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        let f = match OpenOptions::new().append(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(ctx(e)),
        };
        f.sync_data().map_err(ctx)?;
        Ok(f.metadata().map_err(ctx)?.len())
    }

    /// Appends bytes that start at `at`, which must be the hub copy's length.
    pub fn append(&self, job: JobId, attempt: u32, at: u64, bytes: &[u8]) -> io::Result<u64> {
        self.check_log(job, attempt)?;
        let path = self.log_path(job, attempt);
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        fs::create_dir_all(self.dir(job, attempt)).map_err(ctx)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(ctx)?;
        let len = f.metadata().map_err(ctx)?.len();
        if len != at {
            return Err(ctx(io::Error::other(format!(
                "the hub copy is {len} bytes, not {at}"
            ))));
        }
        // A failed write or sync may have extended the file; retry only after a synced rollback.
        if let Err(e) = f.write_all(bytes).and_then(|()| f.sync_data()) {
            if let Err(rollback) = f.set_len(at).and_then(|()| f.sync_data()) {
                let error = ctx(io::Error::new(
                    e.kind(),
                    format!("{e}; rolling back to {at} bytes: {rollback}"),
                ));
                lock(&self.meta)
                    .entry((job, attempt))
                    .or_default()
                    .log_error = Some(error.to_string());
                return Err(error);
            }
            return Err(ctx(e));
        }
        Ok(at + bytes.len() as u64)
    }

    /// Up to `limit` bytes of the hub copy from `offset`, and the copy's length.
    pub fn read_log(
        &self,
        job: JobId,
        attempt: u32,
        offset: u64,
        limit: u64,
    ) -> io::Result<(Vec<u8>, u64)> {
        read_slice(&self.log_path(job, attempt), offset, limit)
    }

    pub fn set_started(&self, job: JobId, attempt: u32, ms: u64) {
        lock(&self.meta)
            .entry((job, attempt))
            .or_default()
            .started_at_ms = Some(ms);
    }

    fn started(&self, job: JobId, attempt: u32) -> Option<u64> {
        lock(&self.meta)
            .get(&(job, attempt))
            .and_then(|m| m.started_at_ms)
    }

    pub fn fetched(&self, job: JobId, attempt: u32) -> Vec<Fetched> {
        let mut m = lock(&self.meta);
        let e = m.entry((job, attempt)).or_default();
        e.fetched
            .get_or_insert_with(|| {
                fs::read(self.dir(job, attempt).join("artifacts.json"))
                    .ok()
                    .and_then(|b| pt_json::from_bytes(&b).ok())
                    .unwrap_or_default()
            })
            .clone()
    }

    pub fn save_fetched(&self, job: JobId, attempt: u32, list: Vec<Fetched>) -> io::Result<()> {
        let path = self.dir(job, attempt).join("artifacts.json");
        crate::home::write_private(&path, list.to_json().to_string().as_bytes())?;
        lock(&self.meta).entry((job, attempt)).or_default().fetched = Some(list);
        Ok(())
    }

    pub fn artifact_path(&self, job: JobId, attempt: u32, file: &str) -> PathBuf {
        self.dir(job, attempt).join("artifacts").join(file)
    }

    /// A slice of fetched artifact `path`, with its record.
    pub fn read_artifact(
        &self,
        job: JobId,
        attempt: u32,
        path: &str,
        offset: u64,
        limit: u64,
    ) -> io::Result<Option<(Fetched, Vec<u8>)>> {
        let Some(f) = self
            .fetched(job, attempt)
            .into_iter()
            .find(|f| f.path == path)
        else {
            return Ok(None);
        };
        let (bytes, _) = read_slice(&self.artifact_path(job, attempt, &f.file), offset, limit)?;
        Ok(Some((f, bytes)))
    }
}

fn read_slice(path: &std::path::Path, offset: u64, limit: u64) -> io::Result<(Vec<u8>, u64)> {
    let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(ctx(e)),
    };
    let len = f.metadata().map_err(ctx)?.len();
    if offset >= len {
        return Ok((Vec::new(), len));
    }
    f.seek(SeekFrom::Start(offset)).map_err(ctx)?;
    let mut buf = Vec::new();
    f.take(limit.min(len - offset))
        .read_to_end(&mut buf)
        .map_err(ctx)?;
    Ok((buf, len))
}

/// A core job as the API shows it.
pub fn view(store: &JobStore, j: &Job) -> api::Job {
    let req = JobRequest::of(&j.spec);
    let fetched = store.fetched(j.id, j.attempt);
    let artifacts = req
        .as_ref()
        .map(|r| r.artifacts.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|p| {
            let f = fetched.iter().find(|f| f.path == *p);
            api::JobArtifact {
                path: p.clone(),
                size_bytes: f.map(|f| f.size_bytes),
                sha256: f.map(|f| f.sha256.clone()),
            }
        })
        .collect();
    api::Job {
        id: j.id.0.to_string(),
        node_id: j.node.map(|n| n.0.to_string()),
        target: req.as_ref().map_or_else(
            || api::JobTarget::Arch {
                arch: j.spec.arch.clone(),
            },
            |r| r.target.clone(),
        ),
        argv: req.as_ref().map(|r| r.argv.clone()).unwrap_or_default(),
        state: api_state(j.state),
        requested_by: req
            .as_ref()
            .map_or_else(|| "owner".into(), |r| r.requested_by.clone()),
        attempt: u64::from(j.attempt),
        even_if_present: j.spec.even_if_present,
        on_present: req.as_ref().map_or(api::OnPresent::Idle, |r| r.on_present),
        approval_id: req.as_ref().and_then(|r| r.approval_id.clone()),
        queued_at_ms: EpochMs(req.as_ref().map_or(0, |r| r.queued_at_ms)),
        started_at_ms: store.started(j.id, j.attempt).map(EpochMs),
        ended_at_ms: ended(j.state).then_some(EpochMs(j.state_at)),
        exit_code: match j.state {
            JobState::Collecting { exit, .. }
            | JobState::Failed(
                pt_core::JobFailure::Exit(exit) | pt_core::JobFailure::Artifact(exit),
            ) => Some(i64::from(exit)),
            JobState::Succeeded => Some(0),
            _ => None,
        },
        error: match j.state {
            JobState::Failed(pt_core::JobFailure::NodeRemoved(n)) => {
                Some(format!("node {} was removed", n.0))
            }
            JobState::Failed(pt_core::JobFailure::Artifact(_)) => {
                let mut issues: Vec<String> = j
                    .artifact_refusals
                    .iter()
                    .map(|r| format!("artifact {}: {}", r.path, r.reason))
                    .collect();
                if let Some(e) = &j.artifact_incomplete {
                    issues.push(format!("collection incomplete: {e}"));
                }
                (!issues.is_empty()).then(|| issues.join("; "))
            }
            _ => None,
        },
        log_bytes: store.log_len(j.id, j.attempt),
        cwd: req.as_ref().and_then(|r| r.cwd.clone()),
        artifacts,
    }
}
