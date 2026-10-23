//! Same-origin Flutter assets served beside the Connect API.
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::{header, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use futures_util::stream;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tokio::io::AsyncReadExt;

const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct Web {
    root: PathBuf,
}

impl Web {
    /// An explicit build must exist at startup. The release `current` pointer
    /// is resolved on each request so a successful update changes the assets.
    pub(super) fn new(explicit: Option<&Path>, current: PathBuf) -> io::Result<Self> {
        let root = if let Some(path) = explicit {
            let root = fs::canonicalize(path)?;
            if !root.is_dir() || !root.join("index.html").is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--web requires a Flutter web build with index.html",
                ));
            }
            root
        } else {
            current
        };
        Ok(Self { root })
    }

    pub(super) fn attach(self, router: Router) -> Router {
        router.layer(middleware::from_fn(move |request: Request, next: Next| {
            let web = self.clone();
            async move {
                if (request.method() == Method::GET || request.method() == Method::HEAD)
                    && !request.uri().path().starts_with("/dl/")
                    && !request.uri().path().starts_with("/ws/")
                    && !request.uri().path().starts_with("/protectorate.v1.")
                {
                    let asset = web
                        .serve(request.method().clone(), request.uri().clone())
                        .await;
                    if asset.status() == StatusCode::OK {
                        return asset;
                    }
                }
                next.run(request).await
            }
        }))
    }

    async fn serve(&self, method: Method, uri: Uri) -> Response {
        if method != Method::GET && method != Method::HEAD {
            return StatusCode::NOT_FOUND.into_response();
        }
        let Some(relative) = safe_path(uri.path()) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let root = match fs::canonicalize(&self.root) {
            Ok(root) => root,
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        };
        let path = match fs::canonicalize(root.join(&relative)) {
            Ok(path) if path.starts_with(&root) => path,
            _ => return StatusCode::NOT_FOUND.into_response(),
        };
        let file = match tokio::fs::File::open(&path).await {
            Ok(file) => file,
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        };
        let metadata = match file.metadata().await {
            Ok(meta) if meta.is_file() && meta.len() <= MAX_ASSET_BYTES => meta,
            _ => return StatusCode::NOT_FOUND.into_response(),
        };
        let body = if method == Method::HEAD {
            Body::empty()
        } else {
            let chunks = stream::unfold(Some(file), |state| async move {
                let mut file = state?;
                let mut buffer = vec![0; 64 * 1024];
                match file.read(&mut buffer).await {
                    Ok(0) => None,
                    Ok(size) => {
                        buffer.truncate(size);
                        Some((Ok::<_, io::Error>(Bytes::from(buffer)), Some(file)))
                    }
                    Err(error) => Some((Err(error), None)),
                }
            });
            Body::from_stream(chunks)
        };
        let mut response = body.into_response();
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            mime(&path).parse().expect("static MIME"),
        );
        headers.insert(
            header::CACHE_CONTROL,
            "no-store".parse().expect("static cache control"),
        );
        headers.insert(
            header::CONTENT_LENGTH,
            metadata.len().to_string().parse().expect("file length"),
        );
        response
    }
}

fn safe_path(path: &str) -> Option<PathBuf> {
    if !path.starts_with('/') {
        return None;
    }
    if path == "/" {
        return Some(PathBuf::from("index.html"));
    }
    let mut out = PathBuf::new();
    for raw in path[1..].split('/') {
        if raw.is_empty() {
            return None;
        }
        let mut bytes = Vec::with_capacity(raw.len());
        let mut source = raw.as_bytes().iter().copied();
        while let Some(byte) = source.next() {
            if byte == b'%' {
                let high = (source.next()? as char).to_digit(16)?;
                let low = (source.next()? as char).to_digit(16)?;
                bytes.push(((high << 4) | low) as u8);
            } else {
                bytes.push(byte);
            }
        }
        let segment = String::from_utf8(bytes).ok()?;
        if segment == "." || segment == ".." || segment.contains(['/', '\\', '\0', ':']) {
            return None;
        }
        out.push(segment);
    }
    Some(out)
}

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_flutter_assets_and_rejects_missing_or_unsafe_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("web");
        fs::create_dir_all(root.join("canvaskit")).unwrap();
        fs::write(root.join("index.html"), b"<html>ok</html>").unwrap();
        fs::write(root.join("canvaskit/canvaskit.wasm"), b"wasm").unwrap();
        fs::write(dir.path().join("outside.txt"), b"secret").unwrap();
        let web = Web::new(Some(&root), PathBuf::new()).unwrap();
        let index = web.serve(Method::GET, "/".parse().unwrap()).await;
        assert_eq!(index.status(), StatusCode::OK);
        assert_eq!(
            index.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        let wasm = web
            .serve(Method::HEAD, "/canvaskit/canvaskit.wasm".parse().unwrap())
            .await;
        assert_eq!(wasm.status(), StatusCode::OK);
        assert_eq!(wasm.headers()[header::CONTENT_TYPE], "application/wasm");
        assert_eq!(wasm.headers()[header::CONTENT_LENGTH], "4");
        for path in [
            "/missing",
            "/%2e%2e/outside.txt",
            "/protectorate.v1.HubService/GetStatus",
        ] {
            assert_eq!(
                web.serve(Method::GET, path.parse().unwrap()).await.status(),
                StatusCode::NOT_FOUND
            );
        }
        assert_eq!(
            web.serve(Method::POST, "/".parse().unwrap()).await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_to_outside_asset_is_rejected() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("web");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("index.html"), b"ok").unwrap();
        fs::write(dir.path().join("secret"), b"secret").unwrap();
        symlink(dir.path().join("secret"), root.join("secret")).unwrap();
        let web = Web::new(Some(&root), PathBuf::new()).unwrap();
        assert_eq!(
            web.serve(Method::GET, "/secret".parse().unwrap())
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn default_web_follows_current_release_pointer() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first/web");
        let second = dir.path().join("second/web");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("index.html"), b"first").unwrap();
        fs::write(second.join("index.html"), b"second release").unwrap();
        let current = dir.path().join("current");
        symlink(dir.path().join("first"), &current).unwrap();
        let web = Web::new(None, current.join("web")).unwrap();
        let first_response = web.serve(Method::HEAD, "/".parse().unwrap()).await;
        assert_eq!(first_response.headers()[header::CONTENT_LENGTH], "5");
        let next = dir.path().join("next");
        symlink(dir.path().join("second"), &next).unwrap();
        fs::rename(next, &current).unwrap();
        let second_response = web.serve(Method::HEAD, "/".parse().unwrap()).await;
        assert_eq!(second_response.headers()[header::CONTENT_LENGTH], "14");
    }

    #[test]
    fn static_paths_are_confined() {
        assert_eq!(safe_path("/"), Some(PathBuf::from("index.html")));
        assert_eq!(
            safe_path("/assets/fonts/Roboto-Regular.ttf"),
            Some(PathBuf::from("assets/fonts/Roboto-Regular.ttf"))
        );
        for path in [
            "/../secret",
            "/%2e%2e/secret",
            "/%2fsecret",
            "/%5csecret",
            "/a//b",
            "/a/%00",
            "/C:/Windows",
        ] {
            assert!(safe_path(path).is_none(), "{path}");
        }
        assert_eq!(mime(Path::new("canvaskit.wasm")), "application/wasm");
        assert_eq!(
            mime(Path::new("main.dart.js")),
            "text/javascript; charset=utf-8"
        );
    }
}
