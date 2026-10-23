fn main() {
    let path = match std::env::args_os().nth(1) {
        Some(path) if std::env::args_os().count() == 2 => std::path::PathBuf::from(path),
        _ => {
            eprintln!("usage: protectorate-sleeve-runner CONFIG");
            std::process::exit(2);
        }
    };
    match protectorate_sleeve_runner::run_file(&path) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("sleeve runner: {error}");
            std::process::exit(125);
        }
    }
}
