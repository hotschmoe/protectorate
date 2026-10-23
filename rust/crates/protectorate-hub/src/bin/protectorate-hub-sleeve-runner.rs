fn main() {
    let mut args = std::env::args_os();
    let _ = args.next();
    let path = match (args.next(), args.next()) {
        (Some(path), None) => std::path::PathBuf::from(path),
        _ => {
            eprintln!("usage: protectorate-hub-sleeve-runner CONFIG");
            std::process::exit(2)
        }
    };
    match protectorate_sleeve_runner::run_file(&path) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("sleeve runner: {error}");
            std::process::exit(125)
        }
    }
}
