use protectorate_tailnet_admin::{Admin, Error};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

struct Step {
    method: &'static str,
    path: &'static str,
    body: Option<Value>,
    response: &'static str,
}

fn server(steps: Vec<Step>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        for step in steps {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let header_end = loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
            assert!(
                headers.starts_with(&format!("{} {} HTTP/1.1", step.method, step.path)),
                "{headers}"
            );
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer secret\r\n"),
                "{headers}"
            );
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.trim().parse().ok())
                })
                .unwrap_or(0);
            while bytes.len() - header_end < length {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            if let Some(expected) = step.body {
                let got: Value =
                    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                for (k, v) in expected.as_object().unwrap() {
                    assert_eq!(got.get(k), Some(v), "{got}");
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                step.response.len(),
                step.response
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (url, task)
}

#[test]
fn policy_user_invite_and_key_expiry() {
    let (url, task) = server(vec![
        Step {
            method: "GET",
            path: "/api/v1/user?name=protectorate",
            body: None,
            response: r#"{"users":[{"id":"42","name":"protectorate"}]}"#,
        },
        Step {
            method: "GET",
            path: "/api/v1/node",
            body: None,
            response: r#"{"nodes":[]}"#,
        },
        Step {
            method: "POST",
            path: "/api/v1/preauthkey",
            body: Some(
                json!({"user":"42","reusable":false,"ephemeral":false,"aclTags":["tag:node","tag:tier-care","tag:pool-alpha"]}),
            ),
            response: r#"{"preAuthKey":{"id":"81","key":"authkey"}}"#,
        },
        Step {
            method: "POST",
            path: "/api/v1/preauthkey/expire",
            body: Some(json!({"id":"81"})),
            response: "{}",
        },
    ]);
    let mut admin = Admin::new(&url, "secret", Duration::from_secs(2)).unwrap();
    assert_eq!(admin.ensure_policy_user().unwrap(), 42);
    let key = admin
        .invite_node(
            "node-a",
            "hub",
            "care",
            &["alpha".into()],
            &["alpha".into()],
            Duration::from_secs(60),
        )
        .unwrap();
    assert_eq!(key.key, "authkey");
    assert_eq!(key.id, 81);
    admin.expire_key(key.id).unwrap();
    task.join().unwrap();
}

#[test]
fn revoke_and_retier_preserve_other_tags() {
    let node = r#"{"nodes":[{"id":"7","name":"node-a","givenName":"node-a","tags":["tag:node","tag:tier-shared","tag:pool-alpha"],"expiry":null,"preAuthKey":{"id":"81"}}]}"#;
    let (url, task) = server(vec![
        Step {
            method: "GET",
            path: "/api/v1/node",
            body: None,
            response: node,
        },
        Step {
            method: "POST",
            path: "/api/v1/node/7/tags",
            body: Some(json!({"tags":["tag:node","tag:pool-alpha","tag:tier-care"]})),
            response: "{}",
        },
        Step {
            method: "GET",
            path: "/api/v1/node",
            body: None,
            response: node,
        },
        Step {
            method: "POST",
            path: "/api/v1/node/7/expire",
            body: Some(json!({})),
            response: "{}",
        },
        Step {
            method: "DELETE",
            path: "/api/v1/node/7",
            body: None,
            response: "{}",
        },
    ]);
    let admin = Admin::new(&url, "secret", Duration::from_secs(2)).unwrap();
    assert!(admin.set_tier("node-a", "care").unwrap());
    assert!(admin.revoke("node-a").unwrap());
    task.join().unwrap();
}

#[test]
fn validation_rejects_bad_input() {
    assert!(Admin::new("file:///x", "secret", Duration::from_secs(1)).is_err());
    assert!(Admin::new("http://127.0.0.1", "bad\nkey", Duration::from_secs(1)).is_err());
    let admin = Admin::new("http://127.0.0.1", "secret", Duration::from_secs(1)).unwrap();
    assert!(matches!(
        admin.create_key(&[], Duration::from_secs(1)),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        admin.invite_node("a", "hub", "wrong", &[], &[], Duration::from_secs(1)),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn local_dial_preserves_public_host() {
    let (local_url, task) = server(vec![Step {
        method: "GET",
        path: "/health",
        body: None,
        response: "ok",
    }]);
    let local = local_url.trim_start_matches("http://").parse().unwrap();
    let admin = Admin::new_with_local_addr(
        "http://headscale.invalid:7480",
        "secret",
        Duration::from_secs(2),
        Some(local),
    )
    .unwrap();
    admin.health().unwrap();
    task.join().unwrap();
}
