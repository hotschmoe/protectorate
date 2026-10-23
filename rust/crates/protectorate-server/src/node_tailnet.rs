//! Direct local Headscale administration for enrolled nodes.

use crate::tailnet_runtime;
use protectorate_tailnet::Config;
use protectorate_tailnet_admin::Admin;
use std::{error::Error, io, path::PathBuf, time::Duration};

pub(crate) enum Command {
    Renew { name: String, home: PathBuf },
    Revoke { name: String, home: PathBuf },
}

pub(crate) fn parse(
    action: &str,
    mut args: impl Iterator<Item = String>,
) -> Result<Command, String> {
    let name = args
        .next()
        .ok_or_else(|| format!("node {action} requires NAME"))?;
    if name.is_empty() || name.starts_with('-') {
        return Err(format!("node {action} requires NAME"));
    }
    let mut home = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            _ => return Err(format!("unknown node {action} argument {flag:?}")),
        }
    }
    let home = home.map_or_else(crate::default_home, Ok)?;
    match action {
        "renew" => Ok(Command::Renew { name, home }),
        "revoke" => Ok(Command::Revoke { name, home }),
        _ => Err(format!("unknown tailnet node action {action}")),
    }
}

pub(crate) async fn run(command: Command) -> Result<(), Box<dyn Error>> {
    let home = match &command {
        Command::Renew { home, .. } | Command::Revoke { home, .. } => home,
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
        let admin =
            Admin::new_with_local_addr(&control, &key, Duration::from_secs(30), Some(local))?;
        match command {
            Command::Renew { name, .. } => {
                let at = admin.renew(&name, i64::from(config.key_expiry_days))?;
                println!("{name} valid until {}", at.format("%Y-%m-%d"));
            }
            Command::Revoke { name, .. } => {
                if !admin.revoke(&name)? {
                    return Err(format!("no node named {name:?} is enrolled").into());
                }
                println!("revoked {name}");
            }
        }
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
    .await?
    .map_err(|error| -> Box<dyn Error> { error })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_node_tailnet_actions() {
        let command = parse(
            "renew",
            [
                "gpu1".to_owned(),
                "--home".to_owned(),
                "/tmp/hub".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap();
        assert!(
            matches!(command, Command::Renew { name, home } if name == "gpu1" && home == std::path::Path::new("/tmp/hub"))
        );
        assert!(parse("revoke", ["--home".to_owned()].into_iter()).is_err());
        assert!(parse(
            "revoke",
            ["gpu1".to_owned(), "--bad".to_owned()].into_iter()
        )
        .is_err());
    }
}
