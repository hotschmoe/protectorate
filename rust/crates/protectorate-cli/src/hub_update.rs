//! Owner hub release update. The stream ends before a cross-version guardian
//! trial completes, so the CLI checks the authenticated listener afterward.
use crate::{value_line, Client, Code, Result, RpcError};
use serde_json::{json, Value};
use std::{
    io::Write,
    thread,
    time::{Duration, Instant},
};

const UPDATE_HUB: &str = "/protectorate.v1.ReleaseService/UpdateHub";
const HUB_STATUS: &str = "/protectorate.v1.HubService/GetStatus";
const CONFIRM_WAIT: Duration = Duration::from_secs(3 * 60);
const ROLLBACK_GRACE: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
struct ConfirmTiming {
    interval: Duration,
    rollback_grace: Duration,
    timeout: Duration,
}

impl Default for ConfirmTiming {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            rollback_grace: ROLLBACK_GRACE,
            timeout: CONFIRM_WAIT,
        }
    }
}

pub(crate) fn run(
    client: &Client,
    channel: &str,
    version: &str,
    transfer_timeout: Duration,
    as_json: bool,
    output: &mut dyn Write,
) -> Result<()> {
    let mut target = String::new();
    let mut previous = String::new();
    let mut restarting = false;
    let mut skipped = false;
    let result = client.stream_with_timeout(
        UPDATE_HUB,
        &json!({"channel": channel, "version": version}),
        transfer_timeout,
        |progress| {
            let phase = progress["phase"].as_str().unwrap_or_default();
            if let Some(value) = progress["version"].as_str() {
                target = value.to_owned();
            }
            if let Some(value) = progress["previousVersion"].as_str() {
                previous = value.to_owned();
            }
            restarting |= phase == "UPDATE_PHASE_RESTARTING";
            skipped |= phase == "UPDATE_PHASE_SKIPPED";
            if as_json {
                write(output, &value_line(&progress)?)?;
            } else if reportable(&progress) {
                write(
                    output,
                    &format!(
                        "{}\t{}\n",
                        phase
                            .trim_start_matches("UPDATE_PHASE_")
                            .to_ascii_lowercase(),
                        progress["message"].as_str().unwrap_or_default()
                    ),
                )?;
            }
            Ok(())
        },
    );
    // The old listener can close while the guardian starts the candidate.
    // Once RESTARTING was observed, the authenticated status check decides the
    // outcome instead of treating that expected disconnect as a final error.
    if skipped {
        return Ok(());
    }
    if let Err(error) = result {
        if !restarting {
            return Err(error);
        }
    }
    if target.is_empty() {
        return Err(RpcError::new(
            Code::DataLoss,
            "hub update ended without a target version",
        ));
    }

    confirm(
        client,
        &target,
        &previous,
        as_json,
        output,
        ConfirmTiming::default(),
    )
}

fn confirm(
    client: &Client,
    target: &str,
    previous: &str,
    as_json: bool,
    output: &mut dyn Write,
    timing: ConfirmTiming,
) -> Result<()> {
    let deadline = Instant::now() + timing.timeout;
    let mut saw_down = false;
    let mut previous_since = None;
    loop {
        if Instant::now() >= deadline {
            return Err(RpcError::new(
                Code::DeadlineExceeded,
                "hub did not confirm update within three minutes",
            ));
        }
        thread::sleep(timing.interval);
        match client.call_with_timeout(HUB_STATUS, &json!({}), Duration::from_secs(2)) {
            Ok(status) if status["version"] == target && status["releaseConfirmed"] == true => {
                if as_json {
                    write(output, &value_line(&status)?)?;
                } else {
                    write(output, &format!("hub reports {target}\n"))?;
                }
                return Ok(());
            }
            Ok(status) if !previous.is_empty() && status["version"] == previous => {
                let first_seen = previous_since.get_or_insert_with(Instant::now);
                if !saw_down && first_seen.elapsed() < timing.rollback_grace {
                    continue;
                }
                return Err(RpcError::new(
                    Code::Aborted,
                    format!("hub rolled back to {previous}"),
                ));
            }
            Ok(_) => previous_since = None,
            Err(error) if matches!(error.code, Code::Unauthenticated | Code::PermissionDenied) => {
                return Err(error);
            }
            Err(_) => {
                saw_down = true;
                previous_since = None;
            }
        }
    }
}

fn reportable(progress: &Value) -> bool {
    let total = progress["totalBytes"]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let sent = progress["sentBytes"]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    total == 0 || sent == total
}

fn write(output: &mut dyn Write, text: &str) -> Result<()> {
    output
        .write_all(text.as_bytes())
        .and_then(|_| output.flush())
        .map_err(|error| RpcError::new(Code::Internal, format!("write hub update: {error}")))
}
