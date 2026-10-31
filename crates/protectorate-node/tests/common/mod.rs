//! Shared by the e2e test files; each file uses part of it.
#![allow(dead_code)]

use pt_json::Value;

pub const BIN: &str = env!("CARGO_BIN_EXE_protectorate-node");

/// Parses with pt-json's strict parser.
pub fn json(s: &str) -> Value {
    pt_json::parse(s.trim()).unwrap_or_else(|e| panic!("invalid JSON ({e}): {s}"))
}

pub fn num(v: &Value, key: &str) -> u64 {
    v[key]
        .as_u64()
        .unwrap_or_else(|| panic!("field {key} is not an unsigned number in {v}"))
}

pub fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("field {key} is not a string in {v}"))
}

pub fn check_inventory(v: &Value) {
    assert!(num(v, "ram_total_bytes") > 0);
    assert!(num(v, "threads") >= 1);
    assert!(num(v, "cores") >= 1 && num(v, "cores") <= num(v, "threads"));
    let disks = v["disks"].as_array().expect("disks");
    assert!(!disks.is_empty(), "no disk in {v}");
    assert!(num(&disks[0], "total_bytes") > 0);
    assert!(!text(v, "hostname").is_empty());
    assert!(!text(v, "cpu_model").is_empty());
    assert!(!text(v, "arch").is_empty());
    assert!(v["toolchains"].as_array().is_some(), "{v}");
}

pub fn check_sample(v: &Value) {
    let total = num(v, "mem_total_bytes");
    let avail = num(v, "mem_available_bytes");
    assert!(total > 0 && avail > 0 && avail <= total);
    assert_eq!(num(v, "mem_used_bytes"), total - avail);
    assert!(num(v, "uptime_secs") > 0);
    assert!(
        !v["disks"].as_array().expect("disks").is_empty(),
        "no disk in {v}"
    );
    let cpu = v["cpu_percent"].as_f64().expect("cpu_percent");
    assert!((0.0..=100.0).contains(&cpu));
}

pub fn check_presence_value(v: &Value, away_after: u64) {
    let state = text(v, "state");
    assert!(
        ["PRESENT", "AWAY", "UNKNOWN"].contains(&state),
        "state {state}"
    );
    assert!(!text(v, "source").is_empty(), "no source in {v}");
    assert_eq!(num(v, "away_after_secs"), away_after);
    if state == "AWAY" {
        // An idle figure, when there is one, must be at or past the threshold or the screen locked.
        if let Some(idle) = v["idle_secs"].as_u64() {
            let locked = v["locked"].as_bool() == Some(true);
            assert!(idle >= away_after || locked, "AWAY with idle {idle} in {v}");
        }
    }
}
