pub(crate) fn unsupported(what: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("{what} is not supported on {}", std::env::consts::OS),
    )
}
