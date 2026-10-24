//! Device login: create a P-256 key, pair this machine with a hub through
//! `StartDeviceAuth` and `PollDeviceAuth`, and keep the key in a private file
//! per hub under the state home. Ordinary commands sign with that key when no
//! `--token-file` and no token environment variable is set.
use crate::{
    format::{minute, unix_millis},
    hub_url, json_line, message_value, value_line, Invocation,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use protectorate_auth::proof::DeviceKey;
use protectorate_connect_client::Client;
use protectorate_wire::{
    v1::{self, device_service},
    Code, Result, RpcError,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use crate::windows_private as windows;

#[cfg(windows)]
pub(crate) fn read_private_token_file(path: &Path) -> std::io::Result<String> {
    windows::read_private_file(path)
}

/// The directory under the state home holding one credential file per hub.
const CREDENTIALS_DIR: &str = "credentials";
const DEFAULT_INTERVAL: u64 = 5;
const MAX_INTERVAL: u64 = 300;
const SLOW_DOWN_STEP: u64 = 5;
const POLL_TIMEOUT_FLOOR: Duration = Duration::from_secs(30);

/// Runs `login` with no harness: start a pairing request, wait for a trusted
/// device or the hub to approve it, and save the device key. Progress is
/// written to `stdout` as it happens, so the code is visible before the wait.
pub(crate) fn run(invocation: &Invocation, stdout: &mut dyn Write) -> Result<String> {
    let hub = hub_url(invocation);
    let home = state_home(invocation)?;
    let path = credential_path(&home, &hub)?;
    let (name, platform) = device_identity(invocation);
    let client = Client::new_public(&hub, invocation.timeout.max(POLL_TIMEOUT_FLOOR))?;
    let (key, pkcs8) =
        DeviceKey::generate().map_err(|error| RpcError::new(Code::Internal, error))?;
    let pkcs8 = zeroize::Zeroizing::new(pkcs8);
    let request = v1::StartDeviceAuthRequest {
        name,
        platform,
        public_key: key.public_key().to_vec(),
    };
    let started = client
        .unary(device_service::START_DEVICE_AUTH, &request)
        .map_err(|error| {
            RpcError::new(
                error.code,
                format!("start device authorization at {hub}: {}", error.message),
            )
        })?;
    if started.device_code.is_empty() {
        return Err(RpcError::new(Code::DataLoss, "hub returned no device code"));
    }
    if started.user_code.is_empty() {
        return Err(RpcError::new(Code::DataLoss, "hub returned no user code"));
    }
    let deadline = unix_millis(started.expires_at.as_ref())
        .and_then(|millis| u64::try_from(millis).ok())
        .map(|millis| UNIX_EPOCH + Duration::from_millis(millis));
    let mut interval = u64::try_from(started.interval)
        .ok()
        .filter(|interval| *interval > 0)
        .unwrap_or(DEFAULT_INTERVAL);
    let user_code = &started.user_code;
    if invocation.json {
        // The device code stays private to this process.
        let shown = v1::StartDeviceAuthResponse {
            device_code: String::new(),
            interval: interval as i32,
            ..started.clone()
        };
        write_stdout(stdout, &json_line(&shown)?)?;
    } else {
        let waiting = match &started.expires_at {
            None => "its expiry".to_owned(),
            at => minute(at.as_ref()),
        };
        write_stdout(
            stdout,
            &format!(
                "\nCode: {user_code}\n\nOn a trusted device, approve only if this code matches.\nOn the hub: protectorate devices approve {user_code}\n\nWaiting for approval until {waiting}...\n"
            ),
        )?;
    }
    let poll = v1::PollDeviceAuthRequest {
        device_code: started.device_code.clone(),
    };
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
        let reply = match client.unary(device_service::POLL_DEVICE_AUTH, &poll) {
            Ok(reply) => reply,
            Err(error) if error.code == Code::ResourceExhausted => {
                interval = next_interval(interval, "rate_limited", 0);
                continue;
            }
            // Keep the visible code usable through a transient lost poll.
            // If the lost reply reported approval, the next poll reports
            // `consumed` and asks the user to restart.
            Err(error) if error.code == Code::Unavailable && deadline.is_some() => continue,
            Err(error) => {
                return Err(RpcError::new(
                    error.code,
                    format!("poll device authorization at {hub}: {}", error.message),
                ))
            }
        };
        interval = next_interval(
            interval,
            &reply.status,
            u64::try_from(reply.interval).unwrap_or(0),
        );
        match reply.status.as_str() {
            "pending" | "slow_down" => continue,
            "approved" => {
                let device = reply.device.unwrap_or_default();
                if device.id != key.id() {
                    return Err(RpcError::new(
                        Code::DataLoss,
                        "hub approved a device with another key; run login again",
                    ));
                }
                save_credential(&path, &hub, &device, &pkcs8)?;
                if invocation.json {
                    return value_line(&json!({
                        "status": "approved",
                        "device": message_value(&device)?,
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
                    "this pairing request was already completed; run login again",
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

/// Save the device key: a private directory, a private file, written through
/// a temporary file and one rename.
fn save_credential(path: &Path, hub: &str, device: &v1::DeviceInfo, pkcs8: &[u8]) -> Result<()> {
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
    let record = json!({
        "hub": normalized_hub(hub)?,
        "deviceId": device.id,
        "name": device.name,
        "platform": device.platform,
        "savedAt": json!(now_millis()),
        "key": STANDARD.encode(pkcs8),
    });
    let body =
        zeroize::Zeroizing::new(serde_json::to_vec_pretty(&record).map_err(|error| {
            RpcError::new(Code::Internal, format!("encode credential: {error}"))
        })?);
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

/// Where the device credential for this invocation's hub lives.
pub(crate) fn credential_file(invocation: &Invocation) -> Result<PathBuf> {
    credential_path(&state_home(invocation)?, &hub_url(invocation))
}

/// The device key saved for this invocation's hub, if this machine has one.
pub(crate) fn saved_device(invocation: &Invocation) -> Result<Option<Arc<DeviceKey>>> {
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
    protectorate_connect_client::device_key(&record)
        .map(Some)
        .map_err(|error| {
            RpcError::new(
                Code::InvalidArgument,
                format!(
                    "device credential {}: {}; run login --hub {} again",
                    path.display(),
                    error.message,
                    hub
                ),
            )
        })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn write_stdout(stdout: &mut dyn Write, text: &str) -> Result<()> {
    stdout
        .write_all(text.as_bytes())
        .and_then(|_| stdout.flush())
        .map_err(|error| RpcError::new(Code::Internal, format!("write login status: {error}")))
}
