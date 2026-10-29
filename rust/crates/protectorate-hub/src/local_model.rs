use std::net::IpAddr;

/// A model is local when its base URL names a `--local-model-host`,
/// `localhost` or a private address. Any other name is not:
/// network location says nothing about who runs a DNS name.
pub(super) fn served_locally(base_url: &str, hosts: &[String]) -> bool {
    let Ok(uri) = base_url.parse::<axum::http::Uri>() else {
        return false;
    };
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    let host = canonical_host(host);
    if host.is_empty() {
        return false;
    }
    if hosts
        .iter()
        .any(|candidate| canonical_host(candidate) == host)
    {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) => protectorate_wire::private_ip(ip),
        Err(_) => host == "localhost" || host.ends_with(".localhost"),
    }
}

fn canonical_host(value: &str) -> String {
    let value = value.trim();
    let host = if let Some(stripped) = value.strip_prefix('[') {
        stripped.split_once(']').map_or(value, |(host, _)| host)
    } else if let Some((host, port)) = value.rsplit_once(':') {
        if !host.contains(':') && port.parse::<u16>().is_ok() {
            host
        } else {
            value
        }
    } else {
        value
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}
