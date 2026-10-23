//! Owner-only, bounded process diagnostics on the hub listener.
use crate::Hub;
use axum::{
    body::Body,
    extract::Path,
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{fs, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;

fn authorized(hub: &Hub, headers: &HeaderMap) -> bool {
    let Some(secret) = super::bearer(headers) else {
        return false;
    };
    !hub.owner_token.is_empty() && bool::from(hub.owner_token.as_bytes().ct_eq(secret.as_bytes()))
}

fn result(
    hub: &Hub,
    headers: &HeaderMap,
    body: impl Into<Body>,
    content_type: &'static str,
) -> Response {
    let mut response = if authorized(hub, headers) {
        Response::new(body.into())
    } else {
        StatusCode::FORBIDDEN.into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if response.status().is_success() {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        response.headers_mut().insert(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
    }
    response
}

fn status_response(code: StatusCode) -> Response {
    let mut response = code.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn process_status() -> String {
    let text = fs::read_to_string("/proc/self/status").unwrap_or_default();
    text.lines()
        .filter(|line| {
            [
                "Name:", "State:", "Threads:", "VmRSS:", "VmSize:", "VmData:", "VmStk:",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn status_value(status: &str, name: &str) -> u64 {
    status
        .lines()
        .find_map(|line| {
            line.strip_prefix(name)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
}

pub(super) async fn metrics(hub: Arc<Hub>, headers: HeaderMap) -> Response {
    if !authorized(&hub, &headers) {
        return result(&hub, &headers, Body::empty(), "application/json");
    }
    let status = process_status();
    let body = json!({
        "/process/uptime:seconds": hub.started.elapsed().as_secs(),
        "/process/threads:count": status_value(&status, "Threads:"),
        "/process/resident:bytes": status_value(&status, "VmRSS:").saturating_mul(1024),
        "/process/virtual:bytes": status_value(&status, "VmSize:").saturating_mul(1024),
        "/process/data:bytes": status_value(&status, "VmData:").saturating_mul(1024),
    });
    result(&hub, &headers, body.to_string(), "application/json")
}

fn cpu_ticks() -> Option<u64> {
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let tail = stat.rsplit_once(") ")?.1;
    let fields = tail.split_whitespace().collect::<Vec<_>>();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

pub(super) async fn profile(hub: Arc<Hub>, headers: HeaderMap, uri: Uri) -> Response {
    if !authorized(&hub, &headers) {
        return result(&hub, &headers, Body::empty(), "text/plain; charset=utf-8");
    }
    let seconds = match uri.query() {
        None => 1,
        Some(query) => match query
            .strip_prefix("seconds=")
            .and_then(|value| value.parse::<u64>().ok())
        {
            Some(seconds @ 1..=5) => seconds,
            _ => return status_response(StatusCode::BAD_REQUEST),
        },
    };
    let start = cpu_ticks();
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    let end = cpu_ticks();
    let body = format!(
        "Rust hub process CPU sample, {seconds}s\nuser and system CPU ticks: {}\n",
        start
            .zip(end)
            .map_or(0, |(before, after)| after.saturating_sub(before))
    );
    result(&hub, &headers, body, "text/plain; charset=utf-8")
}

pub(super) async fn snapshot(
    hub: Arc<Hub>,
    headers: HeaderMap,
    Path(kind): Path<String>,
) -> Response {
    if !authorized(&hub, &headers) {
        return result(&hub, &headers, Body::empty(), "text/plain; charset=utf-8");
    }
    let status = process_status();
    let body = match kind.as_str() {
        "heap" => format!("Rust hub process memory snapshot\n{status}\n"),
        "goroutine" => format!("Rust hub thread snapshot\n{status}\n"),
        _ => return status_response(StatusCode::NOT_FOUND),
    };
    result(&hub, &headers, body, "text/plain; charset=utf-8")
}

pub(super) async fn unknown(hub: Arc<Hub>, headers: HeaderMap) -> Response {
    if !authorized(&hub, &headers) {
        result(&hub, &headers, Body::empty(), "text/plain; charset=utf-8")
    } else {
        status_response(StatusCode::NOT_FOUND)
    }
}

pub(super) async fn index(hub: Arc<Hub>, headers: HeaderMap) -> Response {
    result(
        &hub,
        &headers,
        "Rust hub diagnostics: /debug/metrics, /debug/pprof/heap, /debug/pprof/goroutine, /debug/pprof/profile?seconds=1\n",
        "text/plain; charset=utf-8",
    )
}

pub(super) async fn node(
    hub: Arc<Hub>,
    headers: HeaderMap,
    Path((name, kind)): Path<(String, String)>,
) -> Response {
    if !authorized(&hub, &headers) {
        return result(&hub, &headers, Body::empty(), "application/json");
    }
    if kind != "metrics" {
        return status_response(StatusCode::NOT_IMPLEMENTED);
    }
    let Ok(node) = hub.inventory.get(&name) else {
        return status_response(StatusCode::NOT_FOUND);
    };
    let current = hub.node_monitor.status(&name, &node.address, node.invited);
    result(
        &hub,
        &headers,
        json!({"node": name, "source": "cached NodeAgentService/GetMetrics", "status": current})
            .to_string(),
        "application/json",
    )
}
