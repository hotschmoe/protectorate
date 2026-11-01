mod cli;
mod join;
mod member;
mod rpc;
mod serve;
mod service;
mod stdio;
#[cfg(windows)]
mod winservice;

fn main() {
    std::process::exit(cli::run(std::env::args().skip(1).collect()));
}
