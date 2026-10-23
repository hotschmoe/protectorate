//! Local administration for stock Tailscale clients.

use crate::tailnet_runtime;
use protectorate_tailnet::{Config, HUB_API_PORT};
use protectorate_tailnet_admin::{Admin, TAG_CLIENT};
use std::{error::Error, io, path::PathBuf, time::Duration};

pub(crate) enum Command {
    Invite {
        home: PathBuf,
        name: String,
        expires: Duration,
    },
    Register {
        home: PathBuf,
        key: String,
    },
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let action = args.next().ok_or("client requires invite or register")?;
    let mut home = None;
    let mut name = None;
    let mut expires = Duration::from_secs(3600);
    let mut key = None;
    if action == "register" {
        key = Some(
            args.next()
                .ok_or("client register requires a registration key")?,
        );
    }
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            "--name" if action == "invite" => {
                name = Some(args.next().ok_or("--name requires NAME")?)
            }
            "--expires" if action == "invite" => {
                expires = parse_duration(&args.next().ok_or("--expires requires DURATION")?)?
            }
            _ => return Err(format!("unknown client {action} argument {flag:?}")),
        }
    }
    let home = home.map_or_else(crate::default_home, Ok)?;
    match action.as_str() {
        "invite" => Ok(Command::Invite {
            home,
            name: name.ok_or("client invite requires --name")?,
            expires,
        }),
        "register" => Ok(Command::Register {
            home,
            key: key.ok_or("client register requires a registration key")?,
        }),
        _ => Err("client requires invite or register".into()),
    }
}

fn parse_duration(raw: &str) -> Result<Duration, String> {
    let (digits, unit) = raw
        .chars()
        .last()
        .map(|unit| (&raw[..raw.len() - unit.len_utf8()], unit))
        .ok_or("duration must be positive, such as 1h")?;
    let factor = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86400,
        _ => return Err("duration must end in s, m, h, or d".into()),
    };
    let seconds = digits
        .parse::<u64>()
        .ok()
        .and_then(|amount| amount.checked_mul(factor))
        .filter(|seconds| *seconds > 0)
        .ok_or("duration must be positive and fit in 64 bits")?;
    Ok(Duration::from_secs(seconds))
}

pub(crate) async fn run(command: Command) -> Result<(), Box<dyn Error>> {
    let home = match &command {
        Command::Invite { home, .. } | Command::Register { home, .. } => home,
    };
    let config = Config::load(home).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("tailnet is not initialized: {error}"),
        )
    })?;
    let local = tailnet_runtime::loopback(&config.listen)?;
    let control = tailnet_runtime::local_control_url(&config)?;
    let binary = home.join("bin").join(format!(
        "headscale-{}",
        protectorate_tailnet::HEADSCALE_VERSION
    ));
    let key = tailnet_runtime::load_or_create_key(home, &binary, &control, local).await?;
    tokio::task::spawn_blocking(move || {
        let mut admin =
            Admin::new_with_local_addr(&control, &key, Duration::from_secs(30), Some(local))?;
        admin.ensure_policy_user()?;
        match command {
            Command::Invite { name, expires, .. } => {
                let text = invite(&admin, &config, &name, expires)?;
                print!("{text}");
            }
            Command::Register { key, .. } => {
                register(&admin, &config, &key)?;
                println!("registered as {TAG_CLIENT}");
            }
        }
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
    .await?
    .map_err(|error| -> Box<dyn Error> { error })
}

fn invite(
    admin: &Admin,
    config: &Config,
    name: &str,
    expires: Duration,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let result = admin.invite(name, &config.hub_name, &[TAG_CLIENT.to_owned()], expires)?;
    Ok(format!(
        "Laptop or desktop (Tailscale installed):\n  tailscale up --login-server {} --authkey {} --hostname {}\n\nPhone (Tailscale app): open the account menu, choose \"Use an alternate\nserver\" (Android: Settings > Accounts; iOS: Settings > Account), enter\n  {}\nand sign in. The app cannot take a key, so it shows a registration key; run\n  protectorate client register <that key>\n\nThen reach the hub at http://{}:{}. The key is single-use and expires in {}.\n",
        config.server_url,
        result.key,
        name,
        config.server_url,
        config.hub_name,
        HUB_API_PORT,
        human_duration(expires),
    ))
}

fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds.is_multiple_of(3600) {
        format!("{}h", seconds / 3600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn register(admin: &Admin, config: &Config, key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let node = admin.register_client(key)?;
    let expiry = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(i64::from(config.key_expiry_days)))
        .ok_or("configured client lifetime is too large")?;
    admin.set_expiry(node.id, expiry)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn mock_headscale(
        steps: Vec<(&'static str, &'static str, &'static str)>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            for (method, path_prefix, body) in steps {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut received = Vec::new();
                let mut chunk = [0; 4096];
                loop {
                    let size = stream.read(&mut chunk).unwrap();
                    assert!(size > 0);
                    received.extend_from_slice(&chunk[..size]);
                    if received.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        break;
                    }
                }
                let header_end = received
                    .windows(4)
                    .position(|bytes| bytes == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let headers = String::from_utf8_lossy(&received[..header_end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.trim().parse().ok())
                    })
                    .unwrap_or(0);
                while received.len() - header_end < length {
                    let size = stream.read(&mut chunk).unwrap();
                    assert!(size > 0);
                    received.extend_from_slice(&chunk[..size]);
                }
                let request = String::from_utf8_lossy(&received);
                assert!(
                    request.starts_with(&format!("{method} {path_prefix}")),
                    "{request}"
                );
                if path_prefix == "/api/v1/preauthkey" {
                    assert!(
                        request.contains("\"aclTags\":[\"tag:client\"]"),
                        "{request}"
                    );
                    assert!(request.contains("\"reusable\":false"), "{request}");
                }
                if path_prefix == "/api/v1/node/71/tags" {
                    assert!(request.contains("\"tags\":[\"tag:client\"]"), "{request}");
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (url, task)
    }

    #[test]
    fn parses_client_commands_and_rejects_invalid_expiry() {
        let args = |parts: &[&str]| {
            parts
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert!(matches!(
            parse(args(&["invite", "--name", "phone", "--expires", "2h", "--home", "/tmp/h"])),
            Ok(Command::Invite { expires, .. }) if expires == Duration::from_secs(7200)
        ));
        assert!(matches!(
            parse(args(&["register", "abc", "--home", "/tmp/h"])),
            Ok(Command::Register { key, .. }) if key == "abc"
        ));
        assert!(parse(args(&["invite", "--name", "phone", "--expires", "0h"])).is_err());
        assert!(parse(args(&["register"])).is_err());
    }

    #[test]
    fn invite_creates_single_use_client_key_and_formats_stock_client_instructions() {
        let (url, task) = mock_headscale(vec![
            ("GET", "/api/v1/node", r#"{"nodes":[]}"#),
            (
                "POST",
                "/api/v1/preauthkey",
                r#"{"preAuthKey":{"id":"9","key":"client-auth-key"}}"#,
            ),
        ]);
        let mut config = Config::for_server(url.clone());
        config.hub_name = "hub".into();
        let mut admin = Admin::new(&url, "secret", Duration::from_secs(2)).unwrap();
        admin.set_user_id(42).unwrap();
        let output = invite(&admin, &config, "phone", Duration::from_secs(3600)).unwrap();
        assert!(output.contains("--authkey client-auth-key --hostname phone"));
        assert!(output.contains("protectorate client register <that key>"));
        assert!(output.contains("http://hub:7470"));
        task.join().unwrap();
    }

    #[test]
    fn register_tags_client_and_sets_configured_lifetime() {
        let (url, task) = mock_headscale(vec![
            (
                "POST",
                "/api/v1/node/register?user=protectorate&key=phone%20key",
                r#"{"node":{"id":"71","givenName":"phone"}}"#,
            ),
            ("POST", "/api/v1/node/71/tags", "{}"),
            ("POST", "/api/v1/node/71/expire?expiry=", "{}"),
        ]);
        let admin = Admin::new(&url, "secret", Duration::from_secs(2)).unwrap();
        let config = Config::for_server(url);
        register(&admin, &config, "phone key").unwrap();
        task.join().unwrap();
    }
}
