#![cfg(unix)]

mod common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn capability_lifecycle_uses_real_hub_connect_routes() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let issued: Value = serde_json::from_str(
            &cli(
                &url,
                "owner-secret",
                [
                    "--json", "token", "create", "--scope", "read", "phone", "--scope", "events",
                    "--ttl", "1h",
                ],
            )
            .unwrap(),
        )
        .unwrap();
        let id = issued["token"]["id"].as_str().unwrap();
        let secret = issued["secret"].as_str().unwrap();
        assert!(secret.starts_with("pc1."), "{issued}");
        assert_eq!(
            issued["token"]["scopes"],
            serde_json::json!(["events", "read"])
        );
        assert!(cli(&url, secret, ["status"]).is_ok());
        let listed = cli(&url, "owner-secret", ["token", "ls"]).unwrap();
        assert!(listed.contains("phone\tactive"), "{listed}");
        assert!(!listed.contains(secret), "list exposed the token secret");

        let renewed: Value =
            serde_json::from_str(&cli(&url, secret, ["--json", "token", "renew"]).unwrap())
                .unwrap();
        let fresh = renewed["secret"].as_str().unwrap();
        assert!(fresh.starts_with("pc1."), "{renewed}");
        assert_eq!(renewed["token"]["id"], id);
        assert!(cli(&url, fresh, ["status"]).is_ok());
        let error = cli(&url, "owner-secret", ["token", "renew"]).unwrap_err();
        assert!(
            error.contains("only a capability can be renewed"),
            "{error}"
        );

        // Narrowing happens offline; the hub enforces it on every call.
        let narrowed = cli(&url, secret, ["token", "attenuate", "--scope", "events"]).unwrap();
        let narrowed = narrowed.trim();
        assert!(narrowed.starts_with("pc1.") && narrowed != secret);
        let denied = cli(&url, narrowed, ["status"]).unwrap_err();
        assert!(denied.contains("lacks the required scope"), "{denied}");
        let widened = cli(&url, narrowed, ["token", "attenuate", "--scope", "*"]).unwrap();
        let denied = cli(&url, widened.trim(), ["status"]).unwrap_err();
        assert!(denied.contains("lacks the required scope"), "{denied}");
        let renewed: Value =
            serde_json::from_str(&cli(&url, narrowed, ["--json", "token", "renew"]).unwrap())
                .unwrap();
        let denied = cli(&url, renewed["secret"].as_str().unwrap(), ["status"]).unwrap_err();
        assert!(
            denied.contains("lacks the required scope"),
            "renewal widened a narrowed token: {denied}"
        );

        assert_eq!(
            cli(&url, "owner-secret", ["token", "revoke", id]).unwrap(),
            format!("revoked {id} (phone)\n")
        );
        let listed = cli(&url, "owner-secret", ["token", "ls"]).unwrap();
        assert!(listed.contains("phone\trevoked"), "{listed}");
        for token in [secret, fresh, narrowed] {
            let error = cli(&url, token, ["status"]).unwrap_err();
            assert!(error.contains("revoked"), "{error}");
        }
    })
    .await;
    server.abort();
    result.unwrap();
}
