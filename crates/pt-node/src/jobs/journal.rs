//! The node job journal: append-only JSON lines, each fsynced before the effect it records
//! (SPEC 2 principle 5, 12b item 9). The first line names the journal epoch. A torn last line
//! (no newline) is dropped at open; any other unreadable line stops the agent, because a journal
//! that cannot be trusted must not drive a recovery. pt-journal replaces this (REVISIT).

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::clock::unix_now;
use crate::id::random_id;
use pt_frame::msg::{Isolation, JobState, SubmitJob};
use pt_json::{ToJson, tagged_enum};

tagged_enum! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Entry {
        Epoch = "epoch" { epoch: String, created_at: u64 },
        Submit = "submit" { spec: SubmitJob, at: u64 },
        /// The launch receipt, written as soon as the spawn returns.
        Start = "start" {
            job_id: String,
            attempt: u64,
            pid: u32,
            start_time: Option<u64>,
            scope: Option<String>,
            isolation: Isolation,
            at: u64,
        },
        Lowered = "lowered" { job_id: String, attempt: u64, at: u64 },
        Cancel = "cancel" { job_id: String, attempt: u64, at: u64 },
        End = "end" {
            job_id: String,
            attempt: u64,
            state: JobState,
            exit_code: Option<i64>,
            reason: Option<String>,
            log_bytes: u64,
            log_truncated: bool,
            at: u64,
        },
        Collected = "collected" { job_id: String, attempt: u64, at: u64 },
    }
}

pub struct Journal {
    path: PathBuf,
    file: File,
}

pub struct Opened {
    pub journal: Journal,
    pub epoch: String,
    /// Every entry after the epoch line, in order.
    pub entries: Vec<Entry>,
    /// Bytes of a torn last line that were dropped.
    pub torn: usize,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Opened, String> {
        let ctx = |what: &str, e: io::Error| format!("journal {}: {what}: {e}", path.display());
        if !path.exists() {
            let epoch = random_id();
            let mut journal = Journal {
                path: path.to_path_buf(),
                file: OpenOptions::new()
                    .create_new(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| ctx("creating", e))?,
            };
            journal
                .append(&Entry::Epoch {
                    epoch: epoch.clone(),
                    created_at: unix_now(),
                })
                .map_err(|e| ctx("writing the epoch", e))?;
            sync_dir(path).map_err(|e| ctx("syncing its directory", e))?;
            return Ok(Opened {
                journal,
                epoch,
                entries: Vec::new(),
                torn: 0,
            });
        }
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|mut f| f.read_to_end(&mut bytes))
            .map_err(|e| ctx("reading", e))?;
        let whole = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let torn = bytes.len() - whole;
        let mut lines = bytes[..whole]
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .enumerate();
        let epoch = match lines.next() {
            Some((_, l)) => match pt_json::from_bytes::<Entry>(l) {
                Ok(Entry::Epoch { epoch, .. }) => epoch,
                Ok(_) => {
                    return Err(format!(
                        "journal {}: line 1 is not the epoch",
                        path.display()
                    ));
                }
                Err(e) => return Err(format!("journal {}: line 1: {e}", path.display())),
            },
            None => {
                return Err(format!(
                    "journal {}: no complete epoch line ({torn} bytes); move it aside to start a new epoch",
                    path.display()
                ));
            }
        };
        let mut entries = Vec::new();
        for (i, l) in lines {
            let e = pt_json::from_bytes::<Entry>(l)
                .map_err(|e| format!("journal {}: line {}: {e}", path.display(), i + 1))?;
            entries.push(e);
        }
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|e| ctx("opening for append", e))?;
        if torn > 0 {
            file.set_len(whole as u64)
                .and_then(|()| file.sync_data())
                .map_err(|e| ctx("dropping the torn last line", e))?;
        }
        Ok(Opened {
            journal: Journal {
                path: path.to_path_buf(),
                file,
            },
            epoch,
            entries,
            torn,
        })
    }

    /// Writes one line and fsyncs it; the entry is durable when this returns `Ok`.
    pub fn append(&mut self, e: &Entry) -> io::Result<()> {
        let mut line = e.to_json().to_string();
        line.push('\n');
        self.file
            .write_all(line.as_bytes())
            .and_then(|()| self.file.sync_data())
            .map_err(|err| {
                io::Error::new(
                    err.kind(),
                    format!("appending to journal {}: {err}", self.path.display()),
                )
            })
    }
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => File::open(dir)?.sync_all(),
        _ => File::open(".")?.sync_all(),
    }
}

/// Windows has no directory handle to fsync through std; NTFS journals the metadata itself.
#[cfg(windows)]
fn sync_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}
