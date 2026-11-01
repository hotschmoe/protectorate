//! `protectorate serve`: lay out the home, replay the journal, apply `HubRecovered`, start the
//! core, the executors, the tick and the listeners.

use std::fs;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use pt_core::{Claim, EventKind, Layout, SpawnSpec, codec};
use pt_http::{Config as HttpConfig, Server, ShutdownHandle};
use pt_journal::Journal;

use crate::devices::Devices;
use crate::home::{Config, Home, Tokens, random_hex, write_private};
use crate::http::{Listener, Web, handler};
use crate::hub::{ENVOY, Hub, Input, View};
use crate::log::{info, warning};
use crate::wire;

pub struct ServeOptions {
    pub home: PathBuf,
    /// The loopback TCP listener; `None` turns it off.
    pub listen: Option<SocketAddr>,
    pub proxy_listen: Option<SocketAddr>,
    pub trusted_proxy: Option<IpAddr>,
    pub web: Web,
}

const ENVOY_MD: &str = include_str!("envoy.md");
const TICK: Duration = Duration::from_secs(10);

fn ctx(what: &str) -> impl Fn(io::Error) -> String + '_ {
    move |e| format!("{what}: {e}")
}

/// Points `run/bin/protectorate` at this binary, replacing an older link by rename.
fn link_cli(home: &Home, exe: &Path) -> Result<(), String> {
    let link = home.bin().join("protectorate");
    if fs::read_link(&link).is_ok_and(|t| t == exe) {
        return Ok(());
    }
    let tmp = home.bin().join(format!(
        ".protectorate.{}",
        random_hex(8).map_err(|e| e.to_string())?
    ));
    std::os::unix::fs::symlink(exe, &tmp).map_err(ctx("link the CLI"))?;
    fs::rename(&tmp, &link).map_err(ctx("link the CLI"))
}

/// The journal lock is held, so a socket left at the path is a dead hub's: unlink it to bind.
fn clear_stale_socket(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() => {
            warning!("removing the stale socket {}", path.display());
            fs::remove_file(path).map_err(ctx("remove stale socket"))
        }
        Ok(_) => Err(format!("{} exists and is not a socket", path.display())),
        Err(_) => Ok(()),
    }
}

fn start_server(name: &str, server: Server) -> ShutdownHandle {
    let h = server.shutdown_handle();
    let name = name.to_string();
    thread::Builder::new()
        .name(format!("pt-http-{name}"))
        .spawn(move || {
            if let Err(e) = server.run() {
                warning!("{name} listener: {e}");
            }
        })
        .expect("spawn listener");
    h
}

fn http_config() -> HttpConfig {
    HttpConfig {
        // Wait and EnvoyWait hold a request open up to an hour.
        idle_timeout: Duration::from_secs(3700),
        ..HttpConfig::default()
    }
}

pub fn serve(o: ServeOptions) -> Result<(), String> {
    let home = Home::prepare(&o.home).map_err(|e| e.to_string())?;
    let config = Config::load(&home).map_err(|e| e.to_string())?;
    let (tokens, new_owner) = Tokens::load_or_create(&home).map_err(|e| e.to_string())?;
    let devices = Devices::load(&home).map_err(|e| e.to_string())?;
    let (journal, rec) = Journal::open(&home.journal())
        .map_err(|e| format!("open journal {}: {e}", home.journal().display()))?;
    if let Some(cut) = &rec.cut {
        warning!(
            "journal: cut a torn tail of {} bytes at {} ({:?})",
            cut.dropped,
            cut.offset,
            cut.reason
        );
    }
    let mut events = Vec::with_capacity(rec.frames.len());
    for (i, f) in rec.frames.iter().enumerate() {
        events.push(codec::decode(f).map_err(|e| format!("journal frame {i}: {e}"))?);
    }
    let state = pt_core::replay(&events);
    info!(
        "replayed {} events: {} sleeves, {} effects",
        events.len(),
        state.sleeves.len(),
        state.effects.len()
    );

    let exe = std::env::current_exe().map_err(ctx("current exe"))?;
    link_cli(&home, &exe)?;
    write_private(&home.envoy().join("CLAUDE.md"), ENVOY_MD.as_bytes())
        .map_err(|e| e.to_string())?;
    let uid = fs::metadata("/proc/self")
        .map_err(ctx("stat /proc/self"))?
        .uid();
    let owner_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;

    let (tx, rx) = mpsc::channel::<Input>();
    // HubRecovered is the first input, before any listener or timer can add one (README step 5).
    tx.send(Input {
        kind: EventKind::HubRecovered,
        reply: None,
    })
    .map_err(|_| "core channel closed")?;
    let envoy_missing = Hub::envoy_id(&state).is_none();
    let view = View {
        state: state.clone(),
        batch: 0,
        since: Default::default(),
    };
    let hub = Arc::new(Hub::new(
        home.clone(),
        config,
        tokens,
        devices,
        uid,
        owner_home,
        tx,
        view,
    ));
    if let Some(t) = new_owner {
        eprintln!("owner token (shown once): {t}");
        eprintln!("envoy-only token: {}", hub.tokens.envoy_only());
    }
    if let Some(f) = &hub.fault {
        warning!(
            "PT_TEST_FAULT is set: {}'s first turn reports a usage limit",
            f.sleeve
        );
    }

    let core_hub = hub.clone();
    thread::Builder::new()
        .name("pt-core".into())
        .spawn(move || crate::core::run(core_hub, state, journal, rx))
        .map_err(ctx("spawn core"))?;

    if envoy_missing {
        let e = &hub.config.envoy;
        let harness = wire::harness_from(&e.harness)
            .ok_or_else(|| format!("config envoy.harness: unknown harness {:?}", e.harness))?;
        let target = wire::target(&hub.config, harness, &e.provider, &e.model);
        let resleeve_to = hub
            .config
            .resleeve_to
            .iter()
            .filter_map(|h| wire::harness_from(h))
            .map(|h| wire::target(&hub.config, h, "", ""))
            .collect();
        hub.submit(EventKind::Spawn(SpawnSpec {
            request_id: "envoy:create".into(),
            name: ENVOY.into(),
            parent: None,
            layout: Layout::Root,
            claim: Claim {
                path: home.envoy().to_string_lossy().into_owned(),
                writable: true,
            },
            target,
            resleeve_to,
            budget: None,
            disk_limit: None,
        }));
    }

    let tick_hub = hub.clone();
    thread::Builder::new()
        .name("pt-tick".into())
        .spawn(move || {
            loop {
                thread::sleep(TICK);
                tick_hub.submit(EventKind::Tick);
            }
        })
        .map_err(ctx("spawn tick"))?;

    let web = Arc::new(o.web);
    match &*web {
        Web::None => info!("web app: not embedded in this binary"),
        Web::Dir(d) => info!("web app: from {}", d.display()),
        Web::Embedded(files) => info!("web app: embedded, {} files", files.len()),
    }
    let mut servers = Vec::new();
    let sock = home.socket();
    clear_stale_socket(&sock)?;
    let unix = Server::bind_unix(
        &sock,
        http_config(),
        handler(hub.clone(), Listener::Unix, web.clone()),
    )
    .map_err(|e| e.to_string())?;
    servers.push(start_server("unix", unix));
    if let Some(addr) = o.listen {
        let s = Server::bind_tcp(
            addr,
            http_config(),
            handler(hub.clone(), Listener::Loopback, web.clone()),
        )
        .map_err(|e| e.to_string())?;
        info!("listening on {}", s.local_addr().unwrap_or(addr));
        servers.push(start_server("loopback", s));
    }
    if let Some(addr) = o.proxy_listen {
        let trusted = o
            .trusted_proxy
            .ok_or("--proxy-listen needs --trusted-proxy")?;
        let s = Server::bind_tcp(
            addr,
            http_config(),
            handler(hub.clone(), Listener::Proxy { trusted }, web.clone()),
        )
        .map_err(|e| e.to_string())?;
        info!("listening for {trusted} on {addr}");
        servers.push(start_server("proxy", s));
    }
    info!("ready: {}", sock.display());

    let sig = pt_sys::wait_term().map_err(|e| e.to_string())?;
    info!("signal {sig}: stopping the listeners");
    for s in &servers {
        s.shutdown(Duration::from_secs(2));
    }
    Ok(())
}
