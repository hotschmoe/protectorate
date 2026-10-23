#[cfg(target_os = "linux")]
fn main() {
    match protectorate_sandbox::runtime::helper_main() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("protectorate sandbox helper: {error}");
            std::process::exit(125);
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("protectorate sandbox helper requires Linux");
    std::process::exit(125);
}
