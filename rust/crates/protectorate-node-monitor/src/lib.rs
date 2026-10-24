//! Polls registered node agents and keeps the last successful metrics for hub
//! status and lease admission. HTTP work never holds the status lock.
use protectorate_connect_client::{Client, NodeTls};
use protectorate_inventory::{Inventory, Node};
use protectorate_node_metrics::{metrics_proto, Collector, Options};
use protectorate_wire::{
    self as wire,
    v1::{self, node_agent_service, AlertKind, NodeFreshness},
    Code, RpcError,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

const BUSY_VRAM_FRACTION: f64 = 0.5;
const BUSY_UTIL: f64 = 60.0;
const BUSY_TEMP: f64 = 75.0;
const BUSY_WINDOW: Duration = Duration::from_secs(3 * 60);
const BUSY_SPAN: Duration = Duration::from_secs(2 * 60);
const BUSY_MIN_SAMPLES: usize = 3;
const GPU_HISTORY: usize = 32;

pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(60);
pub const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const AGENT_PORT: u16 = 7475;
const POLL_CONCURRENCY: usize = 16;

pub type PollSink = Arc<dyn Fn(PollEvent) + Send + Sync>;
#[derive(Clone, Debug)]
pub struct PollEvent {
    pub kind: &'static str,
    pub name: String,
    pub address: String,
}

/// How the hub reaches node agents: mutual TLS, checking that the node shows
/// the certificate the hub CA issued at its current join. A node without a
/// certificate is reachable only as a plain loopback agent on this machine.
#[derive(Default)]
pub struct Dialer(Mutex<Option<NodeTls>>);

impl Dialer {
    pub fn set(&self, tls: NodeTls) {
        *self.0.lock().unwrap() = Some(tls);
    }

    pub fn client(&self, node: &Node, timeout: Duration) -> Result<Client, RpcError> {
        if node.address.is_empty() {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("{} has not joined", node.name),
            ));
        }
        let address = node
            .address
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let has_port = if address.starts_with('[') {
            address.contains("]:")
        } else {
            address.contains(':')
        };
        let address = if has_port {
            address.to_owned()
        } else {
            format!("{address}:{AGENT_PORT}")
        };
        if node.cert_id.is_empty() {
            return Client::new_node("", &address, None, timeout);
        }
        let tls =
            self.0.lock().unwrap().clone().ok_or_else(|| {
                RpcError::new(Code::Unavailable, "the hub has no node certificate")
            })?;
        Client::new_node(
            &protectorate_tls::node_name(&node.name, &node.cert_id),
            &address,
            Some(&tls),
            timeout,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub stale_after: Duration,
    pub fetch_timeout: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            stale_after: DEFAULT_STALE_AFTER,
            fetch_timeout: DEFAULT_FETCH_TIMEOUT,
        }
    }
}

#[derive(Clone)]
struct Good {
    at: Instant,
    wall_at: SystemTime,
    metrics: v1::NodeMetrics,
}
#[derive(Clone, Debug)]
struct GpuPoint {
    index: i32,
    memory_used_bytes: Option<u64>,
    memory_total_bytes: u64,
    utilization_percent: f64,
    temperature_celsius: f64,
}

#[derive(Clone, Debug)]
struct GpuSample {
    at: SystemTime,
    gpus: Vec<GpuPoint>,
}

#[derive(Default)]
struct State {
    address: String,
    attempted: bool,
    last_error: String,
    good: Option<Good>,
    gpus: Vec<GpuSample>,
    down_emitted: bool,
    /// The agent instance and sequence of the last accepted sample.
    stamp: Option<(String, u64)>,
}

fn stamp_of(metrics: &v1::NodeMetrics) -> Option<(String, u64)> {
    (metrics.seq > 0 && !metrics.instance.is_empty())
        .then(|| (metrics.instance.clone(), metrics.seq))
}

impl State {
    /// A sample no newer than the last accepted one from the same agent
    /// instance arrived late or twice. A new instance starts a fresh count.
    fn is_stale(&self, metrics: &v1::NodeMetrics) -> bool {
        match (&self.stamp, stamp_of(metrics)) {
            (Some((last, last_seq)), Some((instance, seq))) => {
                *last == instance && seq <= *last_seq
            }
            _ => false,
        }
    }
}

pub struct Monitor {
    inventory: Arc<Inventory>,
    local_name: String,
    local: Option<Collector>,
    pub dialer: Dialer,
    events: Mutex<Option<PollSink>>,
    timing: Timing,
    state: Mutex<HashMap<String, State>>,
    poll_lock: Mutex<()>,
}

impl Monitor {
    pub fn new(inventory: Arc<Inventory>, local_name: String, home: std::path::PathBuf) -> Self {
        Self::new_with_watch_paths(inventory, local_name, home, Vec::new())
    }

    pub fn new_with_watch_paths(
        inventory: Arc<Inventory>,
        local_name: String,
        home: std::path::PathBuf,
        watch_paths: Vec<std::path::PathBuf>,
    ) -> Self {
        let local = cfg!(target_os = "linux").then(|| {
            Collector::linux(Options {
                home,
                watch_paths,
                version: env!("CARGO_PKG_VERSION").into(),
                ..Options::default()
            })
        });
        Self {
            inventory,
            local_name,
            local,
            dialer: Dialer::default(),
            events: Mutex::new(None),
            timing: Timing::default(),
            state: Mutex::new(HashMap::new()),
            poll_lock: Mutex::new(()),
        }
    }

    pub fn with_timing(mut self, timing: Timing) -> Self {
        self.timing = timing;
        self
    }

    pub fn set_event_sink(&self, sink: PollSink) {
        *self.events.lock().unwrap() = Some(sink);
    }

    /// Drops cached health when an inventory record is edited.
    pub fn invalidate(&self, name: &str) {
        self.state.lock().unwrap().remove(name);
    }

    /// Polls the local collector and every inventory node. The hub launches
    /// this on a blocking worker, before each scheduled interval.
    pub fn poll_all(&self) {
        let _guard = self.poll_lock.lock().unwrap();
        let nodes = self.inventory.list();
        self.poll_local();
        for group in nodes.chunks(POLL_CONCURRENCY) {
            std::thread::scope(|scope| {
                for node in group {
                    scope.spawn(move || self.poll_remote(node));
                }
            });
        }
        let current: HashSet<_> = nodes.iter().map(|n| n.name.as_str()).collect();
        self.state
            .lock()
            .unwrap()
            .retain(|name, _| name == &self.local_name || current.contains(name.as_str()));
    }

    /// Polls a just-added node without waiting for the next full round.
    pub fn poll_node(&self, name: &str) {
        let _guard = self.poll_lock.lock().unwrap();
        if name == self.local_name {
            self.poll_local();
        } else if let Ok(node) = self.inventory.get(name) {
            self.poll_remote(&node);
        }
    }

    fn poll_local(&self) {
        let result = self
            .local
            .as_ref()
            .ok_or_else(|| "local collector is unavailable on this platform".to_owned())
            .and_then(|collector| {
                collector
                    .collect()
                    .map(|metrics| metrics_proto(&metrics))
                    .map_err(|error| error.to_string())
            });
        self.record(&self.local_name, "local", false, false, result);
    }

    fn poll_remote(&self, node: &Node) {
        if node.address.is_empty() {
            return;
        }
        let result = self
            .dialer
            .client(node, self.timing.fetch_timeout)
            .and_then(|client| {
                client.unary(node_agent_service::GET_METRICS, &v1::GetMetricsRequest {})
            })
            .map_err(|error| error.to_string());
        if self
            .inventory
            .get(&node.name)
            .ok()
            .is_none_or(|current| current.address != node.address)
        {
            return;
        }
        let mut joined = false;
        if let Ok(metrics) = &result {
            if node.invited && self.inventory.clear_invited(&node.name).is_ok() {
                joined = true;
            }
            let _ = self
                .inventory
                .set_platform(&node.name, &metrics.os, &metrics.arch);
        }
        self.record(
            &node.name,
            &node.address,
            node.invited && !joined,
            joined,
            result,
        );
    }

    fn record(
        &self,
        name: &str,
        address: &str,
        invited: bool,
        joined: bool,
        result: Result<v1::NodeMetrics, String>,
    ) {
        let mut states = self.state.lock().unwrap();
        let state = states.entry(name.to_owned()).or_default();
        if state.address != address {
            *state = State {
                address: address.to_owned(),
                ..State::default()
            };
        }
        state.attempted = true;
        let event = match result {
            Ok(metrics) if state.is_stale(&metrics) => None,
            Ok(metrics) => {
                if let Some(stamp) = stamp_of(&metrics) {
                    state.stamp = Some(stamp);
                }
                let wall_at = SystemTime::now();
                state.gpus = add_gpu_sample(std::mem::take(&mut state.gpus), wall_at, &metrics);
                state.good = Some(Good {
                    at: Instant::now(),
                    wall_at,
                    metrics,
                });
                state.last_error.clear();
                state.down_emitted = false;
                Some("node.metrics")
            }
            Err(error) => {
                state.last_error = error;
                let down = !invited
                    && state
                        .good
                        .as_ref()
                        .is_none_or(|good| good.at.elapsed() > self.timing.stale_after);
                if down && !state.down_emitted {
                    state.down_emitted = true;
                    Some("node.unreachable")
                } else {
                    None
                }
            }
        };
        drop(states);
        if joined {
            self.publish("node.joined", name, address);
        }
        if let Some(kind) = event {
            self.publish(kind, name, address);
        }
    }

    fn publish(&self, kind: &'static str, name: &str, address: &str) {
        let sink = self.events.lock().unwrap().clone();
        if let Some(sink) = sink {
            sink(PollEvent {
                kind,
                name: name.to_owned(),
                address: address.to_owned(),
            });
        }
    }

    /// The node's health without its inventory record (`node` is unset).
    pub fn status(&self, name: &str, address: &str, invited: bool) -> v1::NodeStatus {
        if invited {
            return v1::NodeStatus {
                freshness: NodeFreshness::Invited as i32,
                error: "invited, not yet joined".into(),
                ..Default::default()
            };
        }
        let states = self.state.lock().unwrap();
        let state = states.get(name).filter(|state| state.address == address);
        let Some(state) = state.filter(|state| state.attempted) else {
            return v1::NodeStatus {
                error: "awaiting first poll".into(),
                ..Default::default()
            };
        };
        let Some(good) = &state.good else {
            return v1::NodeStatus {
                freshness: NodeFreshness::Unreachable as i32,
                error: state.last_error.clone(),
                alerts: vec![unreachable_alert(&state.last_error)],
                ..Default::default()
            };
        };
        let age = good.at.elapsed();
        let freshness = if age > self.timing.stale_after {
            NodeFreshness::Unreachable
        } else if state.last_error.is_empty() {
            NodeFreshness::Fresh
        } else {
            NodeFreshness::Stale
        };
        let unreachable = freshness == NodeFreshness::Unreachable;
        v1::NodeStatus {
            metrics: Some(good.metrics.clone()),
            freshness: freshness as i32,
            fetched_at: Some(good.wall_at.into()),
            age: Some(wire::proto_duration(age)),
            error: state.last_error.clone(),
            alerts: if unreachable {
                vec![unreachable_alert(&state.last_error)]
            } else {
                Vec::new()
            },
            busy_gpus: if unreachable {
                Vec::new()
            } else {
                busy_gpus(&state.gpus, SystemTime::now())
            },
            ..Default::default()
        }
    }

    /// Refuses an explicit GPU, or any discrete GPU on a whole-node grant,
    /// when something outside protectorate's leases is using it.
    pub fn gpu_free(&self, name: &str, address: &str, gpu: Option<u32>) -> Result<(), String> {
        gpu_free_from_status(name, gpu, &self.status(name, address, false))
    }

    pub fn is_fresh(&self, name: &str, address: &str) -> bool {
        let states = self.state.lock().unwrap();
        states.get(name).is_some_and(|state| {
            state.address == address
                && state.last_error.is_empty()
                && state
                    .good
                    .as_ref()
                    .is_some_and(|good| good.at.elapsed() <= self.timing.stale_after)
        })
    }
}

fn age(later: SystemTime, earlier: SystemTime) -> Duration {
    later.duration_since(earlier).unwrap_or(Duration::ZERO)
}

fn unreachable_alert(error: &str) -> v1::NodeAlert {
    v1::NodeAlert {
        kind: AlertKind::Unreachable as i32,
        message: format!("node is unreachable: {error}"),
        ..Default::default()
    }
}

fn gpu_points(metrics: &v1::NodeMetrics) -> Vec<GpuPoint> {
    metrics
        .gpus
        .iter()
        .map(|gpu| GpuPoint {
            index: gpu.index,
            memory_used_bytes: gpu.memory_used_bytes,
            memory_total_bytes: gpu.memory_total_bytes,
            utilization_percent: gpu.utilization_percent,
            temperature_celsius: gpu.temperature_celsius,
        })
        .collect()
}

fn add_gpu_sample(
    mut hist: Vec<GpuSample>,
    at: SystemTime,
    metrics: &v1::NodeMetrics,
) -> Vec<GpuSample> {
    hist.push(GpuSample {
        at,
        gpus: gpu_points(metrics),
    });
    let mut cut = 0;
    while cut + 1 < hist.len() && age(at, hist[cut].at) > BUSY_WINDOW {
        cut += 1;
    }
    hist.drain(..cut);
    if hist.len() > GPU_HISTORY {
        let drop = hist.len() - GPU_HISTORY;
        hist.drain(..drop);
    }
    hist
}

fn busy_gpus(hist: &[GpuSample], now: SystemTime) -> Vec<v1::GpuBusy> {
    let Some(latest) = hist.last() else {
        return Vec::new();
    };
    latest
        .gpus
        .iter()
        .filter_map(|gpu| {
            let reason = gpu_busy_reason(gpu, hist, now);
            (!reason.is_empty()).then_some(v1::GpuBusy {
                index: gpu.index,
                reason,
            })
        })
        .collect()
}

fn gpu_busy_reason(gpu: &GpuPoint, hist: &[GpuSample], now: SystemTime) -> String {
    if let Some(used) = gpu.memory_used_bytes {
        if gpu.memory_total_bytes > 0 {
            let fraction = used as f64 / gpu.memory_total_bytes as f64;
            if fraction > BUSY_VRAM_FRACTION {
                return format!("{:.0}% of its VRAM is in use", fraction * 100.0);
            }
        }
    }
    let mut window = Vec::new();
    let mut oldest = None;
    for sample in hist {
        if age(now, sample.at) > BUSY_WINDOW {
            continue;
        }
        if let Some(point) = sample.gpus.iter().find(|point| point.index == gpu.index) {
            if oldest.is_none() {
                oldest = Some(sample.at);
            }
            window.push(point);
        }
    }
    let Some(oldest) = oldest else {
        return String::new();
    };
    if window.len() < BUSY_MIN_SAMPLES || age(now, oldest) < BUSY_SPAN {
        return String::new();
    }
    let span = go_duration(age(now, oldest));
    if let Some(lo) = sustained(&window, |gpu| gpu.utilization_percent, BUSY_UTIL) {
        return format!("utilization at least {lo:.0}% for {span}");
    }
    if let Some(lo) = sustained(&window, |gpu| gpu.temperature_celsius, BUSY_TEMP) {
        return format!("at {lo:.0}C or more for {span}");
    }
    String::new()
}

fn sustained(window: &[&GpuPoint], figure: fn(&GpuPoint) -> f64, floor: f64) -> Option<f64> {
    let mut lo = figure(window[0]);
    for gpu in window {
        let value = figure(gpu);
        if value < 0.0 || value < floor {
            return None;
        }
        lo = lo.min(value);
    }
    Some(lo)
}

fn go_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64().round() as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes < 60 {
        return format!("{minutes}m{seconds}s");
    }
    format!("{}h{}m{seconds}s", minutes / 60, minutes % 60)
}

fn busy_reason(status: &v1::NodeStatus, index: i32) -> String {
    status
        .busy_gpus
        .iter()
        .find(|busy| busy.index == index)
        .map(|busy| busy.reason.clone())
        .unwrap_or_default()
}

fn busy_discrete(status: &v1::NodeStatus) -> Option<(i32, String)> {
    let metrics = status.metrics.as_ref()?;
    metrics
        .gpus
        .iter()
        .filter(|gpu| !gpu.integrated)
        .map(|gpu| (gpu.index, busy_reason(status, gpu.index)))
        .find(|(_, reason)| !reason.is_empty())
}

fn holders(status: &v1::NodeStatus) -> String {
    let names = status
        .metrics
        .as_ref()
        .and_then(|metrics| metrics.engine.as_ref())
        .map(|engine| {
            engine
                .containers
                .iter()
                .filter(|container| container.state == "running" && !container.name.is_empty())
                .take(3)
                .map(|container| container.name.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return String::new();
    }
    format!(" (running containers: {})", names.join(", "))
}

fn gpu_free_from_status(
    name: &str,
    gpu: Option<u32>,
    status: &v1::NodeStatus,
) -> Result<(), String> {
    let (index, reason) = match gpu {
        Some(index) => {
            let index = i32::try_from(index).unwrap_or(i32::MAX);
            (index, busy_reason(status, index))
        }
        None => match busy_discrete(status) {
            Some(found) => found,
            None => return Ok(()),
        },
    };
    if reason.is_empty() {
        return Ok(());
    }
    Err(format!(
        "node {name} gpu {index} is busy: {reason}{}; freeing it is a person's call, ask: protectorate msg send orchestrator \"please free {name} gpu {index} for ...\" (or msg send owner)",
        holders(status)
    ))
}
