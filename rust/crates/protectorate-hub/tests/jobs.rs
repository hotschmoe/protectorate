use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE, SHARED};
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::{
    AcceleratorMetrics, CpuMetrics, GpuMetrics, JobIsolation, MemoryMetrics, NodeMetrics,
};
use protectorate_wire::{Code, RpcError};
use serde_json::{json, Value};
use std::{fs, process::Command};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}
struct FixedMetrics(NodeMetrics);
impl MetricsCollector for FixedMetrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(self.0.clone())
    }
}

async fn serve(router: axum::Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (addr, task)
}
async fn rpc(addr: std::net::SocketAddr, service: &str, method: &str, body: Value) -> (u16, Value) {
    rpc_token(addr, service, method, "owner-secret", body).await
}
async fn rpc_token(
    addr: std::net::SocketAddr,
    service: &str,
    method: &str,
    token: &str,
    body: Value,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let path = format!("/protectorate.v1.{service}/{method}");
    let request=format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

async fn wait_for_job(addr: std::net::SocketAddr, id: &str) -> Value {
    for _ in 0..200 {
        let (status, job) = rpc(addr, "JobService", "GetJob", json!({"id":id})).await;
        assert_eq!(status, 200, "{job}");
        if matches!(
            job["state"].as_str(),
            Some(
                "JOB_STATE_SUCCEEDED"
                    | "JOB_STATE_FAILED"
                    | "JOB_STATE_TIMED_OUT"
                    | "JOB_STATE_CANCELED"
            )
        ) {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("job {id} did not finish");
}

#[tokio::test]
async fn ships_file_directory_and_git_sources_and_collects_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let node_jobs = Jobs::new(root.path().join("node"), 2)
        .unwrap()
        .with_plain_mode();
    let node = Agent::new(Arc::new(Metrics), "node-secret").with_jobs(node_jobs);
    let (node_addr, node_server) = serve(node.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let (addr, hub_server) = serve(hub.router().unwrap()).await;

    let script = root.path().join("script.sh");
    fs::write(&script, b"#!/bin/sh\nprintf file-result > result.txt\n").unwrap();
    let (status, submitted) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({
            "target":{"node":"peer"}, "source":{"artifact":{"path":script}},
            "command":["./script.sh"], "artifactGlobs":["result.txt"], "timeout":"10s",
        }),
    )
    .await;
    assert_eq!(status, 200, "{submitted}");
    let id = submitted["id"].as_str().unwrap();
    let (status, page) = rpc(
        addr,
        "AuditService",
        "ListAuditEntries",
        json!({"procedure":"JobService/SubmitJob"}),
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let entries = page["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{page}");
    assert_eq!(entries[0]["subject"], id);
    assert_eq!(entries[0]["outcome"], "ok");
    assert_eq!(
        entries[0]["detail"],
        "node peer, plain (agent user, unsandboxed)"
    );
    assert!(!page.to_string().contains("./script.sh"));
    let job = wait_for_job(addr, id).await;
    assert_eq!(job["state"], "JOB_STATE_SUCCEEDED", "{job}");
    assert!(job["sourceBytes"].as_str().unwrap().parse::<u64>().unwrap() > 0);
    assert_eq!(job["artifacts"][0]["path"], "result.txt");
    assert_eq!(
        fs::read(
            root.path()
                .join("hub/jobs-rust")
                .join(format!("{id}.artifacts/result.txt"))
        )
        .unwrap(),
        b"file-result"
    );

    let tree = root.path().join("tree");
    fs::create_dir_all(tree.join("sub")).unwrap();
    fs::write(tree.join("sub/input"), b"directory-result").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("input", tree.join("sub/link")).unwrap();
    let (status, submitted) = rpc(addr, "JobService", "SubmitJob", json!({
        "target":{"node":"peer"}, "source":{"artifact":{"path":tree,"workdir":"sub"}},
        "command":["/bin/sh","-c","test ! -L link && cp input result.txt"], "artifactGlobs":["result.txt"], "timeout":"10s",
    })).await;
    assert_eq!(status, 200, "{submitted}");
    let id = submitted["id"].as_str().unwrap();
    let job = wait_for_job(addr, id).await;
    assert_eq!(job["state"], "JOB_STATE_SUCCEEDED", "{job}");
    assert_eq!(
        fs::read(
            root.path()
                .join("hub/jobs-rust")
                .join(format!("{id}.artifacts/result.txt"))
        )
        .unwrap(),
        b"directory-result"
    );

    let payload = root.path().join("payload.bin");
    let payload_size = (1 << 20) + 17;
    fs::write(&payload, vec![b'x'; payload_size]).unwrap();
    let (status, submitted) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({
            "target":{"node":"peer"}, "source":{"artifact":{"path":payload}},
            "command":["/bin/sh","-c","wc -c < payload.bin > result.txt"],
            "artifactGlobs":["result.txt"], "timeout":"10s",
        }),
    )
    .await;
    assert_eq!(status, 200, "{submitted}");
    let id = submitted["id"].as_str().unwrap();
    let job = wait_for_job(addr, id).await;
    assert_eq!(job["state"], "JOB_STATE_SUCCEEDED", "{job}");
    assert_eq!(job["sourceBytes"], payload_size.to_string());
    assert_eq!(
        fs::read_to_string(
            root.path()
                .join("hub/jobs-rust")
                .join(format!("{id}.artifacts/result.txt"))
        )
        .unwrap()
        .trim(),
        payload_size.to_string()
    );

    let oversized = root.path().join("oversized.bin");
    fs::File::create(&oversized)
        .unwrap()
        .set_len((4_u64 << 30) + 1)
        .unwrap();
    let (status, submitted) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({
            "target":{"node":"peer"}, "source":{"artifact":{"path":oversized}},
            "command":["/bin/true"], "timeout":"10s",
        }),
    )
    .await;
    assert_eq!(status, 200, "{submitted}");
    let rejected = wait_for_job(addr, submitted["id"].as_str().unwrap()).await;
    assert_eq!(rejected["state"], "JOB_STATE_FAILED");
    assert!(rejected["error"]
        .as_str()
        .unwrap()
        .contains("exceeds 4 GiB"));

    let repo = root.path().join("repo");
    fs::create_dir(&repo).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        assert!(Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    fs::write(repo.join("input"), b"git-result").unwrap();
    assert!(Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "input"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "-qm", "initial"])
        .status()
        .unwrap()
        .success());
    let (status, submitted) = rpc(addr, "JobService", "SubmitJob", json!({
        "target":{"node":"peer"}, "source":{"git":{"repo":repo}},
        "command":["/bin/sh","-c","cp input result.txt"], "artifactGlobs":["result.txt"], "timeout":"10s",
    })).await;
    assert_eq!(status, 200, "{submitted}");
    let id = submitted["id"].as_str().unwrap();
    let job = wait_for_job(addr, id).await;
    assert_eq!(job["state"], "JOB_STATE_SUCCEEDED", "{job}");
    assert_eq!(job["source"]["git"]["commit"].as_str().unwrap().len(), 40);
    assert_eq!(
        fs::read(
            root.path()
                .join("hub/jobs-rust")
                .join(format!("{id}.artifacts/result.txt"))
        )
        .unwrap(),
        b"git-result"
    );
    let (status, submitted) = rpc(addr, "JobService", "SubmitJob", json!({
        "target":{"node":"peer"}, "source":{"git":{"repo":repo}},
        "command":["/bin/sh","-c","cp input result.txt"], "artifactGlobs":["result.txt"], "timeout":"10s",
    })).await;
    assert_eq!(status, 200, "{submitted}");
    let reused = wait_for_job(addr, submitted["id"].as_str().unwrap()).await;
    assert_eq!(reused["state"], "JOB_STATE_SUCCEEDED", "{reused}");
    assert_eq!(reused["sourceBytes"].as_str().unwrap_or("0"), "0");
    assert_eq!(reused["workspaceReused"], true);

    fs::write(repo.join("input"), b"git-next").unwrap();
    assert!(Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "input"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "-qm", "next"])
        .status()
        .unwrap()
        .success());
    let (status, submitted) = rpc(addr, "JobService", "SubmitJob", json!({
        "target":{"node":"peer"}, "source":{"git":{"repo":repo}},
        "command":["/bin/sh","-c","cp input result.txt"], "artifactGlobs":["result.txt"], "timeout":"10s",
    })).await;
    assert_eq!(status, 200, "{submitted}");
    let id = submitted["id"].as_str().unwrap();
    let updated = wait_for_job(addr, id).await;
    assert_eq!(updated["state"], "JOB_STATE_SUCCEEDED", "{updated}");
    assert_eq!(
        fs::read(
            root.path()
                .join("hub/jobs-rust")
                .join(format!("{id}.artifacts/result.txt"))
        )
        .unwrap(),
        b"git-next"
    );
    hub_server.abort();
    node_server.abort();
}

#[tokio::test]
async fn admin_jobs_require_node_grant_and_fresh_capability_and_audit_command() {
    let root = tempfile::tempdir().unwrap();
    let metrics = NodeMetrics {
        jobs: Some(JobIsolation {
            admin: "sudo".into(),
            ..JobIsolation::default()
        }),
        ..NodeMetrics::default()
    };
    let (node_addr, node_server) = serve(
        Agent::new(Arc::new(FixedMetrics(metrics)), "node-secret")
            .router()
            .unwrap(),
    )
    .await;
    let (limited_addr, limited_server) = serve(
        Agent::new(
            Arc::new(FixedMetrics(NodeMetrics::default())),
            "node-secret",
        )
        .router()
        .unwrap(),
    )
    .await;
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                tier: SHARED.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.inventory
        .add(
            Node {
                name: "limited".into(),
                address: limited_addr.to_string(),
                tier: SHARED.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let monitor = Arc::clone(&hub.node_monitor);
    let monitor_for_poll = Arc::clone(&monitor);
    let audit = Arc::clone(&hub.audit);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let command = json!({"target":{"node":"peer"}, "command":["/bin/true", format!("{}SECRET-END", "🦀".repeat(300))], "admin":true});
    let (status, denied) = rpc(addr, "JobService", "SubmitJob", command.clone()).await;
    assert_eq!(status, 400, "{denied}");
    assert_eq!(denied["code"], "failed_precondition");
    let (status, page) = rpc(
        addr,
        "AuditService",
        "ListAuditEntries",
        json!({"procedure":"JobService/SubmitJob"}),
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let entries = page["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{page}");
    assert_eq!(entries[0]["outcome"], "error");
    assert_eq!(entries[0]["code"], "failed_precondition");
    tokio::task::spawn_blocking(move || {
        monitor_for_poll.poll_node("peer");
        monitor_for_poll.poll_node("limited");
    })
    .await
    .unwrap();
    let (status, denied) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({
            "target":{"node":"limited"}, "command":["/bin/true"], "admin":true,
        }),
    )
    .await;
    assert_eq!(status, 400, "{denied}");
    assert!(denied["message"]
        .as_str()
        .unwrap()
        .contains("cannot run admin"));
    let (status, ordinary) = rpc(
        addr,
        "TokenService",
        "CreateToken",
        json!({"name":"jobber","scopes":["job_*"],"ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{ordinary}");
    let (status, denied) = rpc_token(
        addr,
        "JobService",
        "SubmitJob",
        ordinary["secret"].as_str().unwrap(),
        command.clone(),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    let (status, invalid) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":{}},"command":["/bin/true"],"admin":true}),
    )
    .await;
    assert_eq!(status, 400, "{invalid}");
    let (status, issued) = rpc(
        addr,
        "GrantService",
        "Admin",
        json!({"node":"peer","task":"inspect peer","ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{issued}");
    let (status, accepted) = rpc_token(
        addr,
        "JobService",
        "SubmitJob",
        issued["secret"].as_str().unwrap(),
        command.clone(),
    )
    .await;
    assert_eq!(status, 200, "{accepted}");
    assert_eq!(accepted["mode"], "JOB_MODE_ADMIN");
    assert_eq!(accepted["task"], "inspect peer");
    let entries = audit.tail(20).unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.subject == accepted["id"].as_str().unwrap()
            && entry.detail.contains("/bin/true")
            && entry.task == "inspect peer"));
    let (status, page) = rpc(
        addr,
        "AuditService",
        "ListAuditEntries",
        json!({"procedure":"JobService/SubmitJob", "subject":accepted["id"]}),
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let recorded = page["entries"].as_array().unwrap();
    assert_eq!(recorded.len(), 1, "{page}");
    let detail = recorded[0]["detail"].as_str().unwrap();
    assert!(detail.contains("/bin/true"));
    assert!(detail.ends_with("..."));
    assert!(!detail.contains("SECRET-END"));
    assert!(detail.len() < 600, "{detail}");
    node_server.abort();
    let _ = node_server.await;
    tokio::task::spawn_blocking(move || monitor.poll_node("peer"))
        .await
        .unwrap();
    let (status, denied) = rpc(addr, "JobService", "SubmitJob", command).await;
    assert_eq!(status, 400, "{denied}");
    assert!(denied["message"]
        .as_str()
        .unwrap()
        .contains("fresh admin capability"));
    server.abort();
    limited_server.abort();
}

#[tokio::test]
async fn named_node_job_runs_replays_and_survives_hub_restart() {
    let root = tempfile::tempdir().unwrap();
    let node_jobs = Jobs::new(root.path().join("node"), 2)
        .unwrap()
        .with_plain_mode();
    let node = Agent::new(Arc::new(Metrics), "node-secret").with_jobs(node_jobs);
    let (node_addr, node_server) = serve(node.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let (addr, server) = serve(hub.router().unwrap()).await;
    let request = json!({"requestId":"request-0001","target":{"node":"peer"},"command":["/bin/sh","-c","printf alpha; sleep 0.05; printf beta"],"timeout":"5s"});
    let (status, submitted) = rpc(addr, "JobService", "SubmitJob", request.clone()).await;
    assert_eq!(status, 200, "{submitted}");
    assert_eq!(submitted["mode"], "JOB_MODE_PLAIN");
    let id = submitted["id"].as_str().unwrap().to_owned();
    let (status, duplicate) = rpc(addr, "JobService", "SubmitJob", request).await;
    assert_eq!(status, 200, "{duplicate}");
    assert_eq!(duplicate["id"], id);
    let mut finished = false;
    let mut last_job = Value::Null;
    for _ in 0..100 {
        let (_, job) = rpc(addr, "JobService", "GetJob", json!({"id":id})).await;
        last_job = job.clone();
        if job["state"] == "JOB_STATE_SUCCEEDED" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(finished, "{last_job}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = root.path().join("hub/jobs-rust");
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for extension in ["json", "log"] {
            assert_eq!(
                std::fs::metadata(dir.join(format!("{id}.{extension}")))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    let (_, listed) = rpc(
        addr,
        "JobService",
        "ListJobs",
        json!({"state":"JOB_STATE_SUCCEEDED"}),
    )
    .await;
    assert_eq!(listed["jobs"][0]["id"], id);
    let url = format!("http://{addr}");
    let watched = tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(5)).unwrap();
        let mut events = Vec::new();
        client
            .stream(
                "/protectorate.v1.JobService/WatchJob",
                &json!({"id":id,"fromOffset":"5","follow":false}),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .unwrap();
        events
    })
    .await
    .unwrap();
    assert_eq!(watched[0]["job"]["state"], "JOB_STATE_SUCCEEDED");
    assert_eq!(watched[1]["log"]["offset"], "5");
    let data = watched[1]["log"]["data"].as_str().unwrap();
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data).unwrap(),
        b"beta"
    );
    let (_, slow) = rpc(addr,"JobService","SubmitJob",json!({"requestId":"cancel-0001","target":{"node":"peer"},"command":["/bin/sh","-c","sleep 30 & wait"],"timeout":"40s"})).await;
    let slow_id = slow["id"].as_str().unwrap().to_owned();
    let follow_id = slow_id.clone();
    let follow_url = format!("http://{addr}");
    let following = tokio::task::spawn_blocking(move || {
        let client = Client::new(&follow_url, "owner-secret", Duration::from_secs(5)).unwrap();
        let mut events = Vec::new();
        client
            .stream(
                "/protectorate.v1.JobService/WatchJob",
                &json!({"id":follow_id,"follow":true}),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .unwrap();
        events
    });
    for _ in 0..100 {
        let (_, job) = rpc(addr, "JobService", "GetJob", json!({"id":slow_id})).await;
        if job["state"] == "JOB_STATE_RUNNING" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, canceling) = rpc(
        addr,
        "JobService",
        "CancelJob",
        json!({"requestId":"cancel-0001"}),
    )
    .await;
    assert_eq!(status, 200, "{canceling}");
    let mut canceled = false;
    for _ in 0..100 {
        let (_, job) = rpc(addr, "JobService", "GetJob", json!({"id":slow_id})).await;
        if job["state"] == "JOB_STATE_CANCELED" {
            canceled = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(canceled);
    let (status, again) = rpc(addr, "JobService", "CancelJob", json!({"id":slow_id})).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["state"], "JOB_STATE_CANCELED");
    let followed = tokio::time::timeout(Duration::from_secs(5), following)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(followed.first().unwrap()["job"]["id"], slow_id);
    assert_eq!(
        followed.last().unwrap()["job"]["state"],
        "JOB_STATE_CANCELED"
    );
    server.abort();
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test")
        .unwrap()
        .with_node_token("node-secret".into())
        .unwrap();
    let (addr, restarted) = serve(hub.router().unwrap()).await;
    let (_, persisted) = rpc(addr, "JobService", "ListJobs", json!({})).await;
    assert_eq!(persisted["jobs"][0]["state"], "JOB_STATE_CANCELED");
    assert_eq!(persisted["jobs"][1]["state"], "JOB_STATE_SUCCEEDED");
    restarted.abort();
    node_server.abort();
}

#[tokio::test]
async fn selector_uses_fresh_shared_metrics_and_lease_conflicts() {
    let root = tempfile::tempdir().unwrap();
    let alpha_metrics = NodeMetrics {
        hostname: "alpha".into(),
        os: "linux".into(),
        arch: "amd64".into(),
        cpu: Some(CpuMetrics {
            logical_cores: 2,
            usage_percent: 10.0,
            ..CpuMetrics::default()
        }),
        memory: Some(MemoryMetrics {
            available_bytes: 16 << 30,
            used_percent: 10.0,
            ..MemoryMetrics::default()
        }),
        gpus: vec![GpuMetrics {
            index: 0,
            vendor: "NVIDIA".into(),
            name: "RTX".into(),
            memory_total_bytes: 24 << 30,
            memory_used_bytes: Some(1 << 30),
            ..GpuMetrics::default()
        }],
        npus: vec![AcceleratorMetrics {
            kind: "npu".into(),
            vendor: "Qualcomm".into(),
            name: "Hexagon".into(),
            ..AcceleratorMetrics::default()
        }],
        ..NodeMetrics::default()
    };
    let beta_metrics = NodeMetrics {
        hostname: "beta".into(),
        os: "linux".into(),
        arch: "amd64".into(),
        cpu: Some(CpuMetrics {
            logical_cores: 2,
            usage_percent: 20.0,
            ..CpuMetrics::default()
        }),
        memory: Some(MemoryMetrics {
            available_bytes: 8 << 30,
            used_percent: 20.0,
            ..MemoryMetrics::default()
        }),
        ..NodeMetrics::default()
    };
    let (alpha_addr, alpha_server) = serve(
        Agent::new(Arc::new(FixedMetrics(alpha_metrics)), "node-secret")
            .router()
            .unwrap(),
    )
    .await;
    let (beta_addr, beta_server) = serve(
        Agent::new(Arc::new(FixedMetrics(beta_metrics)), "node-secret")
            .router()
            .unwrap(),
    )
    .await;
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    for (name, address) in [("alpha", alpha_addr), ("beta", beta_addr)] {
        hub.inventory
            .add(
                Node {
                    name: name.into(),
                    address: address.to_string(),
                    tier: SHARED.into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
    }
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let monitor = Arc::clone(&hub.node_monitor);
    tokio::task::spawn_blocking(move || {
        monitor.poll_node("alpha");
        monitor.poll_node("beta");
    })
    .await
    .unwrap();
    assert!(
        hub.node_monitor.is_fresh("alpha", &alpha_addr.to_string()),
        "{}",
        hub.node_monitor
            .status("alpha", &alpha_addr.to_string(), false)
    );
    assert!(
        hub.node_monitor.is_fresh("beta", &beta_addr.to_string()),
        "{}",
        hub.node_monitor
            .status("beta", &beta_addr.to_string(), false)
    );
    let leases = Arc::clone(&hub.leases);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let gpu_selector = json!({"os":"LINUX","arch":"AMD64","minFreeMemoryBytes":"10000000000","gpu":"nvidia","gpuVramBytes":"8000000000","npu":"hexagon"});
    let (status, picked) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":gpu_selector.clone()},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 200, "{picked}");
    assert_eq!(picked["node"], "alpha");
    assert_eq!(picked["gpuIndex"], 0);
    let first_id = picked["id"].as_str().unwrap().to_owned();
    let mut first_done = false;
    for _ in 0..100 {
        let (_, job) = rpc(addr, "JobService", "GetJob", json!({"id":first_id})).await;
        if job["state"] == "JOB_STATE_FAILED" {
            first_done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(first_done);
    let (status, lightly_loaded) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":{}},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 200, "{lightly_loaded}");
    assert_eq!(lightly_loaded["node"], "alpha");
    let second_id = lightly_loaded["id"].as_str().unwrap().to_owned();
    let mut second_done = false;
    for _ in 0..100 {
        let (_, job) = rpc(addr, "JobService", "GetJob", json!({"id":second_id})).await;
        if job["state"] == "JOB_STATE_FAILED" {
            second_done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(second_done);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let gpu_lease = leases
        .acquire("test", Duration::from_secs(60), "alpha:gpu0", now)
        .unwrap();
    let (status, blocked_named) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"node":"alpha"},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 429, "{blocked_named}");
    let (status, error) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":gpu_selector.clone()},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 429, "{error}");
    let (status, picked) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":{}},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 200, "{picked}");
    assert_eq!(picked["node"], "beta");
    leases.release(&gpu_lease.id, now).unwrap();
    leases
        .acquire("test", Duration::from_secs(60), "alpha", now)
        .unwrap();
    let (status, blocked_gpu) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":gpu_selector},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 429, "{blocked_gpu}");
    let (status, no_match) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"selector":{"os":"windows"}},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 400, "{no_match}");
    server.abort();
    alpha_server.abort();
    beta_server.abort();
}

#[tokio::test]
async fn lease_target_uses_live_gpu_lease_and_rejects_released_or_expired_ids() {
    let root = tempfile::tempdir().unwrap();
    let metrics = NodeMetrics {
        hostname: "alpha".into(),
        gpus: vec![GpuMetrics {
            index: 2,
            vendor: "NVIDIA".into(),
            ..GpuMetrics::default()
        }],
        ..NodeMetrics::default()
    };
    let (node_addr, node_server) = serve(
        Agent::new(Arc::new(FixedMetrics(metrics)), "node-secret")
            .router()
            .unwrap(),
    )
    .await;
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "alpha".into(),
                address: node_addr.to_string(),
                tier: SHARED.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let monitor = Arc::clone(&hub.node_monitor);
    tokio::task::spawn_blocking(move || monitor.poll_node("alpha"))
        .await
        .unwrap();
    let leases = Arc::clone(&hub.leases);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let (status, acquired) = rpc(
        addr,
        "LeaseService",
        "AcquireLease",
        json!({"resource":"alpha:gpu2","holder":"test","ttl":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{acquired}");
    let id = acquired["id"].as_str().unwrap();
    let (status, job) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"leaseId":id},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 200, "{job}");
    assert_eq!(job["node"], "alpha");
    assert_eq!(job["gpuIndex"], 2);
    assert_eq!(job["mode"], "JOB_MODE_SANDBOXED");
    let (status, blocked) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"node":"alpha"},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 429, "{blocked}");
    let (status, released) = rpc(addr, "LeaseService", "ReleaseLease", json!({"id":id})).await;
    assert_eq!(status, 200, "{released}");
    let (status, missing) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"leaseId":id},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 404, "{missing}");
    let (status, empty) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"leaseId":""},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 400, "{empty}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let expired = leases
        .acquire("test", Duration::from_secs(1), "alpha", now - 120_000)
        .unwrap();
    let (status, after_expiry) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"node":"alpha"},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 200, "{after_expiry}");
    let (status, no_expired_lease) = rpc(
        addr,
        "JobService",
        "SubmitJob",
        json!({"target":{"leaseId":expired.id},"command":["true"]}),
    )
    .await;
    assert_eq!(status, 404, "{no_expired_lease}");
    server.abort();
    node_server.abort();
}

#[tokio::test]
async fn hub_reattaches_a_node_job_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let node_jobs = Jobs::new(root.path().join("node"), 1)
        .unwrap()
        .with_plain_mode();
    let (node_addr, node_server) = serve(
        Agent::new(Arc::new(Metrics), "node-secret")
            .with_jobs(node_jobs)
            .router()
            .unwrap(),
    )
    .await;
    let id = "recovery-001";
    let node_url = format!("http://{node_addr}");
    let started=tokio::task::spawn_blocking(move || {
        let client=Client::new(&node_url,"node-secret",Duration::from_secs(5)).unwrap();
        client.stream("/protectorate.v1.NodeAgentService/RunJob",&json!({"jobId":id,"command":["/bin/sh","-c","sleep 0.2; printf recovered"],"mode":"JOB_MODE_PLAIN"}),|event| {
            if event.get("started").is_some() {return Err(RpcError::new(Code::Canceled,"disconnect"));}
            Ok(())
        }).unwrap_err()
    }).await.unwrap();
    assert_eq!(started.code, Code::Canceled);
    let home = root.path().join("hub");
    let hub = Hub::open(&home, "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    drop(hub);
    let dir = home.join("jobs-rust");
    std::fs::write(dir.join(format!("{id}.log")), b"").unwrap();
    let job = json!({"id":id,"state":"JOB_STATE_RUNNING","target":{"node":"peer"},"node":"peer","command":["/bin/sh","-c","sleep 0.2; printf recovered"],"mode":"JOB_MODE_PLAIN","timeout":"10s","submittedAt":chrono::Utc::now().to_rfc3339()});
    std::fs::write(dir.join(format!("{id}.json")),json!({"id":id,"job":job,"requestId":"","endpoint":format!("http://{node_addr}"),"cancelRequested":false}).to_string()).unwrap();
    let hub = Hub::open(&home, "owner-secret", "test").unwrap();
    hub.jobs.defer_resume();
    let route_ready = Arc::new(AtomicBool::new(false));
    let ready = Arc::clone(&route_ready);
    let hub = hub
        .with_node_token("node-secret".into())
        .unwrap()
        .with_node_route(Arc::new(move |_, _| {
            if ready.load(Ordering::SeqCst) {
                Ok(format!("http://{node_addr}"))
            } else {
                Err("tailnet route is still starting".into())
            }
        }));
    hub.jobs.resume_deferred();
    let (addr, server) = serve(hub.router().unwrap()).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (_, waiting) = rpc(addr, "JobService", "GetJob", json!({"id":id})).await;
    assert_eq!(waiting["state"], "JOB_STATE_RUNNING", "{waiting}");
    route_ready.store(true, Ordering::SeqCst);
    let mut recovered = false;
    for _ in 0..150 {
        let (_, job) = rpc(addr, "JobService", "GetJob", json!({"id":id})).await;
        if job["state"] == "JOB_STATE_SUCCEEDED" {
            recovered = true;
            assert_eq!(job["logBytes"], "9");
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(recovered);
    assert_eq!(
        std::fs::read(dir.join(format!("{id}.log"))).unwrap(),
        b"recovered"
    );
    server.abort();
    node_server.abort();
}

#[tokio::test]
async fn running_job_reattaches_after_its_node_route_drops() {
    let root = tempfile::tempdir().unwrap();
    let node_jobs = Jobs::new(root.path().join("node"), 1)
        .unwrap()
        .with_plain_mode();
    let node = Agent::new(Arc::new(Metrics), "node-secret").with_jobs(node_jobs);
    let (node_addr, node_server) = serve(node.router().unwrap()).await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let (disconnect, _) = tokio::sync::watch::channel(0_u64);
    let cut = disconnect.clone();
    let proxy_server = tokio::spawn(async move {
        loop {
            let (mut inbound, _) = listener.accept().await.unwrap();
            let mut cut = cut.subscribe();
            tokio::spawn(async move {
                let Ok(mut outbound) = TcpStream::connect(node_addr).await else {
                    return;
                };
                tokio::select! {
                    _ = cut.changed() => {}
                    _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound) => {}
                }
            });
        }
    });

    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: "peer".into(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let ready = Arc::new(AtomicBool::new(true));
    let route_ready = Arc::clone(&ready);
    let hub = hub
        .with_node_token("node-secret".into())
        .unwrap()
        .with_node_route(Arc::new(move |_, _| {
            if route_ready.load(Ordering::SeqCst) {
                Ok(format!("http://{proxy_addr}"))
            } else {
                Err("node route is temporarily unavailable".into())
            }
        }));
    let (addr, server) = serve(hub.router().unwrap()).await;
    let request = json!({"requestId":"route-outage-001","target":{"node":"peer"},
        "command":["/bin/sh","-c","printf start; sleep 4; printf finish"],"timeout":"20s"});
    let (status, submitted) = rpc(addr, "JobService", "SubmitJob", request.clone()).await;
    assert_eq!(status, 200, "{submitted}");
    let id = submitted["id"].as_str().unwrap();
    let log = root.path().join("hub/jobs-rust").join(format!("{id}.log"));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if fs::read(&log).is_ok_and(|bytes| bytes == b"start") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("job did not persist its first output before the route cut");

    ready.store(false, Ordering::SeqCst);
    disconnect.send_replace(1);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (status, running) = rpc(addr, "JobService", "GetJob", json!({"id":id})).await;
    assert_eq!(status, 200, "{running}");
    assert_ne!(running["state"], "JOB_STATE_FAILED", "{running}");
    ready.store(true, Ordering::SeqCst);
    let finished = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let (status, job) = rpc(addr, "JobService", "GetJob", json!({"id":id})).await;
            assert_eq!(status, 200, "{job}");
            if matches!(
                job["state"].as_str(),
                Some("JOB_STATE_SUCCEEDED" | "JOB_STATE_FAILED" | "JOB_STATE_TIMED_OUT")
            ) {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("job did not recover after its node route returned");
    assert_eq!(finished["state"], "JOB_STATE_SUCCEEDED", "{finished}");
    assert_eq!(finished["logBytes"], "11");
    assert_eq!(fs::read(&log).unwrap(), b"startfinish");
    let (status, replay) = rpc(addr, "JobService", "SubmitJob", request).await;
    assert_eq!(status, 200, "{replay}");
    assert_eq!(replay["id"], id);

    server.abort();
    proxy_server.abort();
    node_server.abort();
}
