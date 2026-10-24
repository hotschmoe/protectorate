//! Every protectorate-rs scenario in one test binary, run in parallel
//! threads.
#[cfg(unix)]
mod support;

mod auth;
mod backup_cli;
mod backup_state;
mod driver_run;
mod flutter_client;
mod hub_self_update;
mod job_restart;
mod mtls;
mod service_job_restart;
mod sleeve_guards;
