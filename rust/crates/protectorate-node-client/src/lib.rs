//! Blocking, verified file transfer over Connect JSON. Call from
//! `spawn_blocking` when using an async runtime.

use protectorate_connect_client::Client;
use protectorate_wire::{
    v1::{self, get_file_response::Part, node_agent_service},
    Code, Result, RpcError,
};
use protectorate_xfer::{Chunk, Header, Kind, Sink, Source, Trailer};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
    let first = v1::PutFileRequest {
        name: node.into(),
        transfer_id: random_id()?,
        path: remote.into(),
        kind: file_kind(header.kind) as i32,
        mode: header.mode,
        overwrite,
        ..Default::default()
    };
    let mut offset = 0_u64;
    while let Some(chunk) = source.next_chunk().map_err(xfer_error)? {
        let sent = chunk.raw_size as u64;
        let request = v1::PutFileRequest {
            offset,
            chunk: Some(v1::FileChunk {
                data: chunk.data,
                compressed: chunk.compressed,
                raw_size: chunk.raw_size,
            }),
            ..first.clone()
        };
        let response = put_chunk_with_retry(client, &request)?;
        offset = offset
            .checked_add(sent)
            .ok_or_else(|| loss("transfer offset overflow"))?;
        verify_received(&response, offset, false)?;
        progress(offset)?;
    }
    let trailer = source.trailer().map_err(xfer_error)?;
    let end = Some(v1::FileTrailer {
        size: trailer.size,
        sha256: trailer.sha256.clone(),
        skipped: trailer.skipped,
    });
    // Without any chunk the last call is also the first and carries its fields.
    let last = if offset == 0 {
        v1::PutFileRequest {
            last: true,
            trailer: end,
            ..first
        }
    } else {
        v1::PutFileRequest {
            name: first.name,
            transfer_id: first.transfer_id,
            offset,
            last: true,
            trailer: end,
            ..Default::default()
        }
    };
    let response = put_chunk_with_retry(client, &last)?;
    verify_received(&response, offset, true)?;
    Ok(TransferResult {
        path: PathBuf::from(response.path),
        size: trailer.size,
        sha256: trailer.sha256,
        skipped: trailer.skipped,
    })
}

fn put_chunk_with_retry(
    client: &Client,
    request: &v1::PutFileRequest,
) -> Result<v1::PutFileResponse> {
    let mut retry_until = None;
    let mut pause = Duration::from_millis(200);
    loop {
        match client.unary(node_agent_service::PUT_FILE, request) {
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
    let request = v1::GetFileRequest {
        name: node.into(),
        path: remote.into(),
        no_compress: !compress,
    };
    client.server_stream(node_agent_service::GET_FILE, &request, |message| {
        match message.part {
            Some(Part::Header(header)) => {
                if sink.is_some() || trailer.is_some() {
                    return Err(loss("duplicate file header"));
                }
                let kind = match header.kind() {
                    v1::FileKind::File => Kind::File,
                    v1::FileKind::Directory => Kind::Directory,
                    v1::FileKind::Unspecified => return Err(loss("invalid file kind")),
                };
                let header = Header {
                    kind,
                    mode: header.mode,
                    size: header.size,
                };
                sink = Some(Sink::new(&local, header, overwrite).map_err(xfer_error)?);
            }
            Some(Part::Chunk(part)) => {
                if trailer.is_some() {
                    return Err(loss("chunk after trailer"));
                }
                let target = sink.as_mut().ok_or_else(|| loss("chunk before header"))?;
                target
                    .write(Chunk {
                        data: part.data,
                        compressed: part.compressed,
                        raw_size: part.raw_size,
                    })
                    .map_err(xfer_error)?;
            }
            Some(Part::Trailer(part)) => {
                if sink.is_none() || trailer.is_some() {
                    return Err(loss("unexpected file trailer"));
                }
                trailer = Some(Trailer {
                    size: part.size,
                    sha256: part.sha256,
                    skipped: part.skipped,
                });
            }
            None => return Err(loss("unknown file stream part")),
        }
        Ok(())
    })?;
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
fn file_kind(kind: Kind) -> v1::FileKind {
    match kind {
        Kind::File => v1::FileKind::File,
        Kind::Directory => v1::FileKind::Directory,
    }
}
fn verify_received(response: &v1::PutFileResponse, expected: u64, done: bool) -> Result<()> {
    if response.received != expected || response.done != done {
        return Err(loss(
            "node reported an unexpected transfer offset or completion",
        ));
    }
    Ok(())
}
fn loss(message: impl Into<String>) -> RpcError {
    RpcError::new(Code::DataLoss, message)
}
fn xfer_error(error: protectorate_xfer::Error) -> RpcError {
    RpcError::new(Code::DataLoss, error.to_string())
}
