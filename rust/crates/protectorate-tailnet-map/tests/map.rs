use protectorate_tailnet_map::{MapRequest, MapStream, Netmap, MAX_FRAME};
use serde_json::json;
use std::{io::Cursor, net::IpAddr};

fn key(prefix: &str, byte: char) -> String {
    format!("{prefix}{}", byte.to_string().repeat(64))
}
fn node(id: u64, ip: &str) -> serde_json::Value {
    json!({
        "ID": id,
        "StableID": format!("stable-{id}"),
        "Name": format!("node-{id}.example."),
        "User": 3,
        "Key": key("nodekey:", 'a'),
        "Machine": key("mkey:", 'b'),
        "DiscoKey": key("discokey:", 'c'),
        "Addresses": [format!("{ip}/32")],
        "AllowedIPs": [format!("{ip}/32")],
        "Endpoints": ["127.0.0.1:12345"],
        "HomeDERP": 900,
        "Tags": ["tag:node"]
    })
}
fn frame(value: serde_json::Value) -> Vec<u8> {
    let body = serde_json::to_vec(&value).unwrap();
    let mut bytes = (body.len() as u32).to_le_bytes().to_vec();
    bytes.extend(body);
    bytes
}

#[test]
fn request_uses_tailcfg_wire_names_without_compression() {
    let request = MapRequest::new(key("nodekey:", 'a'), key("discokey:", 'b'), 148).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&request.json().unwrap()).unwrap();
    assert_eq!(json["Version"], 148);
    assert_eq!(json["Stream"], true);
    assert_eq!(json["KeepAlive"], true);
    assert_eq!(json["NodeKey"], key("nodekey:", 'a'));
    assert_eq!(json["DiscoKey"], key("discokey:", 'b'));
    assert!(json.get("Compress").is_none());
}

#[test]
fn preferred_derp_update_is_a_one_shot_hostinfo_report() {
    let request = MapRequest::new(key("nodekey:", 'a'), key("discokey:", 'b'), 148)
        .unwrap()
        .preferred_derp_update(999)
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&request.json().unwrap()).unwrap();
    assert_eq!(json["Stream"], false);
    assert_eq!(json["KeepAlive"], false);
    assert_eq!(json["OmitPeers"], true);
    assert_eq!(json["Hostinfo"]["NetInfo"]["PreferredDERP"], 999);
}

#[test]
fn endpoint_update_uses_one_shot_tailcfg_fields_and_rejects_non_listeners() {
    let request = MapRequest::new(key("nodekey:", 'a'), key("discokey:", 'b'), 148)
        .unwrap()
        .endpoint_update(vec!["127.0.0.1:45678".parse().unwrap()])
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&request.json().unwrap()).unwrap();
    assert_eq!(json["Stream"], false);
    assert_eq!(json["KeepAlive"], false);
    assert_eq!(json["OmitPeers"], true);
    assert_eq!(json["Endpoints"], json!(["127.0.0.1:45678"]));
    assert_eq!(json["EndpointTypes"], json!([1]));
    assert!(
        MapRequest::new(key("nodekey:", 'a'), key("discokey:", 'b'), 148)
            .unwrap()
            .endpoint_update(vec!["0.0.0.0:45678".parse().unwrap()])
            .is_err()
    );
}

#[test]
fn map_frames_apply_full_delta_patch_remove_and_keepalive() {
    let first = json!({
        "Node": node(1, "100.64.0.1"),
        "Peers": [node(2, "100.64.0.2")],
        "DERPMap": {"Regions": {"900": {"RegionID": 900, "RegionCode": "pvt", "Nodes": [{"Name":"one", "HostName":"derp.example", "CertName":"sha256-raw:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "DERPPort": 443, "STUNPort":3478}]}}, "omitDefaultRegions": true},
        "UserProfiles": [{"ID":3,"LoginName":"a@example","DisplayName":"Alice"}],
        "PacketFilter": [{"SrcIPs":["100.64.0.0/10"],"DstPorts":[{"IP":"*","Ports":{"First":0,"Last":65535}}]}],
        "MapSessionHandle":"session-a", "Seq":1
    });
    let delta = json!({
        "PeersChanged":[node(3, "100.64.0.3")],
        "PeersChangedPatch":[{"NodeID":2,"DERPRegion":901,"Endpoints":[],"Online":false,"LastSeen":"2026-10-22T03:04:05Z"}],
        "OnlineChange":{"3":true},
        "PacketFilters":{"extra":[]}, "Seq":2
    });
    let remove = json!({"PeersRemoved":[3],"OnlineChange":{"2":true},"PacketFilters":{"extra":null},"Seq":3});
    let mut bytes = frame(json!({"KeepAlive":true}));
    bytes.extend(frame(first));
    bytes.extend(frame(delta));
    bytes.extend(frame(remove));
    let mut stream = MapStream::new(Cursor::new(bytes));
    let mut map = Netmap::default();
    assert_eq!(stream.next_into(&mut map).unwrap(), Some(false));
    assert_eq!(stream.next_into(&mut map).unwrap(), Some(true));
    assert_eq!(map.self_node.as_ref().unwrap().id, 1);
    assert_eq!(map.peers.len(), 1);
    assert_eq!(
        map.peer_for_address("100.64.0.2".parse::<IpAddr>().unwrap())
            .unwrap()
            .id,
        2
    );
    assert_eq!(
        map.derp_map.as_ref().unwrap().regions["900"].nodes[0].derp_port,
        443
    );
    assert!(map.derp_map.as_ref().unwrap().regions["900"].nodes[0]
        .cert_name
        .starts_with("sha256-raw:"));
    assert_eq!(map.user_profiles[&3].display_name, "Alice");
    assert_eq!(stream.next_into(&mut map).unwrap(), Some(true));
    assert_eq!(map.peers.len(), 2);
    assert_eq!(map.peers[&2].home_derp, 901);
    assert_eq!(map.peers[&2].online, Some(false));
    assert_eq!(
        map.peers[&2].last_seen.as_deref(),
        Some("2026-10-22T03:04:05Z")
    );
    assert_eq!(map.peers[&3].online, Some(true));
    assert!(map.peers[&2].endpoints.is_empty());
    assert_eq!(stream.next_into(&mut map).unwrap(), Some(true));
    assert_eq!(map.peers.len(), 1);
    assert!(!map.peers.contains_key(&3));
    assert!(map.peers[&2].last_seen.is_none());
    assert_eq!(map.session_seq, 3);
    assert_eq!(stream.next_into(&mut map).unwrap(), None);
}

#[test]
fn invalid_frames_do_not_mutate_map() {
    let mut map = Netmap::default();
    assert!(map.apply_json(br#"{"Peers":[]}"#).is_err());
    map.apply_json(
        &serde_json::to_vec(&json!({"Node":node(1,"100.64.0.1"),"Peers":[node(2,"100.64.0.2")]}))
            .unwrap(),
    )
    .unwrap();
    assert!(map
        .apply_json(
            &serde_json::to_vec(&json!({"PeersChanged":[{"ID":3,"Key":"invalid"}]})).unwrap()
        )
        .is_err());
    assert_eq!(map.peers.len(), 1);
    let mut oversized = Cursor::new(((MAX_FRAME + 1) as u32).to_le_bytes().to_vec());
    assert!(MapStream::new(&mut oversized).next_frame().is_err());
    let mut truncated = MapStream::new(Cursor::new(vec![5, 0, 0, 0, b'{']));
    assert!(truncated.next_frame().is_err());
}

#[test]
fn map_session_cannot_replay_or_switch_without_a_fresh_poll() {
    let mut map = Netmap::default();
    map.apply_json(
        &serde_json::to_vec(&json!({
            "Node": node(1, "100.64.0.1"), "Peers": [node(2, "100.64.0.2")],
            "MapSessionHandle": "session-a", "Seq": 1,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(map
        .apply_json(br#"{"MapSessionHandle":"session-a","Seq":1,"PeersRemoved":[2]}"#)
        .is_err());
    assert!(map
        .apply_json(br#"{"MapSessionHandle":"session-b","Seq":2,"PeersRemoved":[2]}"#)
        .is_err());
    assert!(map.peers.contains_key(&2));
    assert_eq!(map.session_seq, 1);
}

#[test]
fn duplicate_address_cannot_assert_peer_identity() {
    let mut map = Netmap::default();
    let response = json!({
        "Node": node(1, "100.64.0.1"),
        "Peers": [node(2, "100.64.0.2"), node(3, "100.64.0.2")]
    });
    map.apply_json(&serde_json::to_vec(&response).unwrap())
        .unwrap();
    assert!(map
        .peer_for_address("100.64.0.2".parse().unwrap())
        .is_none());
    assert!(map
        .peer_for_address("100.64.0.3".parse().unwrap())
        .is_none());
}

#[test]
fn reset_poll_discards_stale_peer_and_requires_initial_self() {
    let mut map = Netmap::default();
    map.apply_json(
        &serde_json::to_vec(&json!({"Node":node(1,"100.64.0.1"),"Peers":[node(2,"100.64.0.2")]}))
            .unwrap(),
    )
    .unwrap();
    map.reset_poll();
    assert!(map.peers.is_empty());
    assert!(map.apply_json(br#"{"PeersChanged":[]}"#).is_err());
    map.apply_json(&serde_json::to_vec(&json!({"Node":node(1,"100.64.0.1"),"Peers":[]})).unwrap())
        .unwrap();
    assert!(map.peers.is_empty());
}

/// End-to-end wire check using the local pinned Headscale binary. The test
/// creates only disposable state, registers a Rust member, and asks Headscale
/// for its uncompressed initial map over the authenticated Noise channel.
#[test]
#[ignore = "requires PROT_HEADSCALE_BINARY"]
fn real_headscale_initial_map() {
    use protectorate_tailnet::Config;
    use protectorate_tailnet_member::{Http2Channel, MemberIdentity, CAPABILITY_VERSION};
    use std::{
        net::TcpListener,
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn free_addr() -> String {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string()
    }
    let binary = std::env::var("PROT_HEADSCALE_BINARY").expect("set PROT_HEADSCALE_BINARY");
    let home = tempfile::tempdir().unwrap();
    let listen = free_addr();
    let origin = format!("http://{listen}");
    let mut config = Config::for_server(&origin);
    config.listen = listen;
    config.metrics_listen = free_addr();
    config.grpc_listen = free_addr();
    config.embedded_derp = false;
    config.public_derp = true;
    config.write_files(home.path(), None).unwrap();
    let cfg = home.path().join("tailnet/headscale.yaml");
    let log_path = home.path().join("headscale.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let child = Command::new(&binary)
        .arg("serve")
        .arg("-c")
        .arg(&cfg)
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            panic!(
                "Headscale exited {status}: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
        }
        if std::net::TcpStream::connect(&config.listen).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Headscale timed out: {}",
            std::fs::read_to_string(&log_path).unwrap()
        );
        thread::sleep(Duration::from_millis(50));
    }
    let cli = |args: &[&str]| -> serde_json::Value {
        let output = Command::new(&binary)
            .arg("-c")
            .arg(&cfg)
            .arg("-o")
            .arg("json")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "headscale {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let user = cli(&["users", "create", "protectorate"]);
    let id = user["id"]
        .as_u64()
        .or_else(|| user["id"].as_str().and_then(|s| s.parse().ok()))
        .unwrap();
    let auth_key = cli(&["preauthkeys", "create", "-u", &id.to_string()]);
    let secret = auth_key["key"].as_str().unwrap();
    let identity = MemberIdentity::open(home.path().join("member")).unwrap();
    let registration = identity
        .register_http(&origin, secret, "rust-map", Duration::from_secs(5))
        .unwrap();
    assert!(registration.machine_authorized);
    let request = MapRequest::new(
        identity.node_public_key(),
        key("discokey:", 'd'),
        CAPABILITY_VERSION,
    )
    .unwrap();
    let channel = identity
        .connect_control_http(&origin, Duration::from_secs(5))
        .unwrap();
    let mut http = Http2Channel::new(channel, &origin).unwrap();
    let mut response = Vec::new();
    http.post_stream("/machine/map", &request.json().unwrap(), 16 << 20, |data| {
        response.extend_from_slice(data);
        if response.len() < 4 {
            return Ok(false);
        }
        let length = u32::from_le_bytes(response[..4].try_into().unwrap()) as usize;
        Ok(length <= MAX_FRAME && response.len() >= length + 4)
    })
    .unwrap_or_else(|e| {
        panic!(
            "map request: {e}; Headscale log: {}",
            std::fs::read_to_string(&log_path).unwrap()
        )
    });
    assert!(
        !response.is_empty(),
        "Headscale returned empty map; log: {}",
        std::fs::read_to_string(&log_path).unwrap()
    );
    let mut stream = MapStream::new(Cursor::new(response));
    let mut map = Netmap::default();
    assert_eq!(stream.next_into(&mut map).unwrap(), Some(true));
    assert!(map.self_node.is_some());
    assert_eq!(
        map.self_node.as_ref().unwrap().key,
        identity.node_public_key()
    );
}
