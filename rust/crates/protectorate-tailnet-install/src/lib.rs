//! Install the pinned Linux Headscale binary from a verified release asset.

use protectorate_release::{Budget, Transport};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const VERSION: &str = "0.29.5";
const RELEASE_BASE: &str = "https://github.com/juanfont/headscale/releases/download";
const MAX_BINARY_BYTES: u64 = 128 * 1024 * 1024;
const AMD64_SHA256: &str = "e979d27fe17d924d33bb6ab7504f6338d0635989a0cfdcfbc0ceb637dbcbe980";
const ARM64_SHA256: &str = "3afddd207b89ac22cfad4235cc163c6e538cc59ada139e7c0bf47640c5d9e945";

pub fn binary_path(home: &Path) -> PathBuf {
    home.join("bin").join(format!("headscale-{VERSION}"))
}

pub fn install(home: &Path) -> io::Result<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Headscale hub installation requires Linux",
        ));
    }
    let (platform, digest) = match std::env::consts::ARCH {
        "x86_64" => ("linux_amd64", AMD64_SHA256),
        "aarch64" => ("linux_arm64", ARM64_SHA256),
        arch => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("no pinned Headscale binary for {arch}"),
            ))
        }
    };
    let asset = format!("headscale_{VERSION}_{platform}");
    let url = format!("{RELEASE_BASE}/v{VERSION}/{asset}");
    install_verified(home, &url, digest)
}

fn install_verified(home: &Path, url: &str, digest: &str) -> io::Result<PathBuf> {
    if !home.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tailnet home must be absolute",
        ));
    }
    let dir = home.join("bin");
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tailnet bin directory must not be a symlink",
            ))
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(&dir)?,
        Err(error) => return Err(error),
    }
    let downloaded = Transport::new(url)
        .get(url, "", Budget::Fetch, MAX_BINARY_BYTES)
        .map_err(|error| io::Error::other(format!("download Headscale: {error}")))?
        .body;
    let got = Sha256::digest(&downloaded);
    let got = got
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if got != digest {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Headscale SHA-256 mismatch: got {got}, expected {digest}"),
        ));
    }
    let path = binary_path(home);
    protectorate_fsutil::write_file(&path, &downloaded, 0o755)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn verified_asset_installs_atomically_and_bad_digest_leaves_old_binary() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("state");
        fs::create_dir_all(&home).unwrap();
        let body = b"#!/bin/sh\nexit 0\n";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = socket.read(&mut request).unwrap();
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                socket.write_all(body).unwrap();
            }
        });
        let digest = Sha256::digest(body)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let installed = install_verified(&home, &url, &digest).unwrap();
        assert_eq!(fs::read(&installed).unwrap(), body);
        assert!(install_verified(&home, &url, &"0".repeat(64)).is_err());
        assert_eq!(fs::read(&installed).unwrap(), body);
        server.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_bin_directory_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("state");
        fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink(root.path(), home.join("bin")).unwrap();
        let error = install_verified(&home, "http://127.0.0.1/asset", &"0".repeat(64)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
