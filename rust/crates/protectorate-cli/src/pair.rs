//! `devices add`: add a device and print its token once, with `--url` also
//! the pairing links and a terminal QR of one of them.
use crate::{message_value, value_line, Client};
use protectorate_pairing::{check_base_url, new_pair_links, write_qr};
use protectorate_wire::{v1, Code, Result, RpcError};
use serde_json::json;

pub(crate) fn add(
    client: &Client,
    name: &str,
    url: Option<&str>,
    app_qr: bool,
    json: bool,
) -> Result<String> {
    let invalid = |error: protectorate_pairing::Error| RpcError::invalid(error.to_string());
    if let Some(url) = url {
        check_base_url(url).map_err(invalid)?;
    }
    let added = client.unary(
        v1::device_service::ADD_DEVICE,
        &v1::AddDeviceRequest { name: name.into() },
    )?;
    let device = added.device.clone().unwrap_or_default();
    let links = url
        .map(|url| new_pair_links(url, &added.token))
        .transpose()
        .map_err(invalid)?;
    if json {
        let mut out = json!({
            "device": message_value(&device)?,
            "token": added.token,
        });
        if let Some(links) = &links {
            out["web"] = json!(links.web);
            out["app"] = json!(links.app);
        }
        return value_line(&out);
    }
    let mut output = String::new();
    if let Some(links) = &links {
        let mut bytes = Vec::new();
        write_qr(&mut bytes, if app_qr { &links.app } else { &links.web })
            .map_err(RpcError::internal)?;
        output = String::from_utf8(bytes).map_err(|error| {
            RpcError::new(Code::Internal, format!("encode pairing QR: {error}"))
        })?;
        output.push('\n');
    }
    output.push_str(&format!(
        "added {} ({})\ntoken: {}\n",
        device.name, device.id, added.token
    ));
    if let Some(links) = &links {
        output.push_str(&format!("web:   {}\napp:   {}\n", links.web, links.app));
    }
    output.push_str(
        "\nThe token is shown only once and works until the device is revoked; anyone holding it or a link is that device.\n",
    );
    Ok(output)
}
