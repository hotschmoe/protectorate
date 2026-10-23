//! Polls registered node agents and keeps the last successful metrics for hub
//! status and lease admission. HTTP work never holds the status lock.
use chrono::{DateTime, SecondsFormat, Utc};
use protectorate_connect_client::{Client, TrustRoots};
use protectorate_inventory::{Inventory, Node};
use protectorate_node_metrics::{Collector, Options};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
use zeroize::Zeroizing;

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

/// Resolve an inventory node to a transport endpoint. A tailnet resolver
/// returns a loopback proxy only after matching the authenticated netmap.
pub type NodeRoute = Arc<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync>;
pub type PollSink = Arc<dyn Fn(PollEvent) + Send + Sync>;
#[derive(Clone, Debug)]
pub struct PollEvent {
    pub kind: &'static str,
    pub name: String,
    pub address: String,
}
pub const VIRTUAL_PREFIX: &str = "tailnet+";

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
    metrics: Value,
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
}

pub struct Monitor {
    inventory: Arc<Inventory>,
    local_name: String,
    local: Option<Collector>,
    token: Mutex<Option<Zeroizing<String>>>,
    trust: Mutex<Option<TrustRoots>>,
    route: Mutex<Option<NodeRoute>>,
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
            token: Mutex::new(None),
            trust: Mutex::new(None),
            route: Mutex::new(None),
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

    pub fn set_token(&self, token: String) -> Result<(), &'static str> {
        if token.is_empty() || token.contains(['\r', '\n']) {
            return Err("invalid node agent token");
        }
        *self.token.lock().unwrap() = Some(Zeroizing::new(token));
        self.state
            .lock()
            .unwrap()
            .retain(|name, _| name == &self.local_name);
        Ok(())
    }

    pub fn set_trust(&self, trust: TrustRoots) {
        *self.trust.lock().unwrap() = Some(trust);
        self.state.lock().unwrap().clear();
    }

    pub fn set_route(&self, route: NodeRoute) {
        *self.route.lock().unwrap() = Some(route);
        self.state.lock().unwrap().clear();
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
                    .map(|metrics| protectorate_node_metrics::metrics_json(&metrics))
                    .map_err(|error| error.to_string())
            });
        self.record(&self.local_name, "local", false, false, result);
    }

    fn poll_remote(&self, node: &Node) {
        let result = (|| {
            let route = self.route.lock().unwrap().clone();
            let endpoint = match route {
                Some(route) => route(&node.name, &node.address)?,
                None => agent_url(&node.address)?,
            };
            let client = if let Some(endpoint) = endpoint.strip_prefix(VIRTUAL_PREFIX) {
                Client::new_virtual(endpoint, self.timing.fetch_timeout)
            } else {
                let token = self
                    .token
                    .lock()
                    .unwrap()
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| "node agent token is not configured".to_owned())?;
                let trust = self.trust.lock().unwrap().clone();
                Client::new_with_trust(
                    &endpoint,
                    token.to_string(),
                    self.timing.fetch_timeout,
                    trust.as_ref(),
                )
            }
            .map_err(|error| error.to_string())?;
            client
                .call("/protectorate.v1.NodeAgentService/GetMetrics", &json!({}))
                .map_err(|error| error.to_string())
        })();
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
            let os = metrics.get("os").and_then(Value::as_str).unwrap_or("");
            let arch = metrics.get("arch").and_then(Value::as_str).unwrap_or("");
            let _ = self.inventory.set_platform(&node.name, os, arch);
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
        result: Result<Value, String>,
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
            Ok(metrics) => {
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

    pub fn status(&self, name: &str, address: &str, invited: bool) -> Value {
        if invited {
            return json!({"freshness":"NODE_FRESHNESS_INVITED", "error":"invited, not yet joined"});
        }
        let states = self.state.lock().unwrap();
        let state = states.get(name).filter(|state| state.address == address);
        let Some(state) = state.filter(|state| state.attempted) else {
            return json!({"error":"awaiting first poll"});
        };
        let Some(good) = &state.good else {
            return json!({
                "freshness":"NODE_FRESHNESS_UNREACHABLE", "error":state.last_error,
                "alerts":[{"kind":"ALERT_KIND_UNREACHABLE", "message":format!("node is unreachable: {}", state.last_error)}]
            });
        };
        let age = good.at.elapsed();
        let freshness = if age > self.timing.stale_after {
            "NODE_FRESHNESS_UNREACHABLE"
        } else if state.last_error.is_empty() {
            "NODE_FRESHNESS_FRESH"
        } else {
            "NODE_FRESHNESS_STALE"
        };
        let mut status = json!({
            "metrics":good.metrics,
            "freshness":freshness,
            "fetchedAt": DateTime::<Utc>::from(good.wall_at).to_rfc3339_opts(SecondsFormat::Nanos, true),
            "age": format!("{}.{:09}s", age.as_secs(), age.subsec_nanos()),
            "error":state.last_error,
        });
        if freshness != "NODE_FRESHNESS_UNREACHABLE" {
            let busy = busy_gpus(&state.gpus, SystemTime::now());
            if !busy.is_empty() {
                status["busyGpus"] = json!(busy);
            }
        }
        if freshness == "NODE_FRESHNESS_UNREACHABLE" {
            status["alerts"] = json!([{"kind":"ALERT_KIND_UNREACHABLE", "message":format!("node is unreachable: {}", state.last_error)}]);
        }
        status
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

fn agent_url(address: &str) -> Result<String, String> {
    let (scheme, authority) = if let Some(authority) = address.strip_prefix("https://") {
        ("https", authority)
    } else if let Some(authority) = address.strip_prefix("http://") {
        ("http", authority)
    } else {
        let loopback = address == "127.0.0.1"
            || address.starts_with("127.0.0.1:")
            || address == "[::1]"
            || address.starts_with("[::1]:");
        if loopback {
            ("http", address)
        } else {
            ("https", address)
        }
    };
    let has_port = if authority.starts_with('[') {
        authority.contains("]:")
    } else {
        authority.contains(':')
    };
    let authority = if has_port {
        authority.to_owned()
    } else {
        format!("{authority}:{AGENT_PORT}")
    };
    if scheme == "http" {
        let port = authority
            .strip_prefix("127.0.0.1:")
            .or_else(|| authority.strip_prefix("[::1]:"))
            .ok_or_else(|| {
                "node agent HTTP requires loopback; configure TLS for remote nodes".to_owned()
            })?;
        if port.parse::<u16>().ok().filter(|port| *port != 0).is_none() {
            return Err("invalid node agent port".into());
        }
    }
    Ok(format!("{scheme}://{authority}"))
}

fn age(later: SystemTime, earlier: SystemTime) -> Duration {
    later.duration_since(earlier).unwrap_or(Duration::ZERO)
}

fn uint(value: &Value, key: &str) -> Option<u64> {
    match value.get(key)? {
        Value::String(text) => text.parse().ok(),
        Value::Number(number) => number.as_u64(),
        _ => None,
    }
}

fn int32(value: &Value, key: &str) -> Option<i32> {
    match value.get(key)? {
        Value::Number(number) => number.as_i64().and_then(|n| i32::try_from(n).ok()),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn float(value: &Value, key: &str) -> f64 {
    match value.get(key) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn gpu_points(metrics: &Value) -> Vec<GpuPoint> {
    metrics
        .get("gpus")
        .and_then(Value::as_array)
        .map(|gpus| {
            gpus.iter()
                .filter_map(|gpu| {
                    Some(GpuPoint {
                        index: int32(gpu, "index")?,
                        memory_used_bytes: uint(gpu, "memoryUsedBytes"),
                        memory_total_bytes: uint(gpu, "memoryTotalBytes").unwrap_or(0),
                        utilization_percent: float(gpu, "utilizationPercent"),
                        temperature_celsius: float(gpu, "temperatureCelsius"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn add_gpu_sample(mut hist: Vec<GpuSample>, at: SystemTime, metrics: &Value) -> Vec<GpuSample> {
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

fn busy_gpus(hist: &[GpuSample], now: SystemTime) -> Vec<Value> {
    let Some(latest) = hist.last() else {
        return Vec::new();
    };
    latest
        .gpus
        .iter()
        .filter_map(|gpu| {
            let reason = gpu_busy_reason(gpu, hist, now);
            (!reason.is_empty()).then(|| json!({"index": gpu.index, "reason": reason}))
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

fn busy_reason(status: &Value, index: i32) -> String {
    status
        .get("busyGpus")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|busy| int32(busy, "index") == Some(index))
        .and_then(|busy| busy.get("reason").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}

fn busy_discrete(status: &Value) -> Option<(i32, String)> {
    let gpus = status.pointer("/metrics/gpus").and_then(Value::as_array)?;
    for gpu in gpus {
        if gpu.get("integrated").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(index) = int32(gpu, "index") else {
            continue;
        };
        let reason = busy_reason(status, index);
        if !reason.is_empty() {
            return Some((index, reason));
        }
    }
    None
}

fn holders(status: &Value) -> String {
    let mut names = Vec::new();
    let Some(containers) = status
        .pointer("/metrics/engine/containers")
        .and_then(Value::as_array)
    else {
        return String::new();
    };
    for container in containers {
        if container.get("state").and_then(Value::as_str) == Some("running") && names.len() < 3 {
            if let Some(name) = container.get("name").and_then(Value::as_str) {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return String::new();
    }
    format!(" (running containers: {})", names.join(", "))
}

fn gpu_free_from_status(name: &str, gpu: Option<u32>, status: &Value) -> Result<(), String> {
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
