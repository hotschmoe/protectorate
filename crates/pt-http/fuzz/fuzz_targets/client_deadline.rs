#![no_main]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some(&mode) = data.first() else { return };
    let timeout = Duration::from_millis(200 + u64::from(data.get(1).copied().unwrap_or(0) % 100));
    let interval = timeout / 8;
    let interim = mode & 1 != 0;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        if !interim {
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n")
                .unwrap();
        }
        let bytes: &[u8] = if interim {
            b"HTTP/1.1 100 Continue\r\n\r\n"
        } else {
            b"x"
        };
        for _ in 0..32 {
            std::thread::sleep(interval);
            if socket.write_all(bytes).is_err() {
                break;
            }
        }
    });
    let client = pt_http::Client::tcp(addr).read_timeout(Some(timeout));
    let start = Instant::now();
    let err = client
        .request("GET", "/", &[], b"")
        .expect_err("trickled response must time out");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "{err}");
    assert!(start.elapsed() < Duration::from_secs(2));
    server.join().unwrap();
});
