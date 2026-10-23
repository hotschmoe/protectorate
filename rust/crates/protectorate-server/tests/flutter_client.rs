//! Exercises the checked-in Flutter Connect client against the real Rust hub.
//! Dart is optional for Rust-only build hosts; CI with Flutter runs this lane.

#[cfg(unix)]
mod unix {
    use protectorate_connect_client::Client;
    use serde_json::json;
    use std::{
        fs,
        net::TcpListener,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    #[cfg(target_os = "linux")]
    use std::{
        os::unix::process::CommandExt,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Hub(Child, String);

    impl Drop for Hub {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = Command::new("tmux")
                .args(["-L", &self.1, "kill-server"])
                .output();
        }
    }

    #[cfg(target_os = "linux")]
    struct ConfiguredHub(Child, String);

    #[cfg(target_os = "linux")]
    impl Drop for ConfiguredHub {
        fn drop(&mut self) {
            let group = -(self.0.id() as i32);
            unsafe { libc::kill(group, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline {
                if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            unsafe { libc::kill(group, libc::SIGKILL) };
            let _ = self.0.wait();
            let _ = Command::new("tmux")
                .args(["-L", &self.1, "kill-server"])
                .output();
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn checked_in_flutter_node_invite_reaches_ready() {
        let required = std::env::var("PROTECTORATE_REQUIRE_FLUTTER_TEST").as_deref() == Ok("1");
        if Command::new("flutter").arg("--version").output().is_err() {
            assert!(
                !required,
                "flutter is required for the node invitation trial"
            );
            eprintln!("skipping node invitation trial: flutter is unavailable");
            return;
        }
        let app = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../app");
        assert!(
            app.join(".dart_tool/package_config.json").is_file(),
            "Flutter package config is required: run make app-check first"
        );
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "protectorate-flutter-invite-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        eprintln!("retained node invitation fixture: {}", root.display());
        let home = root.join("hub");
        fs::create_dir(&home).unwrap();
        let token_file = root.join("owner-token");
        let token = "rust-flutter-node-invite-e2e-token";
        fs::write(&token_file, token).unwrap();
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();

        let reservations = (0..3)
            .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
            .collect::<Vec<_>>();
        let addresses = reservations
            .iter()
            .map(|listener| listener.local_addr().unwrap())
            .collect::<Vec<_>>();
        let [hub_addr, tls_addr, node_addr] = <[_; 3]>::try_from(addresses).unwrap();
        drop(reservations);
        let bin = env!("CARGO_BIN_EXE_protectorate-rs");
        let tmux_socket = format!("p-rust-flutter-invite-{}", std::process::id());
        let log = fs::File::create(root.join("hub.log")).unwrap();
        let server = Command::new(bin)
            .arg("serve")
            .arg("--home")
            .arg(&home)
            .arg("--owner-token-file")
            .arg(&token_file)
            .arg("--listen")
            .arg(hub_addr.to_string())
            .arg("--tls-listen")
            .arg(tls_addr.to_string())
            .arg("--node-interval")
            .arg("1s")
            .arg("--release-api-url")
            .arg("http://127.0.0.1:1")
            .arg("--tmux-socket")
            .arg(&tmux_socket)
            .env("HOME", &root)
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .env_remove("PROTECTORATE_HUB")
            .process_group(0)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut hub = ConfiguredHub(server, tmux_socket);
        let url = format!("http://{hub_addr}");
        let client = Client::new(&url, token, Duration::from_millis(300)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .is_ok()
            {
                break;
            }
            assert!(
                hub.0.try_wait().unwrap().is_none(),
                "configured Rust hub exited before ready; see {}",
                root.join("hub.log").display()
            );
            assert!(
                Instant::now() < deadline,
                "configured Rust hub was not ready"
            );
            thread::sleep(Duration::from_millis(100));
        }

        let mut flutter = Command::new("flutter")
            .args(["test", "--no-pub", "--reporter", "expanded"])
            .arg(format!("--dart-define=PROTECTORATE_E2E_HUB_URL={url}"))
            .arg(format!("--dart-define=PROTECTORATE_E2E_TOKEN={token}"))
            .arg(format!("--dart-define=PROTECTORATE_E2E_NODE_BINARY={bin}"))
            .arg(format!(
                "--dart-define=PROTECTORATE_E2E_NODE_HOME={}",
                root.join("node").display()
            ))
            .arg(format!(
                "--dart-define=PROTECTORATE_E2E_TLS_URL=https://{tls_addr}"
            ))
            .arg(format!(
                "--dart-define=PROTECTORATE_E2E_CA_CERT={}",
                home.join("ca/ca.pem").display()
            ))
            .arg(format!(
                "--dart-define=PROTECTORATE_E2E_NODE_LISTEN={node_addr}"
            ))
            .arg("tool/node_invite_ui_e2e.dart")
            .current_dir(&app)
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let flutter_group = -(flutter.id() as i32);
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if flutter.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                unsafe { libc::kill(flutter_group, libc::SIGTERM) };
                thread::sleep(Duration::from_secs(1));
                unsafe { libc::kill(flutter_group, libc::SIGKILL) };
                let output = flutter.wait_with_output().unwrap();
                panic!(
                    "node invitation trial timed out; fixture {}: stdout={} stderr={}",
                    root.display(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        let output = flutter.wait_with_output().unwrap();
        unsafe { libc::kill(flutter_group, libc::SIGTERM) };
        assert!(
            output.status.success(),
            "node invitation trial failed; fixture {}: stdout={} stderr={}",
            root.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn checked_in_flutter_client_connects_to_rust_hub() {
        let require_flutter =
            std::env::var("PROTECTORATE_REQUIRE_FLUTTER_TEST").as_deref() == Ok("1");
        if Command::new("dart").arg("--version").output().is_err() {
            assert!(
                !require_flutter,
                "dart is required for the Flutter client trial"
            );
            eprintln!("skipping Flutter client trial: dart is unavailable");
            return;
        }
        let app = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../app");
        if !app.join(".dart_tool/package_config.json").is_file() {
            assert!(
                !require_flutter,
                "Flutter package config is required: run make app-check first"
            );
            eprintln!("skipping Flutter client trial: run make app-check first");
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("hub");
        fs::create_dir(&home).unwrap();
        let token_file = root.path().join("owner-token");
        let token = "rust-flutter-real-binary-e2e-token";
        fs::write(&token_file, token).unwrap();
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();

        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let listen = reservation.local_addr().unwrap();
        drop(reservation);
        let url = format!("http://{listen}");
        let tmux_socket = format!("p-rust-flutter-{}", std::process::id());
        let server = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
            .arg("serve")
            .arg("--home")
            .arg(&home)
            .arg("--owner-token-file")
            .arg(&token_file)
            .arg("--listen")
            .arg(listen.to_string())
            .arg("--release-api-url")
            .arg("http://127.0.0.1:1")
            .arg("--tmux-socket")
            .arg(&tmux_socket)
            .env("HOME", root.path())
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .env_remove("PROTECTORATE_HUB")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut hub = Hub(server, tmux_socket);
        let client = Client::new(&url, token, Duration::from_millis(300)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .is_ok()
            {
                break;
            }
            assert!(
                hub.0.try_wait().unwrap().is_none(),
                "Rust hub exited before ready"
            );
            assert!(
                Instant::now() < deadline,
                "Rust hub was not ready in 15 seconds"
            );
            thread::sleep(Duration::from_millis(100));
        }

        run_dart_scenario(&app, "tool/hub_connection_e2e.dart", &url, token);
        run_dart_scenario(&app, "tool/backlog_e2e.dart", &url, token);
        run_dart_scenario(&app, "tool/session_e2e.dart", &url, token);
        if Command::new("flutter").arg("--version").output().is_ok() {
            run_flutter_scenario(&app, "tool/note_draft_e2e.dart", &url, token);
            run_flutter_scenario(&app, "tool/nodes_ui_e2e.dart", &url, token);
        } else {
            assert!(
                !require_flutter,
                "flutter is required for the NoteDraft integration test"
            );
            eprintln!("skipping NoteDraft trial: flutter is unavailable");
        }
    }

    fn run_dart_scenario(app: &std::path::Path, scenario: &str, url: &str, token: &str) {
        let mut dart = Command::new("dart")
            .arg("--packages=.dart_tool/package_config.json")
            .arg(scenario)
            .arg(url)
            .arg(token)
            .current_dir(app)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(75);
        loop {
            if dart.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = dart.kill();
                let output = dart.wait_with_output().unwrap();
                panic!(
                    "Flutter scenario {scenario} timed out: stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        let output = dart.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "Flutter scenario {scenario} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_flutter_scenario(app: &std::path::Path, scenario: &str, url: &str, token: &str) {
        let mut flutter = Command::new("flutter")
            .arg("test")
            .arg("--no-pub")
            .arg(format!("--dart-define=PROTECTORATE_E2E_HUB_URL={url}"))
            .arg(format!("--dart-define=PROTECTORATE_E2E_TOKEN={token}"))
            .arg(scenario)
            .current_dir(app)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if flutter.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = flutter.kill();
                let output = flutter.wait_with_output().unwrap();
                panic!(
                    "Flutter scenario {scenario} timed out: stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        let output = flutter.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "Flutter scenario {scenario} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
