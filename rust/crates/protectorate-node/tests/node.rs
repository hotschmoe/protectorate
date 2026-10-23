use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::{CpuMetrics, FilesystemMetrics, NodeMetrics};
use serde_json::{json, Value};
#[cfg(unix)]
use std::time::Duration;
use std::{io, sync::Arc, time::SystemTime};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Fixed;
impl MetricsCollector for Fixed {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "node-a".into(),
            os: "linux".into(),
            arch: "amd64".into(),
            agent_version: "v-rust".into(),
            uptime_seconds: 123,
            collected_at: Some(SystemTime::UNIX_EPOCH),
            cpu: Some(CpuMetrics {
                logical_cores: 4,
                usage_percent: 25.0,
                load1: Some(1.0),
                ..Default::default()
            }),
            filesystems: vec![FilesystemMetrics {
                mount: "/".into(),
                device: "disk".into(),
                fstype: "ext4".into(),
                total_bytes: 4096,
                used_bytes: 1024,
                free_bytes: 3072,
                primary: true,
                ..Default::default()
            }],
            ..Default::default()
        })
    }
}
async fn rpc(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = body.to_string();
    let request = format!("POST /protectorate.v1.NodeAgentService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

async fn stream_rpc(addr: std::net::SocketAddr, method: &str, body: Value) -> Vec<Value> {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let headers = format!("POST /protectorate.v1.NodeAgentService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\n\r\n", body.len() + 5);
    stream.write_all(headers.as_bytes()).await.unwrap();
    let mut envelope = vec![0];
    envelope.extend_from_slice(&(body.len() as u32).to_be_bytes());
    stream.write_all(&envelope).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
    assert!(line(&mut stream).await.contains("200"));
    while line(&mut stream).await != "\r\n" {}
    let mut parts = Vec::new();
    loop {
        let size = usize::from_str_radix(line(&mut stream).await.trim(), 16).unwrap();
        if size == 0 {
            break;
        }
        let mut frame = vec![0; size];
        stream.read_exact(&mut frame).await.unwrap();
        assert_eq!(line(&mut stream).await, "\r\n");
        if frame[0] == 2 {
            break;
        }
        assert_eq!(frame[0], 0);
        parts.push(serde_json::from_slice(&frame[5..]).unwrap());
    }
    parts
}
async fn line(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n") {
            return String::from_utf8(buf).unwrap();
        }
    }
}

#[tokio::test]
async fn file_rpc_round_trip_boundary_and_confinement() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use protectorate_xfer::Source;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    let input = tmp.path().join("input");
    let mut bytes = vec![0_u8; protectorate_xfer::CHUNK_SIZE];
    let mut seed = 1_u64;
    for byte in &mut bytes {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        *byte = seed as u8;
    }
    std::fs::write(&input, &bytes).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_files(&root, false)
        .unwrap()
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut source = Source::open(&input, false).unwrap();
    let chunk = source.next_chunk().unwrap().unwrap();
    assert_eq!(chunk.data.len(), protectorate_xfer::CHUNK_SIZE);
    let (code, first) = rpc(addr, "PutFile", "node-secret", json!({
        "transferId": "transfer-123", "path": "uploaded", "kind": "FILE_KIND_FILE", "mode": 384,
        "offset": "0", "chunk": {"data": STANDARD.encode(&chunk.data), "rawSize": chunk.raw_size}
    })).await;
    assert_eq!(code, 200, "{first}");
    assert_eq!(
        first["received"],
        json!(protectorate_xfer::CHUNK_SIZE.to_string())
    );
    let (code, bad_offset) = rpc(
        addr,
        "PutFile",
        "node-secret",
        json!({"transferId": "transfer-123", "offset": "1"}),
    )
    .await;
    assert_eq!(code, 400, "{bad_offset}");
    let (code, missing) = rpc(
        addr,
        "PutFile",
        "node-secret",
        json!({"transferId": "transfer-123", "offset": "1"}),
    )
    .await;
    assert_eq!(code, 404, "{missing}");
    assert!(!root.join("uploaded").exists());

    let mut source = Source::open(&input, false).unwrap();
    let chunk = source.next_chunk().unwrap().unwrap();
    let (_, _) = rpc(addr, "PutFile", "node-secret", json!({
        "transferId": "transfer-456", "path": "uploaded", "kind": "FILE_KIND_FILE", "mode": 384,
        "offset": "0", "chunk": {"data": STANDARD.encode(&chunk.data), "rawSize": chunk.raw_size}
    })).await;
    assert!(source.next_chunk().unwrap().is_none());
    let trailer = source.trailer().unwrap();
    let final_request = json!({
        "transferId": "transfer-456", "offset": trailer.size.to_string(), "last": true,
        "trailer": {"size": trailer.size.to_string(), "sha256": trailer.sha256}
    });
    let ((code, done), (concurrent_code, concurrent_reply)) = tokio::join!(
        rpc(addr, "PutFile", "node-secret", final_request.clone()),
        rpc(addr, "PutFile", "node-secret", final_request.clone())
    );
    assert_eq!(code, 200, "{done}");
    assert_eq!(concurrent_code, 200, "{concurrent_reply}");
    assert_eq!(concurrent_reply, done);
    assert_eq!(done["done"], true);
    assert_eq!(std::fs::read(root.join("uploaded")).unwrap(), bytes);
    let (code, replay) = rpc(addr, "PutFile", "node-secret", final_request.clone()).await;
    assert_eq!(code, 200, "{replay}");
    assert_eq!(replay, done);
    let mut altered = final_request;
    altered["trailer"]["sha256"] = json!("00".repeat(32));
    let (code, refused) = rpc(addr, "PutFile", "node-secret", altered).await;
    assert_eq!(code, 400, "{refused}");
    assert_eq!(std::fs::read(root.join("uploaded")).unwrap(), bytes);

    let parts = stream_rpc(
        addr,
        "GetFile",
        json!({"path": "uploaded", "noCompress": true}),
    )
    .await;
    assert_eq!(parts[0]["header"]["kind"], "FILE_KIND_FILE");
    assert_eq!(
        parts[1]["chunk"]["rawSize"],
        protectorate_xfer::CHUNK_SIZE as u64
    );
    assert_eq!(
        STANDARD
            .decode(parts[1]["chunk"]["data"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    assert_eq!(parts[2]["trailer"]["sha256"], trailer.sha256);

    let source_dir = tmp.path().join("source-dir");
    std::fs::create_dir(&source_dir).unwrap();
    std::fs::write(source_dir.join("inside"), b"directory payload").unwrap();
    let mut dir = Source::open(&source_dir, false).unwrap();
    let archive_chunk = dir.next_chunk().unwrap().unwrap();
    let (code, body) = rpc(addr, "PutFile", "node-secret", json!({
        "transferId": "directory-123", "path": "received-dir", "kind": "FILE_KIND_DIRECTORY",
        "offset": "0", "chunk": {"data": STANDARD.encode(&archive_chunk.data), "rawSize": archive_chunk.raw_size}
    })).await;
    assert_eq!(code, 200, "{body}");
    assert!(dir.next_chunk().unwrap().is_none());
    let dir_trailer = dir.trailer().unwrap();
    let (code, body) = rpc(
        addr,
        "PutFile",
        "node-secret",
        json!({
            "transferId": "directory-123", "offset": dir_trailer.size.to_string(), "last": true,
            "trailer": {"size": dir_trailer.size.to_string(), "sha256": dir_trailer.sha256}
        }),
    )
    .await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(
        std::fs::read(root.join("received-dir/inside")).unwrap(),
        b"directory payload"
    );
    let parts = stream_rpc(addr, "GetFile", json!({"path": "received-dir"})).await;
    assert_eq!(parts[0]["header"]["kind"], "FILE_KIND_DIRECTORY");
    assert!(parts.last().unwrap().get("trailer").is_some());

    #[cfg(unix)]
    {
        let (code, denied) = rpc(addr, "PutFile", "node-secret", json!({
            "transferId": "transfer-789", "path": "escape/secret", "kind": "FILE_KIND_FILE", "mode": 384
        })).await;
        assert_eq!(code, 403, "{denied}");
        assert!(!outside.join("secret").exists());
    }
    for index in 0..16 {
        let (code, body) = rpc(
            addr,
            "PutFile",
            "node-secret",
            json!({
                "transferId": format!("pending-{index:02}"), "path": format!("pending-{index:02}"),
                "kind": "FILE_KIND_FILE", "mode": 384
            }),
        )
        .await;
        assert_eq!(code, 200, "{body}");
    }
    let (code, full) = rpc(
        addr,
        "PutFile",
        "node-secret",
        json!({
            "transferId": "pending-17", "path": "pending-17", "kind": "FILE_KIND_FILE"
        }),
    )
    .await;
    assert_eq!(code, 429, "{full}");
    server.abort();
}

#[tokio::test]
async fn unconfined_file_rpc_accepts_absolute_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let target = tmp.path().join("outside");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_files(&root, true)
        .unwrap()
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (code, first) = rpc(
        addr,
        "PutFile",
        "node-secret",
        json!({
            "transferId": "unconfined-123", "path": target.to_string_lossy(),
            "kind": "FILE_KIND_FILE", "mode": 384,
            "chunk": {"data": "eA==", "rawSize": 1}
        }),
    )
    .await;
    assert_eq!(code, 200, "{first}");
    let (code, done) = rpc(addr, "PutFile", "node-secret", json!({
        "transferId": "unconfined-123", "offset": "1", "last": true,
        "trailer": {"size": "1", "sha256": "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"}
    })).await;
    assert_eq!(code, 200, "{done}");
    assert_eq!(std::fs::read(target).unwrap(), b"x");
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn node_jobs_run_replay_cancel_and_authenticate() {
    use protectorate_node::Jobs;
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jobs = Jobs::new(tmp.path(), 1).unwrap().with_plain_mode();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let body = json!({"jobId":"first", "command":["/bin/sh","-c","printf abc; printf def >&2"], "mode":"JOB_MODE_PLAIN"});
    let events = stream_rpc(addr, "RunJob", body).await;
    assert_eq!(events[0]["started"]["isolation"], "plain");
    assert_eq!(events.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    let (code, list) = rpc(addr, "ListJobs", "node-secret", json!({})).await;
    assert_eq!(code, 200, "{list}");
    assert_eq!(list["jobs"][0]["logBytes"], "6");
    assert!(list["jobs"][0]["finishedAt"]
        .as_str()
        .unwrap()
        .ends_with('Z'));
    assert!(list["jobs"][0]["finished"]["runTime"]
        .as_str()
        .unwrap()
        .ends_with('s'));
    let replay = stream_rpc(addr, "AttachJob", json!({"jobId":"first","fromOffset":"3"})).await;
    assert_eq!(replay[0]["started"]["isolation"], "plain");
    let output = replay
        .iter()
        .filter_map(|e| e.get("output"))
        .map(|o| o["data"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(output.len(), 1);
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, output[0])
            .unwrap()
            .len(),
        3
    );
    assert_eq!(replay.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    let (code, result) = rpc(addr, "CancelJob", "node-secret", json!({"jobId":"first"})).await;
    assert_eq!(code, 200);
    assert!(result.get("found").is_none_or(|value| value == false));
    let running = tokio::spawn(stream_rpc(
        addr,
        "RunJob",
        json!({"jobId":"slow", "command":["/bin/sh","-c","sleep 30 & wait"], "mode":"JOB_MODE_PLAIN"}),
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        let (_, list) = rpc(addr, "ListJobs", "node-secret", json!({})).await;
        if list["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|job| job["jobId"] == "slow" && job["started"] == true)
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{list}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (code, result) = rpc(addr, "CancelJob", "node-secret", json!({"jobId":"slow"})).await;
    assert_eq!(code, 200);
    assert_eq!(result["found"], true);
    let events = tokio::time::timeout(Duration::from_secs(120), running)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_CANCELED"
    );
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn node_job_defaults_to_sandbox_and_fails_closed_without_configuration() {
    use protectorate_node::Jobs;
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(Jobs::new(tmp.path(), 1).unwrap())
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (code, result) = rpc(addr, "ListJobs", "node-secret", json!({})).await;
    assert_eq!(code, 200, "{result}");
    let plain = json!({"jobId":"not-allowed", "command":["/bin/true"], "mode":"JOB_MODE_PLAIN"});
    let (code, body) = stream_error(addr, "RunJob", plain).await;
    assert_eq!(code, 200);
    assert!(body.contains("permission_denied"), "{body}");
    let sandboxed = json!({"jobId":"not-run", "command":["/bin/true"]});
    let (code, body) = stream_error(addr, "RunJob", sandboxed).await;
    assert_eq!(code, 200);
    assert!(body.contains("unavailable"), "{body}");
    let (_, list) = rpc(addr, "ListJobs", "node-secret", json!({})).await;
    assert!(list
        .get("jobs")
        .is_none_or(|jobs| jobs.as_array().is_some_and(Vec::is_empty)));
    let (code, prepared) = rpc(
        addr,
        "PrepareJob",
        "node-secret",
        json!({"jobId":"source", "repoKey":"repo", "commit":"0123456789012345678901234567890123456789"}),
    )
    .await;
    assert_eq!(code, 200);
    assert!(prepared.get("hasCommit").is_none_or(|value| value == false));
    let (code, bad_digest) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({"jobId":"source", "offset":"0", "data":"YWJj", "last":true, "sha256":"00"}),
    )
    .await;
    assert_eq!(code, 500);
    assert_eq!(bad_digest["code"], "data_loss");
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn node_job_upload_file_and_replay_artifact() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use protectorate_node::Jobs;
    use sha2::{Digest, Sha256};
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(Jobs::new(tmp.path(), 1).unwrap().with_plain_mode())
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let script = b"#!/bin/sh\nprintf artifact > out.txt\n";
    let digest = format!("{:x}", Sha256::digest(script));
    let (code, _) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({
            "jobId":"file-source", "offset":"0", "data":STANDARD.encode(&script[..10])
        }),
    )
    .await;
    assert_eq!(code, 200);
    let (code, received) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({
            "jobId":"file-source", "offset":"10", "data":STANDARD.encode(&script[10..]),
            "last":true, "sha256":digest
        }),
    )
    .await;
    assert_eq!(code, 200, "{received}");
    assert_eq!(received["received"], script.len().to_string());
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"file-source", "artifactName":"script.sh", "command":["./script.sh"],
            "artifactGlobs":["out.txt"], "mode":"JOB_MODE_PLAIN"
        }),
    )
    .await;
    assert_eq!(events.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    let artifact = events
        .iter()
        .find_map(|event| event.get("artifact"))
        .unwrap();
    assert_eq!(artifact["path"], "out.txt");
    assert_eq!(
        STANDARD.decode(artifact["data"].as_str().unwrap()).unwrap(),
        b"artifact"
    );
    let replay = stream_rpc(addr, "AttachJob", json!({"jobId":"file-source"})).await;
    assert!(replay.iter().any(|event| event.get("artifact").is_some()));
    server.abort();
    let _ = server.await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let restarted_addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(Jobs::new(tmp.path(), 1).unwrap().with_plain_mode())
        .router()
        .unwrap();
    let restarted = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let replay = stream_rpc(restarted_addr, "AttachJob", json!({"jobId":"file-source"})).await;
    assert_eq!(replay.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    assert!(replay.iter().any(|event| event.get("artifact").is_some()));
    restarted.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn node_job_artifact_disk_limit_is_reported() {
    use protectorate_node::{DiskLimits, Jobs};
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jobs = Jobs::new(tmp.path(), 1)
        .unwrap()
        .with_plain_mode()
        .with_disk_limits(DiskLimits {
            git_cache_bytes: 32 << 30,
            artifact_bytes: 4,
        });
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"artifact-limit", "command":["/bin/sh","-c","printf 12345 > out.txt"],
            "artifactGlobs":["out.txt"], "mode":"JOB_MODE_PLAIN"
        }),
    )
    .await;
    assert_eq!(events.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    assert!(events.last().unwrap()["finished"]["error"]
        .as_str()
        .unwrap()
        .contains("disk budget"));
    assert!(events.iter().all(|event| event.get("artifact").is_none()));
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn node_job_archive_workdir_and_git_cache() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use protectorate_node::Jobs;
    use sha2::{Digest, Sha256};
    use std::{fs::File, io::Write, process::Command};
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(
            Jobs::new(tmp.path().join("node"), 1)
                .unwrap()
                .with_plain_mode(),
        )
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let archive = tmp.path().join("source.zip");
    {
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        zip.start_file("sub/input.txt", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(b"from-archive").unwrap();
        zip.finish().unwrap();
    }
    let bytes = std::fs::read(&archive).unwrap();
    let (code, response) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({
            "jobId":"archive-source","offset":"0","data":STANDARD.encode(&bytes),
            "last":true,"sha256":format!("{:x}",Sha256::digest(&bytes))
        }),
    )
    .await;
    assert_eq!(code, 200, "{response}");
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"archive-source","artifactArchive":true,"workdir":"sub",
            "command":["/bin/sh","-c","cp input.txt output.txt"],
            "artifactGlobs":["output.txt"], "mode":"JOB_MODE_PLAIN"
        }),
    )
    .await;
    assert_eq!(events.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    let artifact = events
        .iter()
        .find_map(|event| event.get("artifact"))
        .unwrap();
    assert_eq!(
        STANDARD.decode(artifact["data"].as_str().unwrap()).unwrap(),
        b"from-archive"
    );

    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git(&["init", "--quiet"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("input.txt"), b"from-git").unwrap();
    git(&["add", "input.txt"]);
    git(&["commit", "--quiet", "-m", "first"]);
    let commit = git(&["rev-parse", "HEAD"]);
    let bundle = tmp.path().join("source.bundle");
    let refname = format!("refs/pc/{commit}");
    git(&["update-ref", &refname, &commit]);
    git(&["bundle", "create", bundle.to_str().unwrap(), &refname]);
    let (code, prepared) = rpc(
        addr,
        "PrepareJob",
        "node-secret",
        json!({
            "jobId":"git-source","repoKey":"repo-key","commit":commit
        }),
    )
    .await;
    assert_eq!(code, 200, "{prepared}");
    assert!(prepared.get("hasCommit").is_none_or(|value| value == false));
    let bytes = std::fs::read(bundle).unwrap();
    let (code, response) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({
            "jobId":"git-source","offset":"0","data":STANDARD.encode(&bytes),
            "last":true,"sha256":format!("{:x}",Sha256::digest(&bytes))
        }),
    )
    .await;
    assert_eq!(code, 200, "{response}");
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"git-source","repoKey":"repo-key","commit":commit,"bundleUploaded":true,
            "command":["/bin/cat","input.txt"], "mode":"JOB_MODE_PLAIN"
        }),
    )
    .await;
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let (code, prepared) = rpc(
        addr,
        "PrepareJob",
        "node-secret",
        json!({
            "jobId":"git-source-next","repoKey":"repo-key","commit":commit
        }),
    )
    .await;
    assert_eq!(code, 200, "{prepared}");
    assert_eq!(prepared["hasCommit"], true);
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"git-source-next","repoKey":"repo-key","commit":commit,
            "command":["/bin/cat","input.txt"], "mode":"JOB_MODE_PLAIN"
        }),
    )
    .await;
    assert_eq!(
        events.iter().find_map(|event| event
            .get("started")
            .and_then(|started| started.get("workspaceReused"))),
        Some(&json!(true)),
        "{events:?}"
    );
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn node_job_outlives_disconnected_run_stream() {
    use protectorate_node::Jobs;
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(Jobs::new(tmp.path(), 1).unwrap().with_plain_mode())
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let body = json!({"jobId":"disconnected", "command":["/bin/sh","-c","printf before; sleep 0.2; printf after"], "mode":"JOB_MODE_PLAIN"}).to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let mut envelope = vec![0];
    envelope.extend_from_slice(&(body.len() as u32).to_be_bytes());
    envelope.extend_from_slice(body.as_bytes());
    let headers = format!("POST /protectorate.v1.NodeAgentService/RunJob HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\n\r\n", envelope.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&envelope).await.unwrap();
    assert!(line(&mut stream).await.contains("200"));
    while line(&mut stream).await != "\r\n" {}
    let size = usize::from_str_radix(line(&mut stream).await.trim(), 16).unwrap();
    let mut frame = vec![0; size];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&frame[5..]).unwrap()["started"]["isolation"],
        "plain"
    );
    drop(stream);
    let replay = tokio::time::timeout(
        Duration::from_secs(120),
        stream_rpc(addr, "AttachJob", json!({"jobId":"disconnected"})),
    )
    .await
    .unwrap();
    let bytes = replay
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| {
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                output["data"].as_str().unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(bytes, b"beforeafter");
    assert_eq!(replay.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    server.abort();
}

#[cfg(unix)]
async fn stream_error(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = body.to_string();
    let mut envelope = vec![0];
    envelope.extend_from_slice(&(body.len() as u32).to_be_bytes());
    envelope.extend_from_slice(body.as_bytes());
    let headers = format!("POST /protectorate.v1.NodeAgentService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", envelope.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&envelope).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    (
        response.split_whitespace().nth(1).unwrap().parse().unwrap(),
        response,
    )
}

/// A job ends in exactly one of exited, killed, timed out or canceled, and a
/// timeout or cancel leaves none of the command's background processes alive.
#[cfg(unix)]
#[tokio::test]
async fn node_jobs_classify_how_they_end_and_stop_their_whole_process_tree() {
    use protectorate_node::Jobs;
    let tmp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jobs = Jobs::new(tmp.path().join("jobs"), 4)
        .unwrap()
        .with_plain_mode();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let finished = |events: &[Value]| events.last().unwrap()["finished"].clone();
    let run = |id: &str, script: String, timeout: &str| {
        let body = json!({"jobId":id, "command":["/bin/sh","-c",script], "mode":"JOB_MODE_PLAIN", "timeout":timeout});
        tokio::spawn(stream_rpc(addr, "RunJob", body))
    };
    let alive = |pid: i32| {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|stat| !stat.contains(") Z"))
            .unwrap_or(false)
    };
    let background_pid = |file: std::path::PathBuf| async move {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while std::time::Instant::now() < deadline {
            if let Some(pid) = std::fs::read_to_string(&file)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("background process never started")
    };
    let limit = Duration::from_secs(60);

    let ended = tokio::time::timeout(limit, run("exit3", "exit 3".into(), "30s"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finished(&ended)["end"], "JOB_END_EXITED");
    assert_eq!(finished(&ended)["exitCode"], 3);

    let ended = tokio::time::timeout(limit, run("kill9", "kill -9 $$".into(), "30s"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finished(&ended)["end"], "JOB_END_KILLED");
    assert_eq!(finished(&ended)["error"], "killed by signal 9");

    for (id, timeout, how) in [
        ("times-out", "2s", "JOB_END_TIMED_OUT"),
        ("canceled", "300s", "JOB_END_CANCELED"),
    ] {
        let pidfile = tmp.path().join(format!("{id}.pid"));
        let script = format!("sleep 300 & echo $! > {}; wait", pidfile.display());
        let job = run(id, script, timeout);
        let pid = background_pid(pidfile).await;
        assert!(alive(pid));
        if id == "canceled" {
            let (code, result) = rpc(addr, "CancelJob", "node-secret", json!({"jobId":id})).await;
            assert_eq!(code, 200, "{result}");
        }
        let ended = tokio::time::timeout(limit, job).await.unwrap().unwrap();
        assert_eq!(finished(&ended)["end"], how);
        for _ in 0..250 {
            if !alive(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!alive(pid), "{id} left its background process running");
    }
    server.abort();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn sandboxed_node_job_streams_before_exit_and_reattaches() {
    use protectorate_node::{Jobs, SandboxConfig};
    use protectorate_sandbox::{runtime::LaunchOptions, Policy};
    use std::{collections::BTreeMap, fs};
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let owner = temp.path().join("owner");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&owner).unwrap();
    let policy = Policy {
        state_root: state.clone(),
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let options = LaunchOptions::new(
        env!("CARGO_BIN_EXE_protectorate-node-sandbox-helper"),
        temp.path().join("runtime"),
        &owner,
        temp.path().join("unused"),
    );
    let jobs = Jobs::new(temp.path().join("nodejobs"), 1)
        .unwrap()
        .with_sandbox(SandboxConfig {
            policy,
            options,
            egress: None,
        });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    // The job stays live until the test releases it from its workspace.
    let body = json!({"jobId":"sandbox-live", "command":["sh","-c","printf before; while [ ! -e release ]; do sleep 0.05; done; printf after"]}).to_string();
    let mut envelope = vec![0];
    envelope.extend_from_slice(&(body.len() as u32).to_be_bytes());
    envelope.extend_from_slice(body.as_bytes());
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let headers = format!("POST /protectorate.v1.NodeAgentService/RunJob HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\n\r\n", envelope.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&envelope).await.unwrap();
    assert!(line(&mut stream).await.contains("200"));
    while line(&mut stream).await != "\r\n" {}
    let started = read_job_frame(&mut stream).await;
    assert_eq!(started["started"]["isolation"], "native");
    let first = read_job_frame(&mut stream).await;
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        first["output"]["data"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(bytes, b"before");
    let (_, list) = rpc(addr, "ListJobs", "node-secret", json!({})).await;
    assert!(list["jobs"][0].get("finished").is_none(), "{list}");
    drop(stream);
    let workspace = fs::read_dir(state.join("workspaces"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(workspace.join("release"), "").unwrap();
    let replay = tokio::time::timeout(
        Duration::from_secs(120),
        stream_rpc(
            addr,
            "AttachJob",
            json!({"jobId":"sandbox-live", "fromOffset":"6"}),
        ),
    )
    .await
    .unwrap();
    let bytes = replay
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| {
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                output["data"].as_str().unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(bytes, b"after");
    assert_eq!(replay.last().unwrap()["finished"]["end"], "JOB_END_EXITED");
    let slow = tokio::spawn(stream_rpc(
        addr,
        "RunJob",
        json!({"jobId":"sandbox-cancel", "command":["/usr/bin/sh","-c","sleep 30 & wait"]}),
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        let (_, list) = rpc(addr, "ListJobs", "node-secret", json!({})).await;
        if list["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|job| job["jobId"] == "sandbox-cancel" && job["started"] == true)
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{list}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (_, result) = rpc(
        addr,
        "CancelJob",
        "node-secret",
        json!({"jobId":"sandbox-cancel"}),
    )
    .await;
    assert_eq!(result["found"], true);
    let canceled = tokio::time::timeout(Duration::from_secs(120), slow)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        canceled.last().unwrap()["finished"]["end"],
        "JOB_END_CANCELED"
    );
    server.abort();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn sandboxed_git_job_can_read_its_own_metadata() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use protectorate_node::{Jobs, SandboxConfig};
    use protectorate_sandbox::{runtime::LaunchOptions, Policy};
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, fs, process::Command};
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let owner = temp.path().join("owner");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&owner).unwrap();
    let policy = Policy {
        state_root: state,
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let options = LaunchOptions::new(
        env!("CARGO_BIN_EXE_protectorate-node-sandbox-helper"),
        temp.path().join("runtime"),
        &owner,
        temp.path().join("unused"),
    );
    let jobs = Jobs::new(temp.path().join("nodejobs"), 1)
        .unwrap()
        .with_sandbox(SandboxConfig {
            policy,
            options,
            egress: None,
        })
        .with_plain_mode();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git(&["init", "--quiet"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    fs::write(repo.join("input.txt"), b"content").unwrap();
    fs::write(repo.join("run.sh"), b"#!/bin/sh\nprintf repo-script").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(repo.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&["add", "input.txt", "run.sh"]);
    git(&["commit", "--quiet", "-m", "first"]);
    let commit = git(&["rev-parse", "HEAD"]);
    let refname = format!("refs/pc/{commit}");
    git(&["update-ref", &refname, &commit]);
    let bundle = temp.path().join("source.bundle");
    git(&["bundle", "create", bundle.to_str().unwrap(), &refname]);
    let bytes = fs::read(bundle).unwrap();
    let (code, response) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({
            "jobId":"sandbox-git","offset":"0","data":STANDARD.encode(&bytes),
            "last":true,"sha256":format!("{:x}",Sha256::digest(&bytes))
        }),
    )
    .await;
    assert_eq!(code, 200, "{response}");
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"sandbox-git","repoKey":"repo-key","commit":commit,"bundleUploaded":true,
            "command":["/usr/bin/git","rev-parse","HEAD"]
        }),
    )
    .await;
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let output = events
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf8(output).unwrap().trim(), commit);
    let secret = temp.path().join("host-secret");
    let marker = temp.path().join("host-filter-marker");
    fs::write(&secret, b"host credential").unwrap();
    let poison = r#"printf '#!/bin/sh\nif test -f "%s"; then cat "%s"; printf escaped > "%s"; else printf confined; fi\n' "$1" "$1" "$2" > .git/filter; chmod 700 .git/filter; git config filter.audit.smudge .git/filter; git config filter.audit.clean cat; git config filter.audit.required true; printf 'input.txt filter=audit\n' > .git/info/attributes; printf changed > input.txt"#;
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"sandbox-git-poison","repoKey":"repo-key","commit":commit,
            "command":["/bin/sh","-c",poison,"sh",secret,marker]
        }),
    )
    .await;
    assert_eq!(
        events
            .iter()
            .find_map(|event| event.get("started"))
            .unwrap()["workspaceReused"],
        true
    );
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"plain-git-filter","repoKey":"repo-key","commit":commit,
            "mode":"JOB_MODE_PLAIN",
            "command":["/bin/cat","input.txt"]
        }),
    )
    .await;
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let output = events
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        output, b"content",
        "plain Git used a sandbox-owned checkout"
    );
    assert!(!marker.exists(), "Git filter executed with host authority");
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"sandbox-git-filter","repoKey":"repo-key","commit":commit,
            "command":["/bin/cat","input.txt"]
        }),
    )
    .await;
    assert_eq!(
        events.iter().find_map(|event| event
            .get("started")
            .and_then(|started| started.get("workspaceReused"))),
        Some(&json!(true)),
        "{events:?}"
    );
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let output = events
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(output, b"confined", "Git filter read a host-only secret");
    assert!(!marker.exists(), "Git filter executed with host authority");
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":"sandbox-git-script","repoKey":"repo-key","commit":commit,
            "command":["./run.sh"]
        }),
    )
    .await;
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let output = events
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(output, b"repo-script");
    server.abort();
}

#[cfg(windows)]
#[tokio::test]
async fn windows_runner_git_filter_has_only_runner_authority() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use protectorate_node::Jobs;
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path, process::Command};
    let Ok(runner_home) = std::env::var("PROTECTORATE_WINDOWS_RUNNER_HOME") else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git.exe")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git(&["init", "--quiet"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    let secret = Path::new(&runner_home).join(format!("host-secret-git-filter-{stamp}"));
    let marker = Path::new(&runner_home).join(format!("host-filter-marker-{stamp}"));
    fs::write(&secret, b"host credential").unwrap();
    let filter = format!(
        "#!/bin/sh\nif test -f '{}'; then cat '{}'; printf escaped > '{}'; else printf confined; fi\n",
        secret.display().to_string().replace('\\', "/"),
        secret.display().to_string().replace('\\', "/"),
        marker.display().to_string().replace('\\', "/")
    );
    let smudge = "sh.exe .git/filter.sh";
    let poison = format!(
        "$filter = [Text.Encoding]::ASCII.GetString([Convert]::FromBase64String('{}')); Set-Content -Path .git\\filter.sh -Value $filter -Encoding ascii; git config filter.audit.smudge '{}'; git config filter.audit.clean cat; git config filter.audit.required true; Set-Content -Path .git\\info\\attributes -Value 'input.txt filter=audit'; Set-Content -Path input.txt -Value changed; git config --get filter.audit.smudge; if ($LASTEXITCODE -ne 0) {{ exit 1 }}",
        STANDARD.encode(filter), smudge
    );
    fs::write(repo.join("poison.ps1"), poison).unwrap();
    fs::write(repo.join("input.txt"), b"content").unwrap();
    git(&["add", "input.txt", "poison.ps1"]);
    git(&["commit", "--quiet", "-m", "fixture"]);
    let commit = git(&["rev-parse", "HEAD"]);
    let refname = format!("refs/pc/{commit}");
    git(&["update-ref", &refname, &commit]);
    let bundle = temp.path().join("source.bundle");
    git(&["bundle", "create", bundle.to_str().unwrap(), &refname]);
    let bytes = fs::read(bundle).unwrap();
    let jobs = Jobs::open_windows(&runner_home, 1)
        .unwrap()
        .with_plain_mode();
    assert_eq!(jobs.isolation().sandbox, "runner");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let key = format!("git-filter-{}-{stamp}", std::process::id());
    let upload_id = format!("{key}-upload");
    let (code, response) = rpc(
        addr,
        "UploadJobData",
        "node-secret",
        json!({
            "jobId":upload_id,"offset":"0","data":STANDARD.encode(&bytes),
            "last":true,"sha256":format!("{:x}",Sha256::digest(&bytes))
        }),
    )
    .await;
    assert_eq!(code, 200, "{response}");
    for (id, command, uploaded) in [
        (
            upload_id.clone(),
            vec!["git.exe", "rev-parse", "HEAD"],
            true,
        ),
        (
            format!("{key}-poison"),
            vec![
                "powershell.exe",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                ".\\poison.ps1",
            ],
            false,
        ),
        (
            format!("{key}-read"),
            vec!["cmd.exe", "/d", "/c", "type input.txt"],
            false,
        ),
    ] {
        let events = stream_rpc(
            addr,
            "RunJob",
            json!({
                "jobId":id,"repoKey":key,"commit":commit,"bundleUploaded":uploaded,"command":command
            }),
        )
        .await;
        assert_eq!(
            events.last().unwrap()["finished"]["end"],
            "JOB_END_EXITED",
            "{events:?}"
        );
        assert!(
            events.last().unwrap()["finished"]["error"].is_null(),
            "{events:?}"
        );
        assert!(
            events.last().unwrap()["finished"]["exitCode"].is_null(),
            "{events:?}"
        );
        if id.ends_with("-read") {
            assert!(
                events
                    .iter()
                    .any(|event| event["started"]["workspaceReused"] == true),
                "{events:?}"
            );
            let output = events
                .iter()
                .filter_map(|event| event.get("output"))
                .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                String::from_utf8_lossy(&output).trim(),
                "confined",
                "{events:?}"
            );
        }
    }
    assert!(!marker.exists(), "filter ran with node authority");
    let events = stream_rpc(
        addr,
        "RunJob",
        json!({
            "jobId":format!("{key}-plain"),"repoKey":key,"commit":commit,
            "mode":"JOB_MODE_PLAIN",
            "command":["cmd.exe","/d","/c","type input.txt"]
        }),
    )
    .await;
    assert_eq!(
        events.last().unwrap()["finished"]["end"],
        "JOB_END_EXITED",
        "{events:?}"
    );
    let output = events
        .iter()
        .filter_map(|event| event.get("output"))
        .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf8_lossy(&output).trim(), "content");
    assert!(!marker.exists(), "plain Git used sandbox-owned metadata");
    server.abort();
}

#[cfg(windows)]
#[tokio::test]
async fn windows_runner_stages_file_and_archive_sources() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use protectorate_node::Jobs;
    use sha2::{Digest, Sha256};
    use std::{fs, io::Write};
    let Ok(runner_home) = std::env::var("PROTECTORATE_WINDOWS_RUNNER_HOME") else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let jobs = Jobs::open_windows(&runner_home, 1).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Agent::new(Arc::new(Fixed))
        .with_jobs(jobs)
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("source.zip");
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive_path).unwrap());
        zip.start_file("nested/payload.txt", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(b"archive-data").unwrap();
        zip.finish().unwrap();
    }
    for (suffix, bytes, fields, expected) in [
        (
            "file",
            b"file-data".to_vec(),
            json!({"artifactName":"payload.txt"}),
            "file-data",
        ),
        (
            "archive",
            fs::read(&archive_path).unwrap(),
            json!({"artifactArchive":true,"workdir":"nested"}),
            "archive-data",
        ),
    ] {
        let id = format!("windows-stage-{suffix}-{stamp}");
        let (code, response) = rpc(
            addr,
            "UploadJobData",
            "node-secret",
            json!({
                "jobId":id,"offset":"0","data":STANDARD.encode(&bytes),
                "last":true,"sha256":format!("{:x}",Sha256::digest(&bytes))
            }),
        )
        .await;
        assert_eq!(code, 200, "{response}");
        let mut request = json!({
            "jobId":id,"command":["cmd.exe","/d","/c","type payload.txt"]
        });
        request
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let events = stream_rpc(addr, "RunJob", request).await;
        assert_eq!(
            events.last().unwrap()["finished"]["end"],
            "JOB_END_EXITED",
            "source {suffix}: {events:?}"
        );
        let output = events
            .iter()
            .filter_map(|event| event.get("output"))
            .flat_map(|output| STANDARD.decode(output["data"].as_str().unwrap()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(String::from_utf8_lossy(&output).trim(), expected);
    }
    server.abort();
}

#[cfg(target_os = "linux")]
async fn read_job_frame(stream: &mut TcpStream) -> Value {
    let size = usize::from_str_radix(line(stream).await.trim(), 16).unwrap();
    let mut frame = vec![0; size];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(line(stream).await, "\r\n");
    assert_eq!(frame[0], 0);
    serde_json::from_slice(&frame[5..]).unwrap()
}
