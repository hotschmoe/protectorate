//! Render browser and app enrollment links from a public hub URL.
use crate::{hub_url, Invocation};
use protectorate_pairing::{new_pair_links, write_qr};
use protectorate_wire::{Code, Result, RpcError};
use serde_json::json;

pub(crate) fn run(invocation: &Invocation, url: Option<&str>, app_qr: bool) -> Result<String> {
    let default_url;
    let url = match url {
        Some(url) => url,
        None => {
            default_url = hub_url(invocation);
            &default_url
        }
    };
    let ca = std::env::var_os("PROTECTORATE_TLS_CA_CERT")
        .map(|path| {
            std::fs::read_to_string(&path)
                .map_err(|error| RpcError::invalid(format!("PROTECTORATE_TLS_CA_CERT: {error}")))
        })
        .transpose()?;
    let links = new_pair_links(url, ca.as_deref())
        .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))?;
    if invocation.json {
        return serde_json::to_string(&json!({"web": links.web, "app": links.app}))
            .map(|line| format!("{line}\n"))
            .map_err(|error| {
                RpcError::new(Code::Internal, format!("encode pairing links: {error}"))
            });
    }
    let qr_link = if app_qr { &links.app } else { &links.web };
    let mut bytes = Vec::new();
    write_qr(&mut bytes, qr_link)
        .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
    let mut output = String::from_utf8(bytes)
        .map_err(|error| RpcError::new(Code::Internal, format!("encode pairing QR: {error}")))?;
    output.push_str(&format!(
        "\nweb:  {}\napp:  {}\n\nOpen the link, connect, then approve the matching code on a trusted device.\n",
        links.web, links.app
    ));
    Ok(output)
}
