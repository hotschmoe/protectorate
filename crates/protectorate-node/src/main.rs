mod cli;
mod flags;
mod join;
mod manifest;
mod member;
mod rpc;
mod serve;
mod service;
mod stdio;
mod supervise;
mod update;
#[cfg(windows)]
mod winservice;

fn main() {
    std::process::exit(cli::run(std::env::args().skip(1).collect()));
}
