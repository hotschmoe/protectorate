//! The node's wire answers (pt-frame) as the hub API's view types (pt-api).

use pt_api::EpochMs;
use pt_frame::msg;

fn ms(secs: u64) -> EpochMs {
    EpochMs(secs.saturating_mul(1000))
}

pub fn presence(p: &msg::Presence) -> pt_api::Presence {
    pt_api::Presence {
        state: match p.state {
            msg::PresenceState::Present => pt_api::PresenceState::Present,
            msg::PresenceState::Away => pt_api::PresenceState::Away,
            msg::PresenceState::Unknown => pt_api::PresenceState::Unknown,
        },
        idle_secs: p.idle_secs,
        locked: p.locked,
        source: p.source.clone(),
        sampled_at_ms: Some(ms(p.sampled_at)),
    }
}

pub fn unknown_presence() -> pt_api::Presence {
    pt_api::Presence {
        state: pt_api::PresenceState::Unknown,
        idle_secs: None,
        locked: None,
        source: String::new(),
        sampled_at_ms: None,
    }
}

pub fn inventory(i: &msg::Inventory) -> pt_api::Inventory {
    pt_api::Inventory {
        hostname: i.hostname.clone(),
        os: i.os_name.clone(),
        os_version: i.os_version.clone(),
        arch: i.arch.clone(),
        cpu_model: i.cpu_model.clone(),
        cpu_count: u64::from(i.threads),
        ram_bytes: i.ram_total_bytes,
        gpus: i
            .gpus
            .iter()
            .map(|g| pt_api::Gpu {
                name: g.name.clone(),
                driver: g.driver.clone().unwrap_or_default(),
                vram_bytes: g.vram_bytes,
                util_pct: None,
            })
            .collect(),
        disks: i.disks.iter().map(disk).collect(),
        toolchains: i
            .toolchains
            .iter()
            .map(|t| pt_api::Toolchain {
                name: t.name.clone(),
                version: t.version.clone(),
            })
            .collect(),
        collected_at_ms: ms(i.collected_at),
    }
}

fn disk(d: &msg::Disk) -> pt_api::Disk {
    pt_api::Disk {
        mount: d.mount.clone(),
        size_bytes: d.total_bytes,
        free_bytes: d.free_bytes,
    }
}

pub fn sample(s: &msg::Sample) -> pt_api::Sample {
    pt_api::Sample {
        sampled_at_ms: ms(s.taken_at),
        cpu_pct: s.cpu_percent,
        load1: s.load.as_ref().and_then(|l| l.first().copied()),
        mem_avail_bytes: s.mem_available_bytes,
        swap_used_bytes: match (s.swap_total_bytes, s.swap_free_bytes) {
            (Some(t), Some(f)) => Some(t.saturating_sub(f)),
            _ => None,
        },
    }
}
