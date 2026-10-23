use protectorate_backup::{create, restore, Cancellation, Options};
use protectorate_hub::Hub;
use serde_json::{json, Value};
use std::{fs, net::SocketAddr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(hub: Hub) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    (address, task)
}

async fn call(address: SocketAddr, procedure: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST {procedure} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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

#[tokio::test]
async fn restored_job_artifacts_and_record_survive_hub_restart() {
    let source = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let jobs = source.path().join("jobs-rust");
    fs::create_dir_all(jobs.join("retained.artifacts/nested")).unwrap();
    fs::write(jobs.join("retained.log"), b"job output\n").unwrap();
    fs::write(
        jobs.join("retained.json"),
        json!({
            "id":"retained",
            "requestId":"retained-request",
            "endpoint":"",
            "cancelRequested":false,
            "job":{
                "id":"retained",
                "state":"JOB_STATE_SUCCEEDED",
                "logBytes":"11",
                "artifacts":[{"path":"nested/result.txt","sizeBytes":"6"}]
            }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(jobs.join("retained.artifacts/nested/result.txt"), b"result").unwrap();
    let cancel = Cancellation::default();
    let archive = create(
        &Options {
            home: source.path().into(),
            dir: backups.path().into(),
            version: "test".into(),
            created_at: Some("2026-10-22T00:00:00Z".parse().unwrap()),
        },
        &cancel,
    )
    .unwrap();
    let destination = backups.path().join("restored");
    restore(&archive, &destination, &cancel).unwrap();
    let (address, task) = serve(Hub::open(&destination, "owner-secret", "test").unwrap()).await;
    let (status, job) = call(
        address,
        "/protectorate.v1.JobService/GetJob",
        json!({"id":"retained"}),
    )
    .await;
    assert_eq!(status, 200, "{job}");
    assert_eq!(job["state"], "JOB_STATE_SUCCEEDED");
    assert_eq!(job["artifacts"][0]["path"], "nested/result.txt");
    assert_eq!(
        fs::read(destination.join("jobs-rust/retained.artifacts/nested/result.txt")).unwrap(),
        b"result"
    );
    task.abort();
}

#[tokio::test]
async fn restored_hub_rejects_submit_canceled_before_backup() {
    let source = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let (address, task) = serve(Hub::open(source.path(), "owner-secret", "test").unwrap()).await;
    let request_id = "canceled-before-submit-12345678";
    let (status, canceled) = call(
        address,
        "/protectorate.v1.JobService/CancelJob",
        json!({"requestId":request_id}),
    )
    .await;
    assert_eq!(status, 404, "{canceled}");
    assert!(canceled.to_string().contains("later submit"), "{canceled}");

    let cancel = Cancellation::default();
    let archive = create(
        &Options {
            home: source.path().into(),
            dir: backups.path().into(),
            version: "test".into(),
            created_at: Some("2026-10-22T00:00:00Z".parse().unwrap()),
        },
        &cancel,
    )
    .unwrap();
    task.abort();
    let _ = task.await;

    let destination = backups.path().join("restored");
    let manifest = restore(&archive, &destination, &cancel).unwrap();
    assert!(manifest
        .files
        .iter()
        .any(|file| file.path == "jobs-rust/canceled-requests"));
    let (address, task) = serve(Hub::open(&destination, "owner-secret", "test").unwrap()).await;
    let (status, rejected) = call(
        address,
        "/protectorate.v1.JobService/SubmitJob",
        json!({
            "requestId":request_id,
            "target":{"node":"unused"},
            "command":["/bin/sh","-c","exit 0"]
        }),
    )
    .await;
    assert_eq!(status, 409, "{rejected}");
    assert!(rejected.to_string().contains("canceled"), "{rejected}");
    let (status, listed) = call(address, "/protectorate.v1.JobService/ListJobs", json!({})).await;
    assert_eq!(status, 200, "{listed}");
    assert!(
        listed["jobs"].as_array().is_none_or(Vec::is_empty),
        "{listed}"
    );
    task.abort();
}
