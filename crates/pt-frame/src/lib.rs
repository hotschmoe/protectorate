//! The hub-node link: the frame codec (`codec`, pure), blocking frame I/O
//! (`wire`) and the RPC messages (`msg`).

#![forbid(unsafe_code)]

pub mod codec;
pub mod msg;
pub mod wire;

pub use codec::{Decoder, Frame, FrameError, Kind, MAX_FRAME, MAX_PAYLOAD, decode, encode};
