use std::path::PathBuf;
use std::process::Command;

use crate::event::Harness;

pt_json::string_enum! {
    /// Unsupervised runs with the harness's own approvals off (the sandbox is the boundary);
    /// supervised routes each approval to the hub as `approval_request`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Permission {
        Unsupervised = "unsupervised",
        Supervised = "supervised",
    }
}

/// How to launch one incarnation.
pub struct SpawnSpec {
    pub harness: Harness,
    /// The harness executable, as the prefix (or this process) resolves it.
    pub program: PathBuf,
    pub cwd: PathBuf,
    /// The incarnation's generation, stamped on every event.
    pub g: u64,
    pub model: Option<String>,
    /// Pi only: the provider from Pi's own catalog or models.json.
    pub provider: Option<String>,
    pub effort: Option<String>,
    /// A native session id to resume.
    pub resume: Option<String>,
    pub permission: Permission,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    /// Further official flags, appended where the harness's argv takes options.
    pub extra_args: Vec<String>,
    /// The sandbox or scope wrapper; the harness program and its args are appended to it.
    pub prefix: Option<Command>,
}

impl SpawnSpec {
    pub fn new(harness: Harness, program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        SpawnSpec {
            harness,
            program: program.into(),
            cwd: cwd.into(),
            g: 0,
            model: None,
            provider: None,
            effort: None,
            resume: None,
            permission: Permission::Unsupervised,
            env: Vec::new(),
            env_remove: Vec::new(),
            extra_args: Vec::new(),
            prefix: None,
        }
    }
}
