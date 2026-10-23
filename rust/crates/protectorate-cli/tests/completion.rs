use std::process::Command;
use std::sync::Arc;

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
        let output = Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
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

use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, UnaryServer,
};
use serde_json::json;

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
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
    let root = String::from_utf8(cli(&["__complete", "no"]).stdout).unwrap();
    assert!(root.contains("node\tManage nodes\n"));
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

    let node = String::from_utf8(
        cli(&["__complete", "--hub", "https://hub.example", "node", "sh"]).stdout,
    )
    .unwrap();
    assert!(node.contains("show\tShow a node\n"), "{node}");
    assert!(!node.contains("shell"));
    let all_node = String::from_utf8(cli(&["__complete", "node", ""]).stdout).unwrap();
    assert!(
        all_node.contains("serve\tRun the node agent\n"),
        "{all_node}"
    );

    let serve_flags =
        String::from_utf8(cli(&["__complete", "node", "serve", "--job"]).stdout).unwrap();
    assert!(
        serve_flags.contains("--job-concurrency\tJobs at once\n"),
        "{serve_flags}"
    );
    let serve_all = String::from_utf8(cli(&["__complete", "node", "serve", "--"]).stdout).unwrap();
    assert!(
        serve_all.contains("--agent-token-file\tNode bearer token file\n"),
        "{serve_all}"
    );
    assert!(
        serve_all.contains("--file-unconfined\tUnrestricted file transfers\n"),
        "{serve_all}"
    );

    let flags = String::from_utf8(cli(&["__complete", "job", "run", "--art"]).stdout).unwrap();
    assert!(flags.contains("--artifact\tLocal source\n"), "{flags}");
    assert!(
        flags.contains("--artifact-glob\tCollect artifacts\n"),
        "{flags}"
    );
    assert!(!flags.contains("--all"));

    let restore_flags =
        String::from_utf8(cli(&["__complete", "hub", "restore", "--ho"]).stdout).unwrap();
    assert_eq!(restore_flags.matches("--home\t").count(), 1);

    let plain = String::from_utf8(cli(&["__completeNoDesc", "sleeve", "res"]).stdout).unwrap();
    assert_eq!(plain, "resleeve\n:4\n");
}

#[test]
fn completion_covers_mcp_and_desk_commands() {
    let mcp = String::from_utf8(cli(&["__complete", "mcp", "--"]).stdout).unwrap();
    for flag in [
        "--list",
        "--allow",
        "--deny",
        "--http",
        "--https",
        "--tls-cert",
        "--tls-key",
    ] {
        assert!(mcp.contains(&format!("{flag}\t")), "{flag}: {mcp}");
    }
    let mcp_after_glob =
        String::from_utf8(cli(&["__complete", "mcp", "--allow", "read_*", "--tls"]).stdout)
            .unwrap();
    assert!(mcp_after_glob.contains("--tls-cert\t"), "{mcp_after_glob}");
    assert!(mcp_after_glob.contains("--tls-key\t"), "{mcp_after_glob}");

    let desk = String::from_utf8(cli(&["__complete", "desk", "po"]).stdout).unwrap();
    assert!(desk.contains("policy\tShow or set policy\n"), "{desk}");
    let actions = String::from_utf8(cli(&["__complete", "desk", "win1", ""]).stdout).unwrap();
    for action in [
        "screenshot",
        "windows",
        "foreground",
        "click",
        "move",
        "type",
        "key",
        "run",
    ] {
        assert!(
            actions.contains(&format!("{action}\t")),
            "{action}: {actions}"
        );
    }
    let screenshot =
        String::from_utf8(cli(&["__complete", "desk", "win1", "screenshot", "--"]).stdout).unwrap();
    for flag in ["--lease", "--output", "--max-width"] {
        assert!(
            screenshot.contains(&format!("{flag}\t")),
            "{flag}: {screenshot}"
        );
    }
    let click =
        String::from_utf8(cli(&["__complete", "desk", "win1", "click", "--"]).stdout).unwrap();
    for flag in ["--lease", "--button", "--double"] {
        assert!(click.contains(&format!("{flag}\t")), "{flag}: {click}");
    }
    assert!(!click.contains("--output\t"), "{click}");
}

#[tokio::test(flavor = "multi_thread")]
async fn resource_completion_uses_authenticated_live_names() {
    let auth: Authorizer = Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .is_some_and(|value| value == "Bearer owner-secret"))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), auth);
    server
        .register(
            "/protectorate.v1.SleeveService/ListSleeves",
            |request| async move {
                Ok(decode_json(
                    request.method.output(),
                    json!({"sleeves":[{"name":"alpha"},{"name":"alpine"},{"name":"beta"}]})
                        .to_string()
                        .as_bytes(),
                )?
                .into())
            },
        )
        .unwrap();
    server
        .register(
            "/protectorate.v1.NodeService/ListNodes",
            |request| async move {
                Ok(decode_json(
                    request.method.output(),
                    json!({"nodes":[{"node":{"name":"compute"}},{"node":{"name":"hub"}}]})
                        .to_string()
                        .as_bytes(),
                )?
                .into())
            },
        )
        .unwrap();
    server
        .register(
            "/protectorate.v1.LeaseService/ListLeases",
            |request| async move {
                Ok(decode_json(
                    request.method.output(),
                    json!({"leases":[
                        {"id":"lease-deadbeef","node":"compute"},
                        {"id":"lease-queued","state":"LEASE_STATE_QUEUED"}
                    ]})
                    .to_string()
                    .as_bytes(),
                )?
                .into())
            },
        )
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });

    let run = |token: &str, words: &[&str]| {
        let mut args = vec!["--hub", url.as_str(), "__complete"];
        args.extend_from_slice(words);
        let output = Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
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
    assert_eq!(
        run("owner-secret", &["sleeve", "show", "al"]),
        "alpha\tSleeve\nalpine\tSleeve\n:4\n"
    );
    assert_eq!(
        run("owner-secret", &["chat", "wait", "be"]),
        "beta\tSleeve\n:4\n"
    );
    assert_eq!(
        run("owner-secret", &["node", "show", "com"]),
        "compute\tNode\n:4\n"
    );
    assert_eq!(run("owner-secret", &["desk", "com"]), "compute\tNode\n:4\n");
    assert_eq!(
        run("owner-secret", &["desk", "policy", "hu"]),
        "hub\tNode\n:4\n"
    );
    assert_eq!(
        run("owner-secret", &["node", "show", "--timeout", ""]),
        ":4\n"
    );
    assert_eq!(
        run("owner-secret", &["job", "run", "--on", "lease-"]),
        "lease-deadbeef\tLease\n:4\n"
    );
    assert_eq!(
        run("owner-secret", &["job", "test-cross", "--on=co"]),
        "--on=compute\tNode\n:4\n"
    );
    assert_eq!(
        run("owner-secret", &["lease", "acquire", "--node", "com"]),
        "compute\tNode\n:4\n"
    );
    assert_eq!(
        run("owner-secret", &["lease", "acquire", "--node=hu"]),
        "--node=hub\tNode\n:4\n"
    );
    assert_eq!(run("wrong", &["node", "show", "com"]), ":4\n");
    assert!(run("wrong", &["node", "sh"]).contains("show\tShow a node"));
    task.abort();
}

#[test]
fn completion_covers_service_termux() {
    let actions = String::from_utf8(cli(&["__complete", "service", ""]).stdout).unwrap();
    assert!(
        actions.contains("termux\tTermux run and boot scripts\n"),
        "{actions}"
    );

    let write = String::from_utf8(cli(&["__complete", "service", "termux", "--w"]).stdout).unwrap();
    assert!(write.contains("--write\tOutput path\n"), "{write}");
    assert!(!write.contains("--trigger"), "{write}");

    let install =
        String::from_utf8(cli(&["__complete", "service", "termux", "--in"]).stdout).unwrap();
    assert!(
        install.contains("--install\tInstall into Termux\n"),
        "{install}"
    );

    let all = String::from_utf8(cli(&["__complete", "service", "termux", "--"]).stdout).unwrap();
    for flag in ["--binary", "--home", "--write", "--install"] {
        assert!(all.contains(flag), "{flag} missing from {all}");
    }
}
