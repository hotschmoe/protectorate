use protectorate_derp::{Certificate, Config as ServerConfig, Service};
use protectorate_derp_client::{Client as DerpClient, Config as DerpConfig};
use protectorate_tailnet_map::Netmap;
use protectorate_tailnet_packets::{PacketEngine, ReceiveStatus, SendStatus};
use protectorate_tailnet_wg::{tai64n_now, Identity, Initiator};
use serde_json::json;
use std::{
    io::{self, BufRead, Read, Write},
    net::{TcpListener, UdpSocket},
    thread,
    time::{Duration, Instant},
};

fn direct_event(engine: &mut PacketEngine) -> ReceiveStatus {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = engine.receive_direct().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "direct UDP packet timed out");
        thread::sleep(Duration::from_millis(2));
    }
}

fn verifier(connections: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/verify", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = io::BufReader::new(&mut stream);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            assert!(first.starts_with("POST /verify "));
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            assert!(String::from_utf8_lossy(&body).contains("nodekey:"));
            drop(reader);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\n{\"Allow\":true}")
                .unwrap();
        }
    });
    (url, worker)
}

fn key(private: [u8; 32]) -> String {
    let public = Identity::from_private(private).unwrap().public_key();
    format!(
        "nodekey:{}",
        public
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn map(local: (u64, &str, String), peer: (u64, &str, String), port: u16, pin: &str) -> Netmap {
    let node = |(id, ip, key): (u64, &str, String)| {
        json!({
            "ID":id, "Key":key, "Addresses":[format!("{ip}/32")],
            "AllowedIPs":[format!("{ip}/32")], "HomeDERP":900,
            "MachineAuthorized":true
        })
    };
    let mut map = Netmap::default();
    map.apply_json(&serde_json::to_vec(&json!({
        "Node":node(local), "Peers":[node(peer)],
        "DERPMap":{"Regions":{"900":{"RegionID":900,"RegionCode":"pvt","Nodes":[{
            "Name":"one","HostName":"localhost","IPv4":"127.0.0.1","DERPPort":port,
            "CertName":pin
        }]}},"omitDefaultRegions":true},
        "PacketFilter":[{"SrcIPs":["100.64.0.0/10"],"DstPorts":[{"IP":"100.64.0.0/10","Ports":{"First":0,"Last":65535}}]}],
        "MapSessionHandle":"session-a","Seq":1
    })).unwrap()).unwrap();
    map
}

fn udp(source: [u8; 4], destination: [u8; 4], port: u16) -> Vec<u8> {
    let mut packet = vec![0u8; 28];
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&28u16.to_be_bytes());
    packet[8] = 64;
    packet[9] = 17;
    packet[12..16].copy_from_slice(&source);
    packet[16..20].copy_from_slice(&destination);
    let sum = packet[..20].chunks_exact(2).fold(0u32, |sum, pair| {
        sum + u16::from_be_bytes([pair[0], pair[1]]) as u32
    });
    let folded = (sum & 0xffff) + (sum >> 16);
    packet[10..12].copy_from_slice(&(!(folded as u16)).to_be_bytes());
    packet[20..22].copy_from_slice(&1234u16.to_be_bytes());
    packet[22..24].copy_from_slice(&port.to_be_bytes());
    packet[24..26].copy_from_slice(&8u16.to_be_bytes());
    packet
}

fn tcp(source: [u8; 4], destination: [u8; 4], source_port: u16, dest_port: u16) -> Vec<u8> {
    let mut packet = vec![0u8; 40];
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&40u16.to_be_bytes());
    packet[8] = 64;
    packet[9] = 6;
    packet[12..16].copy_from_slice(&source);
    packet[16..20].copy_from_slice(&destination);
    let sum = packet[..20].chunks_exact(2).fold(0u32, |sum, pair| {
        sum + u16::from_be_bytes([pair[0], pair[1]]) as u32
    });
    let folded = (sum & 0xffff) + (sum >> 16);
    packet[10..12].copy_from_slice(&(!(folded as u16)).to_be_bytes());
    packet[20..22].copy_from_slice(&source_port.to_be_bytes());
    packet[22..24].copy_from_slice(&dest_port.to_be_bytes());
    packet[32] = 0x50;
    packet[33] = 0x12; // SYN-ACK, with no local SYN or return-flow grant.
    packet
}

#[test]
fn unsolicited_ephemeral_tcp_reply_is_dropped_without_relay_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [1; 32];
    let bob_key = [2; 32];
    let mut alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    alice_map.packet_filters.clear();
    let bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let reply = tcp([100, 64, 0, 2], [100, 64, 0, 1], 7470, 49152);
    assert_eq!(bob.send_ip(&reply).unwrap(), SendStatus::HandshakeStarted);
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    let nonce = *b"live0001";
    alice.ping_relay(nonce).unwrap();
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Pong(nonce));
    worker.join().unwrap();
}

#[test]
fn duplicate_bad_and_unmapped_derp_frames_leave_authenticated_relay_live() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(3);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [1; 32];
    let bob_key = [2; 32];
    let bob_identity = Identity::from_private(bob_key).unwrap();
    let bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der.clone()))
            .unwrap();
    let mut config = DerpConfig::new(service.derp_addr, "localhost");
    config.ca_der = Some(cert.der);
    config.certificate_pin = Some(service.certificate_pin.clone());
    let mut alice = DerpClient::connect(&config, alice_key).unwrap();
    let mut eve = DerpClient::connect(&config, [3; 32]).unwrap();
    let alice_identity = Identity::from_private(alice_key).unwrap();
    let (_, initiation) = Initiator::new(
        &alice_identity,
        bob_identity.public_key(),
        [0; 32],
        tai64n_now().unwrap(),
    )
    .unwrap();
    alice.send(bob_identity.public_key(), &initiation).unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    alice.send(bob_identity.public_key(), &initiation).unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    alice
        .send(bob_identity.public_key(), &[4, 0, 0, 0])
        .unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    eve.send(bob_identity.public_key(), &[4, 0, 0, 0]).unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    let nonce = *b"live0003";
    bob.ping_relay(nonce).unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Pong(nonce));
    assert_eq!(
        bob.send_ip(&udp([100, 64, 0, 2], [100, 64, 0, 1], 4099))
            .unwrap(),
        SendStatus::Sent
    );
    assert!(!bob.peer_traffic().contains_key(&3));
    worker.join().unwrap();
}

#[test]
fn two_members_exchange_authenticated_filtered_ip_over_private_derp() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [1; 32];
    let bob_key = [2; 32];
    let alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let nonce = *b"probe001";
    alice.ping_relay(nonce).unwrap();
    assert_eq!(
        alice.receive_timeout(Duration::from_secs(2)).unwrap(),
        Some(ReceiveStatus::Pong(nonce))
    );
    assert_eq!(
        alice.receive_timeout(Duration::from_millis(20)).unwrap(),
        None
    );
    let first = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    assert_eq!(alice.send_ip(&first).unwrap(), SendStatus::HandshakeStarted);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(
        bob.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: first
        }
    );
    let reply = udp([100, 64, 0, 2], [100, 64, 0, 1], 4097);
    assert_eq!(bob.send_ip(&reply).unwrap(), SendStatus::Sent);
    assert_eq!(
        alice.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 2,
            bytes: reply
        }
    );
    // A relay reconnect can deliver old ciphertext before the new WireGuard
    // handshake. It must not tear down the live relay or restore old keys.
    bob.clear_map();
    bob.replace_map(&bob_map).unwrap();
    let stale = udp([100, 64, 0, 1], [100, 64, 0, 2], 4098);
    assert_eq!(alice.send_ip(&stale).unwrap(), SendStatus::Sent);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(bob.transport_stats().derp_handshakes_sent, 1);
    assert_eq!(alice.send_ip(&stale).unwrap(), SendStatus::Sent);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(bob.transport_stats().derp_handshakes_sent, 1);
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(alice.send_ip(&stale).unwrap(), SendStatus::Sent);
    assert_eq!(
        bob.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: stale,
        }
    );
    let fresh = udp([100, 64, 0, 2], [100, 64, 0, 1], 4099);
    assert_eq!(bob.send_ip(&fresh).unwrap(), SendStatus::Sent);
    assert_eq!(
        alice.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 2,
            bytes: fresh
        }
    );
    let spoofed = udp([100, 64, 0, 99], [100, 64, 0, 2], 4096);
    assert_eq!(
        alice.send_ip(&spoofed).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    // A routine peer-status map delta keeps the established packet session.
    let mut benign_map = bob_map.clone();
    benign_map.peers.get_mut(&1).unwrap().online = Some(false);
    benign_map.peers.get_mut(&1).unwrap().name = "renamed-peer".into();
    benign_map.session_seq += 1;
    bob.replace_map(&benign_map).unwrap();
    let after_delta = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    assert_eq!(alice.send_ip(&after_delta).unwrap(), SendStatus::Sent);
    assert_eq!(
        bob.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: after_delta
        }
    );

    // Revocation drops that session. Reauthorizing does not resurrect it.
    let mut revoked_map = benign_map.clone();
    revoked_map.peers.get_mut(&1).unwrap().machine_authorized = Some(false);
    revoked_map.session_seq += 1;
    bob.replace_map(&revoked_map).unwrap();
    assert_eq!(
        bob.send_ip(&udp([100, 64, 0, 2], [100, 64, 0, 1], 4097))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    benign_map.session_seq = revoked_map.session_seq + 1;
    bob.replace_map(&benign_map).unwrap();
    assert_eq!(
        alice
            .send_ip(&udp([100, 64, 0, 1], [100, 64, 0, 2], 4096))
            .unwrap(),
        SendStatus::Sent
    );
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    bob.clear_map();
    assert_eq!(
        bob.send_ip(&udp([100, 64, 0, 2], [100, 64, 0, 1], 4097))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    worker.join().unwrap();
}

#[test]
fn simultaneous_initiations_send_both_queued_datagrams() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [1; 32];
    let bob_key = [2; 32];
    let alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let from_alice = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    let from_bob = udp([100, 64, 0, 2], [100, 64, 0, 1], 4097);
    assert_eq!(
        alice.send_ip(&from_alice).unwrap(),
        SendStatus::HandshakeStarted
    );
    assert_eq!(
        bob.send_ip(&from_bob).unwrap(),
        SendStatus::HandshakeStarted
    );
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    let mut received_by_alice = None;
    let mut received_by_bob = None;
    for _ in 0..3 {
        if received_by_alice.is_none() {
            if let Some(ReceiveStatus::Packet { peer_id, bytes }) =
                alice.receive_timeout(Duration::from_secs(2)).unwrap()
            {
                received_by_alice = Some((peer_id, bytes));
            }
        }
        if received_by_bob.is_none() {
            if let Some(ReceiveStatus::Packet { peer_id, bytes }) =
                bob.receive_timeout(Duration::from_secs(2)).unwrap()
            {
                received_by_bob = Some((peer_id, bytes));
            }
        }
    }
    assert_eq!(received_by_alice, Some((2, from_bob)));
    assert_eq!(received_by_bob, Some((1, from_alice)));
    let retry_from_alice = udp([100, 64, 0, 1], [100, 64, 0, 2], 4098);
    let retry_from_bob = udp([100, 64, 0, 2], [100, 64, 0, 1], 4099);
    assert_eq!(alice.send_ip(&retry_from_alice).unwrap(), SendStatus::Sent);
    assert_eq!(bob.send_ip(&retry_from_bob).unwrap(), SendStatus::Sent);
    assert_eq!(
        bob.receive_timeout(Duration::from_secs(2)).unwrap(),
        Some(ReceiveStatus::Packet {
            peer_id: 1,
            bytes: retry_from_alice,
        })
    );
    assert_eq!(
        alice.receive_timeout(Duration::from_secs(2)).unwrap(),
        Some(ReceiveStatus::Packet {
            peer_id: 2,
            bytes: retry_from_bob,
        })
    );
    worker.join().unwrap();
}

#[test]
fn unauthenticated_direct_data_does_not_cancel_relay_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(1);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let spoof = UdpSocket::bind("127.0.0.1:0").unwrap();
    let alice_key = [1; 32];
    let mut alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key([2; 32])),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    alice_map.peers.get_mut(&2).unwrap().endpoints = vec![spoof.local_addr().unwrap().to_string()];
    let mut alice =
        PacketEngine::connect(alice_key, &alice_map, service.derp_addr, Some(cert.der)).unwrap();
    let direct = alice.bind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    let first = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    assert_eq!(alice.send_ip(&first).unwrap(), SendStatus::HandshakeStarted);
    spoof.send_to(&[4, 0, 0, 0], direct).unwrap();
    assert_eq!(direct_event(&mut alice), ReceiveStatus::Control);
    thread::sleep(Duration::from_millis(1100));
    alice.tick_direct().unwrap();
    assert_eq!(alice.transport_stats().derp_handshakes_sent, 1);
    worker.join().unwrap();
}

#[test]
fn unauthenticated_direct_data_cannot_start_recovery_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(1);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let spoof = UdpSocket::bind("127.0.0.1:0").unwrap();
    let bob_key = [2; 32];
    let mut bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key([1; 32])),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    bob_map.peers.get_mut(&1).unwrap().endpoints = vec![spoof.local_addr().unwrap().to_string()];
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let direct = bob.bind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    spoof.send_to(&[4, 0, 0, 0], direct).unwrap();
    assert_eq!(direct_event(&mut bob), ReceiveStatus::Control);
    assert_eq!(bob.transport_stats().derp_handshakes_sent, 0);
    worker.join().unwrap();
}

#[test]
fn derp_handshake_lost_before_peer_connect_retries_queued_packet() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [21; 32];
    let bob_key = [22; 32];
    let alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let packet = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    assert_eq!(
        alice.send_ip(&packet).unwrap(),
        SendStatus::HandshakeStarted
    );
    let counters = alice.transport_stats();
    assert_eq!(counters.derp_handshakes_sent, 1);
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    assert_eq!(
        bob.receive_timeout(Duration::from_millis(100)).unwrap(),
        None
    );
    let joined = *b"join0001";
    bob.ping_relay(joined).unwrap();
    assert_eq!(
        bob.receive_timeout(Duration::from_secs(2)).unwrap(),
        Some(ReceiveStatus::Pong(joined))
    );
    thread::sleep(Duration::from_millis(3100));
    alice.tick_direct().unwrap();
    assert_eq!(alice.transport_stats().derp_handshakes_sent, 2);
    let deadline = Instant::now() + Duration::from_secs(2);
    while bob
        .peer_traffic()
        .get(&1)
        .and_then(|traffic| traffic.last_handshake)
        .is_none()
    {
        let _ = bob.receive_timeout(Duration::from_millis(200)).unwrap();
        assert!(Instant::now() < deadline, "retry never reached the peer");
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while alice.transport_stats().derp_ip_sent == 0 {
        let _ = alice.receive_timeout(Duration::from_millis(200)).unwrap();
        assert!(
            Instant::now() < deadline,
            "retry did not finish its handshake"
        );
    }
    assert_eq!(
        bob.receive_timeout(Duration::from_secs(2)).unwrap(),
        Some(ReceiveStatus::Packet {
            peer_id: 1,
            bytes: packet
        })
    );
    worker.join().unwrap();
}

#[test]
fn malformed_or_unpinned_relay_map_fails_closed() {
    let alice_key = [1; 32];
    let bob_key = [2; 32];
    let mut map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        443,
        "sha256-raw:bad",
    );
    let error = PacketEngine::connect(alice_key, &map, "127.0.0.1:443".parse().unwrap(), None)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    map.derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("900")
        .unwrap()
        .nodes[0]
        .cert_name
        .clear();
    let error = PacketEngine::connect(alice_key, &map, "127.0.0.1:443".parse().unwrap(), None)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    map.derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("900")
        .unwrap()
        .nodes[0]
        .cert_name = format!("sha256-raw:{}", "a".repeat(64));
    let error = PacketEngine::connect(alice_key, &map, "127.0.0.2:443".parse().unwrap(), None)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn public_relay_uses_webpki_hostname_without_private_pin() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(1);
    let hostname = "derp-test.tailscale.com";
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: hostname.into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), hostname).unwrap();
    let alice_key = [1; 32];
    let bob_key = [2; 32];
    let mut public = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        "",
    );
    public.self_node.as_mut().unwrap().home_derp = 1;
    for peer in public.peers.values_mut() {
        peer.home_derp = 1;
    }
    let derp_map = public.derp_map.as_mut().unwrap();
    let mut region = derp_map.regions.remove("900").unwrap();
    region.region_id = 1;
    region.region_code = "nyc".into();
    derp_map.regions.insert("1".into(), region);
    let relay = &mut public
        .derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("1")
        .unwrap()
        .nodes[0];
    relay.host_name = hostname.into();
    let mut engine = PacketEngine::connect(
        alice_key,
        &public,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    worker.join().unwrap();
    engine.replace_map(&public).unwrap();
    let mut changed = public.clone();
    changed
        .derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("1")
        .unwrap()
        .nodes[0]
        .cert_name = "sha256-raw:bad".into();
    assert!(engine.replace_map(&changed).is_err());
    drop(engine);
    public
        .derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("1")
        .unwrap()
        .nodes[0]
        .host_name = "derp-wrong.tailscale.com".into();
    assert!(PacketEngine::connect(alice_key, &public, service.derp_addr, Some(cert.der)).is_err());
}

#[test]
fn authenticated_packet_denied_by_destination_port_filter() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [3; 32];
    let bob_key = [4; 32];
    let mut alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    // Private Headscale DERP maps may advertise only HostName and CertName.
    alice_map
        .derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("900")
        .unwrap()
        .nodes[0]
        .ipv4
        .clear();
    bob_map
        .derp_map
        .as_mut()
        .unwrap()
        .regions
        .get_mut("900")
        .unwrap()
        .nodes[0]
        .ipv4
        .clear();
    bob_map.packet_filters.insert(
        "base".into(),
        vec![json!({
            "SrcIPs":["100.64.0.1"],
            "DstPorts":[{"IP":"100.64.0.2","Ports":{"First":80,"Last":80}}],
            "IPProto":[17]
        })],
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let forbidden = udp([100, 64, 0, 1], [100, 64, 0, 2], 443);
    assert_eq!(
        alice.send_ip(&forbidden).unwrap(),
        SendStatus::HandshakeStarted
    );
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    let nonce = *b"live0002";
    bob.ping_relay(nonce).unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Pong(nonce));
    worker.join().unwrap();
}

#[test]
fn two_members_exchange_wireguard_ip_over_published_direct_udp() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [21; 32];
    let bob_key = [22; 32];
    let mut alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let alice_udp = alice.bind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    let bob_udp = bob.bind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    alice_map.peers.get_mut(&2).unwrap().endpoints = vec![bob_udp.to_string()];
    bob_map.peers.get_mut(&1).unwrap().endpoints = vec![alice_udp.to_string()];
    alice.replace_map(&alice_map).unwrap();
    bob.replace_map(&bob_map).unwrap();
    let first = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    let first_len = first.len();
    assert_eq!(alice.send_ip(&first).unwrap(), SendStatus::HandshakeStarted);
    assert_eq!(direct_event(&mut bob), ReceiveStatus::Control);
    assert_eq!(direct_event(&mut alice), ReceiveStatus::Control);
    assert_eq!(
        direct_event(&mut bob),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: first
        }
    );
    let reply = udp([100, 64, 0, 2], [100, 64, 0, 1], 4097);
    let reply_len = reply.len();
    assert_eq!(bob.send_ip(&reply).unwrap(), SendStatus::Sent);
    assert_eq!(
        direct_event(&mut alice),
        ReceiveStatus::Packet {
            peer_id: 2,
            bytes: reply
        }
    );
    let peer = alice.peer_traffic().remove(&2).unwrap();
    assert!(peer.last_handshake.is_some());
    assert_eq!(peer.tx_bytes, first_len as u64);
    assert_eq!(peer.rx_bytes, reply_len as u64);
    assert_eq!(peer.direct_addr, Some(bob_udp));
    assert!(!peer.relay);
    let before_move = alice.transport_stats();
    let moved_bob_udp = bob.rebind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    alice_map.peers.get_mut(&2).unwrap().endpoints = vec![moved_bob_udp.to_string()];
    alice.replace_map(&alice_map).unwrap();
    let after_move = udp([100, 64, 0, 1], [100, 64, 0, 2], 4098);
    assert_eq!(alice.send_ip(&after_move).unwrap(), SendStatus::Sent);
    assert_eq!(
        bob.receive_timeout(Duration::from_secs(2)).unwrap(),
        Some(ReceiveStatus::Packet {
            peer_id: 1,
            bytes: after_move,
        })
    );
    assert_eq!(
        alice.transport_stats().direct_ip_sent,
        before_move.direct_ip_sent
    );
    assert_eq!(
        alice.transport_stats().derp_ip_sent,
        before_move.derp_ip_sent + 1
    );
    alice.tick_direct().unwrap();
    assert_eq!(direct_event(&mut bob), ReceiveStatus::Control);
    assert_eq!(direct_event(&mut alice), ReceiveStatus::Control);
    let promoted = udp([100, 64, 0, 1], [100, 64, 0, 2], 4100);
    assert_eq!(alice.send_ip(&promoted).unwrap(), SendStatus::Sent);
    assert_eq!(
        direct_event(&mut bob),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: promoted,
        }
    );
    let moved_reply = udp([100, 64, 0, 2], [100, 64, 0, 1], 4099);
    assert_eq!(bob.send_ip(&moved_reply).unwrap(), SendStatus::Sent);
    assert_eq!(
        direct_event(&mut alice),
        ReceiveStatus::Packet {
            peer_id: 2,
            bytes: moved_reply
        }
    );
    assert!(alice.transport_stats().direct_ip_sent > before_move.direct_ip_sent);
    let stranger = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    stranger.send_to(&[1, 0, 0, 0], moved_bob_udp).unwrap();
    assert_eq!(bob.receive_direct().unwrap(), None);
    bob_map.peers.get_mut(&1).unwrap().machine_authorized = Some(false);
    bob.replace_map(&bob_map).unwrap();
    let stale = udp([100, 64, 0, 1], [100, 64, 0, 2], 4098);
    assert_eq!(alice.send_ip(&stale).unwrap(), SendStatus::Sent);
    assert_eq!(bob.receive_direct().unwrap(), None);
    assert_eq!(
        alice.receive_timeout(Duration::from_millis(50)).unwrap(),
        None
    );
    assert_eq!(
        bob.receive_timeout(Duration::from_millis(50)).unwrap(),
        None
    );
    worker.join().unwrap();
}

#[test]
fn lost_direct_handshake_falls_back_to_pinned_derp() {
    let dir = tempfile::tempdir().unwrap();
    let (verify_url, worker) = verifier(2);
    let service = Service::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        stun_listen: "127.0.0.1:0".parse().unwrap(),
        hostname: "localhost".into(),
        verify_url,
        state_dir: dir.path().to_owned(),
    })
    .unwrap();
    let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
    let alice_key = [23; 32];
    let bob_key = [24; 32];
    let mut alice_map = map(
        (1, "100.64.0.1", key(alice_key)),
        (2, "100.64.0.2", key(bob_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut bob_map = map(
        (2, "100.64.0.2", key(bob_key)),
        (1, "100.64.0.1", key(alice_key)),
        service.derp_addr.port(),
        &service.certificate_pin,
    );
    let mut alice = PacketEngine::connect(
        alice_key,
        &alice_map,
        service.derp_addr,
        Some(cert.der.clone()),
    )
    .unwrap();
    let mut bob =
        PacketEngine::connect(bob_key, &bob_map, service.derp_addr, Some(cert.der)).unwrap();
    let alice_udp = alice.bind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    let bob_udp = bob.bind_direct("127.0.0.1:0".parse().unwrap()).unwrap();
    alice_map.peers.get_mut(&2).unwrap().endpoints = vec![bob_udp.to_string()];
    alice.replace_map(&alice_map).unwrap();
    let first = udp([100, 64, 0, 1], [100, 64, 0, 2], 4096);
    let first_len = first.len();
    assert_eq!(alice.send_ip(&first).unwrap(), SendStatus::HandshakeStarted);
    assert_eq!(bob.receive_direct().unwrap(), None); // Alice's endpoint is not yet in Bob's map.
    assert_eq!(
        bob.receive_timeout(Duration::from_millis(50)).unwrap(),
        None
    );
    thread::sleep(Duration::from_millis(260));
    alice.tick_direct().unwrap();
    assert_eq!(bob.receive_direct().unwrap(), None);
    assert_eq!(
        bob.receive_timeout(Duration::from_millis(50)).unwrap(),
        None
    );
    thread::sleep(Duration::from_millis(750));
    alice.tick_direct().unwrap();
    assert_eq!(bob.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(alice.receive().unwrap(), ReceiveStatus::Control);
    assert_eq!(
        bob.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: first
        }
    );
    assert_eq!(alice.transport_stats().direct_handshakes_sent, 2);
    assert_eq!(alice.transport_stats().derp_handshakes_sent, 1);
    assert_eq!(bob.transport_stats().derp_ip_received, 1);

    // A failed direct reprobe must leave DERP data usable and rate-limit
    // subsequent probes while Bob's authenticated map still lacks Alice's endpoint.
    alice.tick_direct().unwrap();
    assert_eq!(bob.receive_direct().unwrap(), None);
    assert_eq!(alice.transport_stats().direct_keepalives_sent, 1);
    let relayed = udp([100, 64, 0, 1], [100, 64, 0, 2], 4100);
    let relayed_len = relayed.len();
    assert_eq!(alice.send_ip(&relayed).unwrap(), SendStatus::Sent);
    assert_eq!(
        bob.receive().unwrap(),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: relayed
        }
    );
    let peer = bob.peer_traffic().remove(&1).unwrap();
    assert_eq!(peer.rx_bytes, (first_len + relayed_len) as u64);
    assert_eq!(peer.direct_addr, None);
    assert!(peer.relay);
    alice.tick_direct().unwrap();
    assert_eq!(alice.transport_stats().direct_keepalives_sent, 1);

    // Once Bob learns Alice's published source endpoint, the next bounded
    // reprobe promotes the existing WG session without interrupting TCP/IP.
    bob_map.peers.get_mut(&1).unwrap().endpoints = vec![alice_udp.to_string()];
    bob.replace_map(&bob_map).unwrap();
    thread::sleep(Duration::from_secs(10) + Duration::from_millis(20));
    alice.tick_direct().unwrap();
    assert_eq!(direct_event(&mut bob), ReceiveStatus::Control);
    assert_eq!(direct_event(&mut alice), ReceiveStatus::Control);
    assert_eq!(alice.transport_stats().direct_keepalives_sent, 2);
    let promoted = udp([100, 64, 0, 1], [100, 64, 0, 2], 4101);
    assert_eq!(alice.send_ip(&promoted).unwrap(), SendStatus::Sent);
    assert_eq!(
        direct_event(&mut bob),
        ReceiveStatus::Packet {
            peer_id: 1,
            bytes: promoted
        }
    );
    assert_eq!(alice.transport_stats().derp_ip_sent, 2);
    worker.join().unwrap();
}
