use std::process::Command;

pub struct PrivateTmux {
    socket: String,
}

impl PrivateTmux {
    pub fn new(socket: impl Into<String>) -> Self {
        Self {
            socket: socket.into(),
        }
    }
}

impl Drop for PrivateTmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::PrivateTmux;
    use protectorate_session::{CreateOptions, Manager};
    use std::{
        panic,
        path::Path,
        process::Command,
        time::{Duration, Instant},
    };

    #[test]
    fn panic_reaps_private_tmux_and_its_pane() {
        let socket = format!("p-rust-panic-{}", std::process::id());
        let manager = Manager::new(&socket);
        let session = manager
            .create(&CreateOptions {
                name: "panic-cleanup".into(),
                command: vec!["/usr/bin/sleep".into(), "30".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(session.name, "panic-cleanup");
        let output = Command::new("tmux")
            .args([
                "-L",
                &socket,
                "display-message",
                "-p",
                "-t",
                "=panic-cleanup:",
                "#{pane_pid}",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let pane_pid: u32 = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();

        let result = panic::catch_unwind(|| {
            let _tmux = PrivateTmux::new(&socket);
            panic!("simulate assertion failure after sleeve spawn");
        });
        assert!(result.is_err());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Path::new(&format!("/proc/{pane_pid}")).exists() {
            assert!(
                Instant::now() < deadline,
                "pane process survived test cleanup"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(manager.list().unwrap().is_empty());
    }
}
