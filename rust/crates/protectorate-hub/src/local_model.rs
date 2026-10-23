use protectorate_inventory::Inventory;
use std::net::{IpAddr, Ipv4Addr};

pub(super) fn served_locally(base_url: &str, hosts: &[String], inventory: &Inventory) -> bool {
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
    if inventory
        .list()
        .iter()
        .any(|node| canonical_host(&node.name) == host || canonical_host(&node.address) == host)
    {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(ip) => local_ipv4(ip),
            IpAddr::V6(ip) => {
                ip.to_ipv4_mapped().is_some_and(local_ipv4)
                    || ip.is_loopback()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.is_unspecified()
            }
        };
    }
    !host.contains('.')
        || host.ends_with(".local")
        || host.ends_with(".localhost")
        || host.ends_with(".ts.net")
}

fn local_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
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

#[cfg(test)]
mod tests {
    use super::*;
    use protectorate_inventory::Node;

    #[test]
    fn classifies_endpoint_and_inventory_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let inventory = Inventory::open(dir.path().join("nodes.json"), "hub").unwrap();
        inventory
            .add(
                Node {
                    name: "gpu-node".into(),
                    address: "llm.example.com:443".into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        let hosts = vec!["my.model.example".into()];
        for url in [
            "http://127.0.0.1:1234/v1",
            "http://10.1.2.3/v1",
            "http://169.254.4.5/v1",
            "http://100.64.1.2/v1",
            "http://[::1]:1234/v1",
            "http://[fd12::1]/v1",
            "http://gpu-node/v1",
            "https://llm.example.com/v1",
            "https://box.local/v1",
            "https://box.ts.net/v1",
            "https://my.model.example/v1",
        ] {
            assert!(served_locally(url, &hosts, &inventory), "{url}");
        }
        for url in [
            "",
            "not a URL",
            "https://api.deepseek.com/v1",
            "https://100.128.0.1/v1",
            "https://[2001:4860::1]/v1",
        ] {
            assert!(!served_locally(url, &hosts, &inventory), "{url}");
        }
    }
}
