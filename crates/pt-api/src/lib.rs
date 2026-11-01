//! The hub API, defined once: view types, request and response bodies, the method table, the
//! event-stream messages and errors. `pt-apigen` writes the Dart client from the schema tables
//! these declarations register.
//!
//! - `POST /api/<Method>` with the request as a JSON body; 200 with the response body, or an
//!   [`ErrorBody`] with [`ErrorCode::http_status`].
//! - `GET /api/events` upgrades to a WebSocket of [`HubStreamMessage`] text frames; resume with
//!   `?from_seq=<n>`.
//! - Credentials go in `Authorization: Bearer <token>`. A browser cannot set that header on a
//!   WebSocket, so the web client passes `?access_token=<token>` on `/api/events` instead.

#![forbid(unsafe_code)]

#[macro_use]
mod schema;
mod methods;
mod types;

pub use methods::*;
pub use schema::{
    ApiType, Described, EnumValue, EpochMs, FieldDef, Shape, Ty, TypeDef, VariantDef,
};
pub use types::*;

pub const API_PREFIX: &str = "/api/";
pub const EVENTS_PATH: &str = "/api/events";
pub const FROM_SEQ_PARAM: &str = "from_seq";
pub const ACCESS_TOKEN_PARAM: &str = "access_token";

pt_json::string_enum! {
    /// Who may call a method.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Scope {
        /// The owner token (CLI) and `pd1.` device tokens (apps).
        Owner = "owner",
        /// Also a sleeve's `pc1.` capability, for its own sleeve and its children. Envoy's
        /// capability reaches every sleeve, since Envoy drives them.
        Sleeve = "sleeve",
        /// Also the envoy-only capability of vendor general agents (D9).
        EnvoyOnly = "envoy_only",
    }
}

impl Scope {
    /// Whether a credential holding `self` may call a method of scope `method`.
    pub fn allows(self, method: Scope) -> bool {
        match self {
            Scope::Owner => true,
            Scope::Sleeve => method != Scope::Owner,
            Scope::EnvoyOnly => method == Scope::EnvoyOnly,
        }
    }
}

/// One API method, `POST /api/<NAME>`.
pub trait Method {
    const NAME: &'static str;
    const SCOPE: Scope;
    type Request: Described;
    type Response: Described;
}

/// A method as data, for walking the table.
pub struct MethodDef {
    pub name: &'static str,
    pub doc: &'static str,
    pub scope: Scope,
    pub request: fn() -> &'static TypeDef,
    pub response: fn() -> &'static TypeDef,
}

impl MethodDef {
    pub fn path(&self) -> String {
        format!("{API_PREFIX}{}", self.name)
    }
}

/// Called once per method, in table order.
pub trait MethodVisitor {
    fn visit<M: Method>(&mut self);
}

/// The method with this name, as in `/api/<name>`.
pub fn find_method(name: &str) -> Option<&'static MethodDef> {
    METHODS.iter().find(|m| m.name == name)
}
