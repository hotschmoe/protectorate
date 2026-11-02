use std::net::SocketAddr;
use std::path::PathBuf;

use pt_node::presence::Config;

pub struct Flags {
    pub state_dir: Option<PathBuf>,
    pub sockets: Vec<PathBuf>,
    pub tcp: Vec<SocketAddr>,
    pub stdio: bool,
    pub presence_interval: u64,
    pub away_after: u64,
    pub allow_scope: bool,
    /// Exit once no job runs and no connection is open for this long.
    pub idle_exit: Option<u64>,
    pub member: Option<PathBuf>,
    pub no_member: bool,
}

pub fn parse(args: &[String]) -> Result<Flags, String> {
    let mut f = Flags {
        state_dir: None,
        sockets: Vec::new(),
        tcp: Vec::new(),
        stdio: false,
        presence_interval: 15,
        away_after: Config::default().away_after_secs,
        allow_scope: true,
        idle_exit: None,
        member: None,
        no_member: false,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--state-dir" => f.state_dir = Some(PathBuf::from(value()?)),
            "--forward-socket" => f.sockets.push(PathBuf::from(value()?)),
            "--listen-tcp" => {
                let v = value()?;
                let addr: SocketAddr = v
                    .parse()
                    .map_err(|_| format!("--listen-tcp: '{v}' is not an address and port"))?;
                if !addr.ip().is_loopback() {
                    return Err(format!("--listen-tcp {addr}: only loopback addresses"));
                }
                f.tcp.push(addr);
            }
            "--presence-interval" => f.presence_interval = number(flag, Some(value()?))?,
            "--away-after" => f.away_after = number(flag, Some(value()?))?,
            "--no-scope" => f.allow_scope = false,
            "--stdio" => f.stdio = true,
            "--idle-exit" => f.idle_exit = Some(number(flag, Some(value()?))?),
            "--member" => f.member = Some(PathBuf::from(value()?)),
            "--no-member" => f.no_member = true,
            other => return Err(format!("serve: unknown option '{other}'")),
        }
    }
    let listeners = f.sockets.len() + f.tcp.len();
    if f.stdio && listeners > 0 {
        return Err("--stdio takes no --forward-socket or --listen-tcp".into());
    }
    if !f.stdio && listeners == 0 {
        return Err("serve needs --forward-socket, --listen-tcp or --stdio".into());
    }
    if f.member.is_some() && f.no_member {
        return Err("--member and --no-member exclude each other".into());
    }
    if f.presence_interval == 0 {
        return Err("--presence-interval must be at least 1".into());
    }
    Ok(f)
}

/// Whether `args` are valid serve options.
pub fn check(args: &[String]) -> Result<(), String> {
    parse(args).map(drop)
}

pub(crate) fn number(flag: &str, v: Option<&String>) -> Result<u64, String> {
    let v = v.ok_or_else(|| format!("{flag} needs a value"))?;
    v.parse()
        .map_err(|_| format!("{flag}: '{v}' is not a number"))
}
