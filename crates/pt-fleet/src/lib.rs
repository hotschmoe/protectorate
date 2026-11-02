//! The hub's side of the fleet (SPEC 3.2, 3.5): headscale as a sandboxed, supervised child,
//! administered through its own CLI; the hub's `member` and its DIAL socket; enrollment; a client
//! for the node agent's RPCs; and a poller that turns them into `pt_api::Node` values. See
//! README.md for the API.

#![forbid(unsafe_code)]

mod child;
mod client;
mod config;
mod enroll;
mod error;
mod files;
mod fleet;
mod headscale;
mod member;
mod poller;
pub mod view;

pub use child::ChildState;
pub use client::NodeClient;
pub use config::{Config, Derp, NODE_PORT};
pub use enroll::{Enrolled, PENDING_TTL_MS, Pending, Registration, Removal, valid_name};
pub use error::{Error, Result};
pub use fleet::Fleet;
pub use headscale::{Admin, HsNode, TAG_HUB, TAG_NODE, USER};
pub use member::{Peer, Status};
pub use poller::{FleetEvent, NodeView, PollConfig, Poller};
pub use pt_embed::Embedded;
