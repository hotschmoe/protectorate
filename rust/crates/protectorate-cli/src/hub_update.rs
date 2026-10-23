//! Owner hub release update. The stream ends before a cross-version guardian
//! trial completes, so the CLI checks the authenticated listener afterward.
use crate::{json_line, Client, Code, Result, RpcError};
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
                write(output, &json_line(&progress)?)?;
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
                    write(output, &json_line(&status)?)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use protectorate_wire::{decode_json, AuthContext, Authorizer, Catalog, UnaryServer};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use tokio::net::TcpListener;

    #[test]
    fn only_completed_asset_progress_is_reported() {
        assert!(reportable(&json!({"phase":"UPDATE_PHASE_VERIFYING"})));
        assert!(!reportable(&json!({"sentBytes":"1","totalBytes":"2"})));
        assert!(reportable(&json!({"sentBytes":"2","totalBytes":"2"})));
    }

    #[test]
    fn hub_update_parser_accepts_release_overrides_and_rejects_unknown_flags() {
        assert_eq!(
            crate::parse(["hub", "update", "--channel", "dev", "--version", "v2"])
                .unwrap()
                .command,
            crate::Command::HubUpdate {
                channel: "dev".into(),
                version: "v2".into()
            }
        );
        assert!(crate::parse(["hub", "update", "--channel", "nightly"]).is_err());
        assert!(crate::parse(["hub", "update", "--bogus"]).is_err());
        assert!(crate::parse(["hub", "status"]).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_stream_confirms_version_over_real_http() {
        let auth: Authorizer = Arc::new(|_, headers| {
            (headers
                .get("authorization")
                .is_some_and(|v| v == "Bearer owner-secret"))
            .then_some(AuthContext::default())
            .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
        });
        let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
        let request = Arc::new(Mutex::new(None));
        let seen = Arc::clone(&request);
        server.register_streaming(UPDATE_HUB, move |call, sender| {
            let seen = Arc::clone(&seen);
            async move {
                *seen.lock().unwrap() = Some(serde_json::to_value(&call.message).unwrap());
                for progress in [
                    json!({"phase":"UPDATE_PHASE_VERIFYING","version":"0.1.1","previousVersion":"0.1.0","message":"verified"}),
                    json!({"phase":"UPDATE_PHASE_RESTARTING","version":"0.1.1","previousVersion":"0.1.0","message":"restarting"}),
                ] {
                    sender.send(decode_json(call.method.output(), progress.to_string().as_bytes())?).await?;
                }
                Ok(Default::default())
            }
        }).unwrap();
        server
            .register(HUB_STATUS, |call| async move {
                Ok(decode_json(
                    call.method.output(),
                    br#"{"version":"0.1.1","releaseConfirmed":true}"#,
                )?
                .into())
            })
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task =
            tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
        let output = tokio::task::spawn_blocking(move || {
            crate::dispatch_endpoint(
                &endpoint,
                "owner-secret",
                ["hub", "update", "--channel", "stable", "--version", "0.1.1"],
            )
            .unwrap()
        })
        .await
        .unwrap();
        assert!(output.contains("verifying\tverified\n"), "{output}");
        assert!(output.contains("restarting\trestarting\n"), "{output}");
        assert!(output.ends_with("hub reports 0.1.1\n"), "{output}");
        assert_eq!(
            request.lock().unwrap().as_ref().unwrap()["version"],
            "0.1.1"
        );
        task.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn candidate_status_is_not_confirmation_until_guardian_commits() {
        let auth: Authorizer = Arc::new(|_, _| Ok(AuthContext::default()));
        let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        server
            .register(HUB_STATUS, move |call| {
                let seen = Arc::clone(&seen);
                async move {
                    let committed = seen.fetch_add(1, Ordering::SeqCst) >= 2;
                    let body = json!({"version":"0.1.1","releaseConfirmed":committed});
                    Ok(decode_json(call.method.output(), body.to_string().as_bytes())?.into())
                }
            })
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task =
            tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
        let output = tokio::task::spawn_blocking(move || {
            let client = Client::new(&endpoint, "owner-secret", Duration::from_secs(5)).unwrap();
            let mut output = Vec::new();
            confirm(
                &client,
                "0.1.1",
                "0.1.0",
                false,
                &mut output,
                ConfirmTiming {
                    interval: Duration::from_millis(10),
                    rollback_grace: Duration::from_millis(50),
                    timeout: Duration::from_secs(1),
                },
            )
            .unwrap();
            output
        })
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(output, b"hub reports 0.1.1\n");
        task.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stable_previous_version_reports_rollback_without_observed_downtime() {
        let auth: Authorizer = Arc::new(|_, headers| {
            (headers
                .get("authorization")
                .is_some_and(|v| v == "Bearer owner-secret"))
            .then_some(AuthContext::default())
            .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
        });
        let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
        server
            .register(HUB_STATUS, |call| async move {
                Ok(decode_json(call.method.output(), br#"{"version":"0.1.0"}"#)?.into())
            })
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task =
            tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
        let error = tokio::task::spawn_blocking(move || {
            let client = Client::new(&endpoint, "owner-secret", Duration::from_secs(5)).unwrap();
            confirm(
                &client,
                "0.1.1",
                "0.1.0",
                false,
                &mut Vec::new(),
                ConfirmTiming {
                    interval: Duration::from_millis(10),
                    rollback_grace: Duration::from_millis(50),
                    timeout: Duration::from_secs(1),
                },
            )
            .unwrap_err()
        })
        .await
        .unwrap();
        assert_eq!(error.code, Code::Aborted);
        assert!(error.message.contains("rolled back to 0.1.0"));
        task.abort();
    }
}
