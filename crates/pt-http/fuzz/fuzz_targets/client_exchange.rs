#![no_main]

use pt_http::{Client, Config, Message, Request, Response, Server};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn read_head(socket: &mut TcpStream) -> String {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    String::from_utf8(head).unwrap()
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some(&mode) = data.first() else { return };
    let payload = if data.get(1) == Some(&255) {
        (0..200_000).map(|i| i as u8).collect::<Vec<_>>()
    } else {
        let bytes = data.get(1..).unwrap_or_default();
        if bytes.is_empty() {
            vec![0]
        } else {
            bytes[..bytes.len().min(65_536)].to_vec()
        }
    };
    let response_body = payload.clone();
    let server = Server::bind_tcp(
        "127.0.0.1:0".parse().unwrap(),
        Config::default(),
        move |req: &Request, _| match req.path() {
            "/echo" => Response::bytes(200, "application/octet-stream", req.body().to_vec()),
            "/bytes" => Response::bytes(200, "application/octet-stream", response_body.clone()),
            "/stream" => {
                let body = response_body.clone();
                Response::stream(200, "application/octet-stream", move |writer| {
                    let middle = body.len() / 2;
                    writer.write_all(&body[..middle])?;
                    writer.write_all(&body[middle..])
                })
            }
            "/ws" => Response::websocket(|mut ws| {
                loop {
                    match ws.recv() {
                        Ok(Message::Text(t)) if t == "push" => {
                            let writer = ws.writer();
                            std::thread::spawn(move || {
                                for i in 0..3 {
                                    writer.send_text(&format!("pushed {i}")).unwrap();
                                }
                            })
                            .join()
                            .unwrap();
                        }
                        Ok(Message::Text(t)) => ws.send_text(&t).unwrap(),
                        Ok(Message::Binary(b)) => ws.send_binary(&b).unwrap(),
                        Ok(Message::Pong(_)) => {}
                        Ok(Message::Close(..)) | Err(_) => return,
                    }
                }
            }),
            _ => Response::not_found(),
        },
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let shutdown = server.shutdown_handle();
    let handle = std::thread::spawn(move || server.run());
    let client = Client::tcp(addr).read_timeout(Some(Duration::from_secs(5)));
    match mode % 4 {
        0 => {
            let streamed = client.request("GET", "/stream", &[], b"").unwrap();
            assert_eq!(streamed.status, 200);
            assert_eq!(streamed.headers.get("transfer-encoding"), Some("chunked"));
            assert_eq!(streamed.body, payload);
            let echoed = client.request("POST", "/echo", &[], &payload).unwrap();
            assert_eq!(echoed.body, payload);
            let head = client.request("HEAD", "/bytes", &[], b"").unwrap();
            assert_eq!((head.status, head.body.len()), (200, 0));
            assert_eq!(
                client.request("GET", "/missing", &[], b"").unwrap().status,
                404
            );
            let small = Client::tcp(addr).max_body(payload.len() - 1);
            assert!(small.request("GET", "/bytes", &[], b"").is_err());
        }
        1 => {
            let mut ws = client
                .websocket("/ws", &[("Origin", "http://test")])
                .unwrap();
            ws.send_text("hello").unwrap();
            assert_eq!(ws.recv().unwrap(), Message::Text("hello".into()));
            ws.send_binary(&payload).unwrap();
            assert_eq!(ws.recv().unwrap(), Message::Binary(payload));
            ws.writer().ping(b"are you there").unwrap();
            assert_eq!(ws.recv().unwrap(), Message::Pong(b"are you there".to_vec()));
            ws.send_text("push").unwrap();
            for i in 0..3 {
                assert_eq!(ws.recv().unwrap(), Message::Text(format!("pushed {i}")));
            }
            assert_eq!(ws.close(1000, "bye").unwrap(), (1000, String::new()));
            assert!(ws.send_text("late").is_err());
        }
        strip => {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let proxy_addr = listener.local_addr().unwrap();
            let proxy = std::thread::spawn(move || {
                let (mut down, _) = listener.accept().unwrap();
                let request = read_head(&mut down);
                let mut up = TcpStream::connect(addr).unwrap();
                up.write_all(request.as_bytes()).unwrap();
                let answer = read_head(&mut up);
                let name = if strip == 2 {
                    "upgrade:"
                } else {
                    "connection:"
                };
                assert_eq!(
                    answer.lines().next().unwrap().split_whitespace().nth(1),
                    Some("101")
                );
                assert!(
                    answer
                        .lines()
                        .any(|line| line.to_ascii_lowercase().starts_with(name))
                );
                let answer: String = answer
                    .split_inclusive("\r\n")
                    .filter(|line| !line.to_ascii_lowercase().starts_with(name))
                    .collect();
                down.write_all(answer.as_bytes()).unwrap();
            });
            let err = Client::tcp(proxy_addr)
                .read_timeout(Some(Duration::from_secs(5)))
                .websocket("/ws", &[])
                .err()
                .expect("stripped upgrade header must fail");
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
            proxy.join().unwrap();
        }
    }
    shutdown.shutdown(Duration::from_secs(2));
    handle.join().unwrap().unwrap();
});
