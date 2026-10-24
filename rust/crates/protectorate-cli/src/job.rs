//! Submission and resumable job log streaming for the owner CLI.
use super::{format, JobLogs, JobRun, DEFAULT_JOB_TIMEOUT, MAX_JOB_TIMEOUT};
use protectorate_connect_client::Client;
use protectorate_wire::{
    proto_duration,
    v1::{self, job_service, job_source, job_target, watch_job_response},
    Code, RpcError,
};
use std::{
    io::Write,
    time::{Duration, Instant},
};

pub(crate) const FOLLOW_WINDOW: Duration = Duration::from_secs(5 * 60);

#[derive(Debug)]
pub enum RunError {
    Rpc(RpcError),
    Status { code: i32, message: String },
}
impl From<RpcError> for RunError {
    fn from(error: RpcError) -> Self {
        Self::Rpc(error)
    }
}
impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rpc(error) => error.fmt(f),
            Self::Status { message, .. } => f.write_str(message),
        }
    }
}
impl std::error::Error for RunError {}

pub(crate) fn run_job(
    client: &Client,
    cancel: Option<&Client>,
    spec: &JobRun,
    json_mode: bool,
    signals: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<String, RunError> {
    let mut nonce = [0_u8; 16];
    getrandom::getrandom(&mut nonce)
        .map_err(|e| RpcError::new(Code::Internal, format!("request id: {e}")))?;
    let key = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let request = submit_request(spec, &key)?;
    let _guard = SignalGuard::install(signals);
    if signals && _guard.is_none() {
        return Err(RpcError::new(Code::Internal, "install job interrupt handler failed").into());
    }
    std::thread::scope(|scope| {
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher = if signals {
            let finished_ref = std::sync::Arc::clone(&finished);
            Some(scope.spawn(move || {
                let mut launched = false;
                let (cancel_sent, cancel_result) = std::sync::mpsc::channel();
                while !finished_ref.load(std::sync::atomic::Ordering::Relaxed) {
                    let interrupts = SIGNAL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
                    if interrupts > 0 && !launched {
                        eprintln!("interrupted: canceling job (interrupt again to quit)");
                        launched = true;
                        if let Some(cancel) = cancel {
                            let request = v1::CancelJobRequest {
                                request_id: key.clone(),
                                ..Default::default()
                            };
                            let done = std::sync::Arc::clone(&finished_ref);
                            let cancel_sent = cancel_sent.clone();
                            scope.spawn(move || {
                                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                                    match cancel.unary(job_service::CANCEL_JOB, &request) {
                                        Ok(_) => {
                                            let _ = cancel_sent.send(());
                                            break;
                                        }
                                        Err(error) if error.code == Code::NotFound => {
                                            std::thread::sleep(Duration::from_millis(100))
                                        }
                                        Err(_) => break,
                                    }
                                }
                            });
                        }
                    }
                    if interrupts >= 2 {
                        let _ = cancel_result.recv_timeout(Duration::from_secs(1));
                        std::process::exit(130);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }))
        } else {
            None
        };
        let result = (|| {
            let job = client.unary(job_service::SUBMIT_JOB, &request)?;
            if job.id.is_empty() {
                return Err(RpcError::new(Code::DataLoss, "submitted job has no id").into());
            }
            writeln!(stderr, "job {} on {}", job.id, job.node).map_err(io_error)?;
            let log: &mut dyn Write = if json_mode { stderr } else { stdout };
            let final_job = follow(client, &job.id, true, log)?.ok_or_else(|| {
                RpcError::new(Code::DataLoss, "job watch ended without final state")
            })?;
            let summary = format::job_summary_line(&final_job);
            writeln!(stderr, "{summary}").map_err(io_error)?;
            let rendered = if json_mode {
                super::json_line(&final_job)?
            } else {
                String::new()
            };
            if final_job.state() == v1::JobState::Succeeded {
                return Ok(rendered);
            }
            if json_mode {
                stdout.write_all(rendered.as_bytes()).map_err(io_error)?;
            }
            // Scripts tell a timeout from a failure by status, as with timeout(1).
            let code = match final_job.state() {
                v1::JobState::TimedOut => 124,
                v1::JobState::Canceled => 130,
                v1::JobState::Killed => 137,
                _ => final_job.exit_code,
            };
            Err(RunError::Status {
                code: if (1..=255).contains(&code) { code } else { 1 },
                message: summary,
            })
        })();
        finished.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(watcher) = watcher {
            let _ = watcher.join();
        }
        result
    })
}

pub(crate) fn logs(
    client: &Client,
    spec: &JobLogs,
    _signals: bool,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<String, RunError> {
    follow(client, &spec.id, spec.follow, stdout)?;
    Ok(String::new())
}

fn submit_request(spec: &JobRun, key: &str) -> Result<v1::SubmitJobRequest, RpcError> {
    use job_target::Target;
    let on = spec.on.as_str();
    let target = if on.starts_with("lease-")
        && on.len() == 14
        && on[6..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        Target::LeaseId(on.to_owned())
    } else if on.contains(['=', ':']) {
        Target::Selector(selector(on)?)
    } else if !on.is_empty() {
        Target::Node(on.to_owned())
    } else {
        return Err(RpcError::invalid("job run needs --on"));
    };
    let source = if !spec.git.is_empty() {
        let (repo, commit) = spec.git.rsplit_once('@').unwrap_or((&spec.git, ""));
        Some(job_source::Kind::Git(v1::GitSource {
            repo: super::absolute_disk_path(if repo.is_empty() { "." } else { repo })?,
            commit: commit.to_owned(),
        }))
    } else if !spec.artifact.is_empty() {
        Some(job_source::Kind::Artifact(v1::ArtifactSource {
            path: super::absolute_disk_path(&spec.artifact)?,
            workdir: spec.workdir.clone(),
        }))
    } else {
        None
    };
    if spec.timeout > MAX_JOB_TIMEOUT {
        return Err(RpcError::invalid("job timeout cannot exceed 24h"));
    }
    if spec.command.is_empty() {
        return Err(RpcError::invalid("job command is required"));
    }
    let timeout = if spec.timeout.is_zero() {
        DEFAULT_JOB_TIMEOUT
    } else {
        spec.timeout
    };
    Ok(v1::SubmitJobRequest {
        target: Some(v1::JobTarget {
            target: Some(target),
        }),
        source: source.map(|kind| v1::JobSource { kind: Some(kind) }),
        command: spec.command.clone(),
        env: spec.env.iter().cloned().collect(),
        timeout: Some(proto_duration(timeout)),
        artifact_globs: spec.artifact_globs.clone(),
        request_id: key.to_owned(),
        admin: spec.admin,
        limits: (spec.cpu_percent > 0 || spec.memory_bytes > 0).then(|| v1::JobLimits {
            cpu_percent: spec.cpu_percent,
            memory_bytes: spec.memory_bytes,
        }),
    })
}
fn selector(text: &str) -> Result<v1::NodeSelector, RpcError> {
    let mut selector = v1::NodeSelector::default();
    for term in text.split(',') {
        let (key, value) = if let Some((key @ ("gpu" | "npu"), value)) = term.split_once(':') {
            (key, value)
        } else {
            term.split_once('=')
                .ok_or_else(|| RpcError::invalid(format!("bad selector term {term:?}")))?
        };
        if value.is_empty() {
            return Err(RpcError::invalid(format!("bad selector term {term:?}")));
        }
        match key {
            "os" => selector.os = value.into(),
            "arch" => selector.arch = value.into(),
            "gpu" => selector.gpu = value.into(),
            "npu" => selector.npu = value.into(),
            "mem" | "min-free-mem" => selector.min_free_memory_bytes = parse_size(value)?,
            "gpu-vram" | "vram" => selector.gpu_vram_bytes = parse_size(value)?,
            _ => return Err(RpcError::invalid(format!("unknown selector key {key:?}"))),
        }
    }
    Ok(selector)
}
fn parse_size(text: &str) -> Result<u64, RpcError> {
    let mut body = text.trim().to_ascii_uppercase();
    if let Some(stripped) = body.strip_suffix('B') {
        body = stripped.into();
    }
    if let Some(stripped) = body.strip_suffix('I') {
        body = stripped.into();
    }
    let mut mult = 1_u64;
    for (index, suffix) in ["K", "M", "G", "T"].into_iter().enumerate() {
        if let Some(stripped) = body.strip_suffix(suffix) {
            mult = 1_u64 << (10 * (index + 1));
            body = stripped.into();
            break;
        }
    }
    let magnitude: f64 = body
        .trim()
        .parse()
        .map_err(|_| RpcError::invalid("invalid selector size"))?;
    if !magnitude.is_finite() || magnitude < 0.0 {
        return Err(RpcError::invalid("invalid selector size"));
    }
    let bytes = magnitude * mult as f64;
    if bytes > u64::MAX as f64 {
        return Err(RpcError::invalid("selector size overflow"));
    }
    Ok(bytes as u64)
}
fn follow(
    client: &Client,
    id: &str,
    follow: bool,
    out: &mut dyn Write,
) -> Result<Option<v1::Job>, RpcError> {
    let mut offset = 0_u64;
    let mut last: Option<v1::Job> = None;
    let mut progress = Instant::now();
    let mut delay = Duration::from_secs(1);
    loop {
        let mut local_error = None;
        let request = v1::WatchJobRequest {
            id: id.to_owned(),
            from_offset: offset,
            follow,
        };
        let result = client.server_stream(job_service::WATCH_JOB, &request, |response| {
            progress = Instant::now();
            match response.event {
                Some(watch_job_response::Event::Job(job)) => last = Some(job),
                Some(watch_job_response::Event::Log(log)) => {
                    let end = log
                        .offset
                        .checked_add(log.data.len() as u64)
                        .ok_or_else(|| RpcError::new(Code::DataLoss, "job log offset overflow"))?;
                    if log.offset > offset {
                        return Err(RpcError::new(Code::DataLoss, "job log has a gap"));
                    }
                    if end > offset {
                        out.write_all(&log.data[(offset - log.offset) as usize..])
                            .map_err(|e| {
                                let e = io_error(e);
                                local_error = Some(e.clone());
                                e
                            })?;
                        offset = end;
                    }
                }
                None => {}
            }
            Ok(())
        });
        if let Some(error) = local_error {
            return Err(error);
        }
        if result.is_ok() && (!follow || last.as_ref().is_some_and(terminal)) {
            return Ok(last);
        }
        if !follow {
            return result.map(|()| last);
        }
        if let Err(error) = &result {
            if !matches!(
                error.code,
                Code::Unavailable | Code::Unknown | Code::Internal | Code::DeadlineExceeded
            ) {
                return Err(error.clone());
            }
        }
        if progress.elapsed() >= FOLLOW_WINDOW {
            return Err(result.err().unwrap_or_else(|| {
                RpcError::new(
                    Code::Unavailable,
                    format!("job {id} watch ended before terminal state"),
                )
            }));
        }
        std::thread::sleep(delay.min(FOLLOW_WINDOW.saturating_sub(progress.elapsed())));
        delay = (delay * 2).min(Duration::from_secs(5));
    }
}
fn terminal(job: &v1::Job) -> bool {
    matches!(
        job.state(),
        v1::JobState::Succeeded
            | v1::JobState::Failed
            | v1::JobState::Canceled
            | v1::JobState::TimedOut
            | v1::JobState::Killed
    )
}
fn io_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}

static SIGNAL_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(unix)]
extern "C" fn interrupted(_: libc::c_int) {
    SIGNAL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(unix)]
struct SignalGuard {
    old_int: libc::sighandler_t,
    old_term: libc::sighandler_t,
}
#[cfg(unix)]
impl SignalGuard {
    fn install(enabled: bool) -> Option<Self> {
        if !enabled {
            return None;
        }
        SIGNAL_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        // SAFETY: handler only increments a lock-free atomic counter.
        unsafe {
            let old_int =
                libc::signal(libc::SIGINT, interrupted as *const () as libc::sighandler_t);
            if old_int == libc::SIG_ERR {
                return None;
            }
            let old_term = libc::signal(
                libc::SIGTERM,
                interrupted as *const () as libc::sighandler_t,
            );
            if old_term == libc::SIG_ERR {
                libc::signal(libc::SIGINT, old_int);
                return None;
            }
            Some(Self { old_int, old_term })
        }
    }
}
#[cfg(unix)]
impl Drop for SignalGuard {
    fn drop(&mut self) {
        // SAFETY: restore process handlers after the scoped watcher has stopped.
        unsafe {
            libc::signal(libc::SIGINT, self.old_int);
            libc::signal(libc::SIGTERM, self.old_term);
        }
    }
}
#[cfg(windows)]
struct SignalGuard;
#[cfg(windows)]
impl SignalGuard {
    fn install(enabled: bool) -> Option<Self> {
        if !enabled {
            return None;
        }
        SIGNAL_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        // SAFETY: the handler only updates a lock-free atomic and is removed on drop.
        if unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_event), 1)
        } != 0
        {
            Some(Self)
        } else {
            None
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn console_event(event: u32) -> i32 {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    if matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT) {
        SIGNAL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        1
    } else {
        0
    }
}

#[cfg(windows)]
impl Drop for SignalGuard {
    fn drop(&mut self) {
        // SAFETY: the scoped watcher has stopped before the handler is removed.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_event), 0)
        };
    }
}
