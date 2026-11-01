//! Windows job processes: each job runs in its own Job Object with kill-on-close, created
//! suspended and assigned before its first instruction, so no child escapes the job.

use super::win32::{self, Job};
use super::{Orphan, Receipt};
use pt_frame::msg::{Isolation, SubmitJob};
use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

const CREATE_SUSPENDED: u32 = 0x0000_0004;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Isolator;

impl Isolator {
    pub fn probe(_allow_scope: bool) -> (Isolator, String) {
        (Isolator, "job objects with kill-on-close".into())
    }

    pub fn kind(&self) -> Isolation {
        Isolation::JobObject
    }

    pub fn spawn(
        &self,
        spec: &SubmitJob,
        _unit: &str,
        cwd: &Path,
        out: Stdio,
        err: Stdio,
    ) -> io::Result<Proc> {
        let mut cmd = match (&spec.argv, &spec.script) {
            (Some(argv), None) if !argv.is_empty() => {
                let mut c = Command::new(&argv[0]);
                c.args(&argv[1..]);
                c
            }
            (None, Some(script)) => {
                // cmd parses its own command line; /s strips exactly the outer quotes.
                let mut c = Command::new("cmd.exe");
                c.args(["/d", "/s", "/c"]).raw_arg(format!("\"{script}\""));
                c
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a job needs exactly one of argv and script",
                ));
            }
        };
        cmd.current_dir(cwd)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
        let job = Job::new().map_err(|e| ctx("creating the job object", e))?;
        let mut child = cmd.spawn()?;
        if let Err(e) = job.assign(&child).and_then(|()| win32::resume(&child)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ctx("placing the job in its job object", e));
        }
        Ok(Proc { child, job })
    }

    /// Kill-on-close ended every job process when the old agent's handles closed.
    pub fn reap_orphan(&self, _receipt: Option<&Receipt>, _unit: &str) -> Orphan {
        Orphan::Gone
    }
}

fn ctx(what: &str, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{what}: {e}"))
}

pub struct Proc {
    child: Child,
    job: Job,
}

impl Proc {
    pub fn receipt(&self) -> Receipt {
        Receipt {
            pid: self.child.id(),
            start_time: None,
            scope: None,
        }
    }

    pub fn try_wait(&mut self) -> io::Result<Option<i64>> {
        Ok(self
            .child
            .try_wait()?
            .map(|s| s.code().map_or(-1, |c| i64::from(c as u32))))
    }

    /// Windows has no polite stop for a console tree from a service; terminate the job.
    pub fn terminate(&mut self) {
        let _ = self.job.terminate(1);
    }

    pub fn kill(&mut self) {
        let _ = self.job.terminate(1);
    }

    pub fn lower(&mut self) -> io::Result<()> {
        self.job.lower()
    }
}
