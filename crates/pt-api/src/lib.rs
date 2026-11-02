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
#[macro_use]
mod endpoint;
mod methods;
mod types;

pub use endpoint::{
    ACCESS_TOKEN_PARAM, API_PREFIX, EVENTS_PATH, FROM_SEQ_PARAM, Method, MethodDef, MethodVisitor,
    Scope,
};
pub use methods::*;
pub use schema::{
    ApiType, Described, EnumValue, EpochMs, FieldDef, Shape, Ty, TypeDef, VariantDef,
};
pub use types::*;

/// The method with this name, as in `/api/<name>`.
pub fn find_method(name: &str) -> Option<&'static MethodDef> {
    METHODS.iter().find(|m| m.name == name)
}
