//! CLI side of the hub-relayed, chunked node transfer protocol.
use crate::Client;
use protectorate_wire::{
    v1::{self, get_file_response::Part, node_service},
    Code, Result, RpcError,
};
use protectorate_xfer::{Chunk, Header, Kind, Sink, Source, Trailer};
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
) -> Result<(v1::PutFileResponse, u64, u64, Duration)> {
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
    let base = v1::PutFileRequest {
        name: name.to_owned(),
        transfer_id,
        ..Default::default()
    };
    // Only the first request names the destination.
    let mut opening = Some(v1::PutFileRequest {
        path: remote.to_owned(),
        kind: kind(header.kind) as i32,
        mode: header.mode,
        overwrite: force,
        ..base.clone()
    });
    let mut offset = 0_u64;
    let mut wire = 0_u64;
    while let Some(chunk) = source.next_chunk().map_err(xfer_error)? {
        wire += chunk.data.len() as u64;
        let raw_size = chunk.raw_size;
        let request = v1::PutFileRequest {
            offset,
            chunk: Some(v1::FileChunk {
                data: chunk.data,
                compressed: chunk.compressed,
                raw_size,
            }),
            ..opening.take().unwrap_or_else(|| base.clone())
        };
        let response = client.unary(node_service::PUT_FILE, &request)?;
        offset += u64::from(raw_size);
        if response.received != offset {
            return Err(RpcError::new(
                Code::DataLoss,
                "node acknowledged the wrong transfer offset",
            ));
        }
    }
    let trailer = source.trailer().map_err(xfer_error)?;
    let request = v1::PutFileRequest {
        offset,
        last: true,
        trailer: Some(v1::FileTrailer {
            size: trailer.size,
            sha256: trailer.sha256,
            skipped: trailer.skipped,
        }),
        ..opening.unwrap_or(base)
    };
    let response = client.unary(node_service::PUT_FILE, &request)?;
    if !response.done || response.received != trailer.size {
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
    let request = v1::GetFileRequest {
        name: name.to_owned(),
        path: remote.to_owned(),
        no_compress,
    };
    client.server_stream(node_service::GET_FILE, &request, |response| {
        match response.part {
            Some(Part::Header(value)) => {
                if header.is_some() || sink.is_some() || trailer.is_some() {
                    return Err(RpcError::new(Code::DataLoss, "duplicate transfer header"));
                }
                let kind = match value.kind() {
                    v1::FileKind::File => Kind::File,
                    v1::FileKind::Directory => Kind::Directory,
                    v1::FileKind::Unspecified => {
                        return Err(RpcError::new(Code::DataLoss, "invalid transfer kind"))
                    }
                };
                let parsed = Header {
                    kind,
                    mode: value.mode,
                    size: value.size,
                };
                sink = Some(Sink::new(local, parsed, force).map_err(xfer_error)?);
                header = Some(parsed);
            }
            Some(Part::Chunk(value)) => {
                if trailer.is_some() {
                    return Err(RpcError::new(Code::DataLoss, "chunk after trailer"));
                }
                let sink = sink
                    .as_mut()
                    .ok_or_else(|| RpcError::new(Code::DataLoss, "data before transfer header"))?;
                wire += value.data.len() as u64;
                sink.write(Chunk {
                    data: value.data,
                    compressed: value.compressed,
                    raw_size: value.raw_size,
                })
                .map_err(xfer_error)?;
            }
            Some(Part::Trailer(value)) => {
                if trailer.is_some() {
                    return Err(RpcError::new(Code::DataLoss, "duplicate transfer trailer"));
                }
                let parsed = Trailer {
                    size: value.size,
                    sha256: value.sha256,
                    skipped: value.skipped,
                };
                let sink = sink.take().ok_or_else(|| {
                    RpcError::new(Code::DataLoss, "trailer before transfer header")
                })?;
                sink.commit(parsed.clone()).map_err(xfer_error)?;
                trailer = Some(parsed);
            }
            None => {
                return Err(RpcError::new(
                    Code::DataLoss,
                    "unknown transfer response part",
                ))
            }
        }
        Ok(())
    })?;
    trailer
        .map(|trailer| (trailer, wire, start.elapsed()))
        .ok_or_else(|| RpcError::new(Code::DataLoss, "transfer ended without a trailer"))
}

fn kind(kind: Kind) -> v1::FileKind {
    match kind {
        Kind::File => v1::FileKind::File,
        Kind::Directory => v1::FileKind::Directory,
    }
}

fn xfer_error(error: protectorate_xfer::Error) -> RpcError {
    RpcError::new(Code::FailedPrecondition, error.to_string())
}
