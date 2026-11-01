//! `doctor` (SPEC 7): checks of this machine as fixed JSON, cached in `doctor.json`, that gate
//! harness eligibility for dispatch and resleeve.
//!
//! - Harness CLIs: path, version and install method (D33), the native update command (run by
//!   the hub, never here) and whether login files or keys are present. Credentials are never read or
//!   printed; only their presence is.
//! - `gh` login, the sandbox prerequisites (bwrap, `systemd-run --user`, `XDG_RUNTIME_DIR`,
//!   linger), disk, clock, curl, and the Go toolchain with the headscale and member binaries.
//!
//! The JSON keeps the same keys in the same order on every run; a value that could not be found
//! is `""`, `false` or `"unknown"`, and `problems` says why.

#![forbid(unsafe_code)]

mod probe;

pub use probe::Out;

use pt_json::{ToJson, record, string_enum};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SCHEMA: u64 = 1;
/// Less free space than this on a checked path is a problem.
pub const LOW_DISK: u64 = 5 << 30;

string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Install {
        Native = "native",
        Npm = "npm",
        Script = "script",
        Other = "other",
        Missing = "missing",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Tri {
        Yes = "yes",
        No = "no",
        Unknown = "unknown",
    }
}

record! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Binary {
        pub name: String,
        pub present: bool,
        pub path: String,
        /// `path` with symlinks resolved, and through a shell wrapper's `exec` of an absolute path;
        /// the install method is read from this file.
        pub real_path: String,
        pub install: Install,
        pub version: String,
        pub error: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Credentials {
        /// False for a harness whose login location is not known (antigravity).
        pub checked: bool,
        pub present: bool,
        /// Where a login was found: home-relative file names or `env:NAME`. Never a value.
        pub found: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Harness {
        /// The wire name the hub uses (`claude`, `pi_local`, ...).
        pub name: String,
        pub binary: Binary,
        /// The harness's own updater (D33), its program resolved like `binary`; empty when it
        /// has none. Codex's npm launcher tells `codex update` it is npm-managed, so the same
        /// command serves both installs.
        pub update: Vec<String>,
        pub credentials: Credentials,
        pub eligible: bool,
        pub reasons: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Gh {
        pub binary: Binary,
        pub logged_in: bool,
        pub account: String,
        pub error: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Sandbox {
        pub bwrap: Binary,
        pub systemd_run_user: bool,
        pub systemd_run_error: String,
        pub xdg_runtime_dir: String,
        pub user: String,
        /// Needed for a headless hub: the user manager outlives logout and starts at boot.
        pub linger: Tri,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Disk {
        pub path: String,
        pub mount: String,
        pub free_bytes: u64,
        pub total_bytes: u64,
        pub ok: bool,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Clock {
        pub ntp: Tri,
        pub synchronized: Tri,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Net {
        pub go: Binary,
        pub headscale: Binary,
        pub member: Binary,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Report {
        pub schema: u64,
        pub checked_ms: u64,
        pub os: String,
        pub arch: String,
        pub harnesses: Vec<Harness>,
        pub gh: Gh,
        pub sandbox: Sandbox,
        pub disks: Vec<Disk>,
        pub clock: Clock,
        pub curl: Binary,
        pub net: Net,
        /// Wire names of harnesses that must not be dispatched or resleeved to (pt-hub reads it).
        pub ineligible: Vec<String>,
        pub problems: Vec<String>,
    }
}

impl Binary {
    fn missing(name: &str) -> Binary {
        Binary {
            name: name.to_string(),
            present: false,
            path: String::new(),
            real_path: String::new(),
            install: Install::Missing,
            version: String::new(),
            error: String::new(),
        }
    }

    fn works(&self) -> bool {
        self.present && self.error.is_empty()
    }
}

impl Report {
    /// Whether the hub may dispatch or resleeve to `harness` (a wire name). Unknown names are
    /// not eligible.
    pub fn eligible(&self, harness: &str) -> bool {
        self.harnesses
            .iter()
            .any(|h| h.name == harness && h.eligible)
    }

    pub fn load(path: &Path) -> Result<Report, String> {
        let bytes = fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        pt_json::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Writes `doctor.json` by rename, so a reader sees the old or the new report.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let mut f = fs::File::create(&tmp).map_err(ctx)?;
        f.write_all(format!("{}\n", self.to_json()).as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(ctx)?;
        fs::rename(&tmp, path).map_err(ctx)
    }
}

/// Runs `program args` with stdin closed, killed past `timeout`, keeping the first 64 KiB of
/// each stream: how the hub runs a harness's update command and asks for its version.
pub fn run_program(program: &Path, args: &[&str], timeout: Duration) -> Result<Out, String> {
    probe::run(program, args, &[], timeout)
}

/// What to check. [`Config::from_env`] fills it from this process.
#[derive(Debug, Clone)]
pub struct Config {
    /// The owner's home, where harness logins live.
    pub home: PathBuf,
    pub path: Option<OsString>,
    /// Harness checks are for the hub; a node sets this false.
    pub harnesses: bool,
    /// A harness program's path by its name (`claude`, `pi`, ...), in place of PATH lookup:
    /// the hub passes the programs it launches.
    pub programs: Vec<(String, PathBuf)>,
    /// Reasons a harness is not usable that only the caller knows, by harness name: the hub
    /// passes `pi_local` without a provider or model in its config.
    pub unconfigured: Vec<(String, String)>,
    pub disk_paths: Vec<PathBuf>,
    /// headscale and member; when None, looked for beside this executable, then on PATH.
    pub headscale: Option<PathBuf>,
    pub member: Option<PathBuf>,
    pub timeout: Duration,
}

impl Config {
    pub fn from_env() -> Config {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        Config {
            disk_paths: vec![home.clone(), std::env::temp_dir()],
            home,
            path: std::env::var_os("PATH"),
            harnesses: true,
            programs: Vec::new(),
            unconfigured: Vec::new(),
            headscale: None,
            member: None,
            timeout: Duration::from_secs(20),
        }
    }

    fn which(&self, name: &str) -> Option<PathBuf> {
        probe::which(name, self.path.as_ref())
    }

    fn program(&self, name: &str) -> Option<PathBuf> {
        match self.programs.iter().find(|(n, _)| n == name) {
            Some((_, p)) => p.is_file().then(|| p.clone()),
            None => self.which(name),
        }
    }

    fn beside_exe_or_path(&self, given: &Option<PathBuf>, name: &str) -> Option<PathBuf> {
        if let Some(p) = given {
            return p.is_file().then(|| p.clone());
        }
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf));
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        exe_dir
            .map(|d| d.join(&file))
            .filter(|p| p.is_file())
            .or_else(|| self.which(name))
    }
}

struct Spec {
    name: &'static str,
    program: &'static str,
    update: &'static [&'static str],
    files: &'static [&'static str],
    env: &'static [&'static str],
    checked: bool,
}

const HARNESSES: &[Spec] = &[
    Spec {
        name: "claude",
        program: "claude",
        update: &["claude", "update"],
        files: &[".claude/.credentials.json"],
        env: &["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"],
        checked: true,
    },
    Spec {
        name: "codex",
        program: "codex",
        update: &["codex", "update"],
        files: &[".codex/auth.json"],
        env: &["OPENAI_API_KEY"],
        checked: true,
    },
    Spec {
        name: "grok",
        program: "grok",
        update: &["grok", "update"],
        files: &[".grok/auth.json"],
        env: &["XAI_API_KEY"],
        checked: true,
    },
    Spec {
        name: "antigravity",
        program: "agy",
        update: &[],
        files: &[],
        env: &[],
        checked: false,
    },
    Spec {
        name: "pi_vendor",
        program: "pi",
        update: &["pi", "update", "self"],
        files: &[".pi/agent/auth.json"],
        env: &[],
        checked: true,
    },
    Spec {
        name: "pi_local",
        program: "pi",
        update: &["pi", "update", "self"],
        files: &[".pi/agent/models.json"],
        env: &[],
        checked: true,
    },
    Spec {
        name: "pi_durable",
        program: "pi",
        update: &["pi", "update", "self"],
        files: &[".pi/agent/auth.json"],
        env: &[],
        checked: true,
    },
];

fn credentials(cfg: &Config, s: &Spec) -> Credentials {
    let mut found = Vec::new();
    for f in s.files {
        if fs::metadata(cfg.home.join(f)).is_ok_and(|m| m.is_file() && m.len() > 0) {
            found.push(f.to_string());
        }
    }
    for e in s.env {
        if std::env::var_os(e).is_some_and(|v| !v.is_empty()) {
            found.push(format!("env:{e}"));
        }
    }
    Credentials {
        checked: s.checked,
        present: !found.is_empty(),
        found,
    }
}

fn harnesses(cfg: &Config, sandbox_ok: &[String], node: &Binary) -> Vec<Harness> {
    let mut programs: Vec<(&str, Binary)> = Vec::new();
    std::thread::scope(|scope| {
        let mut seen = Vec::new();
        let jobs: Vec<_> = HARNESSES
            .iter()
            .filter(|s| {
                let new = !seen.contains(&s.program);
                seen.push(s.program);
                new
            })
            .map(|s| {
                (
                    s.program,
                    scope.spawn(|| {
                        probe::binary(
                            s.program,
                            cfg.program(s.program),
                            Some(&["--version"]),
                            cfg.timeout,
                        )
                    }),
                )
            })
            .collect();
        for (p, j) in jobs {
            programs.push((p, j.join().expect("probe thread")));
        }
    });
    HARNESSES
        .iter()
        .map(|s| {
            let binary = programs
                .iter()
                .find(|(p, _)| *p == s.program)
                .map(|(_, b)| b.clone())
                .expect("probed");
            let credentials = credentials(cfg, s);
            let mut reasons = Vec::new();
            if !binary.present {
                reasons.push(format!("{} not on PATH", s.program));
            } else if !binary.error.is_empty() {
                reasons.push(format!("{} --version: {}", s.program, binary.error));
            }
            if credentials.checked && !credentials.present {
                reasons.push("no login found".into());
            }
            if s.name == "pi_durable" && !node.works() {
                reasons.push("node (D14) not on PATH".into());
            }
            reasons.extend(
                cfg.unconfigured
                    .iter()
                    .filter(|(n, _)| n == s.name)
                    .map(|(_, why)| why.clone()),
            );
            reasons.extend(sandbox_ok.iter().cloned());
            let mut update: Vec<String> = s.update.iter().map(|a| a.to_string()).collect();
            if let Some(first) = update.first_mut()
                && binary.present
            {
                first.clone_from(&binary.path);
            }
            Harness {
                name: s.name.to_string(),
                binary,
                update,
                credentials,
                eligible: reasons.is_empty(),
                reasons,
            }
        })
        .collect()
}

fn gh(cfg: &Config) -> Gh {
    let binary = probe::binary("gh", cfg.which("gh"), Some(&["--version"]), cfg.timeout);
    let mut g = Gh {
        logged_in: false,
        account: String::new(),
        error: String::new(),
        binary,
    };
    if !g.binary.present {
        return g;
    }
    let path = PathBuf::from(&g.binary.path);
    match probe::run(
        &path,
        &["auth", "status", "--hostname", "github.com"],
        &[("GH_PROMPT_DISABLED", "1"), ("NO_COLOR", "1")],
        cfg.timeout,
    ) {
        Ok(o) => {
            g.logged_in = o.ok;
            let text = format!("{}\n{}", o.stdout, o.stderr);
            g.account = text
                .split_whitespace()
                .skip_while(|w| *w != "account")
                .nth(1)
                .unwrap_or("")
                .to_string();
            if !o.ok {
                g.error = o.failure();
            }
        }
        Err(e) => g.error = e,
    }
    g
}

fn user() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_default()
}

fn sandbox(cfg: &Config) -> Sandbox {
    let bwrap = probe::binary(
        "bwrap",
        cfg.which("bwrap"),
        Some(&["--version"]),
        cfg.timeout,
    );
    let xdg = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    let user = user();
    let mut s = Sandbox {
        bwrap,
        systemd_run_user: false,
        systemd_run_error: String::new(),
        xdg_runtime_dir: xdg,
        linger: Tri::Unknown,
        user: user.clone(),
    };
    if !cfg!(target_os = "linux") {
        s.systemd_run_error = "not Linux".into();
        return s;
    }
    match cfg.which("systemd-run") {
        None => s.systemd_run_error = "systemd-run not on PATH".into(),
        Some(p) => match probe::run(
            &p,
            &["--user", "--scope", "--quiet", "true"],
            &[],
            cfg.timeout,
        ) {
            Ok(o) if o.ok => s.systemd_run_user = true,
            Ok(o) => s.systemd_run_error = o.failure(),
            Err(e) => s.systemd_run_error = e,
        },
    }
    let linger_dir = Path::new("/var/lib/systemd/linger");
    if !user.is_empty() && linger_dir.is_dir() {
        s.linger = if linger_dir.join(&user).exists() {
            Tri::Yes
        } else {
            Tri::No
        };
    }
    s
}

fn disks(cfg: &Config) -> (Vec<Disk>, Vec<String>) {
    let mut problems = Vec::new();
    let Some(df) = cfg.which("df") else {
        return (Vec::new(), vec!["df not on PATH; disk not checked".into()]);
    };
    let mut out = Vec::new();
    for path in &cfg.disk_paths {
        let p = path.display().to_string();
        let res = probe::run(&df, &["-Pk", "--", &p], &[], cfg.timeout).and_then(|o| {
            let line = o.stdout.lines().nth(1).map(str::to_string);
            let f: Vec<&str> = line.as_deref().unwrap_or("").split_whitespace().collect();
            let kib = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok());
            match (o.ok, kib(1), kib(3), f.get(5..)) {
                (true, Some(total), Some(free), Some(mount)) if !mount.is_empty() => Ok(Disk {
                    path: p.clone(),
                    mount: mount.join(" "),
                    free_bytes: free * 1024,
                    total_bytes: total * 1024,
                    ok: free * 1024 >= LOW_DISK,
                }),
                _ => Err(o.failure()),
            }
        });
        match res {
            Ok(d) => {
                if !d.ok {
                    problems.push(format!(
                        "disk: {} has {:.1} GiB free",
                        d.path,
                        d.free_bytes as f64 / (1u64 << 30) as f64
                    ));
                }
                out.push(d);
            }
            Err(e) => problems.push(format!("disk: {p}: {e}")),
        }
    }
    (out, problems)
}

fn clock(cfg: &Config) -> Clock {
    let mut c = Clock {
        ntp: Tri::Unknown,
        synchronized: Tri::Unknown,
    };
    let Some(td) = cfg.which("timedatectl") else {
        return c;
    };
    if let Ok(o) = probe::run(
        &td,
        &["show", "--property=NTP", "--property=NTPSynchronized"],
        &[],
        cfg.timeout,
    ) && o.ok
    {
        for line in o.stdout.lines() {
            let tri = |v: &str| match v {
                "yes" => Tri::Yes,
                "no" => Tri::No,
                _ => Tri::Unknown,
            };
            match line.split_once('=') {
                Some(("NTP", v)) => c.ntp = tri(v),
                Some(("NTPSynchronized", v)) => c.synchronized = tri(v),
                _ => {}
            }
        }
    }
    c
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Runs every check. Takes a few seconds: each harness runs `--version`, `gh auth status`
/// asks GitHub, and `systemd-run --user --scope true` starts a scope.
pub fn run(cfg: &Config) -> Report {
    let t = cfg.timeout;
    let ((sandbox, gh, curl), (net, node, (disks, disk_problems), clock)) =
        std::thread::scope(|s| {
            let a = s.spawn(|| {
                (
                    sandbox(cfg),
                    gh(cfg),
                    probe::binary("curl", cfg.which("curl"), Some(&["--version"]), t),
                )
            });
            let b = s.spawn(|| {
                let net = Net {
                    go: probe::binary("go", cfg.which("go"), Some(&["version"]), t),
                    headscale: probe::binary(
                        "headscale",
                        cfg.beside_exe_or_path(&cfg.headscale, "headscale"),
                        Some(&["version"]),
                        t,
                    ),
                    member: probe::binary(
                        "member",
                        cfg.beside_exe_or_path(&cfg.member, "member"),
                        None,
                        t,
                    ),
                };
                let node = probe::binary("node", cfg.which("node"), Some(&["--version"]), t);
                (net, node, disks(cfg), clock(cfg))
            });
            (
                a.join().expect("probe thread"),
                b.join().expect("probe thread"),
            )
        });

    let mut problems = Vec::new();
    let mut sandbox_reasons = Vec::new();
    if cfg.harnesses {
        if !sandbox.bwrap.works() {
            sandbox_reasons.push("sandbox: bwrap missing or broken".to_string());
        }
        if !sandbox.systemd_run_user {
            sandbox_reasons.push(format!(
                "sandbox: systemd-run --user --scope failed: {}",
                sandbox.systemd_run_error
            ));
        }
    }
    let harnesses = if cfg.harnesses {
        harnesses(cfg, &sandbox_reasons, &node)
    } else {
        Vec::new()
    };
    let ineligible: Vec<String> = harnesses
        .iter()
        .filter(|h| !h.eligible)
        .map(|h| h.name.clone())
        .collect();
    for h in harnesses.iter().filter(|h| h.binary.present && !h.eligible) {
        problems.push(format!("{}: {}", h.name, h.reasons.join("; ")));
    }
    problems.extend(sandbox_reasons);
    if cfg.harnesses && sandbox.xdg_runtime_dir.is_empty() {
        problems.push("sandbox: XDG_RUNTIME_DIR is not set".into());
    }
    if cfg!(target_os = "linux") && sandbox.linger != Tri::Yes {
        problems.push(format!(
            "linger is {} for {}: a headless hub needs `loginctl enable-linger {}`",
            sandbox.linger.as_str(),
            sandbox.user,
            sandbox.user
        ));
    }
    if gh.binary.present && !gh.logged_in {
        problems.push(format!("gh: not logged in to github.com: {}", gh.error));
    } else if !gh.binary.present && cfg.harnesses {
        problems.push("gh: not on PATH".into());
    }
    if !curl.works() {
        problems.push("curl: missing or broken; updates and quota readers need it".into());
    }
    if clock.synchronized != Tri::Yes {
        problems.push(format!(
            "clock: NTP synchronized is {}",
            clock.synchronized.as_str()
        ));
    }
    problems.extend(disk_problems);
    for b in [&net.headscale, &net.member] {
        if !b.present {
            problems.push(format!(
                "{}: not found beside this binary or on PATH",
                b.name
            ));
        } else if !b.error.is_empty() {
            problems.push(format!("{}: {}", b.name, b.error));
        }
    }

    Report {
        schema: SCHEMA,
        checked_ms: now_ms(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        harnesses,
        gh,
        sandbox,
        disks,
        clock,
        curl,
        net,
        ineligible,
        problems,
    }
}
