//! Tmux-backed sessions for long-running harnesses.
use base64::Engine;
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_SOCKET: &str = "protectorate";
pub const MAX_CAPTURE_LINES: usize = 10_000;
const MAX_OUTPUT_BYTES: u64 = 8 << 20;
const COMMAND_OPTION: &str = "@protectorate_command";
const LIST_FORMAT: &str = "#{session_name}\t#{session_created}\t#{session_attached}\t#{session_activity}\t#{window_width}\t#{window_height}\t#{pane_current_path}\t#{pane_dead}\t#{pane_dead_status}\t#{pane_dead_signal}\t#{pane_dead_time}\t#{@protectorate_command}";

#[derive(Debug)]
pub enum Error {
    InvalidName,
    NotFound,
    Exists,
    NotExited,
    NoServer,
    OutputTooLarge,
    InvalidResponse(String),
    Tmux(String),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => write!(f, "invalid session name (want 1-64 of A-Z a-z 0-9 _ -)"),
            Self::NotFound => write!(f, "session not found"),
            Self::Exists => write!(f, "session already exists"),
            Self::NotExited => write!(f, "session is still running"),
            Self::NoServer => write!(f, "no tmux server running"),
            Self::OutputTooLarge => write!(f, "tmux output too large"),
            Self::InvalidResponse(s) => write!(f, "invalid tmux response: {s}"),
            Self::Tmux(s) => write!(f, "tmux: {s}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Running,
    Exited,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    pub name: String,
    pub cwd: PathBuf,
    pub command: Vec<String>,
    pub created: SystemTime,
    pub attached_clients: u32,
    pub last_activity: SystemTime,
    pub cols: u32,
    pub rows: u32,
    pub state: State,
    pub exit_code: i32,
    pub exit_signal: i32,
    pub exited: Option<SystemTime>,
    unreaped: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CreateOptions {
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Manager {
    pub socket: String,
    pub bin: PathBuf,
}

impl Manager {
    pub fn new(socket: impl Into<String>) -> Self {
        let socket = socket.into();
        Self {
            socket: if socket.is_empty() {
                DEFAULT_SOCKET.into()
            } else {
                socket
            },
            bin: "tmux".into(),
        }
    }

    fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-u", "-L", &self.socket]).args(args);
        cmd
    }

    fn run(&self, args: &[String]) -> Result<String, Error> {
        let mut child = self
            .command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let err_thread = thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = stderr.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                if bytes.len() < 64 * 1024 {
                    let keep = count.min(64 * 1024 - bytes.len());
                    bytes.extend_from_slice(&buffer[..keep]);
                }
            }
            Ok::<_, io::Error>(bytes)
        });
        let mut bytes = Vec::new();
        let read_result = stdout
            .by_ref()
            .take(MAX_OUTPUT_BYTES + 1)
            .read_to_end(&mut bytes);
        if bytes.len() as u64 > MAX_OUTPUT_BYTES || read_result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait()?;
        let err_bytes = err_thread
            .join()
            .map_err(|_| Error::InvalidResponse("stderr reader panicked".into()))??;
        read_result?;
        if bytes.len() as u64 > MAX_OUTPUT_BYTES {
            return Err(Error::OutputTooLarge);
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&err_bytes);
            let stderr = stderr.trim();
            if stderr.contains("no server running") || stderr.contains("error connecting to") {
                return Err(Error::NoServer);
            }
            if stderr.contains("can't find session") || stderr.contains("no such session") {
                return Err(Error::NotFound);
            }
            if stderr.contains("duplicate session") {
                return Err(Error::Exists);
            }
            return Err(Error::Tmux(if stderr.is_empty() {
                status.to_string()
            } else {
                stderr.into()
            }));
        }
        String::from_utf8(bytes).map_err(|e| Error::InvalidResponse(e.to_string()))
    }

    pub fn list(&self) -> Result<Vec<Session>, Error> {
        let args = vec!["list-sessions".into(), "-F".into(), LIST_FORMAT.into()];
        let out = match self.run(&args) {
            Ok(out) => out,
            Err(Error::NoServer) => return Ok(Vec::new()),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let sessions = parse_list(&out)?;
        if sessions.iter().any(|s| s.unreaped) {
            self.nudge_reaper();
            thread::sleep(Duration::from_millis(30));
            if let Ok(out) = self.run(&args) {
                return parse_list(&out);
            }
        }
        Ok(sessions)
    }

    fn nudge_reaper(&self) {
        #[cfg(unix)]
        if let Ok(pid) = self.run(&["display-message".into(), "-p".into(), "#{pid}".into()]) {
            if pid.trim().parse::<u32>().is_ok() {
                let _ = Command::new("kill").args(["-CHLD", pid.trim()]).status();
            }
        }
    }

    pub fn get(&self, name: &str) -> Result<Session, Error> {
        valid_name(name)?;
        self.list()?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or(Error::NotFound)
    }

    pub fn create(&self, options: &CreateOptions) -> Result<Session, Error> {
        valid_name(&options.name)?;
        let mut args = vec![
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            options.name.clone(),
        ];
        if let Some(cwd) = &options.cwd {
            args.extend(["-c".into(), escape_arg(&cwd.to_string_lossy())]);
        }
        for (key, value) in &options.env {
            if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0') {
                return Err(Error::InvalidResponse(format!(
                    "invalid environment variable {key:?}"
                )));
            }
            args.extend(["-e".into(), escape_arg(&format!("{key}={value}"))]);
        }
        if !options.command.is_empty() {
            args.push("--".into());
            args.extend(options.command.iter().map(|a| escape_arg(a)));
        }
        args.extend([
            ";".into(),
            "set-option".into(),
            "-w".into(),
            "-t".into(),
            target(&options.name),
            "remain-on-exit".into(),
            "on".into(),
        ]);
        self.run(&args)?;
        if !options.command.is_empty() {
            let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&options.command)
                    .map_err(|e| Error::InvalidResponse(e.to_string()))?,
            );
            self.run(&[
                "set-option".into(),
                "-t".into(),
                target(&options.name),
                COMMAND_OPTION.into(),
                encoded,
            ])?;
        }
        self.get(&options.name)
    }

    pub fn respawn(&self, name: &str) -> Result<Session, Error> {
        let session = self.get(name)?;
        if session.state != State::Exited {
            return Err(Error::NotExited);
        }
        self.run(&[
            "respawn-pane".into(),
            "-k".into(),
            "-t".into(),
            target(name),
        ])?;
        self.get(name)
    }

    pub fn kill(&self, name: &str) -> Result<(), Error> {
        valid_name(name)?;
        match self.run(&["kill-session".into(), "-t".into(), format!("={name}")]) {
            Ok(_) => Ok(()),
            Err(Error::NoServer) => Err(Error::NotFound),
            Err(e) => Err(e),
        }
    }

    pub fn capture(&self, name: &str, lines: usize) -> Result<String, Error> {
        valid_name(name)?;
        let args = [
            "capture-pane".into(),
            "-p".into(),
            "-e".into(),
            "-J".into(),
            "-S".into(),
            format!("-{}", lines.min(MAX_CAPTURE_LINES)),
            "-t".into(),
            target(name),
        ];
        let out = match self.run(&args) {
            Err(Error::NoServer) => return Err(Error::NotFound),
            other => other?,
        };
        Ok(strip_dead_banner(&out))
    }

    pub fn attach_command(&self, name: &str, read_only: bool) -> Result<(Command, Session), Error> {
        let session = self.get(name)?;
        let mut args = vec!["attach-session".into(), "-t".into(), format!("={name}")];
        if read_only || session.state == State::Exited {
            args.push("-r".into());
        }
        let mut cmd = self.command(&args);
        cmd.env("TERM", "xterm-256color");
        Ok((cmd, session))
    }
}

pub fn valid_name(name: &str) -> Result<(), Error> {
    if (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        Ok(())
    } else {
        Err(Error::InvalidName)
    }
}

fn target(name: &str) -> String {
    format!("={name}:")
}
fn escape_arg(s: &str) -> String {
    if let Some(stripped) = s.strip_suffix(';') {
        format!("{stripped}\\;")
    } else {
        s.into()
    }
}

fn unix_time(s: &str) -> Result<SystemTime, Error> {
    let seconds = s
        .parse::<u64>()
        .map_err(|e| Error::InvalidResponse(e.to_string()))?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}
fn number<T: std::str::FromStr>(s: &str) -> Result<T, Error> {
    s.parse()
        .map_err(|_| Error::InvalidResponse(format!("invalid number {s:?}")))
}
fn signal_number(s: &str) -> Result<i32, Error> {
    if let Ok(n) = s.parse() {
        return Ok(n);
    }
    let name = s.to_ascii_uppercase();
    let name = name.strip_prefix("SIG").unwrap_or(&name);
    let n = match name {
        "HUP" => 1,
        "INT" => 2,
        "QUIT" => 3,
        "ILL" => 4,
        "TRAP" => 5,
        "ABRT" => 6,
        "BUS" => 7,
        "FPE" => 8,
        "KILL" => 9,
        "SEGV" => 11,
        "PIPE" => 13,
        "ALRM" => 14,
        "TERM" => 15,
        _ => return Err(Error::InvalidResponse(format!("unknown signal {s:?}"))),
    };
    Ok(n)
}

pub fn parse_list(out: &str) -> Result<Vec<Session>, Error> {
    let mut result = Vec::new();
    for line in out
        .trim_end_matches('\n')
        .split('\n')
        .filter(|line| !line.is_empty())
    {
        let fields: Vec<_> = line.splitn(12, '\t').collect();
        if fields.len() != 12 {
            return Err(Error::InvalidResponse(format!(
                "want 12 fields, got {}",
                fields.len()
            )));
        }
        let dead = fields[7] == "1";
        let unreaped = dead && fields[8].is_empty() && fields[9].is_empty();
        let state = if dead && !unreaped {
            State::Exited
        } else {
            State::Running
        };
        let exit_signal = if state == State::Exited && !fields[9].is_empty() {
            signal_number(fields[9])?
        } else {
            0
        };
        let exit_code = if state == State::Exited && !fields[8].is_empty() {
            number(fields[8])?
        } else if exit_signal > 0 {
            128 + exit_signal
        } else {
            0
        };
        let exited = if state == State::Exited && !fields[10].is_empty() {
            Some(unix_time(fields[10])?)
        } else {
            None
        };
        let command = if fields[11].is_empty() {
            Vec::new()
        } else {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(fields[11])
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        };
        result.push(Session {
            name: fields[0].into(),
            created: unix_time(fields[1])?,
            attached_clients: number(fields[2])?,
            last_activity: unix_time(fields[3])?,
            cols: number(fields[4])?,
            rows: number(fields[5])?,
            cwd: fields[6].into(),
            command,
            state,
            exit_code,
            exit_signal,
            exited,
            unreaped,
        });
    }
    Ok(result)
}

fn strip_dead_banner(out: &str) -> String {
    let body = out.trim_end_matches('\n');
    let line_start = body.rfind('\n').map_or(0, |n| n + 1);
    let last = strip_sgr(&body[line_start..]);
    if last.starts_with("Pane is dead (") {
        format!("{}\n", body[..line_start].trim_end_matches('\n'))
    } else {
        out.into()
    }
}

fn strip_sgr(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            let mut escape = String::new();
            for x in chars.by_ref() {
                if x == 'm' {
                    break;
                }
                escape.push(x);
            }
            if escape.chars().all(|x| x.is_ascii_digit() || x == ';') {
                continue;
            }
            output.push(c);
            output.push('[');
            output.push_str(&escape);
            output.push('m');
        } else {
            output.push(c);
        }
    }
    output
}

pub fn tail(input: &str, lines: usize) -> String {
    let plain = strip_sgr(input).replace('\t', "    ");
    let trimmed = plain.trim_end_matches([' ', '\t', '\r', '\n']);
    let parts: Vec<_> = trimmed.split('\n').collect();
    parts[parts.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_names() {
        for name in ["", "a:b", "a/b", "a b", "a\n", "é", &"a".repeat(65)] {
            assert!(valid_name(name).is_err());
        }
        for name in ["a", "A_z-9", &"a".repeat(64)] {
            assert!(valid_name(name).is_ok());
        }
    }
    #[test]
    fn parses_exit_and_command() {
        let command =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"["sh","-c","exit 7"]"#);
        let line = format!("a\t10\t0\t11\t80\t24\t/tmp\t1\t7\t\t12\t{command}\n");
        let s = parse_list(&line).unwrap().remove(0);
        assert_eq!(s.state, State::Exited);
        assert_eq!(s.exit_code, 7);
        assert_eq!(s.command, ["sh", "-c", "exit 7"]);
    }
    #[test]
    fn cleans_capture() {
        assert_eq!(
            strip_dead_banner("hi\n\u{1b}[31mPane is dead (status 1)\u{1b}[0m\n"),
            "hi\n"
        );
        assert_eq!(tail("a\n\u{1b}[32mb\u{1b}[0m\n\n", 1), "b");
    }

    #[test]
    fn real_tmux_lifecycle() {
        if Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let socket = format!(
            "protectorate-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let manager = Manager::new(socket);
        assert!(manager.list().unwrap().is_empty());
        let options = CreateOptions {
            name: "job".into(),
            command: vec!["sh".into(), "-c".into(), "printf 'hello\\n'; exit 7".into()],
            ..CreateOptions::default()
        };
        let _created = manager.create(&options).unwrap();
        let mut exited = None;
        for _ in 0..50 {
            let session = manager.get("job").unwrap();
            if session.state == State::Exited {
                exited = Some(session);
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let session = exited.expect("session should exit");
        assert_eq!(session.exit_code, 7);
        assert_eq!(session.command, options.command);
        assert!(manager.capture("job", 1).unwrap().contains("hello"));
        manager.respawn("job").unwrap();
        manager.kill("job").unwrap();
        assert!(manager.list().unwrap().is_empty());
    }
}
