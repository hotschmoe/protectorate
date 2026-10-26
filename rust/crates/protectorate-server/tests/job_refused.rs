//! A node that refuses a job reports why instead of losing the job.

#[cfg(target_os = "linux")]
use crate::support;

#[cfg(target_os = "linux")]
mod linux {
    use super::support::{spawn, Hub};
    use serde_json::json;
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    #[test]
    fn care_node_without_plain_jobs_reports_the_refusal() {
        let root = tempfile::tempdir().unwrap();
        let hub = Hub::start(root.path());
        let node_home = root.path().join("node");
        hub.join("refusing-peer", &node_home);
        let node_log = root.path().join("node.stderr");
        let _node = spawn(
            &["node", "serve", "--home", node_home.to_str().unwrap()],
            &hub.user_home,
            &node_log,
        );
        hub.wait_fresh("refusing-peer", &node_log);
        let submitted = hub.call(
            "/protectorate.v1.JobService/SubmitJob",
            json!({"requestId":"refused-job","target":{"node":"refusing-peer"},
                "command":["/bin/true"],"timeout":"60s"}),
        );
        let id = submitted["id"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let job = hub.call("/protectorate.v1.JobService/GetJob", json!({"id":id}));
            if job["state"] == "JOB_STATE_FAILED" {
                let error = job["error"].as_str().unwrap_or_default();
                assert!(
                    error.contains("node refused the job")
                        && error.contains("plain jobs are disabled"),
                    "{job}"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "job did not fail: {job}; node: {}",
                fs::read_to_string(&node_log).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
}
