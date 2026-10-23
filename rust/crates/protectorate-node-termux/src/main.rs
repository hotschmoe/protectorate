//! Android/Termux node executable, without the Linux hub and sleeve runtime.

#[cfg(any(test, target_os = "android"))]
mod lifecycle;

#[cfg(target_os = "android")]
include!("termux.rs");

#[cfg(not(target_os = "android"))]
fn main() {
    eprintln!("protectorate-node-termux must be built for Android");
    std::process::exit(1);
}
