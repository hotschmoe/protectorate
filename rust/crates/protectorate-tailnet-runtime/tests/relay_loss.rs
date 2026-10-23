//! Private DERP relay loss and recovery for two joined members.
//! Run explicitly with PROT_HEADSCALE_BINARY pointing at Headscale 0.29.5.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_derp::{Config as DerpConfig, Service};
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_tailnet::{Config, HUB_API_PORT, NODE_PORT, TAG_HUB, TAG_NODE};
use protectorate_tailnet_admin::Admin;
use protectorate_tailnet_discovery::DiscoIdentity;
use protectorate_tailnet_member::MemberIdentity;
use protectorate_tailnet_runtime::{Runtime, State};
use protectorate_xfer::{Chunk, Kind, Sink, Source, Trailer};
use std::{
    fs,
    io::{self, ErrorKind},
    net::{SocketAddr, TcpListener},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener as TokioTcpListener, TcpStream},
};

const REQUEST: &[u8] = b"GET /relay HTTP/1.1\r\nHost: hub\r\n\r\n";
const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
const UPLOAD_SIZE: usize = 64 * 1024;
const UPLOAD_HEADER: &[u8] = b"POST /upload HTTP/1.1\r\nHost: hub\r\nContent-Length: 65536\r\n\r\n";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
const DROP_TIMEOUT: Duration = Duration::from_secs(45);
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(90);

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

fn start_relay(
    derp_addr: SocketAddr,
    stun_addr: SocketAddr,
    control_addr: SocketAddr,
    home: &Path,
) -> Service {
    Service::start(DerpConfig {
        listen: derp_addr,
        stun_listen: stun_addr,
        hostname: "localhost".into(),
        verify_url: format!("http://127.0.0.1:{}/verify", control_addr.port()),
        state_dir: home.join("tailnet"),
    })
    .unwrap()
}

async fn wait_connected(runtime: &Runtime) -> (u64, SocketAddr) {
    let mut updates = runtime.subscribe_state();
    tokio::time::timeout(CONNECT_TIMEOUT, async move {
        loop {
            if let State::Connected {
                generation,
                listener,
            } = *updates.borrow()
            {
                return (generation, listener);
            }
            updates.changed().await.unwrap();
        }
    })
    .await
    .expect("runtime did not connect through the private relay")
}

async fn wait_disconnected(runtime: &Runtime) {
    let mut updates = runtime.subscribe_state();
    tokio::time::timeout(DROP_TIMEOUT, async move {
        loop {
            if *updates.borrow() == State::Disconnected {
                return;
            }
            updates.changed().await.unwrap();
        }
    })
    .await
    .expect("runtime kept its relay after the relay stopped")
}

async fn wait_peers(hub: &Runtime, client: &Runtime) {
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        loop {
            if hub.peer_addr("client", NODE_PORT).is_ok()
                && client.peer_addr("hub", HUB_API_PORT).is_ok()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("both control maps did not receive the other member");
}

async fn wait_reconnected(runtime: &Runtime) -> (u64, SocketAddr) {
    let mut updates = runtime.subscribe_state();
    tokio::time::timeout(RECONNECT_TIMEOUT, async move {
        loop {
            if let State::Connected {
                generation,
                listener,
            } = *updates.borrow()
            {
                return (generation, listener);
            }
            updates.changed().await.unwrap();
        }
    })
    .await
    .expect("runtime did not reconnect after the relay returned")
}

fn peer_id(runtime: &Runtime, name: &str) -> u64 {
    runtime
        .authorization()
        .current_map()
        .expect("connected map")
        .map
        .peers
        .values()
        .find(|peer| peer.name.starts_with(name))
        .expect("peer present in the connected map")
        .id
}

/// One HTTP/1.1 exchange from hub to client, returning the map generation the
/// client authenticated the accepted stream with.
async fn exchange(hub: &Runtime, served: &mut mpsc::Receiver<u64>) -> u64 {
    let mut stream =
        tokio::time::timeout(Duration::from_secs(25), hub.dial_peer("client", NODE_PORT))
            .await
            .expect("hub dial timed out")
            .expect("hub dial rejected");
    stream.write_all(REQUEST).await.unwrap();
    let mut response = vec![0; RESPONSE.len()];
    tokio::time::timeout(Duration::from_secs(25), stream.read_exact(&mut response))
        .await
        .expect("hub read timed out")
        .expect("hub read failed");
    assert_eq!(response, RESPONSE);
    tokio::time::timeout(Duration::from_secs(25), served.recv())
        .await
        .expect("client did not report the accepted stream")
        .expect("client accept loop stopped")
}

async fn upload(
    hub: &Runtime,
    body: &[u8],
    progress: &mut mpsc::Receiver<usize>,
    complete: bool,
) -> Option<tokio::io::DuplexStream> {
    let mut stream =
        tokio::time::timeout(Duration::from_secs(25), hub.dial_peer("client", NODE_PORT))
            .await
            .expect("upload dial timed out")
            .expect("upload dial rejected");
    stream.write_all(UPLOAD_HEADER).await.unwrap();
    stream.write_all(body).await.unwrap();
    let received = tokio::time::timeout(Duration::from_secs(25), progress.recv())
        .await
        .expect("receiver did not read upload body")
        .expect("upload accept loop stopped");
    assert!(received > 0 && received <= body.len());
    if complete {
        let mut response = vec![0; RESPONSE.len()];
        tokio::time::timeout(Duration::from_secs(25), stream.read_exact(&mut response))
            .await
            .expect("upload response timed out")
            .expect("upload response read failed");
        assert_eq!(response, RESPONSE);
        None
    } else {
        Some(stream)
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
async fn both_members_recover_from_a_private_derp_outage() {
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
    let relay = start_relay(derp_addr, stun_addr, control_addr, home);
    let pin = relay.certificate_pin.clone();
    config.write_files(home, Some(&pin)).unwrap();
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
    let hub = Runtime::start(home.join("hub"), HUB_API_PORT).unwrap();
    let mut client = Runtime::start(home.join("client"), NODE_PORT).unwrap();
    let (hub_generation, hub_addr) = wait_connected(&hub).await;
    let (client_generation, client_addr) = wait_connected(&client).await;
    assert_eq!(hub_addr.port(), HUB_API_PORT);
    assert_eq!(client_addr.port(), NODE_PORT);
    wait_peers(&hub, &client).await;
    let hub_peer = peer_id(&hub, "client");
    let client_peer = peer_id(&client, "hub");

    let upload_body: Vec<u8> = (0..UPLOAD_SIZE).map(|index| (index % 251) as u8).collect();
    let source_path = home.join("upload-source");
    let destination = home.join("client-upload");
    fs::write(&source_path, &upload_body).unwrap();
    let mut source = Source::open(&source_path, false).unwrap();
    while source.next_chunk().unwrap().is_some() {}
    let trailer: Trailer = source.trailer().unwrap();

    let (reported, mut served) = mpsc::channel::<u64>(8);
    let (upload_progress, mut progress) = mpsc::channel::<usize>(8);
    let (upload_result, mut results) = mpsc::channel::<bool>(8);
    let client_auth = client.authorization();
    let mut accepted = client.take_accepted().unwrap();
    let upload_destination = destination.clone();
    let upload_trailer = trailer.clone();
    let accept_loop = tokio::spawn(async move {
        while let Some(accepted) = accepted.recv().await {
            if !client_auth.peer_has_tag(accepted.generation, accepted.peer_id, TAG_HUB) {
                return Err("client accepted a stream its map does not authorize".to_owned());
            }
            let mut connection = accepted.stream;
            let mut prefix = [0; 4];
            connection
                .read_exact(&mut prefix)
                .await
                .map_err(|error| format!("client request read: {error}"))?;
            if &prefix == b"POST" {
                let mut rest = vec![0; UPLOAD_HEADER.len() - prefix.len()];
                connection
                    .read_exact(&mut rest)
                    .await
                    .map_err(|error| error.to_string())?;
                if [&prefix[..], &rest].concat() != UPLOAD_HEADER {
                    return Err("client received an unexpected upload header".to_owned());
                }
                let mut sink =
                    Sink::new_unknown_size(&upload_destination, Kind::File, 0o600, false)
                        .map_err(|error| error.to_string())?;
                let mut remaining = UPLOAD_SIZE;
                let mut first = true;
                while remaining > 0 {
                    let mut buffer = vec![0; remaining.min(4096)];
                    let read = match tokio::time::timeout(
                        Duration::from_secs(30),
                        connection.read(&mut buffer),
                    )
                    .await
                    {
                        Ok(Ok(read)) if read > 0 => read,
                        _ => break,
                    };
                    buffer.truncate(read);
                    sink.write(Chunk {
                        data: buffer,
                        compressed: false,
                        raw_size: read as u32,
                    })
                    .map_err(|error| error.to_string())?;
                    remaining -= read;
                    if first && sink.received() >= (UPLOAD_SIZE / 2) as u64 {
                        upload_progress
                            .send(sink.received() as usize)
                            .await
                            .map_err(|error| error.to_string())?;
                        first = false;
                    }
                }
                if remaining == 0 {
                    sink.commit(upload_trailer.clone())
                        .map_err(|error| error.to_string())?;
                    connection
                        .write_all(RESPONSE)
                        .await
                        .map_err(|error| error.to_string())?;
                    upload_result
                        .send(true)
                        .await
                        .map_err(|error| error.to_string())?;
                } else {
                    drop(sink);
                    upload_result
                        .send(false)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                continue;
            }
            let mut request = vec![0; REQUEST.len() - prefix.len()];
            connection
                .read_exact(&mut request)
                .await
                .map_err(|error| error.to_string())?;
            if [&prefix[..], &request].concat() != REQUEST {
                return Err("client received an unexpected request".to_owned());
            }
            connection
                .write_all(RESPONSE)
                .await
                .map_err(|error| format!("client response write: {error}"))?;
            reported
                .send(accepted.generation)
                .await
                .map_err(|_| "client accept report closed".to_owned())?;
        }
        Ok(())
    });

    let served_generation = exchange(&hub, &mut served).await;
    eprintln!("relay up: hub generation {hub_generation} client generation {served_generation}");

    let in_flight = upload(&hub, &upload_body[..UPLOAD_SIZE / 2], &mut progress, false)
        .await
        .expect("partial upload stream");
    assert!(!home.join("client-upload").exists());
    eprintln!("upload in flight: receiver staged 32768/65536 bytes, destination absent");

    drop(relay);
    wait_disconnected(&hub).await;
    wait_disconnected(&client).await;
    let hub_auth = hub.authorization();
    let client_auth = client.authorization();
    assert!(hub_auth.current_map().is_none());
    assert!(client_auth.current_map().is_none());
    assert!(!hub_auth.peer_has_tag(hub_generation, hub_peer, TAG_NODE));
    assert!(!client_auth.peer_has_tag(client_generation, client_peer, TAG_HUB));
    assert_eq!(
        hub.peer_addr("client", NODE_PORT).unwrap_err().kind(),
        ErrorKind::NotConnected
    );
    assert_eq!(
        hub.dial_peer("client", NODE_PORT).await.unwrap_err().kind(),
        ErrorKind::NotConnected
    );
    assert_eq!(
        hub.dial(client_addr).await.unwrap_err().kind(),
        ErrorKind::NotConnected
    );
    assert_eq!(
        client.dial(hub_addr).await.unwrap_err().kind(),
        ErrorKind::NotConnected
    );
    eprintln!("relay down: hub and client dropped map and dial state");
    drop(in_flight);
    assert!(
        !tokio::time::timeout(Duration::from_secs(35), results.recv())
            .await
            .expect("partial upload did not stop")
            .expect("upload accept loop stopped"),
    );
    assert!(!home.join("client-upload").exists());

    let relay = start_relay(derp_addr, stun_addr, control_addr, home);
    assert_eq!(relay.derp_addr, derp_addr);
    assert_eq!(relay.stun_addr, stun_addr);
    assert_eq!(relay.certificate_pin, pin);
    let (hub_next, hub_addr) = wait_reconnected(&hub).await;
    let (client_next, client_addr) = wait_reconnected(&client).await;
    assert_eq!(hub_addr.port(), HUB_API_PORT);
    assert_eq!(client_addr.port(), NODE_PORT);
    eprintln!("relay restored: hub generation {hub_next} client generation {client_next}");
    assert_eq!(hub.peer_addr("client", NODE_PORT).unwrap(), client_addr);
    let hub_before = hub.transport_stats();
    let client_before = client.transport_stats();
    let served_generation = exchange(&hub, &mut served).await;
    eprintln!("relay restored HTTP on client generation {served_generation}");
    upload(&hub, &upload_body, &mut progress, true).await;
    assert_eq!(results.recv().await, Some(true));
    assert_eq!(fs::read(&destination).unwrap(), upload_body);
    let mut published = Source::open(&destination, false).unwrap();
    while published.next_chunk().unwrap().is_some() {}
    assert_eq!(published.trailer().unwrap().sha256, trailer.sha256);
    eprintln!("relay restored: exact upload retry published once with verified SHA-256");
    let hub_after = hub.transport_stats();
    let client_after = client.transport_stats();
    assert!(
        hub_after.direct_ip_sent + hub_after.derp_ip_sent
            > hub_before.direct_ip_sent + hub_before.derp_ip_sent,
        "hub sent nothing after the relay came back: {hub_after:?}"
    );
    assert!(
        client_after.direct_ip_received + client_after.derp_ip_received
            > client_before.direct_ip_received + client_before.derp_ip_received,
        "client received nothing after the relay came back: {client_after:?}"
    );

    // The accepted-stream sender lives with Runtime even after its supervisor
    // stops, so end the test server explicitly before waiting for shutdown.
    accept_loop.abort();
    let _ = accept_loop.await;
    // Headscale's streaming map poll has a 60-second read timeout before its
    // blocking task observes the stop flag.
    tokio::time::timeout(Duration::from_secs(75), async {
        tokio::join!(client.shutdown(), hub.shutdown());
    })
    .await
    .expect("tailnet runtimes did not stop");
    drop(relay);
    drop(headscale);
}

struct EmptyMetrics;
impl MetricsCollector for EmptyMetrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

fn put_request(body: &serde_json::Value) -> Vec<u8> {
    let body = body.to_string();
    format!(
        "POST /protectorate.v1.NodeAgentService/PutFile HTTP/1.1\r\nHost: node\r\nContent-Type: application/json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn put_over_tailnet(hub: &Runtime, body: &serde_json::Value) -> serde_json::Value {
    let mut stream =
        tokio::time::timeout(Duration::from_secs(30), hub.dial_peer("client", NODE_PORT))
            .await
            .expect("PutFile dial timed out")
            .expect("PutFile dial failed");
    stream.write_all(&put_request(body)).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut response))
        .await
        .expect("PutFile response timed out")
        .expect("PutFile response read failed");
    let (head, body) = response.split_at(
        response
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .expect("HTTP response headers")
            + 4,
    );
    assert!(
        head.starts_with(b"HTTP/1.1 200 "),
        "{}",
        String::from_utf8_lossy(head)
    );
    serde_json::from_slice(body).expect("JSON PutFile response")
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
async fn node_agent_put_file_in_body_cut_retries_after_private_relay_recovery() {
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
    let relay = start_relay(derp_addr, stun_addr, control_addr, home);
    let pin = relay.certificate_pin.clone();
    config.write_files(home, Some(&pin)).unwrap();
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
    let hub = Runtime::start(home.join("hub"), HUB_API_PORT).unwrap();
    let mut client = Runtime::start(home.join("client"), NODE_PORT).unwrap();
    wait_connected(&hub).await;
    wait_connected(&client).await;
    wait_peers(&hub, &client).await;

    let files = home.join("node-files");
    fs::create_dir(&files).unwrap();
    let listener = TokioTcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(EmptyMetrics), "node-secret")
        .with_files(&files, false)
        .unwrap()
        .router()
        .unwrap();
    let http_server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let bytes: Vec<u8> = (0..64 * 1024).map(|index| (index % 251) as u8).collect();
    let source_path = home.join("put-source");
    fs::write(&source_path, &bytes).unwrap();
    let mut source = Source::open(&source_path, false).unwrap();
    let chunk = source.next_chunk().unwrap().unwrap();
    assert!(source.next_chunk().unwrap().is_none());
    let trailer = source.trailer().unwrap();
    let first = serde_json::json!({
        "transferId": "live-cut-transfer", "path": "published", "kind": "FILE_KIND_FILE",
        "mode": 384, "offset": "0",
        "chunk": {"data": STANDARD.encode(&chunk.data), "rawSize": chunk.raw_size}
    });
    let request = put_request(&first);
    let header_end = request
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap()
        + 4;
    let prefix_len = header_end + (request.len() - header_end) / 2;
    let (forwarded, mut progress) = mpsc::channel::<()>(1);
    let mut accepted = client.take_accepted().unwrap();
    let auth = client.authorization();
    let bridge = tokio::spawn(async move {
        let mut first_stream = true;
        while let Some(accepted) = accepted.recv().await {
            assert!(auth.peer_has_tag(accepted.generation, accepted.peer_id, TAG_HUB));
            let mut remote = accepted.stream;
            let mut local = TcpStream::connect(local_addr).await.unwrap();
            let signal = first_stream;
            first_stream = false;
            let forwarded = forwarded.clone();
            tokio::spawn(async move {
                if signal {
                    let mut prefix = vec![0; prefix_len];
                    remote.read_exact(&mut prefix).await.unwrap();
                    local.write_all(&prefix).await.unwrap();
                    forwarded.send(()).await.unwrap();
                }
                let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
            });
        }
    });

    let mut interrupted = hub.dial_peer("client", NODE_PORT).await.unwrap();
    interrupted.write_all(&request[..prefix_len]).await.unwrap();
    tokio::time::timeout(Duration::from_secs(25), progress.recv())
        .await
        .expect("NodeAgent bridge did not forward incomplete request")
        .expect("NodeAgent bridge stopped");
    assert!(!files.join("published").exists());
    drop(relay);
    wait_disconnected(&hub).await;
    wait_disconnected(&client).await;
    drop(interrupted);
    assert!(!files.join("published").exists());

    let restored = start_relay(derp_addr, stun_addr, control_addr, home);
    assert_eq!(restored.certificate_pin, pin);
    wait_reconnected(&hub).await;
    wait_reconnected(&client).await;
    let hub_before = hub.transport_stats();
    let node_before = client.transport_stats();
    let chunk_reply = put_over_tailnet(&hub, &first).await;
    assert_eq!(chunk_reply["received"], trailer.size.to_string());
    assert!(!files.join("published").exists());
    let final_request = serde_json::json!({
        "transferId": "live-cut-transfer", "offset": trailer.size.to_string(), "last": true,
        "trailer": {"size": trailer.size.to_string(), "sha256": trailer.sha256}
    });
    let committed = put_over_tailnet(&hub, &final_request).await;
    assert_eq!(committed["done"], true);
    assert_eq!(fs::read(files.join("published")).unwrap(), bytes);
    let mut published = Source::open(files.join("published"), false).unwrap();
    while published.next_chunk().unwrap().is_some() {}
    assert_eq!(published.trailer().unwrap().sha256, trailer.sha256);
    let hub_after = hub.transport_stats();
    let node_after = client.transport_stats();
    assert!(
        hub_after.direct_ip_sent + hub_after.derp_ip_sent
            > hub_before.direct_ip_sent + hub_before.derp_ip_sent
    );
    assert!(
        node_after.direct_ip_received + node_after.derp_ip_received
            > node_before.direct_ip_received + node_before.derp_ip_received
    );
    eprintln!("PutFile retry packet path: hub direct sent {} DERP sent {}; node direct received {} DERP received {}",
        hub_after.direct_ip_sent - hub_before.direct_ip_sent,
        hub_after.derp_ip_sent - hub_before.derp_ip_sent,
        node_after.direct_ip_received - node_before.direct_ip_received,
        node_after.derp_ip_received - node_before.derp_ip_received);

    bridge.abort();
    let _ = bridge.await;
    http_server.abort();
    let _ = http_server.await;
    tokio::time::timeout(Duration::from_secs(75), async {
        tokio::join!(client.shutdown(), hub.shutdown());
    })
    .await
    .expect("tailnet runtimes did not stop");
    drop(restored);
    drop(headscale);
}
