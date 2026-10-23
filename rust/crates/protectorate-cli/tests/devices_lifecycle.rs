#![cfg(unix)]

mod common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn devices_commands_use_real_hub_connect_routes() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test")
        .unwrap()
        .with_loopback_device_auth();
    let identity = hub.identity.clone();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let phone = identity
        .start_device("phone-a", "android/arm64", "127.0.0.1", now)
        .unwrap();
    let laptop = identity
        .start_device("laptop-b", "linux/amd64", "127.0.0.1", now)
        .unwrap();
    let watch = identity
        .start_device("watch-c", "watchos/arm", "127.0.0.1", now)
        .unwrap();
    let phone_code = phone.request.user_code.clone();
    let laptop_code = laptop.request.user_code.clone();
    let watch_code = watch.request.user_code.clone();
    let phone_pairing = phone.device_code.expose().to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let owner = |args: &[&str]| cli(&url, "owner-secret", args).unwrap();

        let pending = owner(&["devices", "pending"]);
        let rows: Vec<&str> = pending.lines().collect();
        assert_eq!(rows[0], "CODE\tNAME\tPLATFORM\tREQUESTING IP\tEXPIRES");
        assert_eq!(rows.len(), 4, "{pending}");
        assert!(
            rows.iter().any(|row| row.starts_with(&format!(
                "{phone_code}\tphone-a\tandroid/arm64\t127.0.0.1\t"
            ))),
            "{pending}"
        );
        assert!(
            rows.iter().any(|row| row.starts_with(&format!(
                "{laptop_code}\tlaptop-b\tlinux/amd64\t127.0.0.1\t"
            ))),
            "{pending}"
        );
        assert!(
            rows.iter()
                .any(|row| row
                    .starts_with(&format!("{watch_code}\twatch-c\twatchos/arm\t127.0.0.1\t"))),
            "{pending}"
        );

        assert_eq!(
            owner(&["devices", "approve", &phone_code]),
            format!("approved {phone_code}\n")
        );
        assert_eq!(
            owner(&["devices", "deny", &laptop_code]),
            format!("denied {laptop_code}\n")
        );
        assert_eq!(
            owner(&["--json", "devices", "approve", &watch_code]),
            "{}\n"
        );
        assert_eq!(owner(&["devices", "pending"]).lines().count(), 1);
        // proto3 JSON omits an empty repeated field.
        assert_eq!(owner(&["--json", "devices", "pending"]), "{}\n");

        // A decided or unknown code is gone from the pending set.
        cli(&url, "owner-secret", ["devices", "approve", &laptop_code]).unwrap_err();
        cli(&url, "owner-secret", ["devices", "deny", "XXXX-XXXX"]).unwrap_err();

        // The approved pairing poll issues the device credential; the CLI
        // device commands then run as that device, not only as the owner.
        let polled = identity.poll_device(&phone_pairing, now + 1).unwrap();
        let issued = polled.issued.expect("approved pairing issues a credential");
        let secret = issued.secret.expose().to_owned();
        assert!(!secret.is_empty());

        let listed = cli(&url, &secret, ["devices", "ls"]).unwrap();
        let rows: Vec<&str> = listed.lines().collect();
        assert_eq!(
            rows[0],
            "ID\tNAME\tPLATFORM\tSTATE\tLAST USED\tAPPROVED\tAPPROVER"
        );
        assert_eq!(rows.len(), 2, "{listed}");
        assert!(
            rows[1].contains("\tphone-a\tandroid/arm64\tactive\t"),
            "{listed}"
        );
        assert!(rows[1].ends_with("\towner"), "{listed}");

        let raw: Value = serde_json::from_str(&owner(&["--json", "devices", "ls"])).unwrap();
        let devices = raw["devices"].as_array().unwrap();
        assert_eq!(devices.len(), 1, "{raw}");
        assert_eq!(devices[0]["name"], "phone-a");
        assert_eq!(devices[0]["approvedBy"], "owner");
        let id = devices[0]["id"].as_str().unwrap().to_owned();

        assert_eq!(
            cli(
                &url,
                &secret,
                ["devices", "rename", id.as_str(), "desk desktop"]
            )
            .unwrap(),
            format!("renamed {id} to desk desktop\n")
        );
        let listed = owner(&["devices", "ls"]);
        assert!(
            listed.contains(&format!("{id}\tdesk desktop\tandroid/arm64\tactive")),
            "{listed}"
        );
        cli(
            &url,
            "owner-secret",
            ["devices", "rename", id.as_str(), "envoy"],
        )
        .unwrap_err();

        assert_eq!(
            owner(&["devices", "revoke", &id]),
            format!("revoked {id}\n")
        );
        let listed = owner(&["devices", "ls"]);
        assert!(
            listed.contains(&format!("{id}\tdesk desktop\tandroid/arm64\trevoked")),
            "{listed}"
        );
        cli(&url, &secret, ["devices", "ls"]).unwrap_err();
        cli(&url, "owner-secret", ["devices", "revoke", "no-such-id"]).unwrap_err();

        // A scoped token that is not a device credential never reaches
        // DeviceService.
        let issued: Value = serde_json::from_str(&owner(&[
            "--json", "token", "create", "--scope", "read", "reader",
        ]))
        .unwrap();
        let token_secret = issued["secret"].as_str().unwrap().to_owned();
        cli(&url, &token_secret, ["devices", "ls"]).unwrap_err();
    })
    .await;
    server.abort();
    result.unwrap();
}
