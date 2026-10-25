//! A node follows its machine's address. A hub and two nodes run as real
//! processes in unprivileged user, network and PID namespaces joined by a veth
//! pair: the hub at 10.77.0.1 in the outer namespace, the nodes in a child
//! namespace at 10.77.0.2. The test moves the node's IP and checks that the
//! node rebinds, reports over its own certificate, and that the hub records
//! the change, audits it and reaches the node there. It also checks that the
//! report is refused when the IP is not the connection's, and that no other
//! caller can make it.
//!
//! The scenario re-runs this test binary inside the namespaces. A host that
//! forbids unprivileged namespaces or veth links fails it, like the sandbox
//! scenarios.

#[cfg(target_os = "linux")]
mod linux {
    use crate::support::{self, Process, OWNER};
    use protectorate_connect_client::{Client, Credential, NodeTls};
    use serde_json::{json, Value};
    use std::{
        fs,
        path::Path,
        process::{Command, Stdio},
        time::Duration,
    };

    const INSIDE: &str = "PROTECTORATE_TEST_NETNS";
    const HUB_TLS: &str = "10.77.0.1:7480";
    const REPORT: &str = "/protectorate.v1.NodeService/ReportNodeAddress";

    fn namespaces(args: &[&str]) -> Command {
        let mut command = Command::new("unshare");
        command.args([
            "--user",
            "--map-root-user",
            "--net",
            "--pid",
            "--fork",
            "--kill-child",
            "--mount-proc",
        ]);
        command.args(args);
        command
    }

    fn available() -> Result<(), String> {
        let probe = namespaces(&[
            "sh",
            "-c",
            "ip link add probe0 type veth peer name probe1 && ip link set probe0 up",
        ])
        .output()
        .map_err(|error| error.to_string())?;
        if probe.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&probe.stderr).into_owned())
        }
    }

    #[test]
    fn node_follows_a_changed_address() {
        if std::env::var_os(INSIDE).is_some() {
            return;
        }
        if let Err(reason) = available() {
            panic!("node_address needs unprivileged user, network and PID namespaces with veth links: {reason}");
        }
        let exe = std::env::current_exe().unwrap();
        let output = namespaces(&[
            exe.to_str().unwrap(),
            "node_address::linux::inside",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(INSIDE, "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "scenario failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn sh(script: &str) {
        let output = Command::new("sh").args(["-c", script]).output().unwrap();
        assert!(
            output.status.success(),
            "{script}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Runs `args` in the node's network namespace.
    fn in_node_ns(ns: u32, args: &[&str]) -> Command {
        let mut command = Command::new("nsenter");
        command.arg(format!("--net=/proc/{ns}/ns/net")).arg("--");
        command.args(args);
        command
    }

    fn node_ip(ns: u32, script: &str) {
        let output = in_node_ns(ns, &["sh", "-c", script]).output().unwrap();
        assert!(
            output.status.success(),
            "{script}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn protectorate(ns: Option<u32>, args: &[&str], home: &Path) -> Command {
        let mut command = match ns {
            Some(ns) => {
                let mut command = in_node_ns(ns, &[support::binary()]);
                command.args(args);
                command
            }
            None => {
                let mut command = Command::new(support::binary());
                command.args(args);
                command
            }
        };
        command
            .env("HOME", home)
            .env("PROTECTORATE_NODE_ADDRESS_INTERVAL", "1")
            .env_remove("PROTECTORATE_HOME")
            .env_remove("PROTECTORATE_HUB")
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .env_remove("PROTECTORATE_JOIN_CODE");
        command
    }

    fn spawn_in(ns: u32, args: &[&str], home: &Path, log: &Path) -> Process {
        Process(
            protectorate(Some(ns), args, home)
                .stdout(Stdio::null())
                .stderr(Stdio::from(fs::File::create(log).unwrap()))
                .spawn()
                .unwrap(),
        )
    }

    fn call(client: &Client, procedure: &str, input: Value) -> Value {
        client.call(procedure, &input).unwrap()
    }

    fn denied(client: &Client, address: &str) -> String {
        client
            .call(REPORT, &json!({"address": address}))
            .unwrap_err()
            .message
    }

    fn node_status(client: &Client, name: &str) -> Value {
        call(
            client,
            "/protectorate.v1.NodeService/GetNode",
            json!({"name": name}),
        )
    }

    fn audit(client: &Client, outcome: &str) -> Vec<Value> {
        let reply = call(
            client,
            "/protectorate.v1.AuditService/ListAuditEntries",
            json!({"procedure": REPORT, "outcome": outcome, "limit": 100}),
        );
        reply["entries"].as_array().cloned().unwrap_or_default()
    }

    fn join(owner: &Client, ns: u32, name: &str, ca: &Path, home: &Path, user: &Path) {
        let invite = call(
            owner,
            "/protectorate.v1.NodeService/InviteNode",
            json!({"name": name, "tier": "care"}),
        );
        let output = protectorate(
            Some(ns),
            &[
                "node",
                "join",
                "--hub",
                invite["hubUrl"].as_str().unwrap(),
                "--code",
                invite["code"].as_str().unwrap(),
                "--ca-cert",
                ca.to_str().unwrap(),
                "--name",
                name,
                "--home",
                home.to_str().unwrap(),
            ],
            user,
        )
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "node join {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn inside() {
        if std::env::var_os(INSIDE).is_none() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        let hub_home = root.path().join("hub");
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&hub_home).unwrap();
        let tmux = format!("p-addr-{}", std::process::id());
        let result = std::panic::catch_unwind(|| scenario(root.path(), &user, &hub_home, &tmux));
        support::kill_tmux(&tmux);
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    fn scenario(root: &Path, user: &Path, hub_home: &Path, tmux: &str) {
        sh("ip link set lo up");
        // The node namespace is held by a sleeping process; killing the
        // process (or the PID namespace around everything) removes it.
        let holder = Process(
            Command::new("unshare")
                .args(["--net", "sleep", "3600"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let ns = holder.0.id();
        support::until("node namespace", &root.join("none"), || {
            fs::read_link(format!("/proc/{ns}/ns/net")).ok()
                != fs::read_link("/proc/self/ns/net").ok()
        });
        sh(&format!(
            "ip link add vh type veth peer name vn && ip link set vn netns {ns} && ip addr add 10.77.0.1/24 dev vh && ip link set vh up"
        ));
        node_ip(
            ns,
            "ip link set lo up && ip addr add 10.77.0.2/24 dev vn && ip link set vn up",
        );

        let token = root.join("owner-token");
        fs::write(&token, OWNER).unwrap();
        fs::set_permissions(&token, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        let hub_log = root.join("hub.log");
        let public = format!("https://{HUB_TLS}");
        let _hub = support::spawn(
            &[
                "serve",
                "--home",
                hub_home.to_str().unwrap(),
                "--owner-token-file",
                token.to_str().unwrap(),
                "--listen",
                "127.0.0.1:7470",
                "--tls-listen",
                HUB_TLS,
                "--public-url",
                &public,
                "--node-interval",
                "1s",
                "--release-api-url",
                "http://127.0.0.1:1",
                "--tmux-socket",
                tmux,
            ],
            user,
            &hub_log,
        );
        support::until("hub start", &hub_log, || {
            support::bound(&hub_log, "serving TLS").is_some()
        });
        let owner = Client::new("http://127.0.0.1:7470", OWNER, Duration::from_secs(60)).unwrap();
        support::until("hub status", &hub_log, || {
            owner
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .is_ok()
        });
        let ca = hub_home.join("ca/ca.pem");

        // The node joins without --listen: the address its route to the hub
        // leaves from, on the default port.
        let node_home = root.join("mover");
        join(&owner, ns, "mover", &ca, &node_home, user);
        let joined: Value =
            serde_json::from_slice(&fs::read(node_home.join("node.json")).unwrap()).unwrap();
        assert_eq!(joined["address"], "10.77.0.2:7475", "{joined}");
        let node_log = root.join("mover.log");
        let helper =
            Path::new(support::binary()).with_file_name("protectorate-node-sandbox-helper");
        let node = spawn_in(
            ns,
            &[
                "node",
                "serve",
                "--home",
                node_home.to_str().unwrap(),
                "--allow-plain-jobs",
                "--sandbox-helper",
                helper.to_str().unwrap(),
            ],
            user,
            &node_log,
        );
        support::until("first poll over mutual TLS", &node_log, || {
            node_status(&owner, "mover")["freshness"] == "NODE_FRESHNESS_FRESH"
        });
        assert_eq!(
            node_status(&owner, "mover")["node"]["address"],
            "10.77.0.2:7475"
        );
        // The first report finds the hub already right and changes nothing.
        support::until("first report", &node_log, || {
            fs::read(node_home.join("address-report.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|report| report["ok"] == true)
        });
        assert!(
            audit(&owner, "ok").is_empty(),
            "an unchanged report is not audited"
        );

        // The machine's IP changes, as a DHCP lease can.
        node_ip(
            ns,
            "ip addr del 10.77.0.2/24 dev vn && ip addr add 10.77.0.3/24 dev vn",
        );
        support::until("hub records the new address", &node_log, || {
            node_status(&owner, "mover")["node"]["address"] == "10.77.0.3:7475"
        });
        support::until("hub polls the node at the new address", &node_log, || {
            node_status(&owner, "mover")["freshness"] == "NODE_FRESHNESS_FRESH"
        });
        let joined: Value =
            serde_json::from_slice(&fs::read(node_home.join("node.json")).unwrap()).unwrap();
        assert_eq!(joined["address"], "10.77.0.3:7475", "{joined}");
        support::until("node confirms the report", &node_log, || {
            fs::read_to_string(&node_log)
                .unwrap()
                .contains("told the hub it serves on 10.77.0.3:7475")
        });
        let log = fs::read_to_string(&node_log).unwrap();
        assert!(log.contains("now serving on 10.77.0.3:7475"), "{log}");
        support::until("audit entry", &hub_log, || audit(&owner, "ok").len() == 1);
        let entries = audit(&owner, "ok");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0]["actor"], "node:mover");
        assert_eq!(entries[0]["subject"], "mover");
        let detail = entries[0]["detail"].as_str().unwrap();
        assert!(
            detail.contains("10.77.0.2:7475 -> 10.77.0.3:7475"),
            "{detail}"
        );

        // A report the hub already has is accepted and changes nothing.
        fs::write(
            node_home.join("address-report.json"),
            r#"{"address":"10.77.0.3:7475","time":0,"ok":false,"message":"forced"}"#,
        )
        .unwrap();
        support::until("node reports again", &node_log, || {
            fs::read(node_home.join("address-report.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|report| {
                    report["ok"] == true
                        && report["message"]
                            .as_str()
                            .is_some_and(|m| m.contains("already"))
                })
        });
        assert_eq!(audit(&owner, "ok").len(), 1);

        // Doctor names the recorded address, the routed one and the report.
        let doctor = protectorate(
            Some(ns),
            &[
                "node",
                "doctor",
                "--json",
                "--home",
                node_home.to_str().unwrap(),
            ],
            user,
        )
        .output()
        .unwrap();
        let doctor: Value = serde_json::from_slice(&doctor.stdout).unwrap();
        let check = doctor["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == "address")
            .unwrap_or_else(|| panic!("{doctor}"));
        let detail = check["detail"].as_str().unwrap();
        assert_eq!(check["level"], "ok", "{detail}");
        for want in [
            "recorded 10.77.0.3:7475, assigned to this machine",
            "leaves from 10.77.0.3",
            "last report of 10.77.0.3:7475 ok",
        ] {
            assert!(detail.contains(want), "{detail}");
        }

        // Negative: the node's own certificate with an address that is not
        // the connection's source (here the hub's namespace, 10.77.0.1).
        let tls = node_home.join("tls");
        let identity = NodeTls::from_pem(
            &fs::read(tls.join("ca.pem")).unwrap(),
            &fs::read(tls.join("node.pem")).unwrap(),
            &fs::read(tls.join("node-key.pem")).unwrap(),
        )
        .unwrap();
        let as_node = Client::new_for_node(&public, &identity, Duration::from_secs(30)).unwrap();
        let mismatch = denied(&as_node, "10.77.0.3:7475");
        assert!(
            mismatch.contains("10.77.0.1") && mismatch.contains("only report its own address"),
            "{mismatch}"
        );
        let public_ip = denied(&as_node, "8.8.8.8:7475");
        assert!(public_ip.contains("refusing to listen on"), "{public_ip}");
        let garbage = denied(&as_node, "not an address");
        assert!(garbage.contains("IP:PORT"), "{garbage}");
        assert_eq!(
            node_status(&owner, "mover")["node"]["address"],
            "10.77.0.3:7475"
        );

        // Negative: nobody else can call it, on either listener.
        let grant = call(
            &owner,
            "/protectorate.v1.TokenService/CreateToken",
            json!({"name": "everything", "scopes": ["node_*", "read"], "ttl": "600s"}),
        );
        let capability = grant["secret"].as_str().unwrap();
        let ca_pem = fs::read(&ca).unwrap();
        let plain_owner = denied(&owner, "10.77.0.1:7475");
        assert!(
            plain_owner.contains("mutual TLS client certificate"),
            "{plain_owner}"
        );
        let plain_capability =
            Client::new("http://127.0.0.1:7470", capability, Duration::from_secs(30)).unwrap();
        let refused = denied(&plain_capability, "10.77.0.1:7475");
        assert!(
            refused.contains("mutual TLS client certificate"),
            "{refused}"
        );
        let tls_capability =
            Client::new_with_ca_pem(&public, capability, Duration::from_secs(30), Some(&ca_pem))
                .unwrap();
        let refused = denied(&tls_capability, "10.77.0.1:7475");
        assert!(
            refused.contains("mutual TLS client certificate"),
            "{refused}"
        );
        let no_certificate =
            Client::new_with_ca_pem(&public, "", Duration::from_secs(30), Some(&ca_pem))
                .unwrap()
                .with_credential(Credential::Transport);
        let refused = denied(&no_certificate, "10.77.0.1:7475");
        assert!(
            refused.contains("mutual TLS client certificate"),
            "{refused}"
        );
        let hub_denials = audit(&owner, "denied");
        assert!(hub_denials.len() >= 4, "{hub_denials:?}");
        assert_eq!(
            node_status(&owner, "mover")["node"]["address"],
            "10.77.0.3:7475"
        );

        // An explicit --listen is served exactly as given, never replaced.
        let strict_home = root.join("strict");
        join(&owner, ns, "strict", &ca, &strict_home, user);
        let strict = protectorate(
            Some(ns),
            &[
                "node",
                "serve",
                "--home",
                strict_home.to_str().unwrap(),
                "--listen",
                "10.77.0.9:7475",
                "--allow-plain-jobs",
                "--sandbox-helper",
                helper.to_str().unwrap(),
            ],
            user,
        )
        .output()
        .unwrap();
        assert!(!strict.status.success());
        let stderr = String::from_utf8_lossy(&strict.stderr);
        assert!(
            stderr.contains("cannot listen on 10.77.0.9:7475")
                && stderr.contains("--listen is served exactly as given"),
            "{stderr}"
        );
        let _ = &node;
    }
}
