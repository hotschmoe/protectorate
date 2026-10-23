//! Full control-map + private DERP/direct UDP + WireGuard + userspace TCP HTTP proof.
//! Run explicitly with PROT_HEADSCALE_BINARY pointing at Headscale 0.29.5.

use protectorate_derp::{Config as DerpConfig, Service};
use protectorate_tailnet::{Config, HUB_API_PORT, NODE_PORT, TAG_HUB, TAG_NODE};
use protectorate_tailnet_admin::Admin;
use protectorate_tailnet_discovery::DiscoIdentity;
use protectorate_tailnet_member::MemberIdentity;
use protectorate_tailnet_runtime::{Runtime, State};
use std::{
    fs,
    net::{SocketAddr, TcpListener},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn free_addr() -> SocketAddr {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}
fn join(root: &Path, name: &str, control: &str, admin: &Admin, tag: &str) {
    let dir = root.join(name);
    let member = MemberIdentity::open(&dir).unwrap();
    let disco = DiscoIdentity::open(&dir).unwrap();
    let key = admin
        .create_key(&[tag.to_owned()], Duration::from_secs(3600))
        .unwrap();
    member
        .register_http(control, &key.key, name, Duration::from_secs(10))
        .unwrap();
    let state = serde_json::to_vec(&serde_json::json!({
        "controlUrl": control,
        "name": name,
        "nodeKey": member.node_public_key(),
        "discoKey": disco.public_key().to_string(),
    }))
    .unwrap();
    fs::write(dir.join("join.json"), state).unwrap();
    fs::set_permissions(dir.join("join.json"), fs::Permissions::from_mode(0o600)).unwrap();
}
async fn wait_connected(runtime: &Runtime) -> SocketAddr {
    let mut updates = runtime.subscribe_state();
    tokio::time::timeout(Duration::from_secs(30), async move {
        loop {
            if let State::Connected { listener, .. } = *updates.borrow() {
                return listener;
            }
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
async fn two_registered_members_exchange_http_with_published_udp_endpoints() {
    let source = std::env::var("PROT_HEADSCALE_BINARY").expect("set PROT_HEADSCALE_BINARY");
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let control_addr = free_addr();
    let derp_addr = free_addr();
    let stun_addr = free_addr();
    let control = format!("http://localhost:{}", control_addr.port());
    let mut config = Config::for_server(&control);
    config.listen = control_addr.to_string();
    config.grpc_listen = free_addr().to_string();
    config.metrics_listen = free_addr().to_string();
    config.derp_listen = derp_addr.to_string();
    config.stun_listen = stun_addr.to_string();
    config.derp_ipv4 = Some("127.0.0.1".into());
    config.save(home).unwrap();
    let relay = Service::start(DerpConfig {
        listen: derp_addr,
        stun_listen: stun_addr,
        hostname: "localhost".into(),
        verify_url: format!("http://127.0.0.1:{}/verify", control_addr.port()),
        state_dir: home.join("tailnet"),
    })
    .unwrap();
    config
        .write_files(home, Some(&relay.certificate_pin))
        .unwrap();
    let headscale = ChildGuard(
        Command::new(source)
            .args(["serve", "-c"])
            .arg(home.join("tailnet/headscale.yaml"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if Admin::new(&control, "readiness", Duration::from_secs(1))
            .unwrap()
            .health()
            .is_ok()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Headscale did not become healthy"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let output = Command::new(std::env::var("PROT_HEADSCALE_BINARY").unwrap())
        .arg("-c")
        .arg(home.join("tailnet/headscale.yaml"))
        .args(["apikeys", "create", "--expiration", "1d"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let api_key = String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap()
        .to_owned();
    let mut admin = Admin::new(&control, &api_key, Duration::from_secs(10)).unwrap();
    admin.ensure_policy_user().unwrap();
    join(home, "hub", &control, &admin, TAG_HUB);
    join(home, "client", &control, &admin, TAG_NODE);
    let mut hub = Runtime::start(home.join("hub"), HUB_API_PORT).unwrap();
    let mut client = Runtime::start(home.join("client"), NODE_PORT).unwrap();
    let hub_addr = wait_connected(&hub).await;
    let client_addr = wait_connected(&client).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let hub_sees_client = hub.authorization().current_map().is_some_and(|snapshot| {
                snapshot.map.peers.values().any(|peer| {
                    peer.name.starts_with("client")
                        && peer.endpoints.iter().any(|raw| {
                            raw.parse::<SocketAddr>().is_ok_and(|endpoint| {
                                endpoint.ip().is_loopback() && endpoint.port() != 0
                            })
                        })
                })
            });
            let client_sees_hub = client
                .authorization()
                .current_map()
                .is_some_and(|snapshot| {
                    snapshot.map.peers.values().any(|peer| {
                        peer.name.starts_with("hub")
                            && peer.endpoints.iter().any(|raw| {
                                raw.parse::<SocketAddr>().is_ok_and(|endpoint| {
                                    endpoint.ip().is_loopback() && endpoint.port() != 0
                                })
                            })
                    })
                });
            if hub_sees_client && client_sees_hub {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("both authenticated maps received peer UDP endpoints");
    let hub_before = hub.transport_stats();
    let client_before = client.transport_stats();
    let _accepted = hub.take_accepted().unwrap();
    let mut client_accepted = client.take_accepted().unwrap();
    let client_auth = client.authorization();
    const REQUEST: &[u8] = b"GET /health HTTP/1.1\r\nHost: hub\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
    assert_eq!(hub_addr.port(), HUB_API_PORT);
    assert_eq!(hub.peer_addr("client", NODE_PORT).unwrap(), client_addr);

    let node_service = tokio::spawn(async move {
        let accepted = tokio::time::timeout(Duration::from_secs(10), client_accepted.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(client_auth.peer_has_tag(accepted.generation, accepted.peer_id, TAG_HUB));
        let mut stream = accepted.stream;
        let mut request = vec![0; REQUEST.len()];
        stream.read_exact(&mut request).await.unwrap();
        assert_eq!(request, REQUEST);
        stream.write_all(RESPONSE).await.unwrap();
    });
    let mut stream =
        tokio::time::timeout(Duration::from_secs(15), hub.dial_peer("client", NODE_PORT))
            .await
            .unwrap()
            .unwrap();
    stream.write_all(REQUEST).await.unwrap();
    let mut response = vec![0; RESPONSE.len()];
    tokio::time::timeout(Duration::from_secs(15), stream.read_exact(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response, RESPONSE);
    node_service.await.unwrap();
    let hub_after = hub.transport_stats();
    let client_after = client.transport_stats();
    eprintln!("direct path proof: hub={hub_after:?} client={client_after:?}");
    assert!(
        hub_after.direct_ip_sent > hub_before.direct_ip_sent,
        "hub did not send HTTP flow over direct UDP: {hub_after:?}"
    );
    assert!(
        client_after.direct_ip_received > client_before.direct_ip_received,
        "client did not receive HTTP flow over direct UDP: {client_after:?}"
    );
    assert!(
        client_after.direct_ip_sent > client_before.direct_ip_sent,
        "client did not send HTTP response over direct UDP: {client_after:?}"
    );
    assert!(
        hub_after.direct_ip_received > hub_before.direct_ip_received,
        "hub did not receive HTTP response over direct UDP: {hub_after:?}"
    );
    let snapshot = hub.status_snapshot().unwrap();
    let node = snapshot
        .map
        .map
        .peers
        .values()
        .find(|peer| peer.name.starts_with("client"))
        .unwrap();
    let traffic = snapshot.traffic.get(&node.id).unwrap();
    assert_eq!(traffic.key, node.key);
    assert!(traffic.last_handshake.is_some());
    assert!(traffic.tx_bytes > 0 && traffic.rx_bytes > 0);
    assert!(traffic.direct_addr.is_some());
    assert!(!traffic.relay);
    client.shutdown().await;
    hub.shutdown().await;
    drop(relay);
    drop(headscale);
}
