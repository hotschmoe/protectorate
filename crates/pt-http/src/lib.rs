//! HTTP/1.1 server and a small client on blocking std sockets, with RFC 6455 WebSockets.
//!
//! One thread per connection, no async runtime. The server never interprets proxy headers such as
//! X-Forwarded-For: it exposes the headers and the [`Peer`], and the caller decides trust.

#![forbid(unsafe_code)]

mod client;
mod fuzz;
mod handshake;
mod headers;
mod request;
mod response;
mod server;
mod unix;
mod wire;
mod ws;

pub use client::{Client, ClientResponse};
pub use headers::Headers;
pub use pt_sys::PeerCred;
pub use request::Request;
pub use response::{Response, content_type};
pub use server::{Config, Handler, Peer, Server, ShutdownHandle};
pub use ws::{Message, WebSocket, WsWriter};

/// Entry points for the cargo-fuzz targets in `fuzz/`; not a stable API.
#[doc(hidden)]
pub mod fuzzing {
    pub use crate::fuzz::fuzz_request as request;
    pub use crate::ws::fuzz_frames as websocket;
}
