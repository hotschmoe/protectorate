use crate::{DiskBucket, NodeMetrics};
use protectorate_wire::{self as wire, v1, ReflectMessage};
use serde_json::Value;

fn disk_bucket(bucket: &DiskBucket) -> v1::DiskBucket {
    v1::DiskBucket {
        count: bucket.count,
        size_bytes: bucket.size_bytes,
        reclaimable_bytes: bucket.reclaimable_bytes,
    }
}

/// The collector snapshot as the API's NodeMetrics.
pub fn metrics_proto(m: &NodeMetrics) -> v1::NodeMetrics {
    v1::NodeMetrics {
        hostname: m.hostname.clone(),
        os: m.os.clone(),
        arch: m.arch.clone(),
        platform: m.platform.clone(),
        agent_version: m.agent_version.clone(),
        uptime_seconds: m.uptime_seconds,
        collected_at: m.collected_at.map(Into::into),
        cpu: m.cpu.as_ref().map(|cpu| v1::CpuMetrics {
            logical_cores: cpu.logical_cores,
            usage_percent: cpu.usage_percent,
            load1: cpu.load1,
            load5: cpu.load5,
            load15: cpu.load15,
        }),
        memory: m.memory.as_ref().map(|memory| v1::MemoryMetrics {
            total_bytes: memory.total_bytes,
            used_bytes: memory.used_bytes,
            available_bytes: memory.available_bytes,
            used_percent: memory.used_percent,
            swap_total_bytes: memory.swap_total_bytes,
            swap_used_bytes: memory.swap_used_bytes,
            swap_used_percent: memory.swap_used_percent,
        }),
        filesystems: m
            .filesystems
            .iter()
            .map(|f| v1::FilesystemMetrics {
                mount: f.mount.clone(),
                device: f.device.clone(),
                fstype: f.fstype.clone(),
                total_bytes: f.total_bytes,
                used_bytes: f.used_bytes,
                free_bytes: f.free_bytes,
                used_percent: f.used_percent,
                primary: f.primary,
            })
            .collect(),
        gpus: m
            .gpus
            .iter()
            .map(|gpu| v1::GpuMetrics {
                index: gpu.index,
                vendor: gpu.vendor.clone(),
                name: gpu.name.clone(),
                utilization_percent: gpu.utilization_percent,
                memory_used_bytes: gpu.memory_used_bytes,
                memory_total_bytes: gpu.memory_total_bytes,
                temperature_celsius: gpu.temperature_celsius,
                integrated: gpu.integrated,
                driver: gpu.driver.clone(),
                bus_id: gpu.bus_id.clone(),
            })
            .collect(),
        engine: m.engine.as_ref().map(|engine| v1::ContainerEngine {
            kind: engine.kind.clone(),
            version: engine.version.clone(),
            disk: engine.disk.as_ref().map(|disk| v1::EngineDiskUsage {
                images: Some(disk_bucket(&disk.images)),
                containers: Some(disk_bucket(&disk.containers)),
                volumes: Some(disk_bucket(&disk.volumes)),
                build_cache: Some(disk_bucket(&disk.build_cache)),
            }),
            disk_measured_at: engine.disk_measured_at.map(Into::into),
            containers: engine
                .containers
                .iter()
                .map(|container| v1::ContainerMetrics {
                    id: container.id.clone(),
                    name: container.name.clone(),
                    image: container.image.clone(),
                    state: container.state.clone(),
                    cpu_percent: container.cpu_percent,
                    memory_used_bytes: container.memory_used_bytes,
                    memory_limit_bytes: container.memory_limit_bytes,
                    memory_percent: container.memory_percent,
                    writable_layer_bytes: container.writable_layer_bytes,
                    rootfs_bytes: container.rootfs_bytes,
                    stats_error: container.stats_error.clone(),
                    created_at: container.created_at.map(Into::into),
                })
                .collect(),
        }),
        watch_dirs: m
            .watch_dirs
            .iter()
            .map(|dir| v1::DirSize {
                path: dir.path.clone(),
                size_bytes: dir.size_bytes,
                measured_at: dir.measured_at.map(Into::into),
                took: Some(wire::proto_duration(dir.took)),
                complete: dir.complete,
                error: dir.error.clone(),
            })
            .collect(),
        warnings: m.warnings.clone(),
        sleeves: m
            .sleeves
            .iter()
            .map(|sleeve| v1::Sleeve {
                name: if sleeve.name.is_empty() {
                    sleeve.id.clone()
                } else {
                    sleeve.name.clone()
                },
                state: match sleeve.state.to_ascii_lowercase().as_str() {
                    "running" | "sleeve_state_running" => v1::SleeveState::Running,
                    "exited" | "sleeve_state_exited" => v1::SleeveState::Exited,
                    _ => v1::SleeveState::Unspecified,
                } as i32,
                ..Default::default()
            })
            .collect(),
        npus: m
            .npus
            .iter()
            .map(|npu| v1::AcceleratorMetrics {
                kind: npu.kind.clone(),
                vendor: npu.vendor.clone(),
                name: npu.name.clone(),
                driver: npu.driver.clone(),
                utilization_percent: npu.utilization_percent,
                frequency_hz: npu.frequency_hz,
                bus_id: npu.bus_id.clone(),
            })
            .collect(),
        jobs: m.jobs.as_ref().map(|jobs| v1::JobIsolation {
            sandbox: jobs.sandbox.clone(),
            note: jobs.note.clone(),
            admin: jobs.admin.clone(),
        }),
        ..Default::default()
    }
}

/// The snapshot as protobuf JSON, for printing.
pub fn metrics_json(m: &NodeMetrics) -> Value {
    serde_json::to_value(metrics_proto(m).transcode_to_dynamic()).unwrap_or(Value::Null)
}
