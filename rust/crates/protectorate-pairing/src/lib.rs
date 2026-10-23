//! Public pairing links and terminal QR rendering for enrolling another device.

use http::Uri;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::io::{self, Write};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairLinks {
    pub web: String,
    pub app: String,
}

#[derive(Debug)]
pub enum Error {
    InvalidUrl,
    InvalidCa,
    QrTooLong,
    Io(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str(
                "pair URL must be http:// or https:// without credentials, query or fragment",
            ),
            Self::InvalidCa => f.write_str("pair CA must be one PEM certificate"),
            Self::QrTooLong => f.write_str("pair link is too long for a QR code"),
            Self::Io(error) => write!(f, "write pairing QR: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Construct browser and native-app links from a public hub URL.
///
/// The URL may include a path prefix. An empty query or fragment is rejected as
/// well, since it would be ambiguous when adding the pairing route. `ca_pem`
/// is the hub CA the native app must trust; it travels as base64url DER in
/// `ca`, so the link itself is the trust anchor.
pub fn new_pair_links(base_url: &str, ca_pem: Option<&str>) -> Result<PairLinks, Error> {
    if base_url.is_empty()
        || base_url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        || !(base_url.starts_with("http://") || base_url.starts_with("https://"))
    {
        return Err(Error::InvalidUrl);
    }
    if base_url.contains(['?', '#']) || authority_has_userinfo(base_url) {
        return Err(Error::InvalidUrl);
    }
    let uri: Uri = base_url.parse().map_err(|_| Error::InvalidUrl)?;
    let authority = uri.authority().ok_or(Error::InvalidUrl)?;
    if uri.scheme_str() != Some("http") && uri.scheme_str() != Some("https") {
        return Err(Error::InvalidUrl);
    }
    if !valid_authority(authority.as_str()) {
        return Err(Error::InvalidUrl);
    }
    let base = base_url.trim_end_matches('/');
    let mut query = format!("url={}", utf8_percent_encode(base, NON_ALPHANUMERIC));
    if let Some(pem) = ca_pem {
        let pem = pem.trim();
        let body = pem
            .strip_prefix("-----BEGIN CERTIFICATE-----")
            .and_then(|rest| rest.strip_suffix("-----END CERTIFICATE-----"))
            .ok_or(Error::InvalidCa)?
            .split_whitespace()
            .collect::<String>();
        if body.is_empty()
            || !body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        {
            return Err(Error::InvalidCa);
        }
        let der = body
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_");
        query.push_str(&format!("&ca={der}"));
    }
    Ok(PairLinks {
        web: format!("{base}/#/pair?{query}"),
        app: format!("protectorate://pair?{query}"),
    })
}

fn authority_has_userinfo(raw: &str) -> bool {
    let Some((_, after_scheme)) = raw.split_once("://") else {
        return false;
    };
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    authority.contains('@')
}

fn valid_authority(authority: &str) -> bool {
    let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
        let Some((address, rest)) = ipv6.split_once(']') else {
            return false;
        };
        if address.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        if rest.is_empty() {
            return true;
        }
        let Some(port) = rest.strip_prefix(':') else {
            return false;
        };
        (address, Some(port))
    } else if let Some((host, port)) = authority.split_once(':') {
        (host, Some(port))
    } else {
        (authority, None)
    };
    if host.is_empty()
        || (!authority.starts_with('[')
            && !host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
    {
        return false;
    }
    port.is_none_or(|port| {
        !port.is_empty()
            && port.bytes().all(|byte| byte.is_ascii_digit())
            && port.parse::<u16>().is_ok()
    })
}

/// Render `text` as a medium-correction QR code with a two-module quiet zone.
///
/// Two modules occupy each terminal row. ANSI foreground/background colors make
/// both dark and light modules visible on terminals with either color theme.
pub fn write_qr(mut writer: impl Write, text: &str) -> Result<(), Error> {
    use qrcodegen::{QrCode, QrCodeEcc};

    let code = QrCode::encode_text(text, QrCodeEcc::Medium).map_err(|_| Error::QrTooLong)?;
    let size = code.size();
    const QUIET: i32 = 2;
    for y in (-QUIET..size + QUIET).step_by(2) {
        for x in -QUIET..size + QUIET {
            let upper = in_bounds(&code, x, y);
            let lower = in_bounds(&code, x, y + 1);
            let foreground = if upper { 30 } else { 37 };
            let background = if lower { 40 } else { 47 };
            write!(writer, "\x1b[{foreground};{background}m▀")?;
        }
        writer.write_all(b"\x1b[0m\n")?;
    }
    Ok(())
}

fn in_bounds(code: &qrcodegen::QrCode, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < code.size() && y < code.size() && code.get_module(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_preserve_public_path_and_encode_url_once() {
        let links = new_pair_links("https://hub.example:8443/base/", None).unwrap();
        assert_eq!(
            links.web,
            "https://hub.example:8443/base/#/pair?url=https%3A%2F%2Fhub%2Eexample%3A8443%2Fbase"
        );
        assert_eq!(
            links.app,
            "protectorate://pair?url=https%3A%2F%2Fhub%2Eexample%3A8443%2Fbase"
        );
        assert_eq!(
            new_pair_links("http://[::1]:8080", None).unwrap().web,
            "http://[::1]:8080/#/pair?url=http%3A%2F%2F%5B%3A%3A1%5D%3A8080"
        );
    }

    #[test]
    fn reject_unusable_or_ambiguous_urls() {
        for raw in [
            "",
            "https://",
            "https://?x=1",
            "ftp://hub.example",
            "http:hub.example",
            "https://user@hub.example",
            "https://:pass@hub.example",
            "https://@hub.example",
            "https://hub.example?",
            "https://hub.example#",
            "https://hub.example?x=1",
            "https://hub.example/#frag",
            "https://hub.example/path with spaces",
            "https://hub.example\\evil.example",
            "https://hub.example:bad",
        ] {
            assert!(new_pair_links(raw, None).is_err(), "accepted {raw:?}");
        }
    }

    #[test]
    fn qr_renders_quiet_zone_and_modules() {
        let mut output = Vec::new();
        write_qr(
            &mut output,
            "protectorate://pair?url=https%3A%2F%2Fhub.example",
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        let code = qrcodegen::QrCode::encode_text(
            "protectorate://pair?url=https%3A%2F%2Fhub.example",
            qrcodegen::QrCodeEcc::Medium,
        )
        .unwrap();
        let rows: Vec<_> = output.lines().collect();
        assert_eq!(rows.len(), ((code.size() + 4) as usize).div_ceil(2));
        assert!(rows[0].starts_with("\x1b[37;47m▀\x1b[37;47m▀"));
        assert!(rows.iter().all(|row| row.ends_with("\x1b[0m")));
        // A finder pattern's top-left dark module follows the two-module border.
        assert!(rows[1].starts_with("\x1b[37;47m▀\x1b[37;47m▀\x1b[30;40m▀"));
        assert!(output.contains("\x1b[30;40m▀"));
    }

    #[test]
    fn qr_writer_errors_are_returned() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(matches!(write_qr(Broken, "hello"), Err(Error::Io(_))));
    }
}
