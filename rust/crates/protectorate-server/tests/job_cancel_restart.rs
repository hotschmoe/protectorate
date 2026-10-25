//! A cancel the hub acknowledged survives a SIGKILL of the real hub and node.

#[cfg(target_os = "linux")]
use crate::support;

#[cfg(target_os = "linux")]
mod linux {
    use super::support::{serve_node, Hub, Process};
    use serde_json::{json, Value};
    use std::{
        fs,
        io::Write,
        path::Path,
        thread,
        time::{Duration, Instant},
    };

    const JOB: &str = "/protectorate.v1.JobService/";

    struct Rig {
        _root: tempfile::TempDir,
        hub: Hub,
        node: Option<Process>,
        node_home: std::path::PathBuf,
        node_log: std::path::PathBuf,
    }

    impl Rig {
        fn new(name: &str) -> Self {
            let root = tempfile::tempdir().unwrap();
            let mut hub = Hub::start(root.path());
            let node_home = root.path().join("node");
            hub.join(name, &node_home);
            let node_log = root.path().join("node.stderr");
            let node = serve_node(&node_home, &hub.user_home, &node_log);
            hub.wait_fresh(name, &node_log);
            let _ = &mut hub;
            Self {
                _root: root,
                hub,
                node: Some(node),
                node_home,
                node_log,
            }
        }

        fn submit(&self, node: &str, request_id: &str, extra: Value) -> String {
            let mut request = json!({"requestId":request_id,"target":{"node":node},
                "command":["/bin/sh","-c","sleep 600"],"timeout":"900s"});
            for (key, value) in extra.as_object().unwrap() {
                request[key] = value.clone();
            }
            let job = self.hub.call(&format!("{JOB}SubmitJob"), request);
            job["id"].as_str().unwrap().to_owned()
        }

        fn get(&self, id: &str) -> Value {
            self.hub.call(&format!("{JOB}GetJob"), json!({"id":id}))
        }

        fn wait_state(&self, id: &str, want: &str) {
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                let job = self.get(id);
                if job["state"] == want {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "job {id} never reached {want}: {job}; node: {}",
                    fs::read_to_string(&self.node_log).unwrap_or_default()
                );
                thread::sleep(Duration::from_millis(50));
            }
        }

        fn restart(&mut self, node: &str, kill_node: bool) {
            self.hub.stop();
            if kill_node {
                self.node = None;
            }
            self.hub.restart();
            if kill_node {
                self.node = Some(serve_node(
                    &self.node_home,
                    &self.hub.user_home,
                    &self.node_log,
                ));
            }
            self.hub.wait_fresh(node, &self.node_log);
        }
    }

    fn big_source(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        let chunk = vec![0x5a_u8; 1 << 20];
        for part in 0..6 {
            let mut file = fs::File::create(dir.join(format!("blob{part}"))).unwrap();
            for _ in 0..64 {
                file.write_all(&chunk).unwrap();
            }
        }
    }

    fn assert_canceled_for_good(rig: &Rig, id: &str, what: &str) {
        rig.wait_state(id, "JOB_STATE_CANCELED");
        thread::sleep(Duration::from_secs(3));
        let job = rig.get(id);
        assert_eq!(job["state"], "JOB_STATE_CANCELED", "{what}: {job}");
    }

    /// The cancel is acknowledged while the hub still packs the source, so
    /// the node has never heard of the job when the hub dies.
    #[test]
    fn cancel_acknowledged_before_the_node_has_the_job_survives_hub_sigkill() {
        let mut rig = Rig::new("cancel-early");
        let source = rig._root.path().join("source");
        big_source(&source);
        let id = rig.submit(
            "cancel-early",
            "cancel-early-job",
            json!({"source":{"artifact":{"path":source.to_str().unwrap()}}}),
        );
        let canceled = rig.hub.call(&format!("{JOB}CancelJob"), json!({"id":id}));
        assert_eq!(canceled["id"], id.as_str());
        rig.restart("cancel-early", false);
        assert_canceled_for_good(&rig, &id, "hub killed right after the cancel");
    }

    #[test]
    fn cancel_acknowledged_for_a_running_job_survives_hub_sigkill() {
        let mut rig = Rig::new("cancel-run");
        let id = rig.submit("cancel-run", "cancel-run-job", json!({}));
        rig.wait_state(&id, "JOB_STATE_RUNNING");
        rig.hub.call(&format!("{JOB}CancelJob"), json!({"id":id}));
        rig.restart("cancel-run", false);
        assert_canceled_for_good(&rig, &id, "hub killed after the cancel");
    }

    #[test]
    fn cancel_acknowledged_for_a_running_job_survives_hub_and_node_sigkill() {
        let mut rig = Rig::new("cancel-both");
        let id = rig.submit("cancel-both", "cancel-both-job", json!({}));
        rig.wait_state(&id, "JOB_STATE_RUNNING");
        rig.hub.call(&format!("{JOB}CancelJob"), json!({"id":id}));
        rig.restart("cancel-both", true);
        assert_canceled_for_good(&rig, &id, "hub and node killed after the cancel");
    }

    #[test]
    fn cancel_acknowledged_early_survives_hub_and_node_sigkill() {
        let mut rig = Rig::new("cancel-early2");
        let source = rig._root.path().join("source");
        big_source(&source);
        let id = rig.submit(
            "cancel-early2",
            "cancel-early2-job",
            json!({"source":{"artifact":{"path":source.to_str().unwrap()}}}),
        );
        rig.hub.call(&format!("{JOB}CancelJob"), json!({"id":id}));
        rig.restart("cancel-early2", true);
        assert_canceled_for_good(&rig, &id, "hub and node killed after the early cancel");
    }
}
