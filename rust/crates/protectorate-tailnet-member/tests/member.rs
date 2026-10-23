use protectorate_tailnet_member::{Error, MachinePublic, MemberIdentity, CAPABILITY_VERSION};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

const KEY_A: &str = "mkey:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const KEY_B: &str = "mkey:1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn control(responses: Vec<(u16, &'static str)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        for (status, key) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 2048];
            let n = socket.read(&mut request).unwrap();
            let line = format!("GET /key?v={CAPABILITY_VERSION} HTTP/1.1");
            assert!(String::from_utf8_lossy(&request[..n]).starts_with(&line));
            let body = format!(r#"{{"publicKey":"{key}"}}"#);
            let reply = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).unwrap();
        }
    });
    (origin, handle)
}

#[test]
fn stable_machine_key_and_control_key_pin() {
    let dir = tempfile::tempdir().unwrap();
    let identity = MemberIdentity::open(dir.path()).unwrap();
    let public = identity.machine_public_key();
    let node_public = identity.node_public_key();
    assert_ne!(public.bytes(), [0; 32]);
    assert!(node_public.starts_with("nodekey:"));
    assert_eq!(
        node_public,
        MemberIdentity::open(dir.path()).unwrap().node_public_key()
    );
    assert_eq!(
        public,
        MemberIdentity::open(dir.path())
            .unwrap()
            .machine_public_key()
    );

    let (origin, server) = control(vec![(200, KEY_A), (200, KEY_A), (200, KEY_B)]);
    let first = identity
        .discover_control(&origin, Duration::from_secs(2))
        .unwrap();
    assert!(first.newly_pinned);
    assert_eq!(first.public_key, MachinePublic::parse(KEY_A).unwrap());
    let second = identity
        .discover_control(&origin, Duration::from_secs(2))
        .unwrap();
    assert!(!second.newly_pinned);
    assert!(matches!(
        identity.discover_control(&origin, Duration::from_secs(2)),
        Err(Error::ControlKeyChanged { .. })
    ));
    server.join().unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("machine.key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(dir.path().join("node.key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(dir.path().join("control-key.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn refuses_bad_response_without_pin() {
    let dir = tempfile::tempdir().unwrap();
    let identity = MemberIdentity::open(dir.path()).unwrap();
    let (origin, server) = control(vec![(503, KEY_A)]);
    assert!(matches!(
        identity.discover_control(&origin, Duration::from_secs(2)),
        Err(Error::Status(503))
    ));
    server.join().unwrap();
    assert!(!dir.path().join("control-key.json").exists());
}

#[test]
fn rejects_invalid_origins_and_keys() {
    let dir = tempfile::tempdir().unwrap();
    let identity = MemberIdentity::open(dir.path()).unwrap();
    for bad in [
        "http://host/path",
        "http://host/",
        "file:///tmp/x",
        "https://user@host",
        "http://host#fragment",
    ] {
        assert!(matches!(
            identity.discover_control(bad, Duration::from_secs(1)),
            Err(Error::Invalid(_))
        ));
    }
    assert!(MachinePublic::parse(
        "mkey:0000000000000000000000000000000000000000000000000000000000000000"
    )
    .is_err());
    assert!(MachinePublic::parse("0123456789abcdef").is_err());
}

#[test]
fn machine_public_matches_rfc_7748_vector() {
    // RFC 7748, section 6.1: Alice's private and public X25519 keys.
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("machine.key");
    std::fs::write(
        &key_path,
        "privkey:77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let identity = MemberIdentity::open(dir.path()).unwrap();
    assert_eq!(
        identity.machine_public_key().to_string(),
        "mkey:8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
    );
}

#[cfg(unix)]
#[test]
fn refuses_exposed_existing_machine_key() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("machine.key");
    std::fs::write(
        &key_path,
        "privkey:77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a\n",
    )
    .unwrap();
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        MemberIdentity::open(dir.path()),
        Err(Error::Invalid(_))
    ));
}

/// Run with `PROT_HEADSCALE_BINARY=/path/to/headscale cargo test -p
/// protectorate-tailnet-member --test member real_headscale_noise_upgrade
/// -- --ignored` to verify the wire protocol against Headscale itself.
#[test]
#[ignore = "requires a local Headscale 0.29.5 binary"]
fn real_headscale_noise_upgrade() {
    real_headscale_noise_upgrade_inner(false);
}

#[test]
#[ignore = "requires a local Headscale 0.29.5 binary"]
fn real_headscale_https_noise_upgrade() {
    real_headscale_noise_upgrade_inner(true);
}

fn real_headscale_noise_upgrade_inner(https: bool) {
    use protectorate_tailnet::Config;
    use std::{
        process::{Child, Command, Stdio},
        time::Instant,
    };

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn free_addr() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().to_string()
    }

    let binary = std::env::var("PROT_HEADSCALE_BINARY")
        .expect("set PROT_HEADSCALE_BINARY to a Headscale 0.29.5 executable");
    let home = tempfile::tempdir().unwrap();
    let listen = free_addr();
    let origin = if https {
        format!("https://localhost:{}", listen.rsplit_once(':').unwrap().1)
    } else {
        format!("http://{listen}")
    };
    let mut config = Config::for_server(&origin);
    config.listen = listen;
    config.metrics_listen = free_addr();
    config.grpc_listen = free_addr();
    config.embedded_derp = false;
    config.public_derp = true;
    let ca_pem = if https {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_path = home.path().join("control-cert.pem");
        let key_path = home.path().join("control-key.pem");
        let pem = cert.cert.pem();
        std::fs::write(&cert_path, &pem).unwrap();
        std::fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
        config.tls_cert = Some(cert_path);
        config.tls_key = Some(key_path);
        Some(pem)
    } else {
        None
    };
    config.write_files(home.path(), None).unwrap();

    let log_path = home.path().join("headscale.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let child = Command::new(binary)
        .arg("serve")
        .arg("-c")
        .arg(home.path().join("tailnet/headscale.yaml"))
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
            "Headscale startup timed out: {}",
            std::fs::read_to_string(&log_path).unwrap()
        );
        thread::sleep(Duration::from_millis(50));
    }
    let identity = MemberIdentity::open(home.path().join("member")).unwrap();
    if let Some(ref ca_pem) = ca_pem {
        assert!(
            matches!(
                identity.discover_control(&origin, Duration::from_secs(5)),
                Err(Error::Transport(_))
            ),
            "self-signed certificate must fail public-root validation"
        );
        let wrong_name = format!("https://{}", config.listen);
        assert!(
            matches!(
                identity.discover_control_with_ca_pem(
                    &wrong_name,
                    Duration::from_secs(5),
                    Some(ca_pem.as_bytes()),
                ),
                Err(Error::Transport(_))
            ),
            "trusted certificate must still fail hostname validation"
        );
        assert!(!home.path().join("member/control-key.json").exists());
    }
    let mut channel = identity
        .connect_control_with_ca_pem(
            &origin,
            Duration::from_secs(5),
            ca_pem.as_deref().map(str::as_bytes),
        )
        .unwrap_or_else(|e| {
            panic!(
                "Noise upgrade: {e}; Headscale log: {}",
                std::fs::read_to_string(&log_path).unwrap()
            )
        });
    assert_ne!(channel.handshake_hash(), [0; 32]);
    // Exercise encrypted records in both directions with HTTP/2 startup.
    channel
        .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\0\0\0\x04\0\0\0\0\0")
        .unwrap();
    let mut first = [0; 9];
    channel.read_exact(&mut first).unwrap();
    if &first[..5] == b"\xff\xff\xffTS" {
        let length = u32::from_be_bytes(first[5..9].try_into().unwrap()) as usize;
        assert!(length <= 1 << 20, "oversized EarlyNoise payload");
        let mut payload = vec![0; length];
        channel.read_exact(&mut payload).unwrap();
        serde_json::from_slice::<serde_json::Value>(&payload).unwrap();
        channel.read_exact(&mut first).unwrap();
    }
    assert_eq!(first[3], 4, "expected server HTTP/2 SETTINGS frame");
    assert_eq!(&first[5..9], &[0, 0, 0, 0]);
    let settings_length = u32::from_be_bytes([0, first[0], first[1], first[2]]) as usize;
    assert!(settings_length <= 64 * 1024);
    let mut settings = vec![0; settings_length];
    channel.read_exact(&mut settings).unwrap();

    // A second client record checks that the transport nonce advances in the
    // byte order Headscale expects. The server must acknowledge our PING.
    channel
        .write_all(b"\0\0\0\x04\x01\0\0\0\0\0\0\x08\x06\0\0\0\0\0pingpong")
        .unwrap();
    let mut ping_acknowledged = false;
    for _ in 0..16 {
        let mut header = [0; 9];
        channel.read_exact(&mut header).unwrap();
        let length = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
        assert!(length <= 64 * 1024);
        let mut payload = vec![0; length];
        channel.read_exact(&mut payload).unwrap();
        if header[3] == 6 && header[4] == 1 && payload == b"pingpong" {
            ping_acknowledged = true;
            break;
        }
    }
    assert!(
        ping_acknowledged,
        "Headscale did not acknowledge HTTP/2 PING"
    );
}

#[test]
#[ignore = "requires a local Headscale 0.29.5 binary"]
fn real_headscale_registration() {
    use protectorate_tailnet::Config;
    use std::{
        process::{Child, Command, Stdio},
        time::Instant,
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
            "Headscale startup timed out: {}",
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
            "headscale {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let user = cli(&["users", "create", "protectorate"]);
    let id = user["id"]
        .as_u64()
        .or_else(|| user["id"].as_str().and_then(|s| s.parse().ok()))
        .unwrap();
    let key = cli(&["preauthkeys", "create", "-u", &id.to_string()]);
    let secret = key["key"].as_str().unwrap();
    let identity = MemberIdentity::open(home.path().join("member")).unwrap();
    let registration = identity
        .register_http(&origin, secret, "rust-test", Duration::from_secs(5))
        .unwrap_or_else(|e| {
            panic!(
                "registration: {e}; log: {}",
                std::fs::read_to_string(&log_path).unwrap()
            )
        });
    assert!(registration.machine_authorized);
    let nodes = cli(&["nodes", "list"]);
    let nodes = nodes.as_array().unwrap();
    assert!(
        nodes.iter().any(|node| node["name"]
            .as_str()
            .unwrap_or_default()
            .contains("rust-test")),
        "registered node missing: {nodes:?}"
    );
}
