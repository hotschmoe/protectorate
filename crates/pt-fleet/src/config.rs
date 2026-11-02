use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

/// The tailnet port of the node agent (go/policy.hujson).
pub const NODE_PORT: u16 = 7475;

/// The one DERP node in the map, pinned by IP (no DNS, no public DERP).
#[derive(Clone, Debug)]
pub struct Derp {
    /// Hostname, also the TLS name when `tls`.
    pub host: String,
    pub ipv4: Ipv4Addr,
    pub port: u16,
    /// false: plain HTTP straight to headscale (LAN-direct tests); every member is told so.
    pub tls: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The tailnet directory: headscale's state and keys, the hub member's keys and the
    /// enrollment records. A hub adopts an existing one by pointing here.
    pub dir: PathBuf,
    pub headscale: PathBuf,
    pub member: PathBuf,
    pub bwrap: PathBuf,
    /// What nodes use as control URL: `https://hub.example.lan` behind a TLS proxy, or
    /// `http://<listen>` LAN-direct.
    pub server_url: String,
    /// headscale's listener: the hub's LAN address.
    pub listen: SocketAddr,
    /// headscale binds a STUN socket while DERP is on, though the map turns STUN off.
    pub stun_listen: SocketAddr,
    pub derp: Derp,
    /// Proxies whose X-Forwarded-For headscale trusts (the TLS proxy).
    pub trusted_proxies: Vec<String>,
    /// The hub member's tailnet hostname.
    pub hub_name: String,
    /// The hub member's WireGuard UDP port; 0 picks one.
    pub hub_udp_port: u16,
}

impl Config {
    /// Nodes reach headscale's control and DERP directly at `listen` over plain HTTP.
    pub fn lan_direct(
        dir: PathBuf,
        headscale: PathBuf,
        member: PathBuf,
        listen: SocketAddr,
    ) -> Config {
        let ipv4 = match listen.ip() {
            std::net::IpAddr::V4(a) => a,
            std::net::IpAddr::V6(_) => Ipv4Addr::LOCALHOST,
        };
        Config {
            dir,
            headscale,
            member,
            bwrap: PathBuf::from("bwrap"),
            server_url: format!("http://{listen}"),
            listen,
            stun_listen: SocketAddr::from((Ipv4Addr::LOCALHOST, listen.port().wrapping_add(1))),
            derp: Derp {
                host: ipv4.to_string(),
                ipv4,
                port: listen.port(),
                tls: false,
            },
            trusted_proxies: Vec::new(),
            hub_name: "hub".into(),
            hub_udp_port: 0,
        }
    }
}
