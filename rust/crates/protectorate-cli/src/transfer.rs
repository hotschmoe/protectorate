//! CLI side of the hub-relayed, chunked node transfer protocol.
use crate::{Client, GET_FILE, PUT_FILE};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_wire::{Code, Result, RpcError};
use protectorate_xfer::{Chunk, Header, Kind, Sink, Source, Trailer};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn put(
    client: &Client,
    name: &str,
    local: &Path,
    remote: &str,
    force: bool,
    no_compress: bool,
) -> Result<(Value, u64, u64, Duration)> {
    let start = Instant::now();
    let mut source = Source::open(local, !no_compress).map_err(xfer_error)?;
    let header = source.header();
    let transfer_id = format!(
        "rust-{:x}-{:x}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut offset = 0_u64;
    let mut wire = 0_u64;
    let mut first = true;
    while let Some(chunk) = source.next_chunk().map_err(xfer_error)? {
        let data = STANDARD.encode(&chunk.data);
        wire += chunk.data.len() as u64;
        let mut request = json!({
            "name": name, "transferId": transfer_id, "offset": offset.to_string(),
            "chunk": {"data": data, "compressed": chunk.compressed, "rawSize": chunk.raw_size},
        });
        if first {
            request["path"] = json!(remote);
            request["kind"] = json!(kind_name(header.kind));
            request["mode"] = json!(header.mode);
            request["overwrite"] = json!(force);
            first = false;
        }
        let response = client.call(PUT_FILE, &request)?;
        offset += u64::from(chunk.raw_size);
        if number(&response, "received") != Some(offset) {
            return Err(RpcError::new(
                Code::DataLoss,
                "node acknowledged the wrong transfer offset",
            ));
        }
    }
    let trailer = source.trailer().map_err(xfer_error)?;
    let mut request = json!({
        "name": name, "transferId": transfer_id, "offset": offset.to_string(),
        "last": true,
        "trailer": {"size": trailer.size.to_string(), "sha256": trailer.sha256, "skipped": trailer.skipped},
    });
    if first {
        request["path"] = json!(remote);
        request["kind"] = json!(kind_name(header.kind));
        request["mode"] = json!(header.mode);
        request["overwrite"] = json!(force);
    }
    let response = client.call(PUT_FILE, &request)?;
    if response.get("done") != Some(&Value::Bool(true))
        || number(&response, "received") != Some(trailer.size)
    {
        return Err(RpcError::new(
            Code::DataLoss,
            "node did not confirm the completed transfer",
        ));
    }
    Ok((response, trailer.size, wire, start.elapsed()))
}

pub(super) fn get(
    client: &Client,
    name: &str,
    remote: &str,
    local: &Path,
    force: bool,
    no_compress: bool,
) -> Result<(Trailer, u64, Duration)> {
    let start = Instant::now();
    let mut sink: Option<Sink> = None;
    let mut header: Option<Header> = None;
    let mut trailer: Option<Trailer> = None;
    let mut wire = 0_u64;
    client.stream(
        GET_FILE,
        &json!({"name": name, "path": remote, "noCompress": no_compress}),
        |part| {
            if let Some(value) = part.get("header") {
                if header.is_some() || sink.is_some() || trailer.is_some() {
                    return Err(RpcError::new(Code::DataLoss, "duplicate transfer header"));
                }
                let kind = match value.get("kind").and_then(Value::as_str) {
                    Some("FILE_KIND_FILE") => Kind::File,
                    Some("FILE_KIND_DIRECTORY") => Kind::Directory,
                    _ => return Err(RpcError::new(Code::DataLoss, "invalid transfer kind")),
                };
                let parsed = Header {
                    kind,
                    mode: number(value, "mode").unwrap_or(0) as u32,
                    size: number(value, "size").unwrap_or(0),
                };
                sink = Some(Sink::new(local, parsed, force).map_err(xfer_error)?);
                header = Some(parsed);
            } else if let Some(value) = part.get("chunk") {
                if trailer.is_some() {
                    return Err(RpcError::new(Code::DataLoss, "chunk after trailer"));
                }
                let sink = sink
                    .as_mut()
                    .ok_or_else(|| RpcError::new(Code::DataLoss, "data before transfer header"))?;
                let data = value
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::new(Code::DataLoss, "chunk has no data"))?;
                let data = STANDARD
                    .decode(data)
                    .map_err(|error| RpcError::new(Code::DataLoss, error.to_string()))?;
                wire += data.len() as u64;
                let raw_size = number(value, "rawSize")
                    .ok_or_else(|| RpcError::new(Code::DataLoss, "chunk has no raw size"))?;
                let raw_size = u32::try_from(raw_size)
                    .map_err(|_| RpcError::new(Code::DataLoss, "chunk raw size overflows"))?;
                sink.write(Chunk {
                    data,
                    compressed: value
                        .get("compressed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    raw_size,
                })
                .map_err(xfer_error)?;
            } else if let Some(value) = part.get("trailer") {
                if trailer.is_some() {
                    return Err(RpcError::new(Code::DataLoss, "duplicate transfer trailer"));
                }
                let parsed = Trailer {
                    size: number(value, "size")
                        .ok_or_else(|| RpcError::new(Code::DataLoss, "trailer has no size"))?,
                    sha256: value
                        .get("sha256")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    skipped: number(value, "skipped").unwrap_or(0) as u32,
                };
                let sink = sink.take().ok_or_else(|| {
                    RpcError::new(Code::DataLoss, "trailer before transfer header")
                })?;
                sink.commit(parsed.clone()).map_err(xfer_error)?;
                trailer = Some(parsed);
            } else {
                return Err(RpcError::new(
                    Code::DataLoss,
                    "unknown transfer response part",
                ));
            }
            Ok(())
        },
    )?;
    trailer
        .map(|trailer| (trailer, wire, start.elapsed()))
        .ok_or_else(|| RpcError::new(Code::DataLoss, "transfer ended without a trailer"))
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::File => "FILE_KIND_FILE",
        Kind::Directory => "FILE_KIND_DIRECTORY",
    }
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    })
}

fn xfer_error(error: protectorate_xfer::Error) -> RpcError {
    RpcError::new(Code::FailedPrecondition, error.to_string())
}
