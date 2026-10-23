//! Render service definitions; installing them is the caller's decision.
use std::fmt;

pub const ROLE_HUB: &str = "hub";
pub const ROLE_NODE: &str = "node";
pub const TRIGGER_LOGON: &str = "logon";
pub const TRIGGER_BOOT: &str = "boot";
pub const CONHOST_COMMAND: &str = r"%SystemRoot%\System32\conhost.exe";
const TERMUX_PREFIX: &str = "/data/data/com.termux/files/usr";

#[derive(Clone, Debug, Default)]
pub struct Spec {
    pub role: String,
    pub binary: String,
    pub args: Vec<String>,
    pub home: String,
    pub tmux_socket: String,
    pub user: String,
    pub path: String,
    pub elevated: bool,
    pub win_user: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

impl Spec {
    pub fn name(&self) -> String {
        format!("protectorate-{}", self.role)
    }
    fn validate(&self) -> Result<(), Error> {
        if self.role != ROLE_HUB && self.role != ROLE_NODE {
            return Err(Error(format!(
                "role must be hub or node, got {:?}",
                self.role
            )));
        }
        if self.binary.is_empty() {
            return Err(Error("binary path is empty".into()));
        }
        Ok(())
    }
    fn command(&self) -> Vec<&str> {
        let mut command = vec![self.binary.as_str()];
        if self.role == ROLE_NODE {
            command.push(ROLE_NODE);
        }
        command.push("serve");
        command.extend(self.args.iter().map(String::as_str));
        command
    }
}

fn quote_systemd(word: &str) -> String {
    let word = word.replace('%', "%%").replace('$', "$$");
    if !word.is_empty() && !word.contains([' ', '\t', '"', '\'', '\\', ';']) {
        return word;
    }
    format!("\"{}\"", word.replace('\\', "\\\\").replace('"', "\\\""))
}
fn quote_systemd_env(key: &str, value: &str) -> String {
    format!(
        "Environment=\"{key}={}\"\n",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    )
}
pub fn systemd(spec: &Spec) -> Result<String, Error> {
    spec.validate()?;
    let words = spec
        .command()
        .into_iter()
        .map(quote_systemd)
        .collect::<Vec<_>>()
        .join(" ");
    let path = if !spec.path.is_empty() {
        spec.path.as_str()
    } else if spec.user.is_empty() {
        "%h/.local/bin:%h/go/bin:/usr/local/bin:/usr/bin:/bin"
    } else {
        "/usr/local/bin:/usr/bin:/bin"
    };
    let mut out = format!("[Unit]\nDescription=Protectorate {}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\n", spec.role);
    if !spec.user.is_empty() {
        out.push_str(&format!("User={}\n", spec.user));
    }
    if !spec.home.is_empty() {
        out.push_str(&quote_systemd_env(
            "PROTECTORATE_HOME",
            &spec.home.replace('%', "%%"),
        ));
    }
    if spec.role == ROLE_HUB && !spec.tmux_socket.is_empty() {
        out.push_str(&quote_systemd_env(
            "PROTECTORATE_TMUX_SOCKET",
            &spec.tmux_socket.replace('%', "%%"),
        ));
    }
    out.push_str(&quote_systemd_env("PATH", path));
    out.push_str(&format!(
        "ExecStart={words}\nRestart=on-failure\nRestartSec=5\n"
    ));
    if spec.role == ROLE_HUB {
        out.push_str("# A SIGKILLed hub must not orphan the headscale child it supervises.\nKillMode=control-group\n# hub drain plus headscale SIGTERM grace (10s) before SIGKILL.\nTimeoutStopSec=45\n");
    } else {
        out.push_str("KillMode=control-group\nTimeoutStopSec=20\n");
    }
    out.push_str("\n[Install]\n");
    out.push_str(if spec.user.is_empty() {
        "WantedBy=default.target\n"
    } else {
        "WantedBy=multi-user.target\n"
    });
    Ok(out)
}
pub fn systemd_notes(spec: &Spec, unit_path: &str) -> String {
    if spec.user.is_empty() {
        let mut out = format!("Install (not run for you):\n  mkdir -p ~/.config/systemd/user && cp {unit_path} ~/.config/systemd/user/\n  systemctl --user daemon-reload\n  systemctl --user enable --now {}.service\n\nTo keep it running while you are logged out, run once as root:\n  sudo loginctl enable-linger $USER\n", spec.name());
        if spec.role == ROLE_HUB {
            out.push_str("\nSleeves need the user manager (systemd-run --user), so prefer the user unit on a hub that runs sleeves.\n");
        }
        out
    } else {
        let mut out = format!("Install (not run for you):\n  sudo cp {unit_path} /etc/systemd/system/\n  sudo systemctl daemon-reload\n  sudo systemctl enable --now {}.service\n", spec.name());
        if spec.role == ROLE_HUB {
            out.push_str(&format!("\nA system unit has no user bus: sleeves (systemd-run --user) need `loginctl enable-linger {}`.\n", spec.user));
        }
        out
    }
}

fn sh_quote(word: &str) -> String {
    if !word.is_empty()
        && !word
            .chars()
            .any(|c| " \t\n\"'\\$`;&|<>(){}*?[]#~!".contains(c))
    {
        return word.into();
    }
    format!("'{}'", word.replace('\'', "'\\''"))
}
pub fn termux_run(spec: &Spec) -> Result<String, Error> {
    spec.validate()?;
    let words = spec
        .command()
        .into_iter()
        .map(sh_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = format!("#!{TERMUX_PREFIX}/bin/sh\nexec 2>&1\n");
    if !spec.home.is_empty() {
        out.push_str(&format!(
            "export PROTECTORATE_HOME={}\n",
            sh_quote(&spec.home)
        ));
    }
    out.push_str(&format!("exec {words}\n"));
    Ok(out)
}
pub fn termux_boot() -> String {
    format!("#!{TERMUX_PREFIX}/bin/sh\ntermux-wake-lock\n. {TERMUX_PREFIX}/etc/profile.d/start-services.sh\n")
}
pub fn termux_notes(spec: &Spec) -> String {
    format!("Setup (not run for you):\n  pkg install termux-services   # then restart Termux\n  sv-enable {}\nInstall the Termux:Boot app, open it once, and keep the boot script executable (chmod +x).\n", spec.name())
}

fn win_quote(word: &str) -> String {
    if !word.is_empty() && !word.contains([' ', '\t', '"']) {
        return word.into();
    }
    // Task Scheduler and sc.exe pass one command line to CommandLineToArgvW.
    // A trailing backslash must be doubled before the closing quote, and
    // backslashes before an embedded quote must be doubled plus one.
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for ch in word.chars() {
        match ch {
            '\\' => slashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(slashes * 2 + 1));
                quoted.push('"');
                slashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(slashes));
                slashes = 0;
                quoted.push(ch);
            }
        }
    }
    quoted.push_str(&"\\".repeat(slashes * 2));
    quoted.push('"');
    quoted
}
fn win_join(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| win_quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}
pub fn task_xml(spec: &Spec, trigger: &str) -> Result<String, Error> {
    spec.validate()?;
    if spec.role != ROLE_NODE {
        return Err(Error("only the node agent runs as a scheduled task".into()));
    }
    let (trig, principal, headless) = match trigger {
        TRIGGER_LOGON => {
            let user = if spec.win_user.is_empty() {
                String::new()
            } else {
                format!("<UserId>{}</UserId>", xml_escape(&spec.win_user))
            };
            let level = if spec.elevated {
                "HighestAvailable"
            } else {
                "LeastPrivilege"
            };
            (
                format!("<LogonTrigger><Enabled>true</Enabled>{user}</LogonTrigger>"),
                format!(
                    "{user}<LogonType>InteractiveToken</LogonType><RunLevel>{level}</RunLevel>"
                ),
                true,
            )
        }
        TRIGGER_BOOT => (
            "<BootTrigger><Enabled>true</Enabled></BootTrigger>".into(),
            "<UserId>S-1-5-18</UserId><RunLevel>HighestAvailable</RunLevel>".into(),
            false,
        ),
        _ => {
            return Err(Error(format!(
                "trigger must be logon or boot, got {trigger:?}"
            )))
        }
    };
    let mut words = vec![spec.role.as_str(), "serve"];
    if !spec.home.is_empty() {
        words.extend(["--home", spec.home.as_str()]);
    }
    words.extend(spec.args.iter().map(String::as_str));
    let (command, args) = if headless {
        (
            CONHOST_COMMAND.to_string(),
            format!(
                "--headless {} {}",
                win_quote(&spec.binary),
                win_join(&words)
            ),
        )
    } else {
        (spec.binary.clone(), win_join(&words))
    };
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n  <RegistrationInfo><Description>Protectorate {}</Description></RegistrationInfo>\n  <Triggers>{trig}</Triggers>\n  <Principals><Principal id=\"Author\">{principal}</Principal></Principals>\n  <Settings>\n    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n    <StartWhenAvailable>true</StartWhenAvailable>\n    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n    <Hidden>true</Hidden>\n    <RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>\n  </Settings>\n  <Actions Context=\"Author\"><Exec>\n    <Command>{}</Command>\n    <Arguments>{}</Arguments>\n  </Exec></Actions>\n</Task>\n", spec.role, xml_escape(&command), xml_escape(&args)))
}
pub fn task_notes(spec: &Spec, trigger: &str, xml_path: &str) -> String {
    let mut out = format!("Register (not run for you; boot triggers and --elevated need an elevated prompt):\n  schtasks /Create /TN {} /XML {} /F\n  schtasks /Run /TN {}\n", win_quote(&spec.name()), win_quote(xml_path), win_quote(&spec.name()));
    if trigger == TRIGGER_LOGON {
        out.push_str("The agent runs under conhost.exe --headless, so it has no console window.\n");
    }
    out
}
pub fn windows_service(spec: &Spec, home: &str) -> Result<String, Error> {
    spec.validate()?;
    if spec.role != ROLE_NODE {
        return Err(Error(
            "only the node agent runs as a Windows service".into(),
        ));
    }
    let name = spec.name();
    let bin = win_join(&spec.command());
    let mut out = format!("REM run from an elevated prompt\nsc.exe create {name} binPath= {} start= auto DisplayName= \"Protectorate node\"\nsc.exe description {name} \"Protectorate node agent\"\nsc.exe failure {name} reset= 86400 actions= restart/5000/restart/30000/restart/60000\n", win_quote(&bin));
    if !home.is_empty() {
        out.push_str(&format!("reg.exe add HKLM\\SYSTEM\\CurrentControlSet\\Services\\{name} /v Environment /t REG_MULTI_SZ /d \"PROTECTORATE_HOME={home}\"\n"));
    }
    out.push_str(&format!("sc.exe start {name}\n"));
    Ok(out)
}
pub fn encode_utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}
