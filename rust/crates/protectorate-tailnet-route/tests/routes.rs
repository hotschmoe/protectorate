use protectorate_tailnet_map::Netmap;
use protectorate_tailnet_route::{InboundPacket, RouteTable};
use serde_json::{json, Value};
use std::{
    net::IpAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn ip(value: &str) -> IpAddr {
    value.parse().unwrap()
}
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}
macro_rules! inbound {
    ($route:expr, $peer_id:expr, $key:expr, $source:expr, $destination:expr, $protocol:expr, $port:expr, $now:expr) => {
        $route.authorize_inbound(
            InboundPacket {
                peer_id: $peer_id,
                authenticated_node_key: $key,
                source: $source,
                destination: $destination,
                protocol: $protocol,
                destination_port: $port,
            },
            $now,
        )
    };
}
fn node(id: u64, address: &str) -> Value {
    json!({"ID":id,"Key":format!("nodekey:{}", "a".repeat(64)),
        "Addresses":[format!("{address}/32")], "AllowedIPs":[format!("{address}/32")],
        "Online":true,"KeyExpiry":"2030-01-01T00:00:00Z"})
}
fn map(peers: Vec<Value>, rules: Vec<Value>) -> Netmap {
    let mut map = Netmap::default();
    map.apply_json(
        &serde_json::to_vec(&json!({"Node":node(1,"100.64.0.1"),
        "Peers":peers,"PacketFilter":rules}))
        .unwrap(),
    )
    .unwrap();
    map
}
fn allow_rule() -> Value {
    json!({"SrcIPs":["100.64.0.0/10"],"DstPorts":[{"IP":"100.64.0.1","Ports":{"First":80,"Last":80}}],"IPProto":[6]})
}
#[test]
fn exact_routes_and_inbound_acl() {
    let mut peer = node(2, "100.64.0.2");
    peer["AllowedIPs"] = json!(["0.0.0.0/0"]);
    let route = RouteTable::compile(&map(vec![peer], vec![allow_rule()])).unwrap();
    assert_eq!(route.route_outbound(ip("100.64.0.2"), now()).unwrap().id, 2);
    assert!(route.route_outbound(ip("8.8.8.8"), now()).is_none());
    assert!(inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.3"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
    assert!(!inbound!(
        &route,
        3,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.3"),
        6,
        80,
        now()
    ));
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        81,
        now()
    ));
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        17,
        80,
        now()
    ));
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "b".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
}
#[test]
fn offline_is_a_hint_but_expired_duplicate_and_missing_filter_deny() {
    let mut offline = node(2, "100.64.0.2");
    offline["Online"] = json!(false);
    let route = RouteTable::compile(&map(vec![offline], vec![allow_rule()])).unwrap();
    assert!(route.route_outbound(ip("100.64.0.2"), now()).is_some());
    assert!(inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));

    let mut expired = node(2, "100.64.0.2");
    expired["KeyExpiry"] = json!("2020-01-01T00:00:00Z");
    let route = RouteTable::compile(&map(vec![expired], vec![allow_rule()])).unwrap();
    assert!(route.route_outbound(ip("100.64.0.2"), now()).is_none());
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));

    let route = RouteTable::compile(&map(
        vec![node(2, "100.64.0.2"), node(3, "100.64.0.2")],
        vec![allow_rule()],
    ))
    .unwrap();
    assert!(route.route_outbound(ip("100.64.0.2"), now()).is_none());
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));

    let route = RouteTable::compile(&map(vec![node(2, "100.64.0.2")], vec![])).unwrap();
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
}

#[test]
fn revoked_machine_and_unsigned_peer_api_only_cannot_route() {
    for field in ["MachineAuthorized", "UnsignedPeerAPIOnly"] {
        let mut peer = node(2, "100.64.0.2");
        peer[field] = json!(field == "UnsignedPeerAPIOnly");
        let route = RouteTable::compile(&map(vec![peer], vec![allow_rule()])).unwrap();
        assert!(route.route_outbound(ip("100.64.0.2"), now()).is_none());
        assert!(!inbound!(
            &route,
            2,
            &format!("nodekey:{}", "a".repeat(64)),
            ip("100.64.0.2"),
            ip("100.64.0.1"),
            6,
            80,
            now()
        ));
    }
}
#[test]
fn ranges_wildcards_protocol_defaults_and_malformed_filters() {
    let rule = json!({"SrcIPs":["100.64.0.2-100.64.0.3"],
        "DstPorts":[{"IP":"*","Ports":{"First":0,"Last":65535}}]});
    let route = RouteTable::compile(&map(vec![node(2, "100.64.0.2")], vec![rule])).unwrap();
    assert!(inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        58,
        0,
        now()
    ));
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        47,
        0,
        now()
    ));
    let bad = json!({"SrcIPs":["100.64.0.2"],"DstPorts":[{"IP":"*","Bits":0,"Ports":{"First":0,"Last":65535}}]});
    assert!(RouteTable::compile(&map(vec![node(2, "100.64.0.2")], vec![bad])).is_err());
    let cap =
        json!({"SrcIPs":["cap:example"],"DstPorts":[{"IP":"*","Ports":{"First":0,"Last":65535}}]});
    let route = RouteTable::compile(&map(vec![node(2, "100.64.0.2")], vec![cap])).unwrap();
    assert!(!inbound!(
        &route,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
}

#[test]
fn incremental_filter_groups_and_local_address_collision() {
    let mut netmap = map(vec![node(2, "100.64.0.2")], vec![]);
    netmap
        .apply_json(&serde_json::to_vec(&json!({"PacketFilters":{"web":[allow_rule()]}})).unwrap())
        .unwrap();
    let table = RouteTable::compile(&netmap).unwrap();
    assert!(inbound!(
        &table,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));
    netmap
        .apply_json(&serde_json::to_vec(&json!({"PacketFilters":{"web":null}})).unwrap())
        .unwrap();
    let table = RouteTable::compile(&netmap).unwrap();
    assert!(!inbound!(
        &table,
        2,
        &format!("nodekey:{}", "a".repeat(64)),
        ip("100.64.0.2"),
        ip("100.64.0.1"),
        6,
        80,
        now()
    ));

    let table = RouteTable::compile(&map(vec![node(2, "100.64.0.1")], vec![allow_rule()])).unwrap();
    assert!(table.route_outbound(ip("100.64.0.1"), now()).is_none());
}
