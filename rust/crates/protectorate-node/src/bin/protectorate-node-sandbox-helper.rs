//! Node-owned entry point for the direct Linux namespace launcher.
#[cfg(target_os = "linux")]
fn main() {
    match protectorate_sandbox::runtime::helper_main() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("protectorate node sandbox helper: {error}");
            std::process::exit(1);
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("node sandbox helper requires Linux");
    std::process::exit(1);
}
