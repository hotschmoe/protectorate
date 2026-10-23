//! Tailnet status combines the applied control map with authenticated packet telemetry.
use super::{respond, Hub};
use chrono::{DateTime, Utc};
use protectorate_tailnet_map::Node;
use protectorate_tailnet_packets::PeerTraffic;
use protectorate_wire::{Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::{net::IpAddr, sync::Arc};

fn address(prefix: &str) -> Option<String> {
    let (ip, mask) = prefix.split_once('/')?;
    let ip: IpAddr = ip.parse().ok()?;
    let exact = match ip {
        IpAddr::V4(_) => "32",
        IpAddr::V6(_) => "128",
    };
    (mask == exact).then(|| ip.to_string())
}

fn peer(node: &Node, traffic: Option<&PeerTraffic>) -> Value {
    let ips: Vec<_> = node.addresses.iter().filter_map(|ip| address(ip)).collect();
    let mut result = json!({
        "name": node.name.trim_end_matches('.'),
        "tags": node.tags,
        "ips": ips,
        "online": node.online.unwrap_or(false),
        "rxBytes": traffic.map_or(0, |peer| peer.rx_bytes).to_string(),
        "txBytes": traffic.map_or(0, |peer| peer.tx_bytes).to_string(),
    });
    if node.online != Some(true) {
        if let Some(seen) = node
            .last_seen
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        {
            result["lastSeen"] = json!(seen.with_timezone(&Utc).to_rfc3339());
        }
    }
    if let Some(traffic) = traffic {
        if let Some(handshake) = traffic.last_handshake {
            result["lastHandshake"] = json!(DateTime::<Utc>::from(handshake).to_rfc3339());
        }
        if let Some(addr) = traffic.direct_addr {
            result["directAddr"] = json!(addr.to_string());
        } else if traffic.relay {
            result["relay"] = json!("protectorate");
        }
    }
    result
}

fn status(hub: &Hub) -> Result<Value> {
    let source = hub
        .tailnet_status
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
        .ok_or_else(|| {
            RpcError::new(
                Code::FailedPrecondition,
                "tailnet is not enabled on this hub (run `protectorate hub tailnet init`)",
            )
        })?;
    let Some(snapshot) = source() else {
        return Ok(json!({"backendState":"Disconnected"}));
    };
    let mut peers: Vec<Value> = snapshot
        .map
        .map
        .peers
        .values()
        .map(|node| {
            let traffic = snapshot
                .traffic
                .get(&node.id)
                .filter(|traffic| traffic.key == node.key);
            peer(node, traffic)
        })
        .collect();
    peers.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let mut response = json!({"backendState":"Running","peers":peers});
    if let Some(node) = &snapshot.map.map.self_node {
        response["self"] = peer(node, None);
    }
    Ok(response)
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    server.register(
        "/protectorate.v1.TailnetService/GetStatus",
        move |request| {
            let hub = Arc::clone(&hub);
            async move { respond(&request, status(&hub)?) }
        },
    )?;
    Ok(())
}
