//! WebSockets through curl's handshake, our client, and hand-built frames on a raw socket.

mod common;

use common::{curl, start};
use pt_http::{Client, Config, Message};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const SAMPLE_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
// RFC 6455 section 1.3.
const SAMPLE_ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

#[test]
fn curl_handshake_gets_101_with_rfc_accept() {
    let s = start(Config::default());
    let r = curl(&[
        "-i",
        "-N",
        "--max-time",
        "2",
        "-H",
        "Connection: Upgrade",
        "-H",
        "Upgrade: websocket",
        "-H",
        "Sec-WebSocket-Version: 13",
        "-H",
        &format!("Sec-WebSocket-Key: {SAMPLE_KEY}"),
        &s.url("/ws"),
    ]);
    assert!(
        r.stdout.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "{} {}",
        r.stdout,
        r.stderr
    );
    assert!(
        r.stdout
            .contains(&format!("Sec-WebSocket-Accept: {SAMPLE_ACCEPT}\r\n")),
        "{}",
        r.stdout
    );

    let code = |extra: &[&str]| {
        let base = [
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-H",
            "Connection: Upgrade",
            "-H",
            "Upgrade: websocket",
        ];
        curl(&[&base[..], extra, &[&s.url("/ws")]].concat()).stdout
    };
    assert_eq!(
        code(&[
            "-H",
            "Sec-WebSocket-Version: 8",
            "-H",
            &format!("Sec-WebSocket-Key: {SAMPLE_KEY}")
        ]),
        "426"
    );
    assert_eq!(
        code(&[
            "-H",
            "Sec-WebSocket-Version: 13",
            "-H",
            "Sec-WebSocket-Key: short"
        ]),
        "400"
    );
    assert_eq!(
        curl(&["-o", "/dev/null", "-w", "%{http_code}", &s.url("/ws")]).stdout,
        "400"
    );
    s.stop();
}

#[test]
fn client_echo_text_binary_ping_push_close() {
    let s = start(Config::default());
    let mut ws = Client::tcp(s.addr.unwrap())
        .websocket("/ws", &[("Origin", "http://test")])
        .unwrap();
    ws.send_text("hello").unwrap();
    assert_eq!(ws.recv().unwrap(), Message::Text("hello".into()));
    let big: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
    ws.send_binary(&big).unwrap();
    assert_eq!(ws.recv().unwrap(), Message::Binary(big));
    ws.writer().ping(b"are you there").unwrap();
    assert_eq!(ws.recv().unwrap(), Message::Pong(b"are you there".to_vec()));
    ws.send_text("push").unwrap();
    for i in 0..3 {
        assert_eq!(ws.recv().unwrap(), Message::Text(format!("pushed {i}")));
    }
    assert_eq!(ws.close(1000, "bye").unwrap(), (1000, String::new()));
    assert!(ws.send_text("late").is_err());
    s.stop();
}

#[test]
fn idle_ping_keepalive_drops_a_silent_peer() {
    let s = start(Config {
        ws_ping_interval: Some(Duration::from_millis(200)),
        ..Config::default()
    });
    let mut c = raw_handshake(s.addr.unwrap());
    // The server pings after 200 ms of silence; we never answer, so it hangs up.
    let frame = read_frame(&mut c);
    assert_eq!(frame, (true, 9, vec![]));
    let mut tail = Vec::new();
    c.read_to_end(&mut tail).unwrap();
    s.stop();
}

/// A peer that fragments a message and pings between the fragments.
#[test]
fn fragmented_message_with_interleaved_ping() {
    let s = start(Config::default());
    let mut c = raw_handshake(s.addr.unwrap());
    c.write_all(&masked(false, 1, b"frag")).unwrap();
    c.write_all(&masked(true, 9, b"p")).unwrap();
    c.write_all(&masked(false, 0, "mented ".as_bytes()))
        .unwrap();
    c.write_all(&masked(true, 0, "\u{e9}".as_bytes())).unwrap();
    assert_eq!(read_frame(&mut c), (true, 10, b"p".to_vec()));
    assert_eq!(
        read_frame(&mut c),
        (true, 1, "fragmented \u{e9}".as_bytes().to_vec())
    );

    s.stop();
}

#[test]
fn shutdown_closes_open_websockets() {
    let s = start(Config::default());
    let mut ws = Client::tcp(s.addr.unwrap()).websocket("/ws", &[]).unwrap();
    ws.send_text("x").unwrap();
    assert_eq!(ws.recv().unwrap(), Message::Text("x".into()));
    let started = std::time::Instant::now();
    s.stop();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(ws.recv().is_err());
}

fn masked(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
    let key = [0x37, 0xfa, 0x21, 0x3d];
    let mut f = vec![u8::from(fin) << 7 | opcode];
    match payload.len() {
        n @ 0..126 => f.push(0x80 | n as u8),
        n => {
            f.push(0x80 | 126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
    }
    f.extend_from_slice(&key);
    f.extend(payload.iter().zip(key.iter().cycle()).map(|(b, k)| b ^ k));
    f
}

fn read_frame(c: &mut TcpStream) -> (bool, u8, Vec<u8>) {
    let mut h = [0u8; 2];
    c.read_exact(&mut h).unwrap();
    assert_eq!(h[1] & 0x80, 0, "server frames are not masked");
    let len = match h[1] & 0x7f {
        126 => {
            let mut b = [0; 2];
            c.read_exact(&mut b).unwrap();
            u16::from_be_bytes(b) as usize
        }
        n => n as usize,
    };
    let mut payload = vec![0; len];
    c.read_exact(&mut payload).unwrap();
    (h[0] & 0x80 != 0, h[0] & 0x0f, payload)
}

/// Sends the RFC sample handshake and reads the 101 byte by byte.
fn raw_handshake(addr: std::net::SocketAddr) -> TcpStream {
    let mut c = TcpStream::connect(addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        c,
        "GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\n\
         Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {SAMPLE_KEY}\r\n\r\n"
    )
    .unwrap();
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        c.read_exact(&mut b).unwrap();
        head.push(b[0]);
    }
    let head = String::from_utf8(head).unwrap();
    assert!(head.starts_with("HTTP/1.1 101 "), "{head}");
    assert!(head.contains(SAMPLE_ACCEPT));
    c
}
