//! Windows-only node executable, shipped as protectorate-rs.exe.

#[cfg(windows)]
include!("windows.rs");

#[cfg(not(windows))]
fn main() {
    eprintln!("protectorate-node-windows must be built for Windows");
    std::process::exit(1);
}
