use protectorate_connect_client::Client;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_client::{pull, push};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::Code;
use std::{
    fs, io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::net::TcpListener;

struct Fixed;
impl MetricsCollector for Fixed {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

#[tokio::test]
async fn upload_retries_chunks_whose_node_replies_are_lost() {
    use axum::{http::StatusCode, middleware::from_fn, response::IntoResponse};

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("node-root");
    fs::create_dir(&root).unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let fault_hits = Arc::clone(&hits);
    let staged_root = root.clone();
    let router = Agent::new(Arc::new(Fixed), "secret")
        .with_files(&root, false)
        .unwrap()
        .router()
        .unwrap()
        .layer(from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let hits = Arc::clone(&fault_hits);
                let root = staged_root.clone();
                async move {
                    let number = if request.uri().path().ends_with("/PutFile") {
                        hits.fetch_add(1, Ordering::SeqCst) + 1
                    } else {
                        0
                    };
                    let response = next.run(request).await;
                    if number == 1 || number == 3 {
                        assert!(!root.join("recovered.bin").exists());
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else if number == 5 {
                        assert!(root.join("recovered.bin").is_file());
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        response
                    }
                }
            },
        ));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let source = tmp.path().join("source.bin");
    let contents = vec![0x5a; protectorate_xfer::CHUNK_SIZE + 137];
    fs::write(&source, &contents).unwrap();
    let sent = tokio::task::spawn_blocking(move || {
        let client = Client::new(&endpoint, "secret", Duration::from_secs(10)).unwrap();
        push(&client, "node-a", source, "recovered.bin", false, false).unwrap()
    })
    .await
    .unwrap();
    assert_eq!(sent.size, contents.len() as u64);
    assert_eq!(fs::read(root.join("recovered.bin")).unwrap(), contents);
    assert_eq!(hits.load(Ordering::SeqCst), 6);
    server.abort();
}

#[tokio::test]
async fn real_node_file_and_directory_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("node-root");
    fs::create_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let router = Agent::new(Arc::new(Fixed), "secret")
        .with_files(&root, false)
        .unwrap()
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let source_file = tmp.path().join("source.bin");
    let mut contents = vec![0_u8; protectorate_xfer::CHUNK_SIZE + 137];
    let mut seed = 7_u64;
    for byte in &mut contents {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        *byte = seed as u8;
    }
    fs::write(&source_file, &contents).unwrap();
    let output_file = tmp.path().join("downloaded.bin");
    let endpoint_file = endpoint.clone();
    let source_file_clone = source_file.clone();
    let output_file_clone = output_file.clone();
    let file_result = tokio::task::spawn_blocking(move || {
        let client = Client::new(&endpoint_file, "secret", Duration::from_secs(30)).unwrap();
        let sent = push(
            &client,
            "node-a",
            &source_file_clone,
            "uploaded.bin",
            false,
            false,
        )
        .unwrap();
        let received = pull(
            &client,
            "node-a",
            "uploaded.bin",
            &output_file_clone,
            false,
            false,
        )
        .unwrap();
        (sent, received)
    })
    .await
    .unwrap();
    assert_eq!(file_result.0.sha256, file_result.1.sha256);
    assert_eq!(file_result.0.size, contents.len() as u64);
    assert_eq!(fs::read(&output_file).unwrap(), contents);
    assert_eq!(fs::read(root.join("uploaded.bin")).unwrap(), contents);

    let source_dir = tmp.path().join("source-dir");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(source_dir.join("nested")).unwrap();
    fs::write(source_dir.join("nested/data"), b"directory payload").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("nested/data", source_dir.join("skip-me")).unwrap();
    let output_dir = tmp.path().join("downloaded-dir");
    let dir_result = tokio::task::spawn_blocking(move || {
        let client = Client::new(&endpoint, "secret", Duration::from_secs(30)).unwrap();
        let sent = push(&client, "node-a", &source_dir, "uploaded-dir", false, true).unwrap();
        let received = pull(&client, "node-a", "uploaded-dir", &output_dir, false, true).unwrap();
        (sent, received, output_dir)
    })
    .await
    .unwrap();
    assert_eq!(dir_result.0.sha256, dir_result.1.sha256);
    assert_eq!(
        fs::read(dir_result.2.join("nested/data")).unwrap(),
        b"directory payload"
    );
    #[cfg(unix)]
    {
        assert_eq!(dir_result.0.skipped, 1);
        assert!(!dir_result.2.join("skip-me").exists());
    }
    server.abort();
}

#[tokio::test]
async fn existing_file_and_directory_need_force_and_replace_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("node-root");
    fs::create_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let router = Agent::new(Arc::new(Fixed), "secret")
        .with_files(&root, false)
        .unwrap()
        .router()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let endpoint_copy = endpoint.clone();
    let tmp_path = tmp.path().to_path_buf();
    let root_copy = root.clone();
    tokio::task::spawn_blocking(move || {
        let client = Client::new(&endpoint_copy, "secret", Duration::from_secs(30)).unwrap();
        let source = tmp_path.join("source.bin");
        let first = vec![0x41; protectorate_xfer::CHUNK_SIZE + 31];
        let second = vec![0x5a; protectorate_xfer::CHUNK_SIZE + 73];
        fs::write(&source, &first).unwrap();
        let first_sent = push(&client, "node-a", &source, "dest.bin", false, true).unwrap();
        assert_eq!(first_sent.size, first.len() as u64);
        fs::write(&source, &second).unwrap();
        assert!(push(&client, "node-a", &source, "dest.bin", false, true).is_err());
        assert_eq!(fs::read(root_copy.join("dest.bin")).unwrap(), first);
        let second_sent = push(&client, "node-a", &source, "dest.bin", true, true).unwrap();
        assert_eq!(second_sent.size, second.len() as u64);
        assert_ne!(second_sent.sha256, first_sent.sha256);
        assert_eq!(fs::read(root_copy.join("dest.bin")).unwrap(), second);

        let download = tmp_path.join("download.bin");
        fs::write(&download, b"old download").unwrap();
        assert!(pull(&client, "node-a", "dest.bin", &download, false, false).is_err());
        assert_eq!(fs::read(&download).unwrap(), b"old download");
        let received = pull(&client, "node-a", "dest.bin", &download, true, false).unwrap();
        assert_eq!(received.sha256, second_sent.sha256);
        assert_eq!(fs::read(&download).unwrap(), second);

        let source_dir = tmp_path.join("source-dir");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("old.txt"), b"old tree").unwrap();
        push(&client, "node-a", &source_dir, "dest-dir", false, true).unwrap();
        fs::write(source_dir.join("new.txt"), b"new tree").unwrap();
        assert!(push(&client, "node-a", &source_dir, "dest-dir", false, true).is_err());
        let sent_dir = push(&client, "node-a", &source_dir, "dest-dir", true, true).unwrap();
        let download_dir = tmp_path.join("download-dir");
        fs::create_dir(&download_dir).unwrap();
        fs::write(download_dir.join("stale.txt"), b"stale").unwrap();
        assert!(pull(&client, "node-a", "dest-dir", &download_dir, false, true).is_err());
        assert_eq!(fs::read(download_dir.join("stale.txt")).unwrap(), b"stale");
        let received_dir = pull(&client, "node-a", "dest-dir", &download_dir, true, true).unwrap();
        assert_eq!(received_dir.sha256, sent_dir.sha256);
        assert_eq!(fs::read(download_dir.join("new.txt")).unwrap(), b"new tree");
        assert!(!download_dir.join("stale.txt").exists());
    })
    .await
    .unwrap();
    server.abort();
}

fn frame(value: serde_json::Value) -> Vec<u8> {
    let json = value.to_string().into_bytes();
    let mut out = vec![0];
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(&json);
    out
}

fn end_frame() -> Vec<u8> {
    let json = b"{\"metadata\":{}}";
    let mut out = vec![2];
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(json);
    out
}

#[tokio::test]
async fn incomplete_stream_does_not_publish_destination() {
    use axum::{http::header, routing::post, Router};
    let tmp = tempfile::tempdir().unwrap();
    let destination = tmp.path().join("existing");
    fs::write(&destination, b"original").unwrap();
    let mut body = frame(
        serde_json::json!({"header": {"kind": "FILE_KIND_FILE", "mode": 384, "size": "0", "path": "remote"}}),
    );
    body.extend(frame(serde_json::json!({"trailer": {"size": "0", "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}})));
    let router = Router::new().route(
        "/protectorate.v1.NodeAgentService/GetFile",
        post(move || {
            let body = body.clone();
            async move { ([(header::CONTENT_TYPE, "application/connect+json")], body) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let dest_clone = destination.clone();
    let error = tokio::task::spawn_blocking(move || {
        let client = Client::new(&endpoint, "secret", Duration::from_secs(10)).unwrap();
        pull(&client, "node-a", "remote", &dest_clone, true, false).unwrap_err()
    })
    .await
    .unwrap();
    assert_eq!(error.code, Code::DataLoss);
    assert_eq!(fs::read(destination).unwrap(), b"original");
    server.abort();
}

#[tokio::test]
async fn tampered_stream_trailer_does_not_publish_destination() {
    use axum::{http::header, routing::post, Router};
    let tmp = tempfile::tempdir().unwrap();
    let destination = tmp.path().join("existing");
    fs::write(&destination, b"original").unwrap();
    let mut body =
        frame(serde_json::json!({"header": {"kind": "FILE_KIND_FILE", "mode": 384, "size": "1"}}));
    body.extend(frame(
        serde_json::json!({"chunk": {"data": "eA==", "rawSize": 1}}),
    ));
    body.extend(frame(
        serde_json::json!({"trailer": {"size": "1", "sha256": "00".repeat(32)}}),
    ));
    body.extend(end_frame());
    let router = Router::new().route(
        "/protectorate.v1.NodeAgentService/GetFile",
        post(move || {
            let body = body.clone();
            async move { ([(header::CONTENT_TYPE, "application/connect+json")], body) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let dest_clone = destination.clone();
    let error = tokio::task::spawn_blocking(move || {
        let client = Client::new(&endpoint, "secret", Duration::from_secs(10)).unwrap();
        pull(&client, "node-a", "remote", &dest_clone, true, false).unwrap_err()
    })
    .await
    .unwrap();
    assert_eq!(error.code, Code::DataLoss);
    assert_eq!(fs::read(destination).unwrap(), b"original");
    server.abort();
}
