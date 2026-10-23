//! A live node job survives a SIGKILL of the real hub process.

#[cfg(unix)]
mod unix {
    use protectorate_connect_client::Client;
    use serde_json::{json, Value};
    use std::{
        fs,
        net::TcpListener,
        os::unix::fs::PermissionsExt,
        path::Path,
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    struct Process(Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn free_port() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().to_string()
    }

    fn token(path: &Path, value: &str) {
        fs::write(path, value).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn spawn(args: &[&str], home: &Path, output: &Path) -> Process {
        let child = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
            .args(args)
            .env("HOME", home)
            .env_remove("PROTECTORATE_HOME")
            .env_remove("PROTECTORATE_HUB")
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(output).unwrap()))
            .spawn()
            .unwrap();
        Process(child)
    }

    fn call(client: &Client, path: &str, value: Value) -> Value {
        client.call(path, &value).unwrap()
    }

    fn ready(client: &Client, process: &mut Process, output: &Path) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if client
                .call("/protectorate.v1.HubService/GetStatus", &json!({}))
                .is_ok()
            {
                return;
            }
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "server exited before ready: {}",
                fs::read_to_string(output).unwrap_or_default()
            );
            assert!(Instant::now() < deadline, "server did not become ready");
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn running_node_job_replays_once_after_hub_sigkill_and_restart() {
        let root = tempfile::tempdir().unwrap();
        let user_home = root.path().join("user");
        let hub_home = root.path().join("hub");
        let node_home = root.path().join("node");
        fs::create_dir(&user_home).unwrap();
        fs::create_dir(&hub_home).unwrap();
        fs::create_dir(&node_home).unwrap();
        let owner = root.path().join("owner-token");
        let agent = root.path().join("agent-token");
        token(&owner, "real-restart-owner-token");
        token(&agent, "real-restart-agent-token");
        let node_addr = free_port();
        let hub_addr = free_port();
        let node_log = root.path().join("node.stderr");
        let hub_log = root.path().join("hub.stderr");
        let hub_restart_log = root.path().join("hub-restart.stderr");
        let binary = env!("CARGO_BIN_EXE_protectorate-rs");
        let helper = Path::new(binary).with_file_name("protectorate-node-sandbox-helper");
        let mut node = spawn(
            &[
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
            ],
            &user_home,
            &node_log,
        );
        let node_client = Client::new(
            &format!("http://{node_addr}"),
            "real-restart-agent-token",
            Duration::from_millis(500),
        )
        .unwrap();
        // NodeService and HubService differ; wait on the node's metrics API.
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if node_client
                .call("/protectorate.v1.NodeAgentService/GetMetrics", &json!({}))
                .is_ok()
            {
                break;
            }
            assert!(
                node.0.try_wait().unwrap().is_none(),
                "node exited: {}",
                fs::read_to_string(&node_log).unwrap_or_default()
            );
            assert!(Instant::now() < deadline, "node did not become ready");
            thread::sleep(Duration::from_millis(50));
        }
        let hub_args = [
            "serve",
            "--home",
            hub_home.to_str().unwrap(),
            "--owner-token-file",
            owner.to_str().unwrap(),
            "--agent-token-file",
            agent.to_str().unwrap(),
            "--listen",
            &hub_addr,
            "--release-api-url",
            "http://127.0.0.1:1",
        ];
        let mut hub = spawn(&hub_args, &user_home, &hub_log);
        let client = Client::new(
            &format!("http://{hub_addr}"),
            "real-restart-owner-token",
            Duration::from_secs(2),
        )
        .unwrap();
        ready(&client, &mut hub, &hub_log);
        call(
            &client,
            "/protectorate.v1.NodeService/AddNode",
            json!({"node":{"name":"restart-peer","address":node_addr,"tier":"care"}}),
        );
        let submitted = call(
            &client,
            "/protectorate.v1.JobService/SubmitJob",
            json!({"requestId":"real-restart-job","target":{"node":"restart-peer"},
                "command":["/bin/sh","-c","printf start; sleep 4; printf finish"],"timeout":"20s"}),
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
                "job did not produce its first chunk: {}",
                fs::read_to_string(&hub_log).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(20));
        }
        hub.0.kill().unwrap();
        hub.0.wait().unwrap();
        // The node and its job remain alive while the hub process is absent.
        assert!(node.0.try_wait().unwrap().is_none());
        let mut restarted = spawn(&hub_args, &user_home, &hub_restart_log);
        ready(&client, &mut restarted, &hub_restart_log);
        let deadline = Instant::now() + Duration::from_secs(20);
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
            assert!(
                Instant::now() < deadline,
                "job did not recover: {job}; hub: {}",
                fs::read_to_string(&hub_restart_log).unwrap_or_default()
            );
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
        assert_eq!(events[0]["job"]["state"], "JOB_STATE_SUCCEEDED");
        let logs = events
            .iter()
            .filter_map(|event| event.get("log"))
            .collect::<Vec<_>>();
        assert_eq!(
            logs.len(),
            1,
            "output was replayed more than once: {events:?}"
        );
        assert!(logs[0]["offset"].is_null() || logs[0]["offset"] == "0");
        assert_eq!(logs[0]["data"], "c3RhcnRmaW5pc2g=");
        let replay = call(
            &client,
            "/protectorate.v1.JobService/SubmitJob",
            json!({"requestId":"real-restart-job","target":{"node":"restart-peer"},"command":["/bin/sh","-c","printf start; sleep 4; printf finish"],"timeout":"20s"}),
        );
        assert_eq!(replay["id"], id);
    }
}
