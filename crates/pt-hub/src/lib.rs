//! The hub runtime behind `protectorate serve`.
//!
//! ```text
//! listeners (pt-http, a thread per connection) --commands--> core thread --intents--> executors
//!        ^ replies after fsync, reads from the view   |  apply, journal, publish view  | one per sleeve
//!        +---------------- bus (events, Wait) <--------+  <------ reports, observations -+  + pumps
//! ```

#![forbid(unsafe_code)]

pub fn journal_event_tag() -> u8 {
    pt_core::codec::MAX_EVENT_TAG
}

mod api;
mod approvals;
mod brief;
mod bus;
mod core;
mod devices;
mod doctor;
mod exec;
mod fleet;
mod git;
mod hardware;
mod harness_update;
mod history;
mod home;
mod http;
mod hub;
mod jobs;
mod log;
mod logins;
mod quota;
mod rule;
mod select;
mod serve;
mod stream;
mod toolchain;
mod transcript;
mod update;
mod wire;

pub use fleet::{FleetFlags, Net};
pub use http::{Web, WebFile};
pub use log::set_level as set_log_level;
pub use logins::login as harness_login;
pub use pt_fleet::Embedded;
pub use serve::{Exit, ServeOptions, probation_prestart, retry_probation_restore, serve};
pub use update::RESTART_CODE;

/// `$PROTECTORATE_HOME`, else `~/.local/share/protectorate`.
pub fn default_home() -> std::path::PathBuf {
    if let Some(h) = std::env::var_os("PROTECTORATE_HOME") {
        return h.into();
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".local/share/protectorate")
}
