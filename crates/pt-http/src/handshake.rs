use crate::request::Request;
use crate::response::Response;
use crate::ws::accept_key;

/// Validates a server-side handshake and returns the Sec-WebSocket-Accept value.
pub(crate) fn check_handshake(req: &Request) -> Result<String, Response> {
    let h = req.headers();
    if req.method() != "GET"
        || req.http10
        || !h.has_token("Upgrade", "websocket")
        || !h.has_token("Connection", "upgrade")
    {
        return Err(Response::text(400, "not a websocket handshake\n"));
    }
    if h.get("Sec-WebSocket-Version") != Some("13") {
        return Err(Response::text(426, "websocket version 13 only\n")
            .header("Sec-WebSocket-Version", "13"));
    }
    let key = h.get("Sec-WebSocket-Key").unwrap_or("");
    let b64 = |c: u8| c.is_ascii_alphanumeric() || c == b'+' || c == b'/';
    // Base64 of 16 bytes: the 22nd character holds their last 2 bits and 4 zero padding bits.
    if key.len() != 24
        || !key.ends_with("==")
        || !key.bytes().take(22).all(b64)
        || !matches!(key.as_bytes()[21], b'A' | b'Q' | b'g' | b'w')
    {
        return Err(Response::text(400, "bad Sec-WebSocket-Key\n"));
    }
    Ok(accept_key(key))
}
