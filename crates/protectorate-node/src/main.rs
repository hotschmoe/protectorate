mod cli;
mod rpc;
mod serve;
mod stdio;

fn main() {
    std::process::exit(cli::run(std::env::args().skip(1).collect()));
}
