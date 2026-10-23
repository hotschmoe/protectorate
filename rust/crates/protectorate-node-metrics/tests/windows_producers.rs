use protectorate_node_metrics::{accel::ToolRunner, windows, Probe};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

struct NvidiaOutput(Option<String>);

impl ToolRunner for NvidiaOutput {
    fn run(&self, name: &str, args: &[&str]) -> Option<String> {
        assert_eq!(name, "nvidia-smi");
        assert_eq!(args.len(), 2);
        assert!(args[0].contains("pci.bus_id"));
        self.0.clone()
    }
}

#[test]
fn windows_nvidia_gauges_use_bounded_vendor_output() {
    let cards = windows::nvidia_gpus(&NvidiaOutput(Some(
        "0, RTX 6000, 83, 100, 200, 71, 00000000:03:00.0, 565.57\n".into(),
    )));
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].vendor, "nvidia");
    assert_eq!(cards[0].utilization_percent, 83.0);
    assert_eq!(cards[0].memory_used_bytes, Some(100 << 20));
    assert_eq!(cards[0].memory_total_bytes, 200 << 20);
    assert!(windows::nvidia_gpus(&NvidiaOutput(None)).is_empty());
    assert!(windows::nvidia_gpus(&NvidiaOutput(Some("garbage".into()))).is_empty());
}

#[test]
fn configured_windows_docker_tcp_returns_engine_and_containers() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for (expected, body) in [
            (
                "/version",
                r#"{"Version":"27.0","ApiVersion":"1.46","MinAPIVersion":"1.24"}"#,
            ),
            (
                "/v1.43/containers/json?all=1",
                r#"[{"Id":"abcdef0123456789","Names":["/worker"],"Image":"busybox","State":"exited","Created":1}]"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let mut read = 0;
            while !request[..read].windows(2).any(|bytes| bytes == b"\r\n") {
                let count = stream.read(&mut request[read..]).unwrap();
                assert!(count > 0 && read + count < request.len());
                read += count;
            }
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(
                request.starts_with(&format!("GET {expected} HTTP/1.1")),
                "{request}"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let host = format!("tcp://{address}");
    let engine = windows::WindowsProbe
        .engine(Some(&host), false)
        .unwrap()
        .unwrap();
    server.join().unwrap();
    assert_eq!(engine.kind, "docker");
    assert_eq!(engine.version, "27.0");
    assert_eq!(engine.containers.len(), 1);
    assert_eq!(engine.containers[0].id, "abcdef012345");
    assert_eq!(engine.containers[0].name, "worker");
    assert_eq!(engine.containers[0].state, "exited");
    assert!(engine.disk.is_none());
}

#[test]
#[cfg(not(windows))]
fn unsupported_windows_docker_transport_is_explicit() {
    let error = windows::WindowsProbe
        .engine(Some("npipe:////./pipe/docker_engine"), false)
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}
