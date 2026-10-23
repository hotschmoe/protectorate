//! Blocking, verified file transfer over Connect JSON. Call from
//! `spawn_blocking` when using an async runtime.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_connect_client::Client;
use protectorate_wire::{Code, Result, RpcError};
use protectorate_xfer::{Chunk, Header, Kind, Sink, Source, Trailer};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PUT: &str = "/protectorate.v1.NodeAgentService/PutFile";
const GET: &str = "/protectorate.v1.NodeAgentService/GetFile";
const CHUNK_RETRY_WINDOW: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferResult {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub skipped: u32,
}

/// Pushes a local file or directory. Each chunk is one unary PutFile call.
/// The returned path is the destination resolved by the node.
pub fn push(
    client: &Client,
    node: &str,
    local: impl AsRef<Path>,
    remote: &str,
    overwrite: bool,
    compress: bool,
) -> Result<TransferResult> {
    push_with_progress(client, node, local, remote, overwrite, compress, |_| Ok(()))
}

/// Pushes a file while reporting acknowledged uncompressed byte offsets.
/// The callback may abort before any further chunk is sent.
pub fn push_with_progress(
    client: &Client,
    node: &str,
    local: impl AsRef<Path>,
    remote: &str,
    overwrite: bool,
    compress: bool,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<TransferResult> {
    let mut source = Source::open(local, compress).map_err(xfer_error)?;
    let header = source.header();
    let id = random_id()?;
    let kind = kind_json(header.kind);
    let mut offset = 0_u64;
    let mut first = true;
    while let Some(chunk) = source.next_chunk().map_err(xfer_error)? {
        let sent = chunk.raw_size as u64;
        let input = json!({
            "name": node, "transferId": id, "path": remote, "kind": kind,
            "mode": header.mode, "overwrite": overwrite, "offset": offset.to_string(),
            "chunk": chunk_json(chunk),
        });
        let response = put_chunk_with_retry(client, &input)?;
        offset = offset
            .checked_add(sent)
            .ok_or_else(|| loss("transfer offset overflow"))?;
        verify_received(&response, offset, false)?;
        progress(offset)?;
        first = false;
    }
    let trailer = source.trailer().map_err(xfer_error)?;
    let mut final_input = json!({
        "name": node, "transferId": id, "offset": offset.to_string(), "last": true,
        "trailer": trailer_json(&trailer),
    });
    if first {
        final_input["path"] = json!(remote);
        final_input["kind"] = json!(kind);
        final_input["mode"] = json!(header.mode);
        final_input["overwrite"] = json!(overwrite);
    }
    let response = put_chunk_with_retry(client, &final_input)?;
    verify_received(&response, offset, true)?;
    let path = PathBuf::from(string(&response, "path")?);
    Ok(TransferResult {
        path,
        size: trailer.size,
        sha256: trailer.sha256,
        skipped: trailer.skipped,
    })
}

fn put_chunk_with_retry(client: &Client, input: &Value) -> Result<Value> {
    let mut retry_until = None;
    let mut pause = Duration::from_millis(200);
    loop {
        match client.call(PUT, input) {
            Ok(reply) => return Ok(reply),
            Err(error) if matches!(error.code, Code::Unavailable | Code::DeadlineExceeded) => {
                let deadline =
                    *retry_until.get_or_insert_with(|| Instant::now() + CHUNK_RETRY_WINDOW);
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(error);
                }
                std::thread::sleep(pause.min(left));
                pause = pause.saturating_mul(2).min(Duration::from_secs(2));
            }
            Err(error) => return Err(error),
        }
    }
}

/// Pulls a remote file or directory. Publishing waits until a verified
/// trailer *and* a successful Connect end frame have both arrived.
pub fn pull(
    client: &Client,
    node: &str,
    remote: &str,
    local: impl AsRef<Path>,
    overwrite: bool,
    compress: bool,
) -> Result<TransferResult> {
    let local = local.as_ref().to_path_buf();
    let mut sink: Option<Sink> = None;
    let mut trailer: Option<Trailer> = None;
    client.stream(
        GET,
        &json!({"name": node, "path": remote, "noCompress": !compress}),
        |message| {
            if let Some(header) = message.get("header") {
                if sink.is_some() || trailer.is_some() {
                    return Err(loss("duplicate file header"));
                }
                let kind = parse_kind(string(header, "kind")?)?;
                let mode = u32::try_from(number(header, "mode")?)
                    .map_err(|_| loss("invalid file mode"))?;
                let size = number(header, "size")?;
                sink = Some(
                    Sink::new(&local, Header { kind, mode, size }, overwrite)
                        .map_err(xfer_error)?,
                );
            } else if let Some(part) = message.get("chunk") {
                if trailer.is_some() {
                    return Err(loss("chunk after trailer"));
                }
                let target = sink.as_mut().ok_or_else(|| loss("chunk before header"))?;
                let data = STANDARD
                    .decode(string(part, "data")?)
                    .map_err(|e| loss(e.to_string()))?;
                let raw_size = u32::try_from(number(part, "rawSize")?)
                    .map_err(|_| loss("invalid chunk size"))?;
                let compressed = part
                    .get("compressed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                target
                    .write(Chunk {
                        data,
                        compressed,
                        raw_size,
                    })
                    .map_err(xfer_error)?;
            } else if let Some(part) = message.get("trailer") {
                if sink.is_none() || trailer.is_some() {
                    return Err(loss("unexpected file trailer"));
                }
                trailer = Some(Trailer {
                    size: number(part, "size")?,
                    sha256: string(part, "sha256")?.into(),
                    skipped: u32::try_from(number(part, "skipped")?)
                        .map_err(|_| loss("invalid skipped count"))?,
                });
            } else {
                return Err(loss("unknown file stream part"));
            }
            Ok(())
        },
    )?;
    let trailer = trailer.ok_or_else(|| loss("file stream ended without trailer"))?;
    let sink = sink.ok_or_else(|| loss("file stream ended without header"))?;
    sink.commit(trailer.clone()).map_err(xfer_error)?;
    Ok(TransferResult {
        path: local,
        size: trailer.size,
        sha256: trailer.sha256,
        skipped: trailer.skipped,
    })
}

fn random_id() -> Result<String> {
    let mut bytes = [0_u8; 12];
    getrandom::getrandom(&mut bytes).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    let mut id = String::from("xfer-");
    for byte in bytes {
        id.push_str(&format!("{byte:02x}"));
    }
    Ok(id)
}
fn kind_json(kind: Kind) -> &'static str {
    match kind {
        Kind::File => "FILE_KIND_FILE",
        Kind::Directory => "FILE_KIND_DIRECTORY",
    }
}
fn parse_kind(kind: &str) -> Result<Kind> {
    match kind {
        "FILE_KIND_FILE" => Ok(Kind::File),
        "FILE_KIND_DIRECTORY" => Ok(Kind::Directory),
        _ => Err(loss("invalid file kind")),
    }
}
fn chunk_json(chunk: Chunk) -> Value {
    json!({"data": STANDARD.encode(chunk.data), "compressed": chunk.compressed, "rawSize": chunk.raw_size})
}
fn trailer_json(trailer: &Trailer) -> Value {
    json!({"size": trailer.size.to_string(), "sha256": trailer.sha256, "skipped": trailer.skipped})
}
fn verify_received(response: &Value, expected: u64, done: bool) -> Result<()> {
    if number(response, "received")? != expected
        || response
            .get("done")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            != done
    {
        return Err(loss(
            "node reported an unexpected transfer offset or completion",
        ));
    }
    Ok(())
}
fn number(value: &Value, key: &str) -> Result<u64> {
    match value.get(key) {
        None => Ok(0), // protobuf JSON omits scalar fields with default values
        Some(value) => value
            .as_u64()
            .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| loss(format!("invalid {key}"))),
    }
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| loss(format!("missing or invalid {key}")))
}
fn loss(message: impl Into<String>) -> RpcError {
    RpcError::new(Code::DataLoss, message)
}
fn xfer_error(error: protectorate_xfer::Error) -> RpcError {
    RpcError::new(Code::DataLoss, error.to_string())
}
