fn main() {
    match protectorate_sandbox::runtime::helper_main() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("sandbox helper: {error}");
            std::process::exit(125);
        }
    }
}
