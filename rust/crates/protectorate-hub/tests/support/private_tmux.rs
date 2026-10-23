use std::{path::PathBuf, process::Command};

/// A private tmux server for one test. Dropping it stops the server and
/// removes the socket, which tmux itself leaves behind.
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
        let dir =
            std::env::var_os("TMUX_TMPDIR").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let uid = unsafe { libc::getuid() };
        let _ = std::fs::remove_file(dir.join(format!("tmux-{uid}")).join(&self.socket));
    }
}
