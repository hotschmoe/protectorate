use pt_api::ApiError;

#[derive(Debug)]
pub enum Fail {
    Usage(String),
    Api(ApiError),
    Other(String),
    Exit(u8),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(e) | Self::Other(e) => f.write_str(e),
            Self::Api(e) => write!(f, "{e}"),
            Self::Exit(code) => write!(f, "exit {code}"),
        }
    }
}

impl From<String> for Fail {
    fn from(s: String) -> Fail {
        Fail::Usage(s)
    }
}
