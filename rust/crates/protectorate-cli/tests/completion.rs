use std::process::Command;

#[cfg(unix)]
#[test]
fn closed_stdout_pipe_exits_cleanly() {
    use std::fs::File;
    use std::os::fd::FromRawFd;
    use std::process::Stdio;

    for args in [&["--help"][..], &["completion", "bash"][..]] {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let reader = unsafe { File::from_raw_fd(fds[0]) };
        let writer = unsafe { File::from_raw_fd(fds[1]) };
        drop(reader);
        let output = Command::new(env!("CARGO_BIN_EXE_protectorate"))
            .args(args)
            .stdout(Stdio::from(writer))
            .output()
            .expect("run CLI with closed stdout pipe");
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{args:?}: unexpected stderr");
    }
}

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_protectorate"))
        .args(args)
        .output()
        .expect("run CLI")
}

#[test]
fn scripts_use_the_matching_completion_hook() {
    for shell in ["bash", "fish", "zsh", "powershell"] {
        let output = cli(&["completion", shell]);
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let script = String::from_utf8(output.stdout).unwrap();
        assert!(
            script.contains("protectorate __complete "),
            "{shell}: {script}"
        );
        if shell == "zsh" {
            assert!(script.contains("\"${words[@]:1:CURRENT-2}\" \"$PREFIX\""));
            assert!(!script.contains("CURRENT-1"));
        }
        if shell == "fish" {
            assert!(script.contains("commandline -opc"));
            assert!(script.contains("$words[2..-1] (commandline -ct)"));
        }

        let output = cli(&["completion", shell, "--no-descriptions"]);
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("protectorate __completeNoDesc "));
    }
    let invalid = cli(&["completion", "unknown"]);
    assert!(!invalid.status.success());

    let with_global = cli(&["--home", "/tmp/completion-test", "completion", "bash"]);
    assert!(with_global.status.success());
    assert!(String::from_utf8(with_global.stdout)
        .unwrap()
        .contains("protectorate __complete "));
}

#[test]
fn completion_tracks_nested_commands_and_skips_flag_values() {
    let root = String::from_utf8(cli(&["__complete", "m"]).stdout).unwrap();
    assert!(root.contains("msg\tUse the mailbox\n"));
    assert!(!root.contains("node\t"));
    assert!(!root.contains("job\t"));
    let orchestrator =
        String::from_utf8(cli(&["__complete", "orchestrator", "sc"]).stdout).unwrap();
    assert!(
        orchestrator.contains("scopes\tChange Envoy token scopes"),
        "{orchestrator}"
    );
    let scope_flags =
        String::from_utf8(cli(&["__complete", "orchestrator", "scopes", "--a"]).stdout).unwrap();
    assert!(scope_flags.contains("--add\tAdd scope"), "{scope_flags}");

    let restore_flags =
        String::from_utf8(cli(&["__complete", "hub", "restore", "--ho"]).stdout).unwrap();
    assert_eq!(restore_flags.matches("--home\t").count(), 1);

    let plain = String::from_utf8(cli(&["__completeNoDesc", "sleeve", "res"]).stdout).unwrap();
    assert_eq!(plain, "resleeve\n:4\n");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn resource_completion_uses_authenticated_live_names() {
    use protectorate_hub::Hub;
    use protectorate_sleeve_registry::{Harness, Mode, Record};

    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    for name in ["alpha", "alpine", "beta"] {
        let workspace = root.path().join(name);
        std::fs::create_dir_all(&workspace).unwrap();
        hub.sleeves
            .record_spawned(
                Record::new(
                    name,
                    Harness::Pi,
                    Mode::Chat,
                    &workspace,
                    "2026-01-01T00:00:00Z",
                )
                .unwrap(),
            )
            .unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let complete = move |token: &str, words: &[&str]| {
        let mut args = vec!["--hub", url.as_str(), "__complete"];
        args.extend_from_slice(words);
        let output = Command::new(env!("CARGO_BIN_EXE_protectorate"))
            .args(args)
            .env("PROTECTORATE_TOKEN", token)
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let checks = tokio::task::spawn_blocking(move || {
        // Live names are best effort within a short deadline, so a loaded
        // host may need another try; an empty answer each time is the failure.
        let live = |words: &[&str], expected: &str| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            loop {
                let names = complete("owner-secret", words);
                if names == expected {
                    return;
                }
                assert!(
                    names == ":4\n" && std::time::Instant::now() < deadline,
                    "{words:?}: {names}"
                );
            }
        };
        live(
            &["sleeve", "show", "al"],
            "alpha\tSleeve\nalpine\tSleeve\n:4\n",
        );
        live(&["chat", "wait", "be"], "beta\tSleeve\n:4\n");
        assert_eq!(
            complete("owner-secret", &["sleeve", "show", "--timeout", ""]),
            ":4\n"
        );
        assert_eq!(complete("wrong", &["sleeve", "show", "al"]), ":4\n");
        assert!(complete("wrong", &["sleeve", "sh"]).contains("show\tShow a sleeve"));
    })
    .await;
    task.abort();
    checks.unwrap();
}
