//! Owner hub release update. The stream ends before a cross-version guardian
//! trial completes, so the CLI checks the authenticated listener afterward.
use crate::{format::word, json_line, Client, Code, Result, RpcError};
use protectorate_wire::v1::{self, hub_service, release_service, UpdatePhase};
use std::{
    io::Write,
    thread,
    time::{Duration, Instant},
};

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
    let request = v1::UpdateHubRequest {
        channel: channel.to_owned(),
        version: version.to_owned(),
    };
    let result = client.server_stream_with_timeout(
        release_service::UPDATE_HUB,
        &request,
        transfer_timeout,
        |progress| {
            let phase = progress.phase();
            if !progress.version.is_empty() {
                target.clone_from(&progress.version);
            }
            if !progress.previous_version.is_empty() {
                previous.clone_from(&progress.previous_version);
            }
            restarting |= phase == UpdatePhase::Restarting;
            skipped |= phase == UpdatePhase::Skipped;
            // A sending phase is reported once, when the whole bundle is out.
            if as_json {
                write(output, &json_line(&progress)?)?;
            } else if progress.total_bytes == 0 || progress.sent_bytes == progress.total_bytes {
                write(
                    output,
                    &format!(
                        "{}\t{}\n",
                        word(phase.as_str_name(), "UPDATE_PHASE_"),
                        progress.message
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
        match client.unary_with_timeout(
            hub_service::GET_STATUS,
            &v1::GetStatusRequest {},
            Duration::from_secs(2),
        ) {
            Ok(status) if status.version == target && status.release_confirmed => {
                if as_json {
                    write(output, &json_line(&status)?)?;
                } else {
                    write(output, &format!("hub reports {target}\n"))?;
                }
                return Ok(());
            }
            Ok(status) if !previous.is_empty() && status.version == previous => {
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

fn write(output: &mut dyn Write, text: &str) -> Result<()> {
    output
        .write_all(text.as_bytes())
        .and_then(|_| output.flush())
        .map_err(|error| RpcError::new(Code::Internal, format!("write hub update: {error}")))
}
