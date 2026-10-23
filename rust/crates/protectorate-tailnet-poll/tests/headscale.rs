//! Optional wire test against a disposable Headscale process.
use protectorate_tailnet::Config;
use protectorate_tailnet_discovery::DiscoIdentity;
use protectorate_tailnet_member::MemberIdentity;
use protectorate_tailnet_poll::Poller;
use std::{
    fs,
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
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

#[test]
#[ignore = "requires PROT_HEADSCALE_BINARY"]
fn registered_member_poll_clears_map_on_disconnect() {
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
    let log = fs::File::create(&log_path).unwrap();
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
                fs::read_to_string(&log_path).unwrap()
            );
        }
        if TcpStream::connect(&config.listen).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Headscale timed out: {}",
            fs::read_to_string(&log_path).unwrap()
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
            "headscale {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let user = cli(&["users", "create", "protectorate"]);
    let id = user["id"]
        .as_u64()
        .or_else(|| user["id"].as_str().and_then(|s| s.parse().ok()))
        .unwrap();
    let auth_key = cli(&["preauthkeys", "create", "-u", &id.to_string()]);
    let member_dir = home.path().join("tsnet");
    let member = MemberIdentity::open(&member_dir).unwrap();
    let disco = DiscoIdentity::open(&member_dir).unwrap();
    member
        .register_http(
            &origin,
            auth_key["key"].as_str().unwrap(),
            "rust-poll",
            Duration::from_secs(5),
        )
        .unwrap();
    let joined = serde_json::json!({"controlUrl": origin, "nodeKey": member.node_public_key(), "discoKey": disco.public_key().to_string()});
    let path = member_dir.join("join.json");
    fs::write(&path, serde_json::to_vec(&joined).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let poller = Poller::open(&member_dir)
        .unwrap()
        .with_timeout(Duration::from_secs(3))
        .unwrap();
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        let task = scope.spawn(|| poller.poll_once(&stop));
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(map) = poller.snapshot() {
                assert_eq!(map.self_node.unwrap().key, member.node_public_key());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no map: {}",
                fs::read_to_string(&log_path).unwrap()
            );
            thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::Release);
        let _ = task.join().unwrap();
    });
    assert!(poller.snapshot().is_none());
}
