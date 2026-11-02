use std::fmt;

#[derive(Clone, Debug)]
pub enum Error {
    /// The node answered with an error frame.
    Node(pt_frame::msg::ErrorReply),
    Protocol {
        node: u32,
        hub: u32,
    },
    Failed(String),
}

impl Error {
    pub(crate) fn new(msg: impl Into<String>) -> Error {
        Error::Failed(msg.into())
    }

    pub(crate) fn context(self, ctx: impl fmt::Display) -> Error {
        match self {
            Error::Failed(m) => Error::Failed(format!("{ctx}: {m}")),
            Error::Node(mut r) => {
                r.message = format!("{ctx}: {}", r.message);
                Error::Node(r)
            }
            e @ Error::Protocol { .. } => e,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Node(r) => write!(f, "node refused ({}): {}", r.code, r.message),
            Error::Protocol { node, hub } => write!(
                f,
                "node speaks protocol {node}, the hub speaks {hub}: reinstall the node"
            ),
            Error::Failed(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
