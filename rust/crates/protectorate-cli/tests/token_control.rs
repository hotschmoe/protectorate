#![cfg(unix)]

mod common;

use common::cli;
use protectorate_auth::Method as ScopeMethod;
use protectorate_hub::Hub;
use protectorate_wire::Catalog;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn token_lifecycle_uses_real_hub_connect_routes() {
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
        assert!(!secret.is_empty());
        assert_eq!(
            issued["token"]["scopes"],
            serde_json::json!(["events", "read"])
        );
        assert!(cli(&url, secret, ["status"]).is_ok());
        let listed = cli(&url, "owner-secret", ["token", "ls"]).unwrap();
        assert!(listed.contains("phone\tactive"), "{listed}");
        assert!(!listed.contains(secret), "list exposed the token secret");
        let changed = cli(
            &url,
            "owner-secret",
            [
                "token",
                "scopes",
                "phone",
                "--add",
                "terminal:readonly",
                "--remove",
                "events",
            ],
        )
        .unwrap();
        assert!(changed.contains("terminal:readonly"), "{changed}");
        assert!(!changed.contains("events"), "{changed}");
        assert_eq!(
            cli(&url, "owner-secret", ["token", "revoke", id]).unwrap(),
            format!("revoked {id} (phone)\n")
        );
        let listed = cli(&url, "owner-secret", ["token", "ls"]).unwrap();
        assert!(listed.contains("phone\trevoked"), "{listed}");
        assert!(cli(&url, secret, ["status"]).is_err());
    })
    .await;
    server.abort();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn orchestrator_scopes_updates_newest_active_token_during_rotation_grace() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let catalog = Catalog::embedded().unwrap();
    let methods = catalog
        .methods()
        .map(|method| ScopeMethod {
            tool: &method.tool,
            service: &method.service,
            read_only: method.read_only,
            owner_only: method.owner_only,
        })
        .collect::<Vec<_>>();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let expired = hub
        .identity
        .create(
            "orchestrator-envoy",
            &["read"],
            Duration::from_secs(1),
            now - 3600,
            &methods,
        )
        .unwrap();
    let old = hub
        .identity
        .create(
            "orchestrator-envoy",
            &["read"],
            Duration::from_secs(600),
            now - 20,
            &methods,
        )
        .unwrap();
    let revoked = hub
        .identity
        .create(
            "orchestrator-envoy",
            &["read"],
            Duration::from_secs(600),
            now - 10,
            &methods,
        )
        .unwrap();
    hub.identity.revoke(&revoked.token.id, now - 9).unwrap();
    let current = hub
        .identity
        .create(
            "orchestrator-envoy",
            &["read", "events"],
            Duration::from_secs(600),
            now,
            &methods,
        )
        .unwrap();
    let identity = hub.identity.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    tokio::task::spawn_blocking(move || {
        let response = cli(
            &url,
            "owner-secret",
            [
                "orchestrator",
                "scopes",
                "--add",
                "sleeve_kill_sleeve",
                "--remove",
                "events",
                "--json",
            ],
        )
        .unwrap();
        let updated: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(updated["token"]["id"], current.token.id);
        assert_eq!(
            updated["token"]["scopes"],
            serde_json::json!(["read", "sleeve_kill_sleeve"])
        );
        let listing: Value =
            serde_json::from_str(&cli(&url, "owner-secret", ["token", "ls", "--json"]).unwrap())
                .unwrap();
        let tokens = listing["tokens"].as_array().unwrap();
        let find = |id: &str| tokens.iter().find(|token| token["id"] == id).unwrap();
        assert_eq!(find(&old.token.id)["scopes"], serde_json::json!(["read"]));
        assert_eq!(
            find(&expired.token.id)["scopes"],
            serde_json::json!(["read"])
        );
        assert_eq!(
            find(&revoked.token.id)["scopes"],
            serde_json::json!(["read"])
        );
        assert_eq!(
            find(&current.token.id)["scopes"],
            serde_json::json!(["read", "sleeve_kill_sleeve"])
        );
        let text = cli(
            &url,
            "owner-secret",
            ["orchestrator", "scopes", "--remove", "sleeve_kill_sleeve"],
        )
        .unwrap();
        assert!(
            text.contains(&format!("token\t{} (orchestrator-envoy)", current.token.id)),
            "{text}"
        );
        assert!(cli(
            &url,
            current.secret.expose(),
            ["orchestrator", "scopes", "--add", "events"]
        )
        .is_err());
        identity.revoke(&current.token.id, now).unwrap();
        identity.revoke(&old.token.id, now).unwrap();
        let error = cli(
            &url,
            "owner-secret",
            ["orchestrator", "scopes", "--add", "events"],
        )
        .unwrap_err();
        assert!(error.contains("no active Envoy token"), "{error}");
    })
    .await
    .unwrap();
    server.abort();
}
