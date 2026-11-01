//! The hub runtime behind `protectorate serve`.
//!
//! ```text
//! listeners (pt-http, a thread per connection) --commands--> core thread --intents--> executors
//!        ^ replies after fsync, reads from the view   |  apply, journal, publish view  | one per sleeve
//!        +---------------- bus (events, Wait) <--------+  <------ reports, observations -+  + pumps
//! ```

#![forbid(unsafe_code)]

mod api;
mod approvals;
mod brief;
mod bus;
mod core;
mod devices;
mod exec;
mod fleet;
mod git;
mod history;
mod home;
mod http;
mod hub;
mod jobs;
mod log;
mod quota;
mod rule;
mod select;
mod serve;
mod transcript;
mod wire;

pub use fleet::{FleetFlags, Net};
pub use http::{Web, WebFile};
pub use log::set_level as set_log_level;
pub use pt_fleet::Embedded;
pub use serve::{ServeOptions, serve};

/// `$PROTECTORATE_HOME`, else `~/.local/share/protectorate`.
pub fn default_home() -> std::path::PathBuf {
    if let Some(h) = std::env::var_os("PROTECTORATE_HOME") {
        return h.into();
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".local/share/protectorate")
}
