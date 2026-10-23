//! Public installers for the verified Rust node release.
use crate::{download, nodes, Hub};
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use protectorate_release::{self as release, MAX_SUMS};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::Arc};

fn request_base(hub: &Hub, headers: &HeaderMap) -> Result<String, StatusCode> {
    if let Some(public_url) = &hub.public_url {
        return Ok(public_url.clone());
    }
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let scheme = if headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
    {
        "https"
    } else {
        "http"
    };
    nodes::validate_install_base(&format!("{scheme}://{host}")).map_err(|_| StatusCode::BAD_REQUEST)
}

fn script_response(body: String) -> Response {
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(crate) fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn powershell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn release_manifest(hub: &Hub) -> Result<(String, String, String, String), StatusCode> {
    let source = hub
        .releases
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let repo = source.repo().to_owned();
    let current = source.current();
    let version = current.version;
    if !release::valid_version(&version) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let tag = if current.tag.is_empty() {
        version.clone()
    } else {
        current.tag
    };
    if tag != "dev" && !release::valid_version(&tag) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let dir = source.release_dir(&version);
    let metadata = std::fs::symlink_metadata(&dir).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let path = dir.join("SHA256SUMS");
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_SUMS {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(&path)
        .and_then(|file| file.take(MAX_SUMS + 1).read_to_end(&mut bytes))
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if bytes.len() as u64 > MAX_SUMS {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok((version, tag, repo, format!("{:x}", Sha256::digest(&bytes))))
}

pub(super) async fn linux(hub: Arc<Hub>, headers: HeaderMap) -> Response {
    let Some(admin) = hub.tailnet_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let base = match request_base(&hub, &headers) {
        Ok(base) => base,
        Err(status) => return status.into_response(),
    };
    let (_, _, _, manifest_sha) = match release_manifest(&hub) {
        Ok(manifest) => manifest,
        Err(status) => return status.into_response(),
    };
    let control = shell_word(admin.control_url());
    let base = shell_word(&base);
    script_response(format!(
        r#"#!/bin/sh
# Rust node installer supplied by this hub. The two assets are verified against
# the active release before the hub serves them and checked again on the node.
set -eu
hub={base}
control={control}
manifest={manifest_sha}
name=${{PROTECTORATE_NODE_NAME:-}}
print=0
ca_cert=${{PROTECTORATE_CA_CERT:-}}
plain=0
dry=0
no_start=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --name) [ "$#" -ge 2 ] || {{ echo '--name requires a value' >&2; exit 2; }}; name=$2; shift 2 ;;
        --name=*) name=${{1#--name=}}; shift ;;
        --ca-cert) [ "$#" -ge 2 ] || {{ echo '--ca-cert requires a file' >&2; exit 2; }}; ca_cert=$2; shift 2 ;;
        --allow-plain-jobs) plain=1; shift ;;
        --dry-run) dry=1; shift ;;
        --no-start) no_start=1; shift ;;
        --print-target) print=1; shift ;;
        *) echo "unsupported installer option: $1" >&2; exit 2 ;;
    esac
done
case "$(uname -s):$(uname -m)" in
    Linux:x86_64|Linux:amd64) arch=amd64 ;;
    Linux:aarch64|Linux:arm64) arch=arm64 ;;
    *) echo 'unsupported Linux architecture' >&2; exit 2 ;;
esac
file=protectorate-rs-linux-$arch
if [ "$print" -eq 1 ]; then printf '%s\n' "$file"; exit 0; fi
[ -n "${{PROTECTORATE_ENROLL_KEY:-}}" ] || {{ echo 'set PROTECTORATE_ENROLL_KEY from the node invite' >&2; exit 2; }}
command -v curl >/dev/null || {{ echo 'curl is required' >&2; exit 2; }}
command -v sha256sum >/dev/null || {{ echo 'sha256sum is required' >&2; exit 2; }}
cache=${{XDG_CACHE_HOME:-$HOME/.cache}}/protectorate-rust
mkdir -p "$cache"
server=protectorate-rs-linux-$arch
helper=protectorate-node-sandbox-helper-linux-$arch
server_sum=$(curl -fsS --max-redirs 0 "$hub/dl/$server.sha256")
helper_sum=$(curl -fsS --max-redirs 0 "$hub/dl/$helper.sha256")
stage=
for candidate in "$cache"/node-install."$manifest"."$arch".*; do
    [ -d "$candidate" ] || continue
    if [ -x "$candidate/protectorate-rs" ] && [ -f "$candidate/protectorate-node-sandbox-helper" ] &&
        [ "$candidate/$server" -ef "$candidate/protectorate-rs" ] &&
        [ "$candidate/$helper" -ef "$candidate/protectorate-node-sandbox-helper" ] &&
        printf '%s\n%s\n' "$server_sum" "$helper_sum" | (cd "$candidate" && sha256sum --check --status); then
        stage=$candidate
        break
    fi
done
if [ -z "$stage" ]; then
stage=$(mktemp -d "$cache/node-install.$manifest.$arch.XXXXXXXX")
for component in protectorate-rs protectorate-node-sandbox-helper; do
    asset=$component-linux-$arch
    case "$component" in
        protectorate-rs) checksum=$server_sum ;;
        protectorate-node-sandbox-helper) checksum=$helper_sum ;;
    esac
    curl -fsS --max-redirs 0 "$hub/dl/$asset" -o "$stage/$asset"
    printf '%s\n' "$checksum" > "$stage/$asset.sha256"
    (cd "$stage" && sha256sum --check --status "$asset.sha256") || {{ echo "checksum mismatch: $asset" >&2; exit 1; }}
    chmod 755 "$stage/$asset"
    ln "$stage/$asset" "$stage/$component"
done
fi
set -- "$stage/protectorate-rs" node install --control "$control"
if [ -n "$name" ]; then set -- "$@" --name "$name"; fi
if [ -n "$ca_cert" ]; then set -- "$@" --ca-cert "$ca_cert"; fi
if [ "$plain" -eq 1 ]; then set -- "$@" --allow-plain-jobs; fi
if [ "$dry" -eq 1 ]; then set -- "$@" --dry-run; fi
if [ "$no_start" -eq 1 ]; then set -- "$@" --no-start; fi
exec "$@"
"#
    ))
}

pub(super) async fn termux(hub: Arc<Hub>, headers: HeaderMap) -> Response {
    let Some(admin) = hub.tailnet_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let base = match request_base(&hub, &headers) {
        Ok(base) => base,
        Err(status) => return status.into_response(),
    };
    if release_manifest(&hub).is_err() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let asset = "protectorate-rs-android-arm64";
    if download::serve(Arc::clone(&hub), asset.to_owned())
        .await
        .status()
        != StatusCode::OK
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let control = shell_word(admin.control_url());
    let base = shell_word(&base);
    script_response(format!(
        r#"#!/bin/sh
# Rust standalone Termux node installer supplied by this hub.
set -eu
hub={base}
control={control}
name=${{PROTECTORATE_NODE_NAME:-}}
ca_cert=${{PROTECTORATE_CA_CERT:-}}
print=0
plain=0
dry=0
no_start=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --name) [ "$#" -ge 2 ] || {{ echo '--name requires a value' >&2; exit 2; }}; name=$2; shift 2 ;;
        --name=*) name=${{1#--name=}}; shift ;;
        --ca-cert) [ "$#" -ge 2 ] || {{ echo '--ca-cert requires a file' >&2; exit 2; }}; ca_cert=$2; shift 2 ;;
        --allow-plain-jobs) plain=1; shift ;;
        --dry-run) dry=1; shift ;;
        --no-start) no_start=1; shift ;;
        --print-target) print=1; shift ;;
        *) echo "unsupported installer option: $1" >&2; exit 2 ;;
    esac
done
asset=protectorate-rs-android-arm64
if [ "$print" -eq 1 ]; then printf '%s\n' "$asset"; exit 0; fi
case "${{PREFIX:-}}" in
    */com.termux/files/usr) ;;
    *) echo 'run this installer inside Termux' >&2; exit 2 ;;
esac
case "$(uname -m)" in
    aarch64|arm64) ;;
    *) echo 'the Rust Termux node requires Android arm64' >&2; exit 2 ;;
esac
[ -n "${{PROTECTORATE_ENROLL_KEY:-}}" ] || {{ echo 'set PROTECTORATE_ENROLL_KEY from the node invite' >&2; exit 2; }}
command -v curl >/dev/null || {{ echo 'curl is required' >&2; exit 2; }}
command -v sha256sum >/dev/null || {{ echo 'sha256sum is required' >&2; exit 2; }}
cache=${{XDG_CACHE_HOME:-$HOME/.cache}}/protectorate-rust
mkdir -p "$cache"
stage=$(mktemp -d "$cache/node-install.XXXXXXXX")
trap 'rm -f "$stage/$asset" "$stage/$asset.sha256"; rmdir "$stage"' 0
trap 'exit 1' 1 2 3 15
curl -fsS --max-redirs 0 "$hub/dl/$asset" -o "$stage/$asset"
curl -fsS --max-redirs 0 "$hub/dl/$asset.sha256" -o "$stage/$asset.sha256"
(cd "$stage" && sha256sum --check --status "$asset.sha256") || {{ echo "checksum mismatch: $asset" >&2; exit 1; }}
chmod 755 "$stage/$asset"
set -- "$stage/$asset" node install --control "$control"
if [ -n "$name" ]; then set -- "$@" --name "$name"; fi
if [ -n "$ca_cert" ]; then set -- "$@" --ca-cert "$ca_cert"; fi
if [ "$plain" -eq 1 ]; then set -- "$@" --allow-plain-jobs; fi
if [ "$dry" -eq 1 ]; then set -- "$@" --dry-run; fi
if [ "$no_start" -eq 1 ]; then set -- "$@" --no-start; fi
"$@"
"#
    ))
}

pub(super) async fn windows(hub: Arc<Hub>, headers: HeaderMap) -> Response {
    let Some(admin) = hub.tailnet_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if let Err(status) = request_base(&hub, &headers) {
        return status.into_response();
    }
    let (version, tag, repo, manifest_sha) = match release_manifest(&hub) {
        Ok(pair) => pair,
        Err(status) => return status.into_response(),
    };
    let windows_version = |value: &str| {
        value.starts_with('v')
            && value.len() <= 81
            && value
                .as_bytes()
                .get(1)
                .is_some_and(u8::is_ascii_alphanumeric)
    };
    if !windows_version(&version) || (tag != "dev" && !windows_version(&tag)) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let template = include_str!("../../../../install-node-windows.ps1");
    let script = template
        .replacen(
            "[Parameter(Mandatory = $true)][string]$Version,",
            &format!("[string]$Version = {},", powershell_word(&version)),
            1,
        )
        .replacen(
            "[Parameter(Mandatory = $true)][string]$ManifestSha256,",
            &format!(
                "[string]$ManifestSha256 = {},",
                powershell_word(&manifest_sha)
            ),
            1,
        )
        .replacen(
            "[string]$ReleaseTag = \"\",",
            &format!("[string]$ReleaseTag = {},", powershell_word(&tag)),
            1,
        )
        .replacen(
            "[string]$ReleaseRepo = \"hotschmoe/protectorate\",",
            &format!("[string]$ReleaseRepo = {},", powershell_word(&repo)),
            1,
        )
        .replacen(
            "$ReleaseRepo -ne \"hotschmoe/protectorate\"",
            &format!("$ReleaseRepo -ne {}", powershell_word(&repo)),
            1,
        )
        .replacen(
            "Only the official release repo is allowed for network downloads; use BundleDir for a mirror",
            "This installer is pinned to the hub's release repository; use BundleDir for a mirror",
            1,
        )
        .replacen(
            "[string]$Control = \"\",",
            &format!(
                "[string]$Control = {},",
                powershell_word(admin.control_url())
            ),
            1,
        )
        .replacen(
            "[string]$Name = \"\",",
            "[string]$Name = $env:PROTECTORATE_NODE_NAME,",
            1,
        )
        .replacen(
            "[string]$KeyFile = \"\",",
            "[string]$KeyFile = $env:PROTECTORATE_ENROLL_KEY_FILE,",
            1,
        )
        .replacen(
            "[string]$CaCert = \"\",",
            "[string]$CaCert = $env:PROTECTORATE_CA_CERT,",
            1,
        );
    script_response(script)
}
