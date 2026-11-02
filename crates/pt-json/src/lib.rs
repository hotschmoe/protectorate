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
mod error;
mod macros;
mod parse;
mod value;
mod write;

pub use convert::{FromJson, ToJson};
pub use error::Error;
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
