//! Bounded, non-following walks for user-configured directory gauges.
use crate::DirSize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

pub fn expand_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" || text.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(text.trim_start_matches('~').trim_start_matches('/'));
        }
    }
    path.to_path_buf()
}

pub fn measure_dir(path: &Path, budget: Duration) -> DirSize {
    let path = expand_path(path);
    let start = Instant::now();
    let mut result = DirSize {
        path: path.to_string_lossy().into_owned(),
        complete: true,
        ..DirSize::default()
    };
    let mut stack = vec![path];
    while let Some(dir) = stack.pop() {
        if start.elapsed() >= budget {
            result.complete = false;
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                result.complete = false;
                if result.error.is_empty() {
                    result.error = format!("{}: {err}", dir.display());
                }
                continue;
            }
        };
        for entry in entries {
            if start.elapsed() >= budget {
                result.complete = false;
                stack.clear();
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    result.complete = false;
                    if result.error.is_empty() {
                        result.error = err.to_string();
                    }
                    continue;
                }
            };
            let child = entry.path();
            // metadata follows links; symlink_metadata deliberately does not.
            match fs::symlink_metadata(&child) {
                Ok(meta) if meta.is_dir() => stack.push(child),
                Ok(meta) if meta.is_file() => {
                    result.size_bytes = result
                        .size_bytes
                        .saturating_add(meta.len().min(i64::MAX as u64) as i64);
                }
                Ok(_) => {}
                Err(err) => {
                    result.complete = false;
                    if result.error.is_empty() {
                        result.error = format!("{}: {err}", child.display());
                    }
                }
            }
        }
    }
    result.measured_at = Some(SystemTime::now());
    result.took = start.elapsed();
    result
}
