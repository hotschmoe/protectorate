//! JSON for Protectorate: a value type, a strict RFC 8259 parser, a deterministic writer,
//! `ToJson`/`FromJson`, and declarative macros for API types.
//!
//! - Objects keep insertion order. A duplicate key while parsing keeps its first position and
//!   takes the last value.
//! - Numbers keep their source text, so a parse-write round trip is exact. Equality compares that
//!   text: `1` and `1.0` differ.
//! - A non-finite `f64` becomes `null` (`Value::from`) or `None` (`Number::from_f64`).
//! - Nesting deeper than 128 arrays and objects is rejected.
//!
//! ```text
//! pt_json::record! {
//!     #[derive(Debug, Clone, PartialEq)]
//!     pub struct Sleeve {
//!         pub name: String,
//!         pub state: State,
//!         pub note: Option<String>,          // omitted when None; missing or null reads as None
//!         pub tags: Vec<String> = Vec::new(), // missing or null gives the default
//!     }
//! }
//! pt_json::string_enum! {
//!     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//!     pub enum State { Running = "running", Stopped = "stopped" }
//! }
//! pt_json::tagged_enum! {
//!     #[derive(Debug, Clone, PartialEq)]
//!     pub enum Event {                       // {"type": "started", "id": ...}
//!         Started = "started" { id: String },
//!         Stopped = "stopped" { id: String, code: Option<i64> },
//!     }
//! }
//! let v = pt_json::obj! { "a" => 1, "b" => "x" };
//! ```

#![forbid(unsafe_code)]

mod convert;
mod macros;
mod parse;
mod value;
mod write;

use std::borrow::Cow;
use std::fmt;

pub use convert::{FromJson, ToJson};
pub use parse::{parse, parse_bytes};
pub use value::{Map, Number, Value};

#[doc(hidden)]
pub use convert::__private;

/// Parses `text` and converts it with `T::from_json`.
pub fn from_str<T: FromJson>(text: &str) -> Result<T, Error> {
    T::from_json(&parse(text)?)
}

/// Validates UTF-8, parses and converts with `T::from_json`.
pub fn from_bytes<T: FromJson>(bytes: &[u8]) -> Result<T, Error> {
    T::from_json(&parse_bytes(bytes)?)
}

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
