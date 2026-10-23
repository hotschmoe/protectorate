//! Device login: pair this machine with a hub through `StartDeviceAuth` and
//! `PollDeviceAuth`, and keep the credential the hub issues in a private file
//! per hub under the state home. Ordinary commands read that file back when no
//! `--token-file` and no token environment variable is set.
use crate::{
    format::{local_minute, timestamp_millis},
    hub_url, Invocation,
};
use protectorate_connect_client::Client;
use protectorate_wire::{Code, Result, RpcError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
#[path = "login_windows.rs"]
mod windows;

#[cfg(windows)]
pub(crate) fn read_private_token_file(path: &Path) -> std::io::Result<String> {
    windows::read_private_file(path)
}

const START_DEVICE_AUTH: &str = "/protectorate.v1.DeviceService/StartDeviceAuth";
const POLL_DEVICE_AUTH: &str = "/protectorate.v1.DeviceService/PollDeviceAuth";
/// The directory under the state home holding one credential file per hub.
const CREDENTIALS_DIR: &str = "credentials";
const DEFAULT_INTERVAL: u64 = 5;
const MAX_INTERVAL: u64 = 300;
const SLOW_DOWN_STEP: u64 = 5;
const POLL_TIMEOUT_FLOOR: Duration = Duration::from_secs(30);

/// Runs `login` with no harness: start a pairing request, wait for a trusted
/// device or the hub to approve it, and save the issued credential. Progress is
/// written to `stdout` as it happens, so the code is visible before the wait.
pub(crate) fn run(invocation: &Invocation, stdout: &mut dyn Write) -> Result<String> {
    let hub = hub_url(invocation);
    let home = state_home(invocation)?;
    let path = credential_path(&home, &hub)?;
    let (name, platform) = device_identity(invocation);
    let client = Client::new_public(&hub, invocation.timeout.max(POLL_TIMEOUT_FLOOR))?;
    let started = match client.call(
        START_DEVICE_AUTH,
        &json!({"name": name, "platform": platform}),
    ) {
        Ok(started) => started,
        Err(error) => {
            return Err(RpcError::new(
                error.code,
                format!("start device authorization at {hub}: {}", error.message),
            ))
        }
    };
    let device_code = field(&started, "deviceCode")
        .ok_or_else(|| RpcError::new(Code::DataLoss, "hub returned no device code"))?;
    let user_code = field(&started, "userCode")
        .ok_or_else(|| RpcError::new(Code::DataLoss, "hub returned no user code"))?;
    let expiry = field(&started, "expiresAt");
    let deadline = expiry
        .as_deref()
        .and_then(timestamp_millis)
        .and_then(|millis| millis.try_into().ok())
        .map(|millis: u64| UNIX_EPOCH + Duration::from_millis(millis));
    let mut interval = started
        .get("interval")
        .and_then(Value::as_u64)
        .filter(|interval| *interval > 0)
        .unwrap_or(DEFAULT_INTERVAL);
    if invocation.json {
        stdout
            .write_all(
                json_line(&json!({
                    "userCode": user_code,
                    "expiresAt": expiry,
                    "interval": interval,
                }))?
                .as_bytes(),
            )
            .and_then(|_| stdout.flush())
            .map_err(|error| {
                RpcError::new(Code::Internal, format!("write login status: {error}"))
            })?;
    } else {
        let waiting = expiry
            .clone()
            .map(local_minute)
            .unwrap_or_else(|| "its expiry".to_owned());
        write_stdout(
            stdout,
            &format!(
                "\nCode: {user_code}\n\nOn a trusted device, approve only if this code matches.\nOn the hub: protectorate devices approve {user_code}\n\nWaiting for approval until {waiting}...\n"
            ),
        )?;
    }
    loop {
        let now = SystemTime::now();
        if deadline.is_some_and(|deadline| now >= deadline) {
            return Err(expired());
        }
        let wait = match deadline {
            Some(deadline) => match deadline.duration_since(now) {
                Ok(remaining) => Duration::from_secs(interval).min(remaining),
                Err(_) => Duration::ZERO,
            },
            None => Duration::from_secs(interval),
        };
        std::thread::sleep(wait);
        let reply = match client.call(POLL_DEVICE_AUTH, &json!({"deviceCode": device_code})) {
            Ok(reply) => reply,
            Err(error) if error.code == Code::ResourceExhausted => {
                interval = next_interval(interval, "rate_limited", 0);
                continue;
            }
            // Keep the visible code usable through a transient lost poll.
            // If the lost reply contained the one-shot secret, the next poll
            // reports `consumed` and asks the user to restart.
            Err(error) if error.code == Code::Unavailable && deadline.is_some() => continue,
            Err(error) => {
                return Err(RpcError::new(
                    error.code,
                    format!("poll device authorization at {hub}: {}", error.message),
                ))
            }
        };
        let status = field(&reply, "status").unwrap_or_default();
        let hub_interval = reply.get("interval").and_then(Value::as_u64).unwrap_or(0);
        interval = next_interval(interval, &status, hub_interval);
        match status.as_str() {
            "pending" | "slow_down" => continue,
            "approved" => {
                let secret = field(&reply, "secret")
                    .filter(|secret| !secret.is_empty())
                    .ok_or_else(|| {
                        RpcError::new(
                            Code::DataLoss,
                            "hub approved this device without returning its credential; run login again",
                        )
                    })?;
                save_credential(&path, &hub, &reply, &secret)?;
                if invocation.json {
                    return json_line(&json!({
                        "status": "approved",
                        "token": reply.get("token"),
                        "credentialFile": path.display().to_string(),
                    }));
                }
                return Ok(format!(
                    "Connected to {hub}. Credential saved in {}.\n",
                    path.display()
                ));
            }
            "denied" => {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "device authorization denied by a trusted device",
                ))
            }
            "expired" => return Err(expired()),
            "consumed" => {
                return Err(RpcError::new(
                    Code::AlreadyExists,
                    "device credential was already delivered; run login again",
                ))
            }
            other => {
                return Err(RpcError::new(
                    Code::DataLoss,
                    format!("unexpected device authorization status {other:?}"),
                ))
            }
        }
    }
}

/// The next poll delay. The hub's own interval never shortens the wait, and a
/// `slow_down` or a rate-limited poll pushes it back the way the hub does.
fn next_interval(current: u64, status: &str, hub_interval: u64) -> u64 {
    let next = match status {
        "slow_down" | "rate_limited" => current + SLOW_DOWN_STEP,
        _ => current,
    };
    next.max(hub_interval).clamp(1, MAX_INTERVAL)
}

fn expired() -> RpcError {
    RpcError::new(
        Code::DeadlineExceeded,
        "device authorization expired; run login again to retry",
    )
}

/// The device name and platform the hub records. Defaults are this machine's
/// hostname and its operating system and architecture.
fn device_identity(invocation: &Invocation) -> (String, String) {
    let name = match &invocation.command {
        crate::Command::DeviceLogin { name, .. } => name.clone(),
        _ => None,
    };
    let platform = match &invocation.command {
        crate::Command::DeviceLogin { platform, .. } => platform.clone(),
        _ => None,
    };
    (
        name.unwrap_or_else(host_name),
        platform.unwrap_or_else(|| format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)),
    )
}

fn host_name() -> String {
    let hostname = env::var(if cfg!(windows) {
        "COMPUTERNAME"
    } else {
        "HOSTNAME"
    })
    .ok()
    .filter(|value| !value.trim().is_empty());
    #[cfg(not(windows))]
    let hostname = hostname.or_else(|| fs::read_to_string("/etc/hostname").ok());
    let hostname = hostname.unwrap_or_else(|| "device".to_owned());
    let label = hostname
        .trim()
        .chars()
        .filter(|value| !value.is_control())
        .take(100)
        .collect::<String>();
    if label.is_empty() {
        return "device".to_owned();
    }
    label
}

/// The state home holding this machine's device credentials: `--home`, then
/// `PROTECTORATE_HOME`, then the user's profile directory.
pub(crate) fn state_home(invocation: &Invocation) -> Result<PathBuf> {
    let asked = match &invocation.command {
        crate::Command::DeviceLogin { home, .. } => home.clone(),
        _ => None,
    };
    if let Some(home) = asked.or_else(|| invocation.home.clone()) {
        if home.as_os_str().is_empty() {
            return Err(RpcError::invalid("--home is empty"));
        }
        return Ok(home);
    }
    if let Some(home) = env::var_os(crate::HOME_ENV) {
        let home = PathBuf::from(home);
        if home.as_os_str().is_empty() {
            return Err(RpcError::invalid(format!("{} is empty", crate::HOME_ENV)));
        }
        return Ok(home);
    }
    let profile = env::var_os("HOME");
    #[cfg(windows)]
    let profile = profile.or_else(|| env::var_os("USERPROFILE"));
    profile
        .map(|home| PathBuf::from(home).join(".protectorate"))
        .ok_or_else(|| {
            RpcError::invalid(format!(
                "no state home: pass --home DIR or set {} or HOME/USERPROFILE",
                crate::HOME_ENV
            ))
        })
}

/// The hub URL in the form its credential file is keyed by: scheme and host
/// lowercased, no trailing slash, no credentials, query or fragment.
fn normalized_hub(hub: &str) -> Result<String> {
    let hub = hub.trim().trim_end_matches('/');
    let Some((scheme, rest)) = hub.split_once("://") else {
        return Err(RpcError::invalid(
            "hub URL must start with http:// or https://",
        ));
    };
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err(RpcError::invalid("hub URL must be http:// or https://"));
    }
    if rest.contains(['@', '?', '#', ' ', '\\']) || rest.split('/').next().unwrap_or("").is_empty()
    {
        return Err(RpcError::invalid(
            "hub URL must have a host and no credentials, query or fragment",
        ));
    }
    let mut parts = rest.splitn(2, '/');
    let authority = parts.next().unwrap_or("").to_ascii_lowercase();
    let path = parts.next().unwrap_or("");
    let hub = format!("{scheme}://{authority}");
    if path.is_empty() {
        Ok(hub)
    } else {
        Ok(format!("{hub}/{path}"))
    }
}

fn credential_path(home: &Path, hub: &str) -> Result<PathBuf> {
    let hub = normalized_hub(hub)?;
    let digest = Sha256::digest(hub.as_bytes());
    let name = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(home.join(CREDENTIALS_DIR).join(format!("{name}.json")))
}

/// Save the issued credential: a private directory, a private file, written
/// through a temporary file and one rename.
fn save_credential(path: &Path, hub: &str, reply: &Value, secret: &str) -> Result<()> {
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    #[cfg(windows)]
    windows::ensure_private_directory(dir)?;
    #[cfg(not(windows))]
    let mut builder = fs::DirBuilder::new();
    #[cfg(not(windows))]
    {
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir).map_err(|error| {
            RpcError::new(Code::Internal, format!("create {}: {error}", dir.display()))
        })?;
    }
    let dir_metadata = fs::symlink_metadata(dir).map_err(|error| {
        RpcError::new(Code::Internal, format!("check {}: {error}", dir.display()))
    })?;
    if !dir_metadata.is_dir() || dir_metadata.file_type().is_symlink() {
        return Err(RpcError::new(
            Code::PermissionDenied,
            format!(
                "credential directory {} must be a real directory",
                dir.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|error| {
            RpcError::new(
                Code::Internal,
                format!("protect {}: {error}", dir.display()),
            )
        })?;
    }
    let token = reply.get("token").cloned().unwrap_or(Value::Null);
    let record = json!({
        "hub": normalized_hub(hub)?,
        "deviceId": token.get("id").cloned().unwrap_or(Value::Null),
        "name": token.get("name").cloned().unwrap_or(Value::Null),
        "platform": token.get("platform").cloned().unwrap_or(Value::Null),
        "savedAt": json!(now_millis()),
        "secret": secret,
    });
    let body = serde_json::to_vec_pretty(&record)
        .map_err(|error| RpcError::new(Code::Internal, format!("encode credential: {error}")))?;
    #[cfg(windows)]
    let write = windows::write_private_file(path, &[body.as_slice(), b"\n"].concat());
    #[cfg(not(windows))]
    let write = protectorate_fsutil::write_file(path, &[body.as_slice(), b"\n"].concat(), 0o600);
    write.map_err(|error| {
        RpcError::new(
            if error.kind() == ErrorKind::PermissionDenied {
                Code::PermissionDenied
            } else {
                Code::Internal
            },
            format!("save device credential {}: {error}", path.display()),
        )
    })
}

/// The credential saved for this invocation's hub, if this machine has one.
pub(crate) fn saved_token(invocation: &Invocation) -> Result<Option<String>> {
    let home = match state_home(invocation) {
        Ok(home) => home,
        Err(_) => return Ok(None),
    };
    let hub = normalized_hub(&hub_url(invocation))?;
    let path = credential_path(&home, &hub)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let dir_metadata = match fs::symlink_metadata(dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RpcError::new(
                Code::Internal,
                format!("check credential directory {}: {error}", dir.display()),
            ))
        }
    };
    if !dir_metadata.is_dir() || dir_metadata.file_type().is_symlink() {
        return Err(RpcError::new(
            Code::PermissionDenied,
            format!(
                "credential directory {} must be a real directory",
                dir.display()
            ),
        ));
    }
    #[cfg(windows)]
    windows::check_private_directory(dir)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RpcError::new(
                Code::Internal,
                format!("check device credential {}: {error}", path.display()),
            ))
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 {
        return Err(RpcError::new(
            Code::PermissionDenied,
            format!(
                "device credential {} must be a regular file under 64 KiB",
                path.display()
            ),
        ));
    }
    #[cfg(windows)]
    let read = windows::read_private_file(&path);
    #[cfg(not(windows))]
    let read = fs::read_to_string(&path);
    let raw = match read {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            return Err(RpcError::new(
                Code::PermissionDenied,
                format!("read device credential {}: {error}", path.display()),
            ))
        }
        Err(error) => {
            return Err(RpcError::new(
                Code::Internal,
                format!("read device credential {}: {error}", path.display()),
            ))
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o777 & 0o077 != 0 {
            return Err(RpcError::new(
                Code::PermissionDenied,
                format!(
                    "device credential {} is readable by others; chmod 600 it or run login again",
                    path.display()
                ),
            ));
        }
    }
    let record: Value = serde_json::from_str(&raw).map_err(|error| {
        RpcError::new(
            Code::DataLoss,
            format!("device credential {}: {error}", path.display()),
        )
    })?;
    if record.get("hub").and_then(Value::as_str) != Some(hub.as_str()) {
        return Err(RpcError::new(
            Code::DataLoss,
            format!(
                "device credential {} belongs to another hub; run login again",
                path.display()
            ),
        ));
    }
    match record
        .get("secret")
        .and_then(Value::as_str)
        .filter(|secret| !secret.is_empty())
    {
        Some(secret) => Ok(Some(secret.to_owned())),
        None => Err(RpcError::new(
            Code::InvalidArgument,
            format!(
                "device credential {} has no secret; run login --hub {} again",
                path.display(),
                hub
            ),
        )),
    }
}

fn field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn json_line(value: &Value) -> Result<String> {
    serde_json::to_string(value)
        .map(|line| format!("{line}\n"))
        .map_err(|error| RpcError::new(Code::Internal, format!("encode JSON: {error}")))
}

fn write_stdout(stdout: &mut dyn Write, text: &str) -> Result<()> {
    stdout
        .write_all(text.as_bytes())
        .and_then(|_| stdout.flush())
        .map_err(|error| RpcError::new(Code::Internal, format!("write login status: {error}")))
}
