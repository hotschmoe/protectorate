//! A live node job survives a SIGKILL of the real hub process.

#[cfg(target_os = "linux")]
use crate::support;

#[cfg(target_os = "linux")]
mod linux {
    use super::support::{serve_node, Hub};
    use protectorate_connect_client::Client;
    use serde_json::{json, Value};
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    fn call(client: &Client, path: &str, value: Value) -> Value {
        client.call(path, &value).unwrap()
    }

    #[test]
    fn running_node_job_replays_once_after_hub_sigkill_and_restart() {
        let root = tempfile::tempdir().unwrap();
        let mut hub = Hub::start(root.path());
        let node_home = root.path().join("node");
        hub.join("restart-peer", &node_home);
        let node_log = root.path().join("node.stderr");
        let mut node = serve_node(&node_home, &hub.user_home, &node_log);
        hub.wait_fresh("restart-peer", &node_log);
        // The job finishes only once the hub is back and the test opens the gate.
        let gate = root.path().join("gate");
        let script = format!(
            "printf start; until [ -e '{}' ]; do sleep 0.02; done; printf finish",
            gate.display()
        );
        let hub_home = hub.home.clone();
        let hub_log = root.path().join("hub.log");
        let submitted = call(
            &hub.client,
            "/protectorate.v1.JobService/SubmitJob",
            json!({"requestId":"real-restart-job","target":{"node":"restart-peer"},
                "command":["/bin/sh","-c",script],"timeout":"60s"}),
        );
        let id = submitted["id"].as_str().unwrap();
        let job_log = hub_home.join("jobs").join(format!("{id}.log"));
        let deadline = Instant::now() + Duration::from_secs(120);
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
        hub.stop();
        // The node and its job remain alive while the hub process is absent.
        assert!(node.0.try_wait().unwrap().is_none());
        hub.restart();
        fs::write(&gate, b"").unwrap();
        let client = hub.client.clone();
        let deadline = Instant::now() + Duration::from_secs(120);
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
                fs::read_to_string(&hub_log).unwrap_or_default()
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
            json!({"requestId":"real-restart-job","target":{"node":"restart-peer"},"command":["/bin/sh","-c",script],"timeout":"60s"}),
        );
        assert_eq!(replay["id"], id);
    }
}
