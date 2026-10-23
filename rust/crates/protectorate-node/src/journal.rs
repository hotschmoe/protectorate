//! Durable node job records. Incomplete records become explicit unknown results on restart.
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const MAX_RECORD: u64 = 1 << 20;
pub(crate) const MAX_LOG_FILE: u64 = 64 << 20;

pub(crate) struct Recovered {
    pub id: String,
    pub started: Option<Value>,
    pub finished: Value,
    pub finished_at: SystemTime,
    pub log: Vec<u8>,
    pub dir: PathBuf,
}

pub(crate) fn create(root: &Path, id: &str) -> io::Result<PathBuf> {
    let runs = root.join("runs");
    fs::create_dir_all(&runs)?;
    let dir = runs.join(id);
    fs::create_dir(&dir)?;
    write_record(&dir, None, None, None)?;
    Ok(dir)
}
pub(crate) fn append_log(dir: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("out.log"))?;
    file.write_all(bytes)
}
pub(crate) fn replace_log(dir: &Path, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() as u64 > MAX_LOG_FILE {
        return Err(io::Error::other("job log exceeds 64 MiB"));
    }
    let part = dir.join("out.log.part");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&part)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(part, dir.join("out.log"))
}
pub(crate) fn write_record(
    dir: &Path,
    started: Option<&Value>,
    finished: Option<&Value>,
    finished_at: Option<SystemTime>,
) -> io::Result<()> {
    let time = finished_at.map(|time| DateTime::<Utc>::from(time).to_rfc3339());
    let value = json!({"version":1,"started":started,"finished":finished,"finishedAt":time});
    let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err(io::Error::other("job record exceeds 1 MiB"));
    }
    let part = dir.join("record.json.part");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&part)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(part, dir.join("record.json"))?;
    Ok(())
}

pub(crate) fn recover(root: &Path, retention: Duration) -> io::Result<Vec<Recovered>> {
    let runs = root.join("runs");
    fs::create_dir_all(&runs)?;
    let mut recovered = Vec::new();
    for entry in fs::read_dir(&runs)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if !crate::sources::valid_id(&id) {
            continue;
        }
        let dir = entry.path();
        let record = fs::read(dir.join("record.json"))
            .ok()
            .filter(|bytes| bytes.len() as u64 <= MAX_RECORD)
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let started = record
            .as_ref()
            .and_then(|record| record.get("started"))
            .filter(|v| !v.is_null())
            .cloned();
        let finished = record
            .as_ref()
            .and_then(|record| record.get("finished"))
            .filter(|v| !v.is_null())
            .cloned();
        let at = record
            .as_ref()
            .and_then(|record| record.get("finishedAt"))
            .and_then(Value::as_str)
            .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
            .map(SystemTime::from);
        let finished_at = at.unwrap_or_else(SystemTime::now);
        if finished.is_some()
            && SystemTime::now()
                .duration_since(finished_at)
                .is_ok_and(|age| age >= retention)
        {
            fs::remove_dir_all(&dir)?;
            continue;
        }
        let log_path = dir.join("out.log");
        let log = match fs::metadata(&log_path) {
            Ok(metadata) if metadata.len() <= MAX_LOG_FILE => {
                let mut file = OpenOptions::new().read(true).open(log_path)?;
                let mut bytes = Vec::with_capacity(metadata.len() as usize);
                Read::by_ref(&mut file)
                    .take(MAX_LOG_FILE + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() as u64 > MAX_LOG_FILE {
                    return Err(io::Error::other("recovered job log exceeds 64 MiB"));
                }
                bytes
            }
            Ok(_) => return Err(io::Error::other("recovered job log exceeds 64 MiB")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        let finished = finished.unwrap_or_else(|| json!({"end":"JOB_END_ERROR","exitCode":-1,"error":"node restarted before job completion; outcome unknown"}));
        write_record(&dir, started.as_ref(), Some(&finished), Some(finished_at))?;
        recovered.push(Recovered {
            id,
            started,
            finished,
            finished_at,
            log,
            dir,
        });
    }
    Ok(recovered)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unfinished_job_is_reconciled_after_restart() {
        let temp = tempfile::tempdir().unwrap();
        let dir = create(temp.path(), "job-one").unwrap();
        append_log(&dir, b"before").unwrap();
        let jobs = recover(temp.path(), Duration::from_secs(3600)).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].log, b"before");
        assert_eq!(jobs[0].finished["end"], "JOB_END_ERROR");
        assert!(jobs[0].finished["error"]
            .as_str()
            .unwrap()
            .contains("outcome unknown"));
    }
}
