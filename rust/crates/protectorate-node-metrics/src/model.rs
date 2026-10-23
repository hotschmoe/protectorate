use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// The measurements supplied by a node. Optional subsystems can fill in the
/// accelerator, engine, directory, sleeve and job fields before publication.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMetrics {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub platform: String,
    pub agent_version: String,
    pub uptime_seconds: i64,
    pub collected_at: Option<SystemTime>,
    pub cpu: Option<CpuMetrics>,
    pub memory: Option<MemoryMetrics>,
    pub filesystems: Vec<FilesystemMetrics>,
    pub gpus: Vec<GpuMetrics>,
    pub engine: Option<ContainerEngine>,
    pub watch_dirs: Vec<DirSize>,
    pub warnings: Vec<String>,
    pub sleeves: Vec<SleeveMetrics>,
    pub npus: Vec<AcceleratorMetrics>,
    pub jobs: Option<JobIsolation>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuMetrics {
    pub logical_cores: i32,
    pub usage_percent: f64,
    pub load1: Option<f64>,
    pub load5: Option<f64>,
    pub load15: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_used_percent: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemMetrics {
    pub mount: String,
    pub device: String,
    pub fstype: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub used_percent: f64,
    pub primary: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetrics {
    pub index: i32,
    pub vendor: String,
    pub name: String,
    pub utilization_percent: f64,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: u64,
    pub temperature_celsius: f64,
    pub integrated: bool,
    pub driver: String,
    pub bus_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceleratorMetrics {
    pub kind: String,
    pub vendor: String,
    pub name: String,
    pub driver: String,
    pub utilization_percent: f64,
    pub frequency_hz: u64,
    pub bus_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerEngine {
    pub kind: String,
    pub version: String,
    pub disk: Option<EngineDiskUsage>,
    pub disk_measured_at: Option<SystemTime>,
    pub containers: Vec<ContainerMetrics>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDiskUsage {
    pub images: DiskBucket,
    pub containers: DiskBucket,
    pub volumes: DiskBucket,
    pub build_cache: DiskBucket,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskBucket {
    pub count: i64,
    pub size_bytes: i64,
    pub reclaimable_bytes: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerMetrics {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_limit_bytes: u64,
    pub memory_percent: f64,
    pub writable_layer_bytes: i64,
    pub rootfs_bytes: i64,
    pub stats_error: String,
    pub created_at: Option<SystemTime>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirSize {
    pub path: String,
    pub size_bytes: i64,
    pub measured_at: Option<SystemTime>,
    pub took: Duration,
    pub complete: bool,
    pub error: String,
}

/// Produced by the sleeve runtime; this crate only carries it in snapshots.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleeveMetrics {
    pub id: String,
    pub name: String,
    pub state: String,
}

/// Produced by the job runtime; this crate only carries it in snapshots.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobIsolation {
    pub sandbox: String,
    pub note: String,
    pub admin: String,
}
