//! Real node agents whose collector replays the metrics a test sets, plus a
//! hub router served over loopback HTTP.
#![allow(dead_code)]
use protectorate_connect_client::Client;
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::{GpuMetrics, NodeMetrics};
use protectorate_wire::RpcError;
use serde_json::Value;
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

/// The metrics the agent reports on its next poll; None fails the probe.
#[derive(Clone, Default)]
pub struct Metrics(Arc<Mutex<Option<NodeMetrics>>>);

impl Metrics {
    pub fn new(metrics: NodeMetrics) -> Self {
        Self(Arc::new(Mutex::new(Some(metrics))))
    }
    pub fn set(&self, metrics: Option<NodeMetrics>) {
        *self.0.lock().unwrap() = metrics;
    }
    pub fn edit(&self, change: impl FnOnce(&mut NodeMetrics)) {
        change(self.0.lock().unwrap().as_mut().expect("metrics are set"));
    }
}

impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        self.0
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| io::Error::other("probe failed"))
    }
}

pub fn gpu(index: i32, used_gib: u64, total_gib: u64, integrated: bool) -> GpuMetrics {
    GpuMetrics {
        index,
        vendor: "NVIDIA".into(),
        name: "RTX".into(),
        memory_used_bytes: Some(used_gib << 30),
        memory_total_bytes: total_gib << 30,
        temperature_celsius: 40.0,
        integrated,
        ..GpuMetrics::default()
    }
}

pub async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (addr, task)
}

/// An agent with plain-mode jobs under `jobs_root` when given.
pub async fn agent(
    metrics: &Metrics,
    jobs_root: Option<&std::path::Path>,
) -> (String, tokio::task::JoinHandle<()>) {
    let mut agent = Agent::new(Arc::new(metrics.clone()));
    if let Some(root) = jobs_root {
        agent = agent.with_jobs(Jobs::new(root, 2).unwrap().with_plain_mode());
    }
    serve(agent.router().unwrap()).await
}

pub fn client(addr: &str, token: &str) -> Client {
    Client::new(&format!("http://{addr}"), token, Duration::from_secs(20)).unwrap()
}

/// Runs a blocking RPC off the test's runtime.
pub async fn call(
    addr: &str,
    token: &str,
    procedure: &str,
    input: Value,
) -> Result<Value, RpcError> {
    let client = client(addr, token);
    let procedure = format!("/protectorate.v1.{procedure}");
    tokio::task::spawn_blocking(move || client.call(&procedure, &input))
        .await
        .unwrap()
}

pub async fn poll(monitor: &Arc<protectorate_node_monitor::Monitor>, name: &str) {
    let (monitor, name) = (Arc::clone(monitor), name.to_owned());
    tokio::task::spawn_blocking(move || monitor.poll_node(&name))
        .await
        .unwrap();
}
