//! `protectorate serve`: lay out the home, replay the journal, open the stream log, apply
//! `HubRecovered`, start the core, the executors, the fleet, doctor, the harness updater, the tick
//! and the listeners; on a signal, or an update's restart, stop the listeners and the fleet. A
//! release on trial (`update.rs`) is switched back when this fails to start or fails its health
//! check.

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
use crate::envoy::{ENVOY, envoy_id};
use crate::fleet::{FleetFlags, Net};
use crate::home::{Config, Home, Tokens, random_hex, write_private};
use crate::http::{Listener, Web, handler};
use crate::hub::{Hub, Input, Stop};
use crate::log::{info, warning};
use crate::view::View;
use crate::wire;

pub struct ServeOptions {
    pub home: PathBuf,
    /// The loopback TCP listener; `None` turns it off.
    pub listen: Option<SocketAddr>,
    pub proxy_listen: Option<SocketAddr>,
    pub trusted_proxy: Option<IpAddr>,
    pub web: Web,
    pub fleet: FleetFlags,
    pub net: Net,
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

/// How `serve` ended.
pub enum Exit {
    Stopped,
    /// Exit with `RESTART_CODE` so the service manager starts `releases/current`.
    Restart,
}

/// `hub probation`, run by the unit before each start from `releases/previous`: counts the
/// start of a release on trial and switches back after too many or once its window is over.
/// The journal lock is taken before an interrupted swap is finished and before judging.
/// While the hub holds that lock, the probation state is printed and this returns. It does
/// not judge, restore, or count a start.
pub fn probation_prestart(home: &Path) -> Result<String, String> {
    probation_prestart_locked(home, false)
}

/// `held` means this process already holds the journal lock. A second exclusive flock fails.
fn probation_prestart_locked(home: &Path, held: bool) -> Result<String, String> {
    let _journal = match crate::update::prepare_prestart(home, held)? {
        crate::update::Prestart::Busy => return crate::update::probation_report(home),
        crate::update::Prestart::Ready(lock) => lock,
    };
    let store = pt_update::Store::open(home).map_err(|e| e.to_string())?;
    Ok(
        match store
            .start_attempt_with_restore(pt_update::MAX_TRIAL_STARTS, |p| {
                crate::update::restore_snapshots(home, &store, p)
            })
            .map_err(|e| e.to_string())?
        {
            pt_update::Verdict::Idle => "no release on trial".into(),
            pt_update::Verdict::Confirmed => {
                if store
                    .probation()
                    .map_err(|e| e.to_string())?
                    .is_some_and(|p| p.restore_failed.is_some())
                {
                    "snapshot restore failed; keeping the current release".into()
                } else {
                    "the release on trial was confirmed".into()
                }
            }
            pt_update::Verdict::Waiting { remaining } => {
                format!("on trial, {} s left", remaining.as_secs())
            }
            pt_update::Verdict::RolledBack(sw) => format!(
                "rolled back from {} to {}",
                sw.from.as_deref().unwrap_or("-"),
                sw.to
            ),
        },
    )
}

pub fn retry_probation_restore(home: &Path) -> Result<String, String> {
    crate::update::ensure_headscale_stopped(home)?;
    let _journal = crate::update::acquire_journal_lock(home)?;
    let store = pt_update::Store::open(home).map_err(|e| e.to_string())?;
    store.retry_restore().map_err(|e| e.to_string())?;
    let result = probation_prestart_locked(home, true)?;
    if let Some(error) = store
        .probation()
        .map_err(|e| e.to_string())?
        .and_then(|p| p.restore_failed)
    {
        return Err(error);
    }
    Ok(result)
}

pub fn serve(o: ServeOptions) -> Result<Exit, String> {
    let home = Home::prepare(&o.home).map_err(|e| e.to_string())?;
    let trial = crate::update::probation_at_start(&home)?;
    if let Some(v) = &trial {
        info!("release {v} is on trial: it stays once the health check passes");
    }
    run(o, home.clone(), trial).map_err(|e| crate::update::failed_start(&home, &e))
}

fn run(o: ServeOptions, home: Home, trial: Option<String>) -> Result<Exit, String> {
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
    crate::update::stop_orphan_scopes(&tokens.hub_tag, state.next_sleeve);
    info!(
        "replayed {} events: {} sleeves, {} effects",
        events.len(),
        state.sleeves.len(),
        state.effects.len()
    );

    let store = pt_update::Store::open(&home.root).map_err(|e| e.to_string())?;
    let exe = match store.started_from_current("protectorate") {
        Ok(()) => store.current_dir().join("protectorate"),
        Err(_) => std::env::current_exe().map_err(ctx("current exe"))?,
    };
    link_cli(&home, &exe)?;
    write_private(&home.envoy().join("CLAUDE.md"), ENVOY_MD.as_bytes())
        .map_err(|e| e.to_string())?;
    let uid = fs::metadata("/proc/self")
        .map_err(ctx("stat /proc/self"))?
        .uid();
    let owner_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;

    let (bus, stream_log) = crate::bus::Bus::open(home.stream())
        .map_err(|e| format!("open stream log {}: {e}", home.stream().display()))?;

    let (tx, rx) = mpsc::channel::<Input>();
    // HubRecovered is the first input, before any listener or timer can add one (README step 5).
    tx.send(Input::Event {
        kind: EventKind::HubRecovered,
        reply: None,
    })
    .map_err(|_| "core channel closed")?;
    let envoy_missing = envoy_id(&state).is_none();
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
        bus,
    ));
    let writer_hub = hub.clone();
    thread::Builder::new()
        .name("pt-stream".into())
        .spawn(move || writer_hub.bus.write(stream_log))
        .map_err(ctx("spawn stream writer"))?;
    crate::update::announce_restore_failure(&hub);
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
            effort: None,
            resleeve_to,
            budget: None,
            disk_limit: None,
            d9: false,
        }));
    }

    crate::doctor::start(&hub, o.net);
    crate::hardware::start(&hub);
    crate::harness_update::start(&hub);
    crate::workers::start_fleet(&hub, o.fleet, o.net);
    crate::workers::start_quotas(&hub);
    crate::workers::start_history(&hub);

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
        HttpConfig::default(),
        handler(hub.clone(), Listener::Unix, web.clone()),
    )
    .map_err(|e| e.to_string())?;
    servers.push(start_server("unix", unix));
    let sleeve_sock = home.sleeve_socket();
    clear_stale_socket(&sleeve_sock)?;
    let sleeves = Server::bind_unix(
        &sleeve_sock,
        HttpConfig::default(),
        handler(hub.clone(), Listener::Sleeves, web.clone()),
    )
    .map_err(|e| e.to_string())?;
    servers.push(start_server("sleeves", sleeves));
    if let Some(addr) = o.listen {
        let s = Server::bind_tcp(
            addr,
            HttpConfig::default(),
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
            HttpConfig::default(),
            handler(hub.clone(), Listener::Proxy { trusted }, web.clone()),
        )
        .map_err(|e| e.to_string())?;
        info!("listening for {trusted} on {addr}");
        servers.push(start_server("proxy", s));
    }
    info!("ready: {}", sock.display());

    let (stop_tx, stop_rx) = mpsc::channel();
    pt_sys::install_term_handler().map_err(|e| e.to_string())?;
    let signals = stop_tx.clone();
    thread::Builder::new()
        .name("pt-signals".into())
        .spawn(move || match pt_sys::wait_term() {
            Ok(sig) => drop(signals.send(Stop::Signal(sig))),
            Err(e) => warning!("waiting for signals: {e}"),
        })
        .map_err(ctx("spawn the signal thread"))?;
    *crate::lock::lock(&hub.stop) = Some(stop_tx);
    if let Some(v) = trial {
        crate::update::watch_probation(&hub, v);
    }

    let exit = match stop_rx.recv() {
        Ok(Stop::Signal(sig)) => {
            info!("signal {sig}: stopping the listeners and the fleet");
            Exit::Stopped
        }
        Ok(Stop::Restart(why)) => {
            info!("restarting ({why}): stopping the listeners and the fleet");
            Exit::Restart
        }
        Err(_) => return Err("the stop channel closed".into()),
    };
    for s in &servers {
        s.shutdown(Duration::from_secs(2));
    }
    hub.fleet.shutdown();
    Ok(exit)
}
