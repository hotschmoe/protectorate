//! A user systemd service restarts a killed hub and reattaches a running node job.

#[cfg(target_os = "linux")]
mod linux {
    use protectorate_connect_client::Client;
    use protectorate_svcgen::{self, Spec, ROLE_HUB};
    use serde_json::{json, Value};
    use std::{
        fs,
        net::TcpListener,
        os::unix::fs::PermissionsExt,
        path::Path,
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    struct Node(Child);
    impl Drop for Node {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    struct UserService(String);
    impl UserService {
        fn systemctl(&self, args: &[&str]) -> std::process::Output {
            Command::new("systemctl")
                .arg("--user")
                .args(args)
                .arg(&self.0)
                .output()
                .unwrap()
        }

        fn show(&self, property: &str) -> String {
            let output = self.systemctl(&["show", "--property", property, "--value"]);
            assert!(
                output.status.success(),
                "systemctl show {property}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        }
    }
    impl Drop for UserService {
        fn drop(&mut self) {
            let _ = self.systemctl(&["stop"]);
            let _ = self.systemctl(&["reset-failed"]);
        }
    }

    fn free_port() -> String {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string()
    }

    fn token(path: &Path, value: &str) {
        fs::write(path, value).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn call(client: &Client, path: &str, value: Value) -> Value {
        client.call(path, &value).unwrap()
    }

    fn wait_ready(client: &Client, service: &UserService) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .is_ok()
            {
                return;
            }
            let state = service.show("ActiveState");
            assert!(
                state == "active" || state == "activating",
                "hub service stopped in {state} state"
            );
            assert!(
                Instant::now() < deadline,
                "hub service did not become ready"
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn start_hub_service(spec: &Spec, home: &Path) -> UserService {
        let rendered = protectorate_svcgen::systemd(spec).unwrap();
        let mut command = Command::new("systemd-run");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!(
            "protectorate-job-restart-{}-{unique}.service",
            std::process::id()
        );
        command
            .arg("--user")
            .arg(format!("--unit={name}"))
            .arg("--collect")
            .arg(format!("--setenv=HOME={}", home.display()));
        for property in ["Restart", "RestartSec", "KillMode", "TimeoutStopSec"] {
            let value = rendered
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{property}=")))
                .unwrap_or_else(|| panic!("generated unit missing {property}"));
            command.arg(format!("--property={property}={value}"));
        }
        command.arg(&spec.binary).arg("serve").args(&spec.args);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "systemd-run: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        UserService(name)
    }

    #[test]
    #[ignore = "requires a Linux host with a working systemd user manager"]
    fn systemd_restarts_hub_and_running_node_job_replays_once() {
        let manager = Command::new("systemctl")
            .args(["--user", "show-environment"])
            .output()
            .unwrap();
        assert!(manager.status.success(), "systemd user manager unavailable");

        let root = tempfile::tempdir().unwrap();
        let user_home = root.path().join("user");
        let hub_home = root.path().join("hub");
        let node_home = root.path().join("node");
        fs::create_dir(&user_home).unwrap();
        fs::create_dir(&hub_home).unwrap();
        fs::create_dir(&node_home).unwrap();
        let owner = root.path().join("owner-token");
        let agent = root.path().join("agent-token");
        token(&owner, "service-restart-owner-token");
        token(&agent, "service-restart-agent-token");
        let node_addr = free_port();
        let hub_addr = free_port();
        let binary = env!("CARGO_BIN_EXE_protectorate-rs");
        let helper = Path::new(binary).with_file_name("protectorate-node-sandbox-helper");
        let node_log = fs::File::create(root.path().join("node.stderr")).unwrap();
        let node = Command::new(binary)
            .args([
                "node",
                "serve",
                "--home",
                node_home.to_str().unwrap(),
                "--agent-token-file",
                agent.to_str().unwrap(),
                "--allow-plain-jobs",
                "--sandbox-helper",
                helper.to_str().unwrap(),
                "--listen",
                &node_addr,
            ])
            .env("HOME", &user_home)
            .env_remove("PROTECTORATE_HOME")
            .env_remove("PROTECTORATE_HUB")
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .stdout(Stdio::null())
            .stderr(Stdio::from(node_log))
            .spawn()
            .unwrap();
        let mut node = Node(node);
        let node_client = Client::new(
            &format!("http://{node_addr}"),
            "service-restart-agent-token",
            Duration::from_millis(500),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if node_client
                .call("/protectorate.v1.NodeAgentService/GetMetrics", &json!({}))
                .is_ok()
            {
                break;
            }
            assert!(node.0.try_wait().unwrap().is_none(), "node exited");
            assert!(Instant::now() < deadline, "node did not become ready");
            thread::sleep(Duration::from_millis(50));
        }
        let spec = Spec {
            role: ROLE_HUB.into(),
            binary: binary.into(),
            args: vec![
                "--home".into(),
                hub_home.to_str().unwrap().into(),
                "--owner-token-file".into(),
                owner.to_str().unwrap().into(),
                "--agent-token-file".into(),
                agent.to_str().unwrap().into(),
                "--listen".into(),
                hub_addr.clone(),
                "--release-api-url".into(),
                "http://127.0.0.1:1".into(),
            ],
            home: hub_home.to_str().unwrap().into(),
            ..Spec::default()
        };
        let service = start_hub_service(&spec, &user_home);
        let client = Client::new(
            &format!("http://{hub_addr}"),
            "service-restart-owner-token",
            Duration::from_secs(2),
        )
        .unwrap();
        wait_ready(&client, &service);
        call(
            &client,
            "/protectorate.v1.NodeService/AddNode",
            json!({"node":{"name":"service-restart-peer","address":node_addr,"tier":"care"}}),
        );
        let submitted = call(
            &client,
            "/protectorate.v1.JobService/SubmitJob",
            json!({"requestId":"service-restart-job","target":{"node":"service-restart-peer"},
                "command":["/bin/sh","-c","printf start; sleep 12; printf finish"],"timeout":"30s"}),
        );
        let id = submitted["id"].as_str().unwrap();
        let job_log = hub_home.join("jobs-rust").join(format!("{id}.log"));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if fs::read(&job_log).is_ok_and(|bytes| bytes == b"start") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "job did not produce its first chunk"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let original_pid = service.show("MainPID").parse::<u32>().unwrap();
        assert_ne!(original_pid, 0);
        let command_line = fs::read(format!("/proc/{original_pid}/cmdline")).unwrap();
        assert!(command_line
            .windows(binary.len())
            .any(|part| part == binary.as_bytes()));
        assert!(command_line
            .windows(hub_home.as_os_str().len())
            .any(|part| part == hub_home.as_os_str().as_encoded_bytes()));
        let output = Command::new("kill")
            .args(["-KILL", &original_pid.to_string()])
            .output()
            .unwrap();
        assert!(output.status.success(), "failed to kill hub process");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let pid = service.show("MainPID").parse::<u32>().unwrap();
            let restarts = service.show("NRestarts").parse::<u32>().unwrap();
            if restarts >= 1 && pid != 0 && pid != original_pid {
                break;
            }
            assert!(Instant::now() < deadline, "systemd did not restart the hub");
            thread::sleep(Duration::from_millis(100));
        }
        wait_ready(&client, &service);
        assert!(
            node.0.try_wait().unwrap().is_none(),
            "node exited during restart"
        );
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let job = call(
                &client,
                "/protectorate.v1.JobService/GetJob",
                json!({"id":id}),
            );
            if job["state"] == "JOB_STATE_SUCCEEDED" {
                assert_eq!(job["logBytes"], "11");
                break;
            }
            assert!(Instant::now() < deadline, "job did not recover: {job}");
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(fs::read(job_log).unwrap(), b"startfinish");
        let mut events = Vec::new();
        client
            .stream(
                "/protectorate.v1.JobService/WatchJob",
                &json!({"id":id,"fromOffset":"0","follow":false}),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .unwrap();
        let logs = events
            .iter()
            .filter_map(|event| event.get("log"))
            .collect::<Vec<_>>();
        assert_eq!(logs.len(), 1, "output was duplicated: {events:?}");
        assert_eq!(logs[0]["data"], "c3RhcnRmaW5pc2g=");
        let replay = call(
            &client,
            "/protectorate.v1.JobService/SubmitJob",
            json!({"requestId":"service-restart-job","target":{"node":"service-restart-peer"},
                "command":["/bin/sh","-c","printf start; sleep 12; printf finish"],"timeout":"30s"}),
        );
        assert_eq!(replay["id"], id);
    }
}
