use crate::{DiskBucket, NodeMetrics};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};

fn timestamp(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Nanos, true)
}
fn duration(duration: Duration) -> String {
    if duration.subsec_nanos() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}.{:09}s", duration.as_secs(), duration.subsec_nanos())
    }
}
fn disk_bucket(bucket: &DiskBucket) -> Value {
    json!({
        "count": bucket.count.to_string(),
        "sizeBytes": bucket.size_bytes.to_string(),
        "reclaimableBytes": bucket.reclaimable_bytes.to_string(),
    })
}
/// Encodes a collector snapshot with the NodeMetrics proto JSON field shape.
pub fn metrics_json(m: &NodeMetrics) -> Value {
    let mut out = json!({
        "hostname": m.hostname, "os": m.os, "arch": m.arch,
        "platform": m.platform, "agentVersion": m.agent_version,
        "uptimeSeconds": m.uptime_seconds.to_string(), "warnings": m.warnings,
        "filesystems": m.filesystems.iter().map(|f| json!({
            "mount": f.mount, "device": f.device, "fstype": f.fstype,
            "totalBytes": f.total_bytes.to_string(), "usedBytes": f.used_bytes.to_string(),
            "freeBytes": f.free_bytes.to_string(), "usedPercent": f.used_percent,
            "primary": f.primary,
        })).collect::<Vec<_>>(),
        "gpus": m.gpus.iter().map(|gpu| {
            let mut value = json!({
                "index": gpu.index, "vendor": gpu.vendor, "name": gpu.name,
                "utilizationPercent": gpu.utilization_percent,
                "memoryTotalBytes": gpu.memory_total_bytes.to_string(),
                "temperatureCelsius": gpu.temperature_celsius,
                "integrated": gpu.integrated, "driver": gpu.driver, "busId": gpu.bus_id,
            });
            if let Some(used) = gpu.memory_used_bytes {
                value["memoryUsedBytes"] = json!(used.to_string());
            }
            value
        }).collect::<Vec<_>>(),
        "npus": m.npus.iter().map(|npu| json!({
            "kind": npu.kind, "vendor": npu.vendor, "name": npu.name,
            "driver": npu.driver, "utilizationPercent": npu.utilization_percent,
            "frequencyHz": npu.frequency_hz.to_string(), "busId": npu.bus_id,
        })).collect::<Vec<_>>(),
        "watchDirs": m.watch_dirs.iter().map(|dir| {
            let mut value = json!({
                "path": dir.path, "sizeBytes": dir.size_bytes.to_string(),
                "took": duration(dir.took), "complete": dir.complete, "error": dir.error,
            });
            if let Some(at) = dir.measured_at {
                value["measuredAt"] = json!(timestamp(at));
            }
            value
        }).collect::<Vec<_>>(),
        "sleeves": m.sleeves.iter().map(|sleeve| json!({
            "name": if sleeve.name.is_empty() { &sleeve.id } else { &sleeve.name },
            "state": match sleeve.state.to_ascii_lowercase().as_str() {
                "running" | "sleeve_state_running" => "SLEEVE_STATE_RUNNING",
                "exited" | "sleeve_state_exited" => "SLEEVE_STATE_EXITED",
                _ => "SLEEVE_STATE_UNSPECIFIED",
            },
        })).collect::<Vec<_>>(),
    });
    if let Some(time) = m.collected_at {
        out["collectedAt"] = json!(timestamp(time));
    }
    if let Some(cpu) = &m.cpu {
        let mut value =
            json!({"logicalCores": cpu.logical_cores, "usagePercent": cpu.usage_percent});
        if let Some(load) = cpu.load1 {
            value["load1"] = json!(load);
        }
        if let Some(load) = cpu.load5 {
            value["load5"] = json!(load);
        }
        if let Some(load) = cpu.load15 {
            value["load15"] = json!(load);
        }
        out["cpu"] = value;
    }
    if let Some(memory) = &m.memory {
        out["memory"] = json!({
            "totalBytes": memory.total_bytes.to_string(), "usedBytes": memory.used_bytes.to_string(),
            "availableBytes": memory.available_bytes.to_string(), "usedPercent": memory.used_percent,
            "swapTotalBytes": memory.swap_total_bytes.to_string(), "swapUsedBytes": memory.swap_used_bytes.to_string(),
            "swapUsedPercent": memory.swap_used_percent,
        });
    }
    if let Some(engine) = &m.engine {
        let mut value = json!({
            "kind": engine.kind, "version": engine.version,
            "containers": engine.containers.iter().map(|container| {
                let mut value = json!({
                    "id": container.id, "name": container.name, "image": container.image,
                    "state": container.state, "cpuPercent": container.cpu_percent,
                    "memoryUsedBytes": container.memory_used_bytes.to_string(),
                    "memoryLimitBytes": container.memory_limit_bytes.to_string(),
                    "memoryPercent": container.memory_percent,
                    "writableLayerBytes": container.writable_layer_bytes.to_string(),
                    "rootfsBytes": container.rootfs_bytes.to_string(),
                    "statsError": container.stats_error,
                });
                if let Some(at) = container.created_at {
                    value["createdAt"] = json!(timestamp(at));
                }
                value
            }).collect::<Vec<_>>(),
        });
        if let Some(disk) = &engine.disk {
            value["disk"] = json!({
                "images": disk_bucket(&disk.images),
                "containers": disk_bucket(&disk.containers),
                "volumes": disk_bucket(&disk.volumes),
                "buildCache": disk_bucket(&disk.build_cache),
            });
        }
        if let Some(at) = engine.disk_measured_at {
            value["diskMeasuredAt"] = json!(timestamp(at));
        }
        out["engine"] = value;
    }
    if let Some(jobs) = &m.jobs {
        out["jobs"] = json!({
            "sandbox": jobs.sandbox, "note": jobs.note, "admin": jobs.admin,
        });
    }
    out
}
