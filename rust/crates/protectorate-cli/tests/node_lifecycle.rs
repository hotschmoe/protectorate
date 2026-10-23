use protectorate_cli::{dispatch_endpoint, parse, Command};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, UnaryServer,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn node_lifecycle_dispatches_real_connect_calls() {
    let catalog = Catalog::embedded().unwrap();
    let auth: Authorizer = Arc::new(|_, headers| {
        (headers
            .get("authorization")
            .is_some_and(|v| v == "Bearer owner-secret"))
        .then_some(AuthContext::default())
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "bad token"))
    });
    let mut server = UnaryServer::new(catalog, auth);
    let seen = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    for (method, output) in [
        (
            "/protectorate.v1.NodeService/AddNode",
            json!({"name":"compute","address":"compute","tier":"care"}),
        ),
        (
            "/protectorate.v1.NodeService/SetNodeTier",
            json!({"name":"compute","tier":"shared"}),
        ),
    ] {
        let seen = Arc::clone(&seen);
        server
            .register(method, move |request| {
                let seen = Arc::clone(&seen);
                let output = output.clone();
                async move {
                    seen.lock().unwrap().push((
                        method.to_owned(),
                        serde_json::to_value(&request.message).unwrap(),
                    ));
                    Ok(decode_json(request.method.output(), output.to_string().as_bytes())?.into())
                }
            })
            .unwrap();
    }
    let seen_rm = Arc::clone(&seen);
    server.register_streaming("/protectorate.v1.NodeService/RemoveNode", move |request, sender| {
        let seen = Arc::clone(&seen_rm);
        async move {
            seen.lock().unwrap().push(("RemoveNode".into(), serde_json::to_value(&request.message).unwrap()));
            for progress in [
                json!({"phase":"REMOVE_PHASE_DRAINING","runningJobs":1,"message":"waiting for a job"}),
                json!({"phase":"REMOVE_PHASE_REMOVED","message":"removed compute"}),
            ] {
                sender.send(decode_json(request.method.output(), progress.to_string().as_bytes())?).await?;
            }
            Ok(Default::default())
        }
    }).unwrap();
    server.register("/protectorate.v1.NodeService/ListNodes", |request| async move {
        Ok(decode_json(request.method.output(), json!({"nodes":[
            {"node":{"name":"hub","local":true},"freshness":"NODE_FRESHNESS_FRESH","metrics":{}},
            {"node":{"name":"compute"},"freshness":"NODE_FRESHNESS_FRESH","metrics":{"agentVersion":"old"}}
        ]}).to_string().as_bytes())?.into())
    }).unwrap();
    let seen_update = Arc::clone(&seen);
    server.register_streaming("/protectorate.v1.NodeService/UpdateNode", move |request, sender| {
        let seen = Arc::clone(&seen_update);
        async move {
            seen.lock().unwrap().push(("UpdateNode".into(), serde_json::to_value(&request.message).unwrap()));
            for progress in [
                json!({"phase":"UPDATE_PHASE_VERIFYING"}),
                json!({"phase":"UPDATE_PHASE_SWAPPING","previousVersion":"old","version":"new","restart":"systemd"}),
                json!({"phase":"UPDATE_PHASE_CONFIRMED","version":"new"}),
            ] {
                if sender.send(decode_json(request.method.output(), progress.to_string().as_bytes())?).await.is_err() { break; }
            }
            Ok(Default::default())
        }
    }).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| dispatch_endpoint(&url, "owner-secret", args);
        assert_eq!(call(&["node","add","compute","--addr","compute","--tier","care","--tag","gpu","--tag","fast","--ssh","c","--notes","test"]).unwrap(), "added compute (compute, tier care)\n");
        assert_eq!(call(&["node","tier","compute","shared"]).unwrap(), "compute is now tier shared\n");
        assert_eq!(call(&["node","rm","compute","--jobs","wait","--wait-timeout","2m","--uninstall"]).unwrap(), "draining\twaiting for a job\nremoved\tremoved compute\n");
        assert_eq!(call(&["node","update","compute","--force"]).unwrap(), "compute\tverifying\ncompute\tswapped old -> new, restarting (systemd)\ncompute\treports new\n");
        assert_eq!(call(&["node","update","compute","--no-wait"]).unwrap(), "compute\tverifying\ncompute\tswapped old -> new, restarting (systemd)\n");
        assert!(call(&["node","update","--all"]).unwrap().contains("compute\treports new"));
        assert!(call(&["node","update","hub"]).is_err());
        assert!(call(&["node","update","missing"]).is_err());
    }).await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 6);
    assert_eq!(seen[0].1["node"]["tags"], json!(["gpu", "fast"]));
    assert_eq!(seen[0].1["node"]["sshAlias"], "c");
    assert_eq!(seen[2].1["jobs"], "REMOVE_JOBS_WAIT");
    assert_eq!(seen[2].1["waitTimeout"], "120s");
    assert_eq!(seen[2].1["uninstall"], true);
    assert_eq!(seen[3].1["force"], true);
    task.abort();
}

#[test]
fn node_lifecycle_argument_constraints() {
    let add_before = parse([
        "node", "add", "--addr", "compute", "--tag", "gpu", "compute", "--tier", "care",
    ])
    .unwrap()
    .command;
    let add_after = parse([
        "node", "add", "compute", "--addr", "compute", "--tag", "gpu", "--tier", "care",
    ])
    .unwrap()
    .command;
    assert_eq!(add_before, add_after);
    let rm_before = parse([
        "node",
        "rm",
        "--jobs",
        "wait",
        "--uninstall",
        "compute",
        "--wait-timeout",
        "2m",
    ])
    .unwrap()
    .command;
    let rm_after = parse([
        "node",
        "rm",
        "compute",
        "--jobs",
        "wait",
        "--uninstall",
        "--wait-timeout",
        "2m",
    ])
    .unwrap()
    .command;
    assert_eq!(rm_before, rm_after);
    assert!(matches!(
        parse(["node", "update", "--all", "--force"])
            .unwrap()
            .command,
        Command::NodeUpdate {
            all: true,
            force: true,
            ..
        }
    ));
    for args in [
        vec!["node", "add", "x"],
        vec!["node", "add", "x", "--addr", "x", "--tier", "server"],
        vec!["node", "tier", "x", "family"],
        vec!["node", "rm", "x", "--jobs", "maybe"],
        vec!["node", "update"],
        vec!["node", "update", "x", "--all"],
        vec!["node", "add", "--addr", "x"],
        vec!["node", "add", "x", "y", "--addr", "x"],
        vec!["node", "rm", "--jobs", "wait"],
        vec!["node", "rm", "x", "y"],
    ] {
        assert!(parse(args).is_err());
    }
}

#[cfg(unix)]
#[test]
fn installed_cli_doctor_uses_sibling_rust_binary() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    let root = tempfile::tempdir().unwrap();
    let cli = root.path().join("protectorate-cli");
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    let companion = root.path().join("protectorate-rs");
    fs::write(&companion, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&companion, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(cli)
        .args([
            "--token-file",
            "/tmp/unused",
            "--json",
            "node",
            "doctor",
            "--home",
            "/tmp/rust-node",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "node\ndoctor\n--json\n--home\n/tmp/rust-node\n"
    );
}
