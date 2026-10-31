fn main() {
    println!(
        "protectorate {}",
        option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or("dev")
    );
}
