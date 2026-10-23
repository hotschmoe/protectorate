//! Small read-only Docker Engine client. Linux defaults to the local Unix
//! socket; Windows probes only an explicitly configured TCP or named pipe host.
use crate::{ContainerEngine, ContainerMetrics, EngineDiskUsage};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_BODY: usize = 64 << 20;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Clone)]
enum Endpoint {
    Unix(String),
    Tcp(String),
    Pipe(String),
}

fn pipe_name(raw: &str) -> io::Result<String> {
    let path = raw.replace('/', "\\");
    let Some(name) = path.strip_prefix("\\\\.\\pipe\\") else {
        return Err(invalid(
            "Docker named pipe must be a local \\\\.\\pipe\\ path",
        ));
    };
    if name.is_empty()
        || name.contains('\\')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid("invalid Docker named pipe name"));
    }
    Ok(path)
}

fn endpoint(host: Option<&str>) -> io::Result<Endpoint> {
    let host = host
        .map(str::to_owned)
        .or_else(|| std::env::var("DOCKER_HOST").ok())
        .unwrap_or_else(|| "unix:///var/run/docker.sock".into());
    if let Some(path) = host.strip_prefix("unix://") {
        if path.is_empty() || !Path::new(path).is_absolute() {
            return Err(invalid("invalid Docker Unix socket"));
        }
        return Ok(Endpoint::Unix(path.into()));
    }
    if let Some(address) = host.strip_prefix("tcp://") {
        if address.is_empty() || address.contains(['/', '\r', '\n']) {
            return Err(invalid("invalid Docker TCP endpoint"));
        }
        return Ok(Endpoint::Tcp(address.into()));
    }
    if let Some(path) = host.strip_prefix("npipe://") {
        return Ok(Endpoint::Pipe(pipe_name(path)?));
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "unsupported Docker host scheme",
    ))
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn connect_unix(socket: &str, timeout: Duration) -> io::Result<Box<dyn ReadWrite>> {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;
        let stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Box::new(stream))
    }
    #[cfg(not(unix))]
    {
        let _ = (socket, timeout);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Docker Unix sockets are unavailable on Windows",
        ))
    }
}

impl Endpoint {
    fn connect(&self, timeout: Duration) -> io::Result<Box<dyn ReadWrite>> {
        match self {
            Self::Unix(socket) => connect_unix(socket, timeout),
            Self::Tcp(address) => {
                let mut addresses = address.to_socket_addrs()?;
                let addr = addresses
                    .next()
                    .ok_or_else(|| invalid("Docker TCP host did not resolve"))?;
                let stream = TcpStream::connect_timeout(&addr, timeout)?;
                stream.set_read_timeout(Some(timeout))?;
                stream.set_write_timeout(Some(timeout))?;
                Ok(Box::new(stream))
            }
            Self::Pipe(path) => connect_pipe(path, timeout),
        }
    }
}

#[cfg(windows)]
fn connect_pipe(path: &str, timeout: Duration) -> io::Result<Box<dyn ReadWrite>> {
    Ok(Box::new(crate::docker_pipe::Pipe::connect(path, timeout)?))
}

#[cfg(not(windows))]
fn connect_pipe(_path: &str, _timeout: Duration) -> io::Result<Box<dyn ReadWrite>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Docker named pipes are available only on Windows",
    ))
}

fn decode_chunked(mut body: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let Some(split) = body.windows(2).position(|w| w == b"\r\n") else {
            return Err(invalid("invalid chunk header"));
        };
        let size = std::str::from_utf8(&body[..split])
            .ok()
            .and_then(|s| usize::from_str_radix(s.split(';').next().unwrap_or(""), 16).ok())
            .ok_or_else(|| invalid("invalid chunk size"))?;
        body = &body[split + 2..];
        if size == 0 {
            return Ok(out);
        }
        if size > MAX_BODY.saturating_sub(out.len())
            || body.len() < size + 2
            || &body[size..size + 2] != b"\r\n"
        {
            return Err(invalid("truncated or oversized chunk"));
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

fn get(endpoint: &Endpoint, path: &str, timeout: Duration) -> io::Result<Value> {
    let mut stream = endpoint.connect(timeout)?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: docker\r\nConnection: close\r\nAccept: application/json\r\n\r\n")?;
    let mut raw = Vec::new();
    stream
        .take((MAX_BODY + 16 * 1024) as u64)
        .read_to_end(&mut raw)?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| invalid("missing Docker HTTP headers"))?;
    let head =
        std::str::from_utf8(&raw[..split]).map_err(|_| invalid("invalid Docker HTTP headers"))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1));
    if status != Some("200") {
        return Err(io::Error::other(format!(
            "Docker {path}: HTTP {}",
            status.unwrap_or("unknown")
        )));
    }
    let data = &raw[split + 4..];
    let body = if head.lines().any(|line| {
        line.to_ascii_lowercase().starts_with("transfer-encoding:")
            && line.to_ascii_lowercase().contains("chunked")
    }) {
        decode_chunked(data)?
    } else {
        if data.len() > MAX_BODY {
            return Err(invalid("Docker response too large"));
        }
        data.to_vec()
    };
    serde_json::from_slice(&body).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn uint(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn int(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}
fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn version(server: &str, minimum: &str) -> String {
    fn pair(s: &str) -> (u16, u16) {
        let mut p = s.split('.');
        (
            p.next().and_then(|v| v.parse().ok()).unwrap_or(1),
            p.next().and_then(|v| v.parse().ok()).unwrap_or(0),
        )
    }
    let chosen = pair(server).min((1, 43)).max(pair(minimum));
    format!("{}.{}", chosen.0, chosen.1)
}

pub fn parse_disk(value: &Value) -> (EngineDiskUsage, Vec<(String, i64, i64)>) {
    let mut out = EngineDiskUsage::default();
    let mut sizes = Vec::new();
    let mut image_sum: i64 = 0;
    for image in array(value, "Images") {
        out.images.count += 1;
        let size = int(image, "Size").max(0);
        image_sum = image_sum.saturating_add(size);
        if int(image, "Containers") == 0 {
            out.images.reclaimable_bytes = out.images.reclaimable_bytes.saturating_add(size);
        }
    }
    out.images.size_bytes = int(value, "LayersSize").max(0);
    if out.images.size_bytes == 0 {
        out.images.size_bytes = image_sum;
    }
    out.images.reclaimable_bytes = out.images.reclaimable_bytes.min(out.images.size_bytes);
    for container in array(value, "Containers") {
        let rw = int(container, "SizeRw").max(0);
        let rootfs = int(container, "SizeRootFs").max(0);
        out.containers.count += 1;
        out.containers.size_bytes = out.containers.size_bytes.saturating_add(rw);
        if string(container, "State") != "running" {
            out.containers.reclaimable_bytes = out.containers.reclaimable_bytes.saturating_add(rw);
        }
        sizes.push((string(container, "Id").into(), rw, rootfs));
    }
    for volume in array(value, "Volumes") {
        out.volumes.count += 1;
        if let Some(usage) = volume.get("UsageData") {
            let size = int(usage, "Size").max(0);
            out.volumes.size_bytes = out.volumes.size_bytes.saturating_add(size);
            if int(usage, "RefCount") == 0 {
                out.volumes.reclaimable_bytes = out.volumes.reclaimable_bytes.saturating_add(size);
            }
        }
    }
    for cache in array(value, "BuildCache") {
        let size = int(cache, "Size").max(0);
        out.build_cache.count += 1;
        out.build_cache.size_bytes = out.build_cache.size_bytes.saturating_add(size);
        if !cache.get("InUse").and_then(Value::as_bool).unwrap_or(false) {
            out.build_cache.reclaimable_bytes =
                out.build_cache.reclaimable_bytes.saturating_add(size);
        }
    }
    (out, sizes)
}

pub fn apply_stats(container: &mut ContainerMetrics, value: &Value) {
    let memory = &value["memory_stats"];
    let usage = uint(memory, "usage");
    let mut cache = uint(&memory["stats"], "inactive_file");
    if cache == 0 {
        cache = uint(&memory["stats"], "total_inactive_file");
    }
    container.memory_used_bytes = usage.saturating_sub(cache);
    container.memory_limit_bytes = uint(memory, "limit");
    if container.memory_limit_bytes > 0 {
        container.memory_percent =
            container.memory_used_bytes as f64 / container.memory_limit_bytes as f64 * 100.0;
    }
    let cpu = &value["cpu_stats"];
    let old = &value["precpu_stats"];
    let delta = uint(&cpu["cpu_usage"], "total_usage")
        .saturating_sub(uint(&old["cpu_usage"], "total_usage"));
    let wall = uint(cpu, "system_cpu_usage").saturating_sub(uint(old, "system_cpu_usage"));
    let cores = uint(cpu, "online_cpus").max(array(&cpu["cpu_usage"], "percpu_usage").len() as u64);
    if wall > 0 {
        container.cpu_percent = delta as f64 / wall as f64 * cores as f64 * 100.0;
    }
}

pub fn probe(host: Option<&str>, include_disk: bool) -> io::Result<Option<ContainerEngine>> {
    let endpoint = endpoint(host)?;
    if let Endpoint::Unix(socket) = &endpoint {
        if fs::metadata(socket).is_err() {
            return Ok(None);
        }
    }
    let info = match get(&endpoint, "/version", Duration::from_secs(3)) {
        Ok(value) => value,
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None)
        }
        Err(err) => return Err(err),
    };
    let prefix = format!(
        "/v{}",
        version(string(&info, "ApiVersion"), string(&info, "MinAPIVersion"))
    );
    let list = get(
        &endpoint,
        &format!("{prefix}/containers/json?all=1"),
        Duration::from_secs(10),
    )?;
    let mut engine = ContainerEngine {
        kind: "docker".into(),
        version: string(&info, "Version").into(),
        ..ContainerEngine::default()
    };
    let mut sizes = Vec::new();
    if include_disk {
        if let Ok(value) = get(
            &endpoint,
            &format!("{prefix}/system/df"),
            Duration::from_secs(30),
        ) {
            let (disk, container_sizes) = parse_disk(&value);
            engine.disk = Some(disk);
            engine.disk_measured_at = Some(SystemTime::now());
            sizes = container_sizes;
        }
    }
    let Some(containers) = list.as_array() else {
        return Err(invalid("Docker container list is not an array"));
    };
    let mut pending = Vec::new();
    for item in containers {
        let id = string(item, "Id");
        if id.is_empty() {
            continue;
        }
        let (rw, rootfs) = sizes
            .iter()
            .find(|(name, _, _)| name == id)
            .map(|(_, rw, rootfs)| (*rw, *rootfs))
            .unwrap_or_default();
        let container = ContainerMetrics {
            id: id.chars().take(12).collect(),
            name: array(item, "Names")
                .first()
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_start_matches('/')
                .into(),
            image: string(item, "Image").into(),
            state: string(item, "State").into(),
            writable_layer_bytes: rw,
            rootfs_bytes: rootfs,
            created_at: u64::try_from(int(item, "Created"))
                .ok()
                .filter(|&n| n > 0)
                .map(|n| UNIX_EPOCH + Duration::from_secs(n)),
            ..ContainerMetrics::default()
        };
        pending.push((id.to_owned(), container));
    }
    for batch in pending.chunks(8) {
        let fetched = thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .cloned()
                .map(|(id, mut container)| {
                    let endpoint = &endpoint;
                    let prefix = &prefix;
                    scope.spawn(move || {
                        if container.state == "running"
                            && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                        {
                            match get(
                                endpoint,
                                &format!("{prefix}/containers/{id}/stats?stream=false"),
                                Duration::from_secs(10),
                            ) {
                                Ok(value) => apply_stats(&mut container, &value),
                                Err(err) => container.stats_error = err.to_string(),
                            }
                        }
                        container
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_default())
                .collect::<Vec<_>>()
        });
        engine.containers.extend(fetched);
    }
    engine.containers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Some(engine))
}
