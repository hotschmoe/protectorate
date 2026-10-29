#![cfg(unix)]

use crate::common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt};
use tokio::net::TcpListener;

fn token_line(output: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix("token: "))
        .unwrap_or_else(|| panic!("no token line: {output}"))
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn devices_commands_add_list_rename_and_revoke_device_tokens() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let scratch = home.path().to_owned();
    let result = tokio::task::spawn_blocking(move || {
        let owner = |args: &[&str]| cli(&url, "owner-secret", args).unwrap();

        // A bad link base is refused before a device exists.
        let error = cli(
            &url,
            "owner-secret",
            ["devices", "add", "phone", "--url", "ftp://hub"],
        )
        .unwrap_err();
        assert!(error.contains("pair URL"), "{error}");
        let error = cli(&url, "owner-secret", ["devices", "add", "phone", "--app"]).unwrap_err();
        assert!(error.contains("--app needs --url"), "{error}");
        cli(&url, "owner-secret", ["devices", "add", "envoy"]).unwrap_err();
        // proto3 JSON omits an empty repeated field.
        assert_eq!(owner(&["--json", "devices", "ls"]), "{}\n");

        let added = owner(&["devices", "add", "laptop"]);
        assert!(added.starts_with("added laptop ("), "{added}");
        assert!(added.contains("shown only once"), "{added}");
        let laptop = token_line(&added);
        assert!(laptop.starts_with("pd1."), "{added}");
        assert!(!added.contains("web:"), "{added}");

        let base = "https://protectorate.example";
        let raw: Value = serde_json::from_str(&owner(&[
            "--json", "devices", "add", "phone", "--url", base,
        ]))
        .unwrap();
        let phone = raw["token"].as_str().unwrap().to_owned();
        assert_eq!(raw["device"]["name"], "phone", "{raw}");
        assert_eq!(raw["device"]["addedBy"], "owner", "{raw}");
        let query = format!("url=https%3A%2F%2Fprotectorate%2Eexample&token={phone}");
        assert_eq!(raw["web"], format!("{base}/#/pair?{query}"), "{raw}");
        assert_eq!(raw["app"], format!("protectorate://pair?{query}"), "{raw}");

        let shown = owner(&["devices", "add", "tablet", "--url", base, "--app"]);
        assert!(shown.starts_with("\x1b["), "no QR: {shown}");
        let tablet = token_line(&shown);
        assert!(
            shown.contains(&format!(
                "app:   protectorate://pair?url=https%3A%2F%2Fprotectorate%2Eexample&token={tablet}"
            )),
            "{shown}"
        );

        // Devices list and manage devices with their own tokens.
        let listed = cli(&url, &laptop, ["devices", "ls"]).unwrap();
        let rows: Vec<&str> = listed.lines().collect();
        assert_eq!(rows[0], "ID\tNAME\tLAST USED\tADDED\tADDED BY");
        assert_eq!(rows.len(), 4, "{listed}");
        let laptop_row = rows
            .iter()
            .find(|row| row.contains("\tlaptop\t"))
            .unwrap_or_else(|| panic!("{listed}"));
        assert!(laptop_row.ends_with("\towner"), "{listed}");
        let id = laptop_row.split('\t').next().unwrap().to_owned();
        assert_eq!(id.len(), 16, "{listed}");

        assert_eq!(
            cli(&url, &laptop, ["devices", "rename", id.as_str(), "desk"]).unwrap(),
            format!("renamed {id} to desk\n")
        );
        cli(&url, &laptop, ["devices", "rename", id.as_str(), "envoy"]).unwrap_err();
        assert!(owner(&["devices", "ls"]).contains(&format!("{id}\tdesk\t")));

        // A token file holding a device token is used as it is, never renewed.
        let file = scratch.join("device.token");
        fs::write(&file, format!("{phone}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let path = file.to_str().unwrap();
        cli(&url, "", ["--token-file", path, "devices", "ls"]).unwrap();
        cli(&url, "", ["--token-file", path, "status"]).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), format!("{phone}\n"));

        assert_eq!(
            owner(&["devices", "revoke", &id]),
            format!("revoked {id}\n")
        );
        assert!(!owner(&["devices", "ls"]).contains(&id));
        let error = cli(&url, &laptop, ["devices", "ls"]).unwrap_err();
        assert!(error.contains("unknown credential"), "{error}");
        cli(&url, &phone, ["devices", "ls"]).unwrap();
        cli(&url, "owner-secret", ["devices", "revoke", "no-such-id"]).unwrap_err();
        for gone in ["pending", "approve"] {
            let error = cli(&url, "owner-secret", ["devices", gone, "X"]).unwrap_err();
            assert!(error.contains("unknown devices subcommand"), "{error}");
        }
        for gone in [&["pair"][..], &["login", "--name", "x"][..]] {
            cli(&url, "owner-secret", gone).unwrap_err();
        }

        // A capability, whatever its scopes, never reaches DeviceService.
        let issued: Value = serde_json::from_str(&owner(&[
            "--json",
            "token",
            "create",
            "--scope",
            "*",
            "everything",
        ]))
        .unwrap();
        let capability = issued["secret"].as_str().unwrap().to_owned();
        cli(&url, &capability, ["devices", "ls"]).unwrap_err();
        cli(&url, &capability, ["devices", "add", "sneaky"]).unwrap_err();
    })
    .await;
    server.abort();
    result.unwrap();
}
