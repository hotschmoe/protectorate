use std::borrow::Cow;
use std::fmt;

/// A parse error (with a byte offset) or a conversion error (with a path such as
/// `sleeves[2].state`).
#[derive(Debug)]
pub struct Error(Box<Inner>);

#[derive(Debug)]
struct Inner {
    offset: Option<usize>,
    path: String,
    message: Cow<'static, str>,
}

impl Error {
    pub fn new(message: impl Into<Cow<'static, str>>) -> Error {
        Error(Box::new(Inner {
            offset: None,
            path: String::new(),
            message: message.into(),
        }))
    }

    pub(crate) fn at(offset: usize, message: &'static str) -> Error {
        Error(Box::new(Inner {
            offset: Some(offset),
            path: String::new(),
            message: Cow::Borrowed(message),
        }))
    }

    /// Byte offset into the input, for parse errors.
    pub fn offset(&self) -> Option<usize> {
        self.0.offset
    }

    /// Where a conversion failed, like `sleeves[2].state`; empty for parse errors.
    pub fn path(&self) -> &str {
        &self.0.path
    }

    pub fn message(&self) -> &str {
        &self.0.message
    }

    /// Prefixes the path with an object field.
    pub fn in_field(mut self, name: &str) -> Error {
        let path = &mut self.0.path;
        if !path.is_empty() && !path.starts_with('[') {
            path.insert(0, '.');
        }
        path.insert_str(0, name);
        self
    }

    /// Prefixes the path with an array index.
    pub fn in_index(mut self, index: usize) -> Error {
        let path = &mut self.0.path;
        if !path.is_empty() && !path.starts_with('[') {
            path.insert(0, '.');
        }
        path.insert_str(0, &format!("[{index}]"));
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.0.offset, self.0.path.as_str()) {
            (Some(offset), _) => write!(f, "at byte {offset}: {}", self.0.message),
            (None, "") => f.write_str(&self.0.message),
            (None, path) => write!(f, "{path}: {}", self.0.message),
        }
    }
}

impl std::error::Error for Error {}
