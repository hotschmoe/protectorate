use std::net::SocketAddr;

use protectorate_tailnet_stream::{Accepted, Driver};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};

const SRC: [u8; 4] = [100, 64, 0, 2];
const DST: [u8; 4] = [100, 64, 0, 1];
const SPORT: u16 = 54321;
const DPORT: u16 = 8080;
const SYN: u8 = 0x02;
const FIN: u8 = 0x01;
const ACK: u8 = 0x10;
const PSH: u8 = 0x08;

fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in data.chunks(2) {
        sum += u32::from(u16::from_be_bytes([chunk[0], *chunk.get(1).unwrap_or(&0)]));
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}
fn client_packet(seq: u32, ack: u32, flags: u8, payload: &[u8]) -> Vec<u8> {
    let len = 40 + payload.len();
    let mut packet = vec![0u8; len];
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&(len as u16).to_be_bytes());
    packet[8] = 64;
    packet[9] = 6;
    packet[12..16].copy_from_slice(&SRC);
    packet[16..20].copy_from_slice(&DST);
    let ip_sum = checksum(&packet[..20]);
    packet[10..12].copy_from_slice(&ip_sum.to_be_bytes());
    let tcp = &mut packet[20..];
    tcp[0..2].copy_from_slice(&SPORT.to_be_bytes());
    tcp[2..4].copy_from_slice(&DPORT.to_be_bytes());
    tcp[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp[12] = 0x50;
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&u16::MAX.to_be_bytes());
    tcp[20..].copy_from_slice(payload);
    let mut pseudo = Vec::with_capacity(12 + tcp.len());
    pseudo.extend(SRC);
    pseudo.extend(DST);
    pseudo.extend([0, 6]);
    pseudo.extend((tcp.len() as u16).to_be_bytes());
    pseudo.extend_from_slice(tcp);
    let tcp_sum = checksum(&pseudo);
    tcp[16..18].copy_from_slice(&tcp_sum.to_be_bytes());
    packet
}
fn server_segment(packet: &[u8]) -> (u32, u8, &[u8]) {
    assert_eq!(&packet[12..16], &DST);
    assert_eq!(&packet[16..20], &SRC);
    assert_eq!(checksum(&packet[..20]), 0);
    let tcp = &packet[20..];
    let mut pseudo = Vec::new();
    pseudo.extend(DST);
    pseudo.extend(SRC);
    pseudo.extend([0, 6]);
    pseudo.extend((tcp.len() as u16).to_be_bytes());
    pseudo.extend_from_slice(tcp);
    assert_eq!(checksum(&pseudo), 0);
    (
        u32::from_be_bytes(tcp[4..8].try_into().unwrap()),
        tcp[13],
        &tcp[20..],
    )
}

#[tokio::test]
async fn http_11_request_and_response_over_virtual_tcp() {
    let listener: SocketAddr = "100.64.0.1:8080".parse().unwrap();
    let (driver, mut ports) = Driver::new_with_generation(listener, 7);
    let task = tokio::spawn(driver.run());
    let request = b"GET /health HTTP/1.1\r\nHost: protectorate\r\n\r\n";
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
    ports
        .ingress
        .send(1, client_packet(1000, 0, SYN, &[]))
        .await
        .unwrap();
    let synack = timeout(Duration::from_secs(2), ports.egress.recv())
        .await
        .unwrap()
        .unwrap();
    let (server_isn, flags, payload) = server_segment(&synack);
    assert_eq!(flags, SYN | ACK);
    assert!(payload.is_empty());
    let mut client_seq = 1001u32;
    let mut server_next = server_isn.wrapping_add(1);
    ports
        .ingress
        .send(2, client_packet(client_seq, server_next, ACK, &[]))
        .await
        .unwrap();
    assert!(timeout(Duration::from_millis(50), ports.accepted.recv())
        .await
        .is_err());
    ports
        .ingress
        .send(1, client_packet(client_seq, server_next, ACK, &[]))
        .await
        .unwrap();
    let Accepted {
        mut stream,
        peer_id,
        generation,
        ..
    } = timeout(Duration::from_secs(2), ports.accepted.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer_id, 1);
    assert_eq!(generation, 7);
    let service = tokio::spawn(async move {
        let mut input = vec![0; request.len()];
        stream.read_exact(&mut input).await.unwrap();
        assert_eq!(input, request);
        stream.write_all(response).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    ports
        .ingress
        .send(
            1,
            client_packet(client_seq, server_next, PSH | ACK, request),
        )
        .await
        .unwrap();
    client_seq += request.len() as u32;
    let mut received: Vec<u8> = Vec::new();
    let mut saw_fin = false;
    for _ in 0..20 {
        let packet = timeout(Duration::from_secs(2), ports.egress.recv())
            .await
            .unwrap()
            .unwrap();
        let (seq, flags, payload) = server_segment(&packet);
        if !payload.is_empty() && seq == server_next {
            received.extend_from_slice(payload);
            server_next = server_next.wrapping_add(payload.len() as u32);
            ports
                .ingress
                .send(1, client_packet(client_seq, server_next, ACK, &[]))
                .await
                .unwrap();
        }
        if flags & FIN != 0 && seq == server_next {
            server_next = server_next.wrapping_add(1);
            ports
                .ingress
                .send(1, client_packet(client_seq, server_next, ACK, &[]))
                .await
                .unwrap();
            saw_fin = true;
        }
        if saw_fin && received == response {
            break;
        }
    }
    assert_eq!(received, response);
    assert!(saw_fin);
    timeout(Duration::from_secs(2), service)
        .await
        .unwrap()
        .unwrap();
    ports.shutdown();
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn ingress_rejects_oversized_packets_and_has_finite_capacity() {
    let listener: SocketAddr = "100.64.0.1:8080".parse().unwrap();
    let (_driver, ports) = Driver::new(listener);
    assert_eq!(
        ports
            .ingress
            .try_send(1, vec![0; protectorate_tailnet_stream::MAX_IP_PACKET + 1]),
        Err(protectorate_tailnet_stream::IngressError::Oversized)
    );
    for _ in 0..64 {
        ports.ingress.try_send(1, vec![]).unwrap();
    }
    assert_eq!(
        ports.ingress.try_send(1, vec![]),
        Err(protectorate_tailnet_stream::IngressError::Full)
    );
}
