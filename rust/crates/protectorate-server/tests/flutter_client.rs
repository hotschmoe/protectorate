//! Exercises the checked-in Flutter Connect client against the real Rust hub.
//! Dart is optional for Rust-only build hosts; CI with Flutter runs this lane.

#[cfg(unix)]
use crate::support;

#[cfg(unix)]
mod unix {
    use super::support::{bound, free_port, kill_tmux, until};
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, process::CommandExt},
        path::{Path, PathBuf},
        process::{Child, Command, ExitStatus},
        thread,
        time::{Duration, Instant},
    };

    #[cfg(target_os = "linux")]
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A process group of its own; dropping it stops the whole tree, so a
    /// guardian's hub child or `flutter`'s tester cannot outlive the test.
    /// If the test binary is killed before the drop runs, the kernel sends
    /// the leader SIGTERM, and `flutter` and the hub guardian stop their
    /// children on it.
    struct Group(Child);

    impl Group {
        fn spawn(command: &mut Command) -> Self {
            // SAFETY: prctl is async-signal-safe and touches no memory.
            unsafe {
                command.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            Self(command.process_group(0).spawn().unwrap())
        }

        fn alive(&mut self) -> bool {
            self.0.try_wait().unwrap().is_none()
        }

        fn wait(&mut self, limit: Duration) -> Option<ExitStatus> {
            let deadline = Instant::now() + limit;
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    return Some(status);
                }
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    impl Drop for Group {
        fn drop(&mut self) {
            let group = -(self.0.id() as i32);
            unsafe { libc::kill(group, libc::SIGTERM) };
            if self.wait(Duration::from_secs(15)).is_none() {
                let _ = self.0.kill();
            }
            unsafe { libc::kill(group, libc::SIGKILL) };
            let _ = self.0.wait();
        }
    }

    /// A configured hub as a service runs it, under its update guardian.
    struct Hub {
        group: Group,
        tmux: String,
        log: PathBuf,
    }

    impl Hub {
        fn start(root: &Path, token: &str, extra: &[&str]) -> Self {
            let home = root.join("hub");
            fs::create_dir(&home).unwrap();
            let token_file = root.join("owner-token");
            fs::write(&token_file, token).unwrap();
            fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
            let tmux = format!(
                "p-rust-flutter-{}-{}",
                std::process::id(),
                root.file_name().unwrap().to_string_lossy()
            );
            let log = root.join("hub.log");
            let output = fs::File::create(&log).unwrap();
            let group = Group::spawn(
                Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
                    .arg("serve")
                    .arg("--home")
                    .arg(&home)
                    .arg("--owner-token-file")
                    .arg(&token_file)
                    .args(["--listen", "127.0.0.1:0"])
                    .args(["--release-api-url", "http://127.0.0.1:1"])
                    .args(["--tmux-socket", &tmux])
                    .args(extra)
                    .env("HOME", root)
                    .env_remove("PROTECTORATE_TOKEN")
                    .env_remove("PROTECTORATE_TOKEN_FILE")
                    .env_remove("PROTECTORATE_HUB")
                    .stdout(output.try_clone().unwrap())
                    .stderr(output),
            );
            let mut hub = Self { group, tmux, log };
            let ready = if extra.contains(&"--tls-listen") {
                "serving TLS"
            } else {
                "serving"
            };
            until("hub start", &hub.log.clone(), || {
                assert!(hub.group.alive(), "the hub exited: {}", hub.output());
                bound(&hub.log, ready).is_some()
            });
            hub
        }

        fn url(&self) -> String {
            format!("http://{}", bound(&self.log, "serving").unwrap())
        }

        fn tls_url(&self) -> String {
            format!("https://{}", bound(&self.log, "serving TLS").unwrap())
        }

        fn output(&self) -> String {
            fs::read_to_string(&self.log).unwrap_or_default()
        }
    }

    impl Drop for Hub {
        fn drop(&mut self) {
            kill_tmux(&self.tmux);
        }
    }

    /// Runs a Dart or Flutter scenario to completion. Its output goes to files,
    /// so a chatty run cannot block on a full pipe, and the deadline allows
    /// for compiling the scenario on a loaded host.
    fn scenario(hub: &Hub, root: &Path, name: &str, command: &mut Command, limit: Duration) {
        let stdout = root.join(format!("{name}.stdout"));
        let stderr = root.join(format!("{name}.stderr"));
        command
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let mut run = Group::spawn(command);
        let status = run.wait(limit);
        let report = || {
            format!(
                "stdout={} stderr={} hub={}",
                fs::read_to_string(&stdout).unwrap_or_default(),
                fs::read_to_string(&stderr).unwrap_or_default(),
                hub.output()
            )
        };
        match status {
            Some(status) => assert!(status.success(), "{name} failed: {}", report()),
            None => {
                drop(run);
                panic!("{name} timed out: {}", report());
            }
        }
    }

    fn app() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../app")
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
        let app = app();
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
        let token = "rust-flutter-node-invite-e2e-token";
        let hub = Hub::start(
            &root,
            token,
            &["--tls-listen", "127.0.0.1:0", "--node-interval", "1s"],
        );
        let bin = env!("CARGO_BIN_EXE_protectorate-rs");
        scenario(
            &hub,
            &root,
            "node_invite_ui_e2e",
            Command::new("flutter")
                .args(["test", "--no-pub", "--reporter", "expanded"])
                .arg(format!(
                    "--dart-define=PROTECTORATE_E2E_HUB_URL={}",
                    hub.url()
                ))
                .arg(format!("--dart-define=PROTECTORATE_E2E_TOKEN={token}"))
                .arg(format!("--dart-define=PROTECTORATE_E2E_NODE_BINARY={bin}"))
                .arg(format!(
                    "--dart-define=PROTECTORATE_E2E_NODE_HOME={}",
                    root.join("node").display()
                ))
                .arg(format!(
                    "--dart-define=PROTECTORATE_E2E_TLS_URL={}",
                    hub.tls_url()
                ))
                .arg(format!(
                    "--dart-define=PROTECTORATE_E2E_CA_CERT={}",
                    root.join("hub/ca/ca.pem").display()
                ))
                .arg(format!(
                    "--dart-define=PROTECTORATE_E2E_NODE_LISTEN={}",
                    free_port()
                ))
                .arg("tool/node_invite_ui_e2e.dart")
                .current_dir(&app),
            Duration::from_secs(600),
        );
    }

    /// The app directory when the toolchain for `tool` and the package config
    /// are present; `None` skips the trial unless Flutter is required.
    fn app_for(tool: &str, trial: &str) -> Option<PathBuf> {
        let required = std::env::var("PROTECTORATE_REQUIRE_FLUTTER_TEST").as_deref() == Ok("1");
        if Command::new(tool).arg("--version").output().is_err() {
            assert!(!required, "{tool} is required for the {trial}");
            eprintln!("skipping {trial}: {tool} is unavailable");
            return None;
        }
        let app = app();
        if !app.join(".dart_tool/package_config.json").is_file() {
            assert!(
                !required,
                "Flutter package config is required: run make app-check first"
            );
            eprintln!("skipping {trial}: run make app-check first");
            return None;
        }
        Some(app)
    }

    /// Runs each Dart script against one fresh hub; each test has a hub of
    /// its own so the trials run side by side.
    fn dart_scripts(trial: &str, names: &[&str]) {
        let Some(app) = app_for("dart", trial) else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let token = "rust-flutter-real-binary-e2e-token";
        let hub = Hub::start(root, token, &["--device-interval", "1s"]);
        let url = hub.url();
        for name in names {
            scenario(
                &hub,
                root,
                name,
                Command::new("dart")
                    .arg("--packages=.dart_tool/package_config.json")
                    .arg(format!("tool/{name}.dart"))
                    .arg(&url)
                    .arg(token)
                    .current_dir(&app),
                Duration::from_secs(300),
            );
        }
    }

    fn flutter_test(trial: &str, name: &str) {
        let Some(app) = app_for("flutter", trial) else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let token = "rust-flutter-real-binary-e2e-token";
        let hub = Hub::start(root, token, &[]);
        let url = hub.url();
        scenario(
            &hub,
            root,
            name,
            Command::new("flutter")
                .arg("test")
                .arg("--no-pub")
                .arg(format!("--dart-define=PROTECTORATE_E2E_HUB_URL={url}"))
                .arg(format!("--dart-define=PROTECTORATE_E2E_TOKEN={token}"))
                .arg(format!("tool/{name}.dart"))
                .current_dir(&app),
            Duration::from_secs(600),
        );
    }

    #[test]
    fn checked_in_dart_client_pairs_and_signs_against_rust_hub() {
        dart_scripts("Dart hub connection trial", &["hub_connection_e2e"]);
    }

    #[test]
    fn checked_in_dart_client_runs_backlog_and_sessions_against_rust_hub() {
        dart_scripts(
            "Dart backlog and session trial",
            &["backlog_e2e", "session_e2e"],
        );
    }

    #[test]
    fn checked_in_flutter_note_draft_autosaves_to_rust_hub() {
        flutter_test("NoteDraft trial", "note_draft_e2e");
    }

    #[test]
    fn checked_in_flutter_nodes_tab_renders_rust_inventory() {
        flutter_test("Nodes tab trial", "nodes_ui_e2e");
    }
}
