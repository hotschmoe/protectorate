//! The hub and its nodes bind private addresses only and authenticate each
//! other with certificates the hub CA issues when a node joins.
#![cfg(target_os = "linux")]
use crate::support;

use protectorate_connect_client::{Client, NodeTls};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
use support::*;

const METRICS: &str = "/protectorate.v1.NodeAgentService/GetMetrics";

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn refused(args: &[&str], home: &Path) -> String {
    let output = run(args, home);
    assert!(!output.status.success(), "{args:?} was accepted");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn listeners_refuse_public_and_unspecified_addresses() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("hub");
    let token = root.path().join("token");
    fs::write(&token, OWNER).unwrap();
    let serve = |flag: &'static str, address: &'static str| {
        [
            "serve",
            "--home",
            home.to_str().unwrap(),
            "--owner-token-file",
            token.to_str().unwrap(),
            flag,
            address,
        ]
        .map(str::to_owned)
    };
    for address in [
        "0.0.0.0:7470",
        "[::]:7470",
        "8.8.8.8:7470",
        "169.254.1.1:7470",
    ] {
        let args = serve("--tls-listen", address);
        let error = refused(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            root.path(),
        );
        assert!(error.contains("refusing to listen"), "{address}: {error}");
    }
    let args = serve("--listen", "192.168.1.10:7470");
    let error = refused(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        root.path(),
    );
    assert!(error.contains("must be loopback"), "{error}");
    for address in ["0.0.0.0:7475", "1.1.1.1:7475"] {
        let error = refused(&["node", "serve", "--listen", address], root.path());
        assert!(error.contains("refusing to listen"), "{address}: {error}");
        let ca = root.path().join("ca.pem");
        fs::write(&ca, "unused").unwrap();
        let error = refused(
            &[
                "node",
                "join",
                "--hub",
                "https://127.0.0.1:1",
                "--code",
                "unused",
                "--ca-cert",
                ca.to_str().unwrap(),
                "--listen",
                address,
                "--home",
                root.path().join("node").to_str().unwrap(),
            ],
            root.path(),
        );
        assert!(error.contains("refusing to listen"), "{address}: {error}");
    }
}

#[test]
fn joined_node_trusts_only_the_hub_and_removal_revokes_it() {
    let root = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(root.path());
    let ca = hub.ca();
    let ca_pem = fs::read(&ca).unwrap();

    // With the hub CA the TLS handshake succeeds and the hub itself answers,
    // refusing the owner token off its loopback listener.
    let status = "/protectorate.v1.HubService/GetStatus";
    let error = Client::new_with_ca(&hub.tls, OWNER, Duration::from_secs(5), Some(&ca))
        .unwrap()
        .call(status, &json!({}))
        .unwrap_err();
    assert!(error.message.contains("loopback listener"), "{error:?}");
    let error = Client::new_with_trust(&hub.tls, OWNER, Duration::from_secs(5), None)
        .unwrap()
        .call(status, &json!({}))
        .unwrap_err();
    assert!(
        !error.message.contains("loopback listener"),
        "public roots must not verify the hub certificate: {error:?}"
    );

    let node_home = root.path().join("node");
    let invite = hub.join("n1", &node_home);
    assert_eq!(
        invite["caSha256"],
        format!("{:x}", Sha256::digest(&ca_pem)),
        "the invite pins the CA served at /ca.pem"
    );
    assert_eq!(invite["hubUrl"], hub.tls);
    let node_log = root.path().join("node.log");
    let _node = serve_node(&node_home, &hub.user_home, &node_log);
    hub.wait_fresh("n1", &node_log);

    let reuse = run(
        &[
            "node",
            "join",
            "--hub",
            &hub.tls,
            "--code",
            invite["code"].as_str().unwrap(),
            "--ca-cert",
            ca.to_str().unwrap(),
            "--name",
            "n1",
            "--listen",
            &free_port(),
            "--home",
            root.path().join("again").to_str().unwrap(),
        ],
        &hub.user_home,
    );
    assert!(!reuse.status.success(), "a join code works once");

    let address = read_json(&node_home.join("node.json"))["address"]
        .as_str()
        .unwrap()
        .to_owned();
    let cert_id = read_json(&hub.home.join("nodes.json"))["nodes"][0]["certId"]
        .as_str()
        .unwrap()
        .to_owned();
    let server_name = protectorate_tls::node_name("n1", &cert_id);
    let dial = |tls: NodeTls| {
        Client::new_node(&server_name, &address, Some(&tls), Duration::from_secs(5))
            .unwrap()
            .call(METRICS, &json!({}))
    };
    let rogue = protectorate_tls::Ca::open_or_create(&root.path().join("rogue"))
        .unwrap()
        .hub_identity(&[])
        .unwrap();
    assert!(
        dial(
            NodeTls::from_pem(&ca_pem, rogue.cert_pem.as_bytes(), rogue.key_pem.as_bytes())
                .unwrap()
        )
        .is_err(),
        "a hub certificate from another CA is refused"
    );
    assert!(
        dial(
            NodeTls::from_pem(
                &ca_pem,
                &fs::read(node_home.join("tls/node.pem")).unwrap(),
                &fs::read(node_home.join("tls/node-key.pem")).unwrap(),
            )
            .unwrap()
        )
        .is_err(),
        "a node certificate cannot act as the hub"
    );

    // A certificate close to expiry is renewed by the restarted hub, and the
    // node serves the new one without restarting.
    hub.stop();
    let inventory = hub.home.join("nodes.json");
    let mut nodes = read_json(&inventory);
    let soon = protectorate_tls::unix_now() + 24 * 3600;
    nodes["nodes"][0]["certExpires"] = json!(soon);
    fs::write(&inventory, serde_json::to_vec(&nodes).unwrap()).unwrap();
    let old_cert = fs::read(node_home.join("tls/node.pem")).unwrap();
    hub.restart();
    until("certificate renewal", &node_log, || {
        read_json(&inventory)["nodes"][0]["certExpires"]
            .as_i64()
            .is_some_and(|expires| expires > soon + 300 * 24 * 3600)
            && fs::read(node_home.join("tls/node.pem")).unwrap() != old_cert
    });
    hub.wait_fresh("n1", &node_log);

    let mut phases = Vec::new();
    hub.client
        .stream(
            "/protectorate.v1.NodeService/RemoveNode",
            &json!({"name":"n1"}),
            |event| {
                phases.push(event);
                Ok(())
            },
        )
        .unwrap();
    assert!(phases
        .iter()
        .any(|phase| phase["phase"] == "REMOVE_PHASE_REMOVED"));

    // The same name joined again gets a new certificate identity; the removed
    // machine, still serving its old certificate, is no longer trusted.
    let second_home = root.path().join("second");
    hub.join("n1", &second_home);
    let second_log = root.path().join("second.log");
    let _second = serve_node(&second_home, &hub.user_home, &second_log);
    hub.wait_fresh("n1", &second_log);
    hub.call(
        "/protectorate.v1.NodeService/AddNode",
        json!({"node":{"name":"n1","address":address},"update":true}),
    );
    until("hub refuses the removed node", &node_log, || {
        let status = hub.status("n1");
        status["freshness"] == "NODE_FRESHNESS_UNREACHABLE"
            && status["error"]
                .as_str()
                .is_some_and(|error| error.contains("certificate"))
    });
}
