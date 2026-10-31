//! The node job runner (SPEC 3.5, 12a items 5 and 6, 12b item 9).
//!
//! - Each job is journaled (fsync) before exec, and its launch receipt right after the spawn.
//! - The node refuses to start a job unless presence is AWAY, or the job says `even_if_present`.
//! - A job runs isolated (systemd scope or process group on Linux, Job Object on Windows), with
//!   its own timeout and a capped log of stdout and stderr together.
//! - While jobs run, presence is watched: a person arriving lowers each job to idle priority, or
//!   kills it when it asked for `on_present: kill`.
//! - Every (job, attempt) stays as a tombstone until the hub acknowledges collection.
//! - A job never re-runs by itself: after an agent restart, a job that was running is reported
//!   Interrupted (its processes were still alive and were killed) or Lost (it ended unobserved).

mod journal;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use pt_frame::msg::{
    self, ErrorCode, JobInfo, JobState, LogEnd, OnPresent, PresenceState, SubmitJob,
};

use crate::presence::{Detector, Gate, job_gate};
use crate::sys::{Isolator, Orphan, Proc, Receipt};
use journal::{Entry, Journal};

const POLL: Duration = Duration::from_millis(50);
/// Time between SIGTERM and SIGKILL when the node stops a job.
const GRACE: Duration = Duration::from_secs(3);
/// How long the end of a job waits for its output pipe to close once the tree is killed.
const DRAIN: Duration = Duration::from_secs(5);
const MAX_ID: usize = 64;
const MAX_ARGS_BYTES: usize = 256 << 10;
const MAX_ENV: usize = 256;

#[derive(Debug)]
pub struct JobError {
    pub code: ErrorCode,
    pub message: String,
}

impl JobError {
    fn new(code: ErrorCode, message: impl Into<String>) -> JobError {
        JobError {
            code,
            message: message.into(),
        }
    }
}

pub struct Options {
    /// Linux: allow systemd scopes (a trial run still decides).
    pub allow_scope: bool,
    pub presence_interval: Duration,
}

#[derive(Clone)]
pub struct Runner {
    inner: Arc<Inner>,
}

struct Inner {
    logs: PathBuf,
    work: PathBuf,
    epoch: String,
    isolator: Isolator,
    isolation_note: String,
    detector: Arc<Mutex<Detector>>,
    table: Mutex<Table>,
}

struct Table {
    journal: Journal,
    jobs: BTreeMap<(String, u64), Job>,
}

struct Job {
    info: JobInfo,
    log_cap: u64,
    /// From the launch receipt, to tell the job's process from a reused pid after a restart.
    receipt_start: Option<u64>,
    live: Option<Arc<Live>>,
}

/// Shared between the request side, the presence watcher and the job's waiter thread.
#[derive(Default)]
struct Live {
    cancel: AtomicBool,
    kill_present: AtomicBool,
    lower: AtomicBool,
    log_bytes: AtomicU64,
    log_truncated: AtomicBool,
}

#[derive(Clone, Copy)]
enum Stop {
    Cancel,
    Timeout,
    Present,
}

impl Runner {
    /// Opens or creates the journal under `dir`, settles every job it shows unfinished, and
    /// starts the presence watcher.
    pub fn open(
        dir: &Path,
        detector: Arc<Mutex<Detector>>,
        opts: Options,
    ) -> Result<Runner, String> {
        let logs = dir.join("logs");
        let work = dir.join("work");
        for d in [dir, &logs, &work] {
            fs::create_dir_all(d).map_err(|e| format!("creating {}: {e}", d.display()))?;
        }
        let opened = Journal::open(&dir.join("jobs.jsonl"))?;
        if opened.torn > 0 {
            eprintln!(
                "protectorate-node: journal: dropped a torn last line of {} bytes",
                opened.torn
            );
        }
        let (isolator, isolation_note) = Isolator::probe(opts.allow_scope);
        let mut jobs = BTreeMap::new();
        for e in opened.entries {
            replay(&mut jobs, e);
        }
        let inner = Arc::new(Inner {
            logs,
            work,
            epoch: opened.epoch,
            isolator,
            isolation_note,
            detector,
            table: Mutex::new(Table {
                journal: opened.journal,
                jobs,
            }),
        });
        let runner = Runner { inner };
        runner.settle_orphans()?;
        let watcher = runner.clone();
        thread::Builder::new()
            .name("presence-watch".into())
            .spawn(move || watcher.watch_presence(opts.presence_interval))
            .map_err(|e| format!("starting the presence watcher: {e}"))?;
        Ok(runner)
    }

    pub fn epoch(&self) -> &str {
        &self.inner.epoch
    }

    /// How jobs are isolated here, and why.
    pub fn isolation(&self) -> &str {
        &self.inner.isolation_note
    }

    fn table(&self) -> MutexGuard<'_, Table> {
        self.inner.table.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn settle_orphans(&self) -> Result<(), String> {
        let mut t = self.table();
        let t = &mut *t;
        for ((id, attempt), job) in t.jobs.iter_mut() {
            if job.info.state.is_terminal() {
                continue;
            }
            let receipt = job.info.pid.map(|pid| Receipt {
                pid,
                start_time: job.receipt_start,
                scope: job.info.scope.clone(),
            });
            let unit = unit_name(&self.inner.epoch, id, *attempt);
            let (state, reason) = match self.inner.isolator.reap_orphan(receipt.as_ref(), &unit) {
                Orphan::Killed => (
                    JobState::Interrupted,
                    "the node agent restarted while the job ran; its processes were killed",
                ),
                Orphan::Gone if receipt.is_none() => (
                    JobState::Lost,
                    "the node agent stopped before the job's launch was recorded",
                ),
                Orphan::Gone => (
                    JobState::Lost,
                    "the node agent restarted while the job ran; it ended unobserved",
                ),
            };
            let log_bytes = fs::metadata(log_path(&self.inner.logs, id, *attempt))
                .map(|m| m.len())
                .unwrap_or(0);
            let entry = Entry::End {
                job_id: id.clone(),
                attempt: *attempt,
                state,
                exit_code: None,
                reason: Some(reason.into()),
                log_bytes,
                log_truncated: log_bytes >= job.log_cap,
                at: crate::unix_now(),
            };
            t.journal.append(&entry).map_err(|e| e.to_string())?;
            apply_end(&mut job.info, &entry);
        }
        Ok(())
    }

    pub fn submit(&self, spec: SubmitJob) -> Result<JobInfo, JobError> {
        validate(&spec)?;
        if let Some(info) = self.admit(&spec)? {
            return Ok(info);
        }
        if !spec.even_if_present {
            let p = self.detect();
            if let Gate::Refuse(why) = job_gate(&p) {
                return Err(JobError::new(ErrorCode::Present, why));
            }
        }
        let cwd = match &spec.cwd {
            Some(c) => PathBuf::from(c),
            None => self.inner.work.clone(),
        };
        if !cwd.is_dir() {
            return Err(JobError::new(
                ErrorCode::BadRequest,
                format!("cwd {} is not a directory", cwd.display()),
            ));
        }

        let mut t = self.table();
        if let Some(info) = admit_locked(&t, &spec)? {
            return Ok(info);
        }
        let now = crate::unix_now();
        t.journal
            .append(&Entry::Submit {
                spec: spec.clone(),
                at: now,
            })
            .map_err(internal)?;
        let key = (spec.job_id.clone(), spec.attempt);
        let mut job = Job {
            info: new_info(&spec, now),
            log_cap: spec.log_cap,
            receipt_start: None,
            live: None,
        };
        let launched = self.launch(&spec, &cwd);
        match launched {
            Ok((proc, reader, live)) => {
                let r = proc.receipt();
                let entry = Entry::Start {
                    job_id: spec.job_id.clone(),
                    attempt: spec.attempt,
                    pid: r.pid,
                    start_time: r.start_time,
                    scope: r.scope.clone(),
                    isolation: self.inner.isolator.kind(),
                    at: crate::unix_now(),
                };
                // The process already runs: a failed receipt is logged, never a reason to stop.
                if let Err(e) = t.journal.append(&entry) {
                    eprintln!("protectorate-node: {e}");
                }
                apply_start(&mut job.info, &entry);
                job.receipt_start = r.start_time;
                job.live = Some(live.clone());
                let info = job.info.clone();
                t.jobs.insert(key.clone(), job);
                drop(t);
                let runner = self.clone();
                let timeout = Duration::from_secs(spec.timeout_secs);
                let spawned = thread::Builder::new()
                    .name(format!("job-{}-{}", key.0, key.1))
                    .spawn(move || runner.wait(key, proc, reader, live, timeout));
                if let Err(e) = spawned {
                    return Err(internal(io::Error::other(format!(
                        "starting the job's waiter thread: {e}"
                    ))));
                }
                Ok(info)
            }
            Err(e) => {
                let entry = Entry::End {
                    job_id: spec.job_id.clone(),
                    attempt: spec.attempt,
                    state: JobState::Failed,
                    exit_code: None,
                    reason: Some(format!("start failed: {e}")),
                    log_bytes: 0,
                    log_truncated: false,
                    at: crate::unix_now(),
                };
                if let Err(e) = t.journal.append(&entry) {
                    eprintln!("protectorate-node: {e}");
                }
                apply_end(&mut job.info, &entry);
                let info = job.info.clone();
                t.jobs.insert(key, job);
                Ok(info)
            }
        }
    }

    /// The existing job when this (job, attempt) is already known; an error when it may not run.
    fn admit(&self, spec: &SubmitJob) -> Result<Option<JobInfo>, JobError> {
        admit_locked(&self.table(), spec)
    }

    fn launch(
        &self,
        spec: &SubmitJob,
        cwd: &Path,
    ) -> io::Result<(Proc, thread::JoinHandle<()>, Arc<Live>)> {
        let path = log_path(&self.inner.logs, &spec.job_id, spec.attempt);
        let log = File::create(&path)
            .map_err(|e| io::Error::new(e.kind(), format!("creating {}: {e}", path.display())))?;
        let (rx, tx) = io::pipe()?;
        let tx2 = tx.try_clone()?;
        let unit = unit_name(&self.inner.epoch, &spec.job_id, spec.attempt);
        let proc = self
            .inner
            .isolator
            .spawn(spec, &unit, cwd, tx.into(), tx2.into())?;
        let live = Arc::new(Live::default());
        let cap = spec.log_cap;
        let l = live.clone();
        let reader = thread::Builder::new()
            .name(format!("log-{}-{}", spec.job_id, spec.attempt))
            .spawn(move || capture(rx, log, cap, &l))?;
        Ok((proc, reader, live))
    }

    fn wait(
        &self,
        key: (String, u64),
        mut proc: Proc,
        reader: thread::JoinHandle<()>,
        live: Arc<Live>,
        timeout: Duration,
    ) {
        let started = Instant::now();
        let mut stop: Option<(Stop, Instant)> = None;
        let mut killed = false;
        let mut lowered = false;
        let mut wait_err = None;
        let code = loop {
            match proc.try_wait() {
                Ok(Some(code)) => break Some(code),
                Ok(None) => {}
                Err(e) => {
                    wait_err = Some(e.to_string());
                    proc.kill();
                    break None;
                }
            }
            let now = Instant::now();
            match stop {
                None => {
                    let why = if live.cancel.load(Ordering::SeqCst) {
                        Some(Stop::Cancel)
                    } else if live.kill_present.load(Ordering::SeqCst) {
                        Some(Stop::Present)
                    } else if now.duration_since(started) >= timeout {
                        Some(Stop::Timeout)
                    } else {
                        None
                    };
                    if let Some(why) = why {
                        proc.terminate();
                        stop = Some((why, now));
                    }
                }
                Some((_, at)) if !killed && now.duration_since(at) >= GRACE => {
                    proc.kill();
                    killed = true;
                }
                Some(_) => {}
            }
            if !lowered && live.lower.load(Ordering::SeqCst) {
                lowered = true;
                self.lowered(&key, proc.lower());
            }
            thread::sleep(POLL);
        };
        // Whatever the leader left behind goes with it; the output pipe then closes.
        proc.kill();
        let (done_tx, done_rx) = mpsc::channel();
        let joiner = thread::spawn(move || {
            let _ = reader.join();
            let _ = done_tx.send(());
        });
        let drained = done_rx.recv_timeout(DRAIN).is_ok();
        if drained {
            let _ = joiner.join();
        }
        let (state, exit_code, mut reason) = match (stop.map(|s| s.0), code) {
            (Some(Stop::Cancel), _) => (
                JobState::Cancelled,
                Some(msg::EXIT_CANCELLED),
                Some("cancelled by the hub".to_string()),
            ),
            (Some(Stop::Timeout), _) => (
                JobState::Failed,
                Some(msg::EXIT_TIMEOUT),
                Some(format!("timed out after {} s", timeout.as_secs())),
            ),
            (Some(Stop::Present), _) => (
                JobState::Failed,
                Some(msg::EXIT_KILLED),
                Some("killed: a person is present (on_present: kill)".to_string()),
            ),
            (None, Some(0)) => (JobState::Succeeded, Some(0), None),
            (None, Some(c)) => (JobState::Failed, Some(c), None),
            (None, None) => (
                JobState::Lost,
                None,
                Some(format!(
                    "waiting for the job failed: {}",
                    wait_err.unwrap_or_default()
                )),
            ),
        };
        if !drained {
            let note = "output still open after the job ended; the log may be incomplete";
            reason = Some(match reason {
                Some(r) => format!("{r}; {note}"),
                None => note.to_string(),
            });
        }
        let entry = Entry::End {
            job_id: key.0.clone(),
            attempt: key.1,
            state,
            exit_code,
            reason,
            log_bytes: live.log_bytes.load(Ordering::SeqCst),
            log_truncated: live.log_truncated.load(Ordering::SeqCst),
            at: crate::unix_now(),
        };
        let mut t = self.table();
        if let Err(e) = t.journal.append(&entry) {
            eprintln!("protectorate-node: {e}");
        }
        if let Some(job) = t.jobs.get_mut(&key) {
            apply_end(&mut job.info, &entry);
            job.live = None;
        }
    }

    fn lowered(&self, key: &(String, u64), result: io::Result<()>) {
        if let Err(e) = &result {
            eprintln!(
                "protectorate-node: lowering job {} attempt {}: {e}",
                key.0, key.1
            );
        }
        let mut t = self.table();
        let entry = Entry::Lowered {
            job_id: key.0.clone(),
            attempt: key.1,
            at: crate::unix_now(),
        };
        if let Err(e) = t.journal.append(&entry) {
            eprintln!("protectorate-node: {e}");
        }
        if let Some(job) = t.jobs.get_mut(key) {
            job.info.lowered = true;
        }
    }

    pub fn status(&self, job_id: &str, attempt: Option<u64>) -> Option<JobInfo> {
        let t = self.table();
        match attempt {
            Some(a) => t.jobs.get(&(job_id.to_string(), a)).map(|j| j.current()),
            None => t
                .jobs
                .range((job_id.to_string(), 0)..=(job_id.to_string(), u64::MAX))
                .next_back()
                .map(|(_, j)| j.current()),
        }
    }

    pub fn list(&self) -> Vec<JobInfo> {
        self.table().jobs.values().map(Job::current).collect()
    }

    pub fn cancel(&self, job_id: &str, attempt: u64) -> Result<JobInfo, JobError> {
        let mut t = self.table();
        let t = &mut *t;
        let key = (job_id.to_string(), attempt);
        let job = t
            .jobs
            .get_mut(&key)
            .ok_or_else(|| not_found(job_id, attempt))?;
        if job.info.state == JobState::Running {
            let entry = Entry::Cancel {
                job_id: job_id.to_string(),
                attempt,
                at: crate::unix_now(),
            };
            t.journal.append(&entry).map_err(internal)?;
            job.info.state = JobState::Cancelling;
            if let Some(live) = &job.live {
                live.cancel.store(true, Ordering::SeqCst);
            }
        }
        Ok(job.current())
    }

    /// Drops the tombstone and the log once the hub has what it needs.
    pub fn ack(&self, job_id: &str, attempt: u64) -> Result<JobInfo, JobError> {
        let mut t = self.table();
        let key = (job_id.to_string(), attempt);
        let job = t.jobs.get(&key).ok_or_else(|| not_found(job_id, attempt))?;
        if !job.info.state.is_terminal() {
            return Err(JobError::new(
                ErrorCode::NotFinished,
                format!(
                    "job {job_id} attempt {attempt} is {}, not finished",
                    job.info.state
                ),
            ));
        }
        let info = job.current();
        t.journal
            .append(&Entry::Collected {
                job_id: job_id.to_string(),
                attempt,
                at: crate::unix_now(),
            })
            .map_err(internal)?;
        t.jobs.remove(&key);
        drop(t);
        let path = log_path(&self.inner.logs, job_id, attempt);
        if let Err(e) = fs::remove_file(&path)
            && e.kind() != io::ErrorKind::NotFound
        {
            eprintln!("protectorate-node: removing {}: {e}", path.display());
        }
        Ok(info)
    }

    /// Sends log bytes from `offset`, at most `max`, in chunks to `sink`.
    pub fn read_log(
        &self,
        job_id: &str,
        attempt: u64,
        offset: u64,
        max: u64,
        mut sink: impl FnMut(&[u8]) -> io::Result<()>,
    ) -> Result<LogEnd, JobError> {
        let (finished, truncated) = {
            let t = self.table();
            let job = t
                .jobs
                .get(&(job_id.to_string(), attempt))
                .ok_or_else(|| not_found(job_id, attempt))?;
            let info = job.current();
            (info.state.is_terminal(), info.log_truncated)
        };
        let path = log_path(&self.inner.logs, job_id, attempt);
        let mut file = match File::open(&path) {
            Ok(f) => Some(f),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(internal(e)),
        };
        let size = match &file {
            Some(f) => f.metadata().map_err(internal)?.len(),
            None => 0,
        };
        let start = offset.min(size);
        let end = start.saturating_add(max.min(msg::MAX_FETCH)).min(size);
        if let Some(f) = file.as_mut()
            && end > start
        {
            f.seek(SeekFrom::Start(start)).map_err(internal)?;
            let mut left = end - start;
            let mut buf = vec![0u8; msg::CHUNK];
            while left > 0 {
                let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
                let n = f.read(&mut buf[..want]).map_err(internal)?;
                if n == 0 {
                    break;
                }
                sink(&buf[..n]).map_err(internal)?;
                left -= n as u64;
            }
        }
        let next = end;
        Ok(LogEnd {
            offset: next,
            size,
            done: finished && next >= size,
            truncated,
        })
    }

    pub fn detect(&self) -> msg::Presence {
        self.inner
            .detector
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .detect()
    }

    fn watch_presence(&self, interval: Duration) {
        loop {
            thread::sleep(interval);
            let running: Vec<(OnPresent, Arc<Live>)> = self
                .table()
                .jobs
                .values()
                .filter_map(|j| Some((j.info.on_present, j.live.clone()?)))
                .collect();
            if running.is_empty() {
                continue;
            }
            if self.detect().state != PresenceState::Present {
                continue;
            }
            for (on_present, live) in running {
                match on_present {
                    OnPresent::Lower => live.lower.store(true, Ordering::SeqCst),
                    OnPresent::Kill => live.kill_present.store(true, Ordering::SeqCst),
                }
            }
        }
    }
}

impl Job {
    /// The info with live log counters folded in.
    fn current(&self) -> JobInfo {
        let mut info = self.info.clone();
        if let Some(live) = &self.live {
            info.log_bytes = live.log_bytes.load(Ordering::SeqCst);
            info.log_truncated = live.log_truncated.load(Ordering::SeqCst);
        }
        info
    }
}

fn admit_locked(t: &Table, spec: &SubmitJob) -> Result<Option<JobInfo>, JobError> {
    if let Some(job) = t.jobs.get(&(spec.job_id.clone(), spec.attempt)) {
        return Ok(Some(job.current()));
    }
    let id = spec.job_id.clone();
    for ((_, attempt), job) in t.jobs.range((id.clone(), 0)..=(id, u64::MAX)) {
        if *attempt > spec.attempt {
            return Err(JobError::new(
                ErrorCode::StaleAttempt,
                format!(
                    "job {} already has attempt {attempt}, newer than {}",
                    spec.job_id, spec.attempt
                ),
            ));
        }
        if !job.info.state.is_terminal() {
            return Err(JobError::new(
                ErrorCode::Busy,
                format!(
                    "job {} attempt {attempt} is still {}",
                    spec.job_id, job.info.state
                ),
            ));
        }
    }
    Ok(None)
}

/// Copies the job's output into its log, up to `cap` bytes; the rest is read and dropped so the
/// job never blocks on a full pipe.
fn capture(mut rx: io::PipeReader, mut log: File, cap: u64, live: &Live) {
    let mut buf = vec![0u8; msg::CHUNK];
    let mut written = 0u64;
    loop {
        let n = match rx.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let room = cap.saturating_sub(written);
        let keep = (n as u64).min(room) as usize;
        if keep < n {
            live.log_truncated.store(true, Ordering::SeqCst);
        }
        if keep > 0 {
            if log.write_all(&buf[..keep]).is_err() {
                live.log_truncated.store(true, Ordering::SeqCst);
                written = cap;
                continue;
            }
            written += keep as u64;
            live.log_bytes.store(written, Ordering::SeqCst);
        }
    }
    let _ = log.sync_data();
}

fn validate(spec: &SubmitJob) -> Result<(), JobError> {
    let bad = |m: String| Err(JobError::new(ErrorCode::BadRequest, m));
    let id = &spec.job_id;
    if id.is_empty()
        || id.len() > MAX_ID
        || id.starts_with(['.', '-'])
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return bad(format!(
            "job_id must be 1 to {MAX_ID} of [A-Za-z0-9._-], not starting with . or -"
        ));
    }
    let mut size = 0usize;
    match (&spec.argv, &spec.script) {
        (Some(argv), None) => {
            if argv.first().is_none_or(String::is_empty) {
                return bad("argv needs a program".into());
            }
            for a in argv {
                size += a.len();
                if a.contains('\0') {
                    return bad("argv holds a NUL".into());
                }
            }
        }
        (None, Some(script)) => {
            size += script.len();
            if script.contains('\0') {
                return bad("script holds a NUL".into());
            }
        }
        _ => return bad("give exactly one of argv and script".into()),
    }
    if spec.env.len() > MAX_ENV {
        return bad(format!("at most {MAX_ENV} env entries"));
    }
    for (k, v) in &spec.env {
        size += k.len() + v.len();
        if k.is_empty() || k.contains(['=', '\0']) || v.contains('\0') {
            return bad(format!("env key {k:?} or its value is not allowed"));
        }
    }
    if size > MAX_ARGS_BYTES {
        return bad(format!(
            "argv, script and env exceed {MAX_ARGS_BYTES} bytes"
        ));
    }
    if spec.timeout_secs == 0 || spec.timeout_secs > msg::MAX_TIMEOUT_SECS {
        return bad(format!(
            "timeout_secs must be 1 to {}",
            msg::MAX_TIMEOUT_SECS
        ));
    }
    if spec.log_cap == 0 || spec.log_cap > msg::MAX_LOG_CAP {
        return bad(format!("log_cap must be 1 to {}", msg::MAX_LOG_CAP));
    }
    if let Some(c) = &spec.cwd
        && !Path::new(c).is_absolute()
    {
        return bad(format!("cwd {c:?} is not absolute"));
    }
    Ok(())
}

fn replay(jobs: &mut BTreeMap<(String, u64), Job>, e: Entry) {
    let key = |id: &str, attempt: u64| (id.to_string(), attempt);
    match &e {
        Entry::Epoch { .. } => {}
        Entry::Submit { spec, at } => {
            jobs.insert(
                key(&spec.job_id, spec.attempt),
                Job {
                    info: new_info(spec, *at),
                    log_cap: spec.log_cap,
                    receipt_start: None,
                    live: None,
                },
            );
        }
        Entry::Start {
            job_id, attempt, ..
        } => {
            if let Some(j) = jobs.get_mut(&key(job_id, *attempt)) {
                apply_start(&mut j.info, &e);
                if let Entry::Start { start_time, .. } = e {
                    j.receipt_start = start_time;
                }
            }
        }
        Entry::Lowered {
            job_id, attempt, ..
        } => {
            if let Some(j) = jobs.get_mut(&key(job_id, *attempt)) {
                j.info.lowered = true;
            }
        }
        Entry::Cancel {
            job_id, attempt, ..
        } => {
            if let Some(j) = jobs.get_mut(&key(job_id, *attempt))
                && !j.info.state.is_terminal()
            {
                j.info.state = JobState::Cancelling;
            }
        }
        Entry::End {
            job_id, attempt, ..
        } => {
            if let Some(j) = jobs.get_mut(&key(job_id, *attempt)) {
                apply_end(&mut j.info, &e);
            }
        }
        Entry::Collected {
            job_id, attempt, ..
        } => {
            jobs.remove(&key(job_id, *attempt));
        }
    }
}

fn new_info(spec: &SubmitJob, at: u64) -> JobInfo {
    JobInfo {
        job_id: spec.job_id.clone(),
        attempt: spec.attempt,
        state: JobState::Running,
        submitted_at: at,
        started_at: None,
        ended_at: None,
        pid: None,
        isolation: None,
        scope: None,
        exit_code: None,
        reason: None,
        log_bytes: 0,
        log_truncated: false,
        lowered: false,
        on_present: spec.on_present,
        timeout_secs: spec.timeout_secs,
    }
}

fn apply_start(info: &mut JobInfo, e: &Entry) {
    if let Entry::Start {
        pid,
        scope,
        isolation,
        at,
        ..
    } = e
    {
        info.pid = Some(*pid);
        info.scope = scope.clone();
        info.isolation = Some(*isolation);
        info.started_at = Some(*at);
    }
}

fn apply_end(info: &mut JobInfo, e: &Entry) {
    if let Entry::End {
        state,
        exit_code,
        reason,
        log_bytes,
        log_truncated,
        at,
        ..
    } = e
    {
        info.state = *state;
        info.exit_code = *exit_code;
        info.reason = reason.clone();
        info.log_bytes = *log_bytes;
        info.log_truncated = *log_truncated;
        info.ended_at = Some(*at);
    }
}

fn log_path(logs: &Path, job_id: &str, attempt: u64) -> PathBuf {
    logs.join(format!("{job_id}.{attempt}.log"))
}

/// The scope name: the epoch prefix keeps two agents on one machine apart.
fn unit_name(epoch: &str, job_id: &str, attempt: u64) -> String {
    let short: String = epoch.chars().take(8).collect();
    format!("pt-job-{short}-{job_id}-{attempt}")
}

fn not_found(job_id: &str, attempt: u64) -> JobError {
    JobError::new(
        ErrorCode::NotFound,
        format!("no job {job_id} attempt {attempt} on this node"),
    )
}

fn internal(e: io::Error) -> JobError {
    JobError::new(ErrorCode::Internal, e.to_string())
}
