//! Exercises the checked-in Flutter Connect client against the real Rust hub.
//! Dart is optional for Rust-only build hosts; CI with Flutter runs this lane.

#[cfg(unix)]
use crate::support;

#[cfg(unix)]
mod unix {
    use super::support::{bound, kill_tmux, until, Group};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
        sync::Mutex,
        thread,
        time::{Duration, Instant},
    };

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
    /// `flutter test` copies its assets into app/build/unit_test_assets before
    /// it prints that it is loading the test, and two of them copying at once
    /// crash the tool. Starts are taken one at a time; the tests then run
    /// side by side.
    static FLUTTER_STARTUP: Mutex<()> = Mutex::new(());

    fn scenario(
        hub: &Hub,
        root: &Path,
        name: &str,
        command: &mut Command,
        limit: Duration,
        flutter: bool,
    ) {
        let stdout = root.join(format!("{name}.stdout"));
        let stderr = root.join(format!("{name}.stderr"));
        command
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let starting = flutter.then(|| FLUTTER_STARTUP.lock().unwrap_or_else(|e| e.into_inner()));
        let mut run = Group::spawn(command);
        if starting.is_some() {
            let deadline = Instant::now() + limit;
            while run.alive()
                && Instant::now() < deadline
                && !fs::read_to_string(&stdout).is_ok_and(|out| out.contains("loading "))
            {
                thread::sleep(Duration::from_millis(50));
            }
        }
        drop(starting);
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
                false,
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
            true,
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
}
