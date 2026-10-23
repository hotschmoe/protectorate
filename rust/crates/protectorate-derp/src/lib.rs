//! A private DERP relay and Tailscale-compatible STUN service for Protectorate.

mod cert;
mod protocol;
mod server;
mod stun;

pub use cert::Certificate;
pub use server::{Config, Service};
pub use stun::stun_response;

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}
