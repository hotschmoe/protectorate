//! What a node reports about its accelerators and container engine comes
//! from vendor tool output and the Docker Engine API; neither may crash the
//! node agent or put nonsense gauges in front of the scheduler.
use crate::fields;
use protectorate_node_metrics::{
    accel::{probe_with_tools, AccelState, ToolRunner},
    docker,
};
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    os::unix::fs::symlink,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

/// Discrete NVIDIA, AMD and Intel cards and an integrated Intel GPU, so every
/// vendor tool runs and its gauges are merged onto a sysfs card.
const CARDS: &[(&str, &str, &str, &str)] = &[
    ("0000:00:02.0", "0x8086", "0x46a6", "intel"),
    ("0000:01:00.0", "0x10de", "0x2684", "nvidia"),
    ("0000:03:00.0", "0x1002", "0x744c", "amd"),
    ("0000:04:00.0", "0x8086", "0x56a0", "intel"),
];

fn sysfs(root: &Path) {
    for (bus, vendor, device, _) in CARDS {
        let dev = root.join("bus/pci/devices").join(bus);
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("class"), "0x030000\n").unwrap();
        fs::write(dev.join("vendor"), format!("{vendor}\n")).unwrap();
        fs::write(dev.join("device"), format!("{device}\n")).unwrap();
    }
    let drm = root.join("class/drm/card1");
    fs::create_dir_all(&drm).unwrap();
    symlink("../../../bus/pci/devices/0000:03:00.0", drm.join("device")).unwrap();
}

/// nvidia-smi, rocm-smi, xpu-smi discovery and xpu-smi dump output.
struct Tools(Vec<String>);

impl ToolRunner for Tools {
    fn run(&self, name: &str, args: &[&str]) -> Option<String> {
        let index = match (name, args.first().copied()) {
            ("nvidia-smi", _) => 0,
            ("rocm-smi", _) => 1,
            ("xpu-smi", Some("discovery")) => 2,
            ("xpu-smi", Some("dump")) => 3,
            _ => return None,
        };
        Some(self.0[index].clone()).filter(|out| !out.is_empty())
    }
}

/// Vendor tool output merged onto a fixed sysfs inventory.
pub fn gpu_tools(data: &[u8]) {
    let parts = fields(data, 4);
    let tools = Tools(parts.iter().map(|part| crate::text(part)).collect());
    let root = tempfile::tempdir().unwrap();
    sysfs(root.path());
    let (gpus, _) = probe_with_tools(root.path(), &mut AccelState::default(), &tools).unwrap();
    for (index, gpu) in gpus.iter().enumerate() {
        assert_eq!(gpu.index as usize, index, "indices are not dense: {gpus:?}");
        let utilization = gpu.utilization_percent;
        assert!(
            utilization == -1.0 || (0.0..=100.0).contains(&utilization),
            "utilization {utilization} out of range: {gpu:?}"
        );
        assert!(gpu.temperature_celsius.is_finite(), "{gpu:?}");
        if gpu.integrated {
            assert_eq!((gpu.memory_total_bytes, gpu.memory_used_bytes), (0, None));
        }
    }
    assert!(
        gpus.windows(2).all(|w| w[0].integrated <= w[1].integrated),
        "an integrated GPU sorts before a discrete one: {gpus:?}"
    );
    for (bus, _, _, vendor) in CARDS {
        assert_eq!(
            gpus.iter()
                .filter(|gpu| gpu.bus_id == *bus && gpu.vendor == *vendor)
                .count(),
            1,
            "sysfs card {bus} lost or duplicated: {gpus:?}"
        );
    }
}

/// A loopback Docker Engine answering each API path with a raw HTTP response
/// from the input, then closing the connection.
fn engine(responses: Vec<Vec<u8>>) -> (String, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&stop);
    let server = thread::spawn(move || {
        let responses = Arc::new(responses);
        for stream in listener.incoming() {
            if done.load(Ordering::Acquire) {
                break;
            }
            let Ok(mut stream) = stream else { continue };
            let responses = Arc::clone(&responses);
            thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut head = Vec::new();
                let mut buffer = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buffer[..n]),
                    }
                }
                let line = String::from_utf8_lossy(&head);
                let path = line.split(' ').nth(1).unwrap_or("");
                let route = if path == "/version" {
                    0
                } else if path.contains("/containers/json") {
                    1
                } else if path.contains("/system/df") {
                    2
                } else {
                    3
                };
                let _ = stream.write_all(&responses[route]);
                let _ = stream.shutdown(Shutdown::Both);
            });
        }
    });
    (address, stop, server)
}

/// /version, /containers/json, /system/df and container stats responses.
pub fn docker_engine(data: &[u8]) {
    let responses = fields(data, 4).into_iter().map(<[u8]>::to_vec).collect();
    let (address, stop, server) = engine(responses);
    let probed = docker::probe(Some(&format!("tcp://{address}")), true);
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(&address);
    server.join().unwrap();
    let Ok(Some(engine)) = probed else {
        return;
    };
    assert_eq!(engine.kind, "docker");
    assert!(
        engine.containers.windows(2).all(|w| w[0].name <= w[1].name),
        "containers are not sorted by name"
    );
    for container in &engine.containers {
        assert!(!container.id.is_empty() && container.id.chars().count() <= 12);
        assert!(container.cpu_percent.is_finite() && container.cpu_percent >= 0.0);
        assert!(container.memory_percent.is_finite() && container.memory_percent >= 0.0);
    }
    if let Some(disk) = engine.disk {
        for bucket in [
            &disk.images,
            &disk.containers,
            &disk.volumes,
            &disk.build_cache,
        ] {
            assert!(bucket.size_bytes >= 0 && bucket.reclaimable_bytes >= 0);
        }
        assert!(disk.images.reclaimable_bytes <= disk.images.size_bytes);
    }
}
