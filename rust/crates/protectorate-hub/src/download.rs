//! Public downloads of locally staged, checksummed Rust release components.
use crate::Hub;
use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use protectorate_release::{self as release, RUST_BUNDLE};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, sync::Arc};

// A published Rust component can be large. Bound both the read and the number
// of concurrent in-memory responses until the installer can use disk streaming.
static READ_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

fn component_name(file: &str) -> Option<(&str, bool)> {
    if file == "SHA256SUMS" {
        return Some((file, false));
    }
    let (name, checksum) = file
        .strip_suffix(".sha256")
        .map_or((file, false), |name| (name, true));
    for component in RUST_BUNDLE {
        for arch in ["amd64", "arm64"] {
            if release::rust_bundle_name(component, "linux", arch)
                .ok()
                .as_deref()
                == Some(name)
            {
                return Some((name, checksum));
            }
        }
    }
    for component in release::RUST_WINDOWS {
        for arch in ["amd64", "arm64"] {
            if release::rust_bundle_name(component, "windows", arch)
                .ok()
                .as_deref()
                == Some(name)
            {
                return Some((name, checksum));
            }
        }
    }
    if release::rust_binary_name("android", "arm64")
        .ok()
        .as_deref()
        == Some(name)
    {
        return Some((name, checksum));
    }
    None
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "release asset is not a bounded regular file",
        ));
    }
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "release asset grew beyond download limit",
        ));
    }
    Ok(bytes)
}

fn verified_asset(hub: &Hub, name: &str, checksum: bool) -> Result<(Vec<u8>, bool), StatusCode> {
    let source = hub.releases.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let version = source.current().version;
    if !release::valid_version(&version) {
        return Err(StatusCode::NOT_FOUND);
    }
    let dir = source.release_dir(&version);
    let metadata = std::fs::symlink_metadata(&dir).map_err(|_| StatusCode::NOT_FOUND)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let sums = read_bounded(&dir.join("SHA256SUMS"), release::MAX_SUMS)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // The Windows installer pins the manifest's hash and checks each asset
    // against it, so the manifest itself is served as is.
    if name == "SHA256SUMS" {
        return Ok((sums, true));
    }
    let bytes =
        read_bounded(&dir.join(name), release::MAX_ASSET).map_err(|_| StatusCode::NOT_FOUND)?;
    release::verify_checksum(name, &bytes, &sums).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if checksum {
        let digest = Sha256::digest(&bytes);
        Ok((format!("{digest:x}  {name}\n").into_bytes(), true))
    } else {
        Ok((bytes, false))
    }
}

pub(super) async fn serve(hub: Arc<Hub>, file: String) -> Response {
    let Some((name, checksum)) = component_name(&file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let name = name.to_owned();
    let Ok(_permit) = READ_SLOTS.acquire().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let result = tokio::task::spawn_blocking(move || verified_asset(&hub, &name, checksum)).await;
    let (bytes, is_checksum) = match result {
        Ok(Ok(asset)) => asset,
        Ok(Err(status)) => return status.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(if is_checksum {
            "text/plain; charset=utf-8"
        } else {
            "application/octet-stream"
        }),
    );
    if !is_checksum {
        let content_disposition = format!("attachment; filename=\"{file}\"");
        if let Ok(value) = HeaderValue::from_str(&content_disposition) {
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, value);
        }
    }
    response
}
