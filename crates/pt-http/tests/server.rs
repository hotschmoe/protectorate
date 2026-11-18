//! The server against the real `curl` binary and raw sockets, and our client against the server.

mod common;

use common::{curl, header, start, start_unix, temp_dir};
use pt_http::Config;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::time::{Duration, Instant};

#[test]
fn get_stream_query_and_errors() {
    let s = start(Config::default());
    let r = curl(&["-i", &s.url("/hello")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stdout
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1)),
        Some("200")
    );
    assert_eq!(
        header(&r.stdout, "content-length").and_then(|v| v.parse::<u64>().ok()),
        Some(6)
    );
    assert!(r.stdout.ends_with("\r\n\r\nhello\n"));

    let r = curl(&["-i", &s.url("/stream")]);
    assert!(
        header(&r.stdout, "transfer-encoding").is_some_and(|value| value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))),
        "{}",
        r.stdout
    );
    assert!(r.stdout.ends_with("one two three\n"));

    assert_eq!(
        curl(&[&s.url("/query?x=1&name=a%20b+c%2")]).stdout,
        "a b c%2"
    );
    let code =
        |args: &[&str]| curl(&[&["-o", "/dev/null", "-w", "%{http_code}"], args].concat()).stdout;
    assert_eq!(code(&[&s.url("/nope")]), "404");
    assert_eq!(code(&["-X", "DELETE", &s.url("/hello")]), "405");
    assert_eq!(code(&["--http1.0", &s.url("/hello")]), "200");
    let big = format!("X-Big: {}", "a".repeat(20_000));
    assert_eq!(code(&["-H", &big, &s.url("/hello")]), "431");
    assert_eq!(code(&["-H", "Expect: tea", &s.url("/hello")]), "417");
    s.stop();
}

#[test]
fn post_content_length_and_chunked() {
    let s = start(Config::default());
    let dir = temp_dir("post");
    let file = dir.join("body.bin");
    let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&file, &body).unwrap();
    let data = format!("@{}", file.display());
    let out = dir.join("out.bin");
    let out_s = out.to_str().unwrap();

    let r = curl(&[
        "-D",
        "-",
        "-o",
        out_s,
        "--data-binary",
        &data,
        &s.url("/echo"),
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(std::fs::read(&out).unwrap(), body);

    let r = curl(&[
        "-D",
        "-",
        "-o",
        out_s,
        "-H",
        "Transfer-Encoding: chunked",
        "--data-binary",
        &data,
        &s.url("/echo"),
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(std::fs::read(&out).unwrap(), body);
    s.stop();
}

#[test]
fn keep_alive_reuses_the_connection() {
    let s = start(Config::default());
    let r = curl(&[&s.url("/peer"), &s.url("/peer"), &s.url("/hello")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let ports: Vec<&str> = r
        .stdout
        .split("tcp ")
        .skip(1)
        .map(|p| p.trim_end_matches("hello\n"))
        .collect();
    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0], ports[1], "both requests on one connection");
    s.stop();
}

#[test]
fn body_over_limit_is_413() {
    let s = start(Config {
        max_body: 1 << 20,
        ..Config::default()
    });
    let dir = temp_dir("big");
    let file = dir.join("big.bin");
    std::fs::write(&file, vec![1u8; 2 << 20]).unwrap();
    let data = format!("@{}", file.display());
    let code = |extra: &[&str]| {
        let r = curl(
            &[
                &[
                    "-o",
                    "/dev/null",
                    "-w",
                    "%{http_code}",
                    "--data-binary",
                    &data,
                ],
                extra,
                &[&s.url("/echo")],
            ]
            .concat(),
        );
        r.stdout
    };
    assert_eq!(code(&[]), "413");
    assert_eq!(code(&["-H", "Expect:"]), "413");
    assert_eq!(code(&["-H", "Transfer-Encoding: chunked"]), "413");
    s.stop();
}

#[test]
fn slow_client_gets_408() {
    let s = start(Config {
        header_timeout: Duration::from_millis(500),
        ..Config::default()
    });
    let mut c = TcpStream::connect(s.addr.unwrap()).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let start = Instant::now();
    c.write_all(b"GET /hello HTTP/1.1\r\nHost: x\r\n").unwrap();
    // Trickling bytes does not extend the header deadline.
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(200));
        let _ = c.write_all(b"X: y\r\n");
    }
    let mut out = String::new();
    c.read_to_string(&mut out).unwrap();
    assert_eq!(
        out.lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1)),
        Some("408"),
        "{out}"
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    s.stop();
}

#[test]
fn unix_socket_with_peer_credentials() {
    let dir = temp_dir("unix");
    let sock = dir.join("hub.sock");
    let s = start_unix(Config::default(), &sock);
    let meta = std::fs::metadata(&sock).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let sock_s = sock.to_str().unwrap();
    let r = curl(&["--unix-socket", sock_s, "http://hub/hello"]);
    assert_eq!(r.stdout, "hello\n", "{}", r.stderr);
    let r = curl(&["--unix-socket", sock_s, "http://hub/peer"]);
    assert!(
        r.stdout.starts_with(&format!("unix {} ", meta.uid())),
        "{}",
        r.stdout
    );

    s.stop();
}

#[test]
fn static_file_etag_and_head() {
    let s = start(Config::default());
    let r = curl(&["-i", &s.url("/static/index.html")]);
    let mut content_type = header(&r.stdout, "content-type")
        .unwrap()
        .split(';')
        .map(str::trim);
    assert!(
        content_type
            .next()
            .is_some_and(|v| v.eq_ignore_ascii_case("text/html"))
    );
    assert!(content_type.any(|v| v.eq_ignore_ascii_case("charset=utf-8")));
    assert!(r.stdout.ends_with("<!doctype html><title>t</title>\n"));
    let etag = header(&r.stdout, "etag").unwrap().to_string();

    let r = curl(&[
        "-i",
        "-H",
        &format!("If-None-Match: {etag}"),
        &s.url("/static/index.html"),
    ]);
    assert_eq!(
        r.stdout
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1)),
        Some("304")
    );
    assert!(r.stdout.ends_with("\r\n\r\n"));

    let r = curl(&["-I", &s.url("/static/app.wasm")]);
    assert_eq!(
        r.stdout
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1)),
        Some("200")
    );
    assert!(header(&r.stdout, "content-type").is_some_and(|v| {
        v.split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/wasm"))
    }));
    assert_eq!(
        header(&r.stdout, "content-length").and_then(|v| v.parse::<u64>().ok()),
        Some(100_000)
    );
    assert!(r.stdout.ends_with("\r\n\r\n"), "HEAD has no body");

    let get = curl(&[&s.url("/static/app.wasm")]);
    assert_eq!(get.stdout.as_bytes(), vec![7u8; 100_000]);
    assert_eq!(
        curl(&[
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-X",
            "POST",
            &s.url("/static/app.wasm")
        ])
        .stdout,
        "405"
    );
    assert_eq!(
        curl(&[
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            &s.url("/static/missing")
        ])
        .stdout,
        "404"
    );
    s.stop();
}

#[test]
fn connection_limit_answers_503() {
    let s = start(Config {
        max_connections: 1,
        ..Config::default()
    });
    let mut held = TcpStream::connect(s.addr.unwrap()).unwrap();
    held.write_all(b"GET /hello HTTP/1.1\r\nHost: x\r\n\r\n")
        .unwrap();
    let mut buf = [0u8; 256];
    let n = held.read(&mut buf).unwrap();
    assert!(buf[..n].starts_with(b"HTTP/1.1 200 "));
    let r = curl(&["-o", "/dev/null", "-w", "%{http_code}", &s.url("/hello")]);
    assert_eq!(r.stdout, "503");
    drop(held);
    let deadline = Instant::now() + Duration::from_secs(5);
    while curl(&["-o", "/dev/null", "-w", "%{http_code}", &s.url("/hello")]).stdout != "200" {
        assert!(Instant::now() < deadline, "slot never freed");
        std::thread::sleep(Duration::from_millis(20));
    }
    s.stop();
}

#[test]
fn graceful_shutdown_finishes_in_flight_requests() {
    let s = start(Config::default());
    let addr = s.addr.unwrap();
    let mut idle = TcpStream::connect(addr).unwrap();
    idle.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let slow = std::thread::spawn(move || curl(&["-i", &format!("http://{addr}/slow")]));
    std::thread::sleep(Duration::from_millis(100));
    let start = Instant::now();
    s.handle.shutdown(Duration::from_secs(5));
    let r = slow.join().unwrap();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.ends_with("\r\n\r\nslow done\n"), "{}", r.stdout);
    assert!(header(&r.stdout, "connection").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("close"))
    }));
    s.thread.join().unwrap().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "idle connection did not hold up shutdown"
    );
    let mut rest = Vec::new();
    assert_eq!(
        idle.read_to_end(&mut rest).unwrap(),
        0,
        "idle connection closed"
    );
    assert!(TcpStream::connect(addr).is_err(), "listener closed");
}

#[test]
fn panicking_handler_frees_its_slot() {
    let s = start(Config {
        max_connections: 2,
        ..Config::default()
    });
    for _ in 0..4 {
        let mut c = TcpStream::connect(s.addr.unwrap()).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(b"GET /panic HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        assert!(common::closed_by_peer(&mut c));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while curl(&["-o", "/dev/null", "-w", "%{http_code}", &s.url("/hello")]).stdout != "200" {
        assert!(
            Instant::now() < deadline,
            "panicked handlers kept their slots"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    s.stop();
}
