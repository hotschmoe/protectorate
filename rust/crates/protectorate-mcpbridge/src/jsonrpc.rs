//! Newline-delimited JSON-RPC 2.0 framing for the MCP transport.
use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const SERVER_ERROR: i64 = -32000;
pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// One request line. `id` is absent for notifications, which never get a reply.
#[derive(Clone, Debug)]
pub struct Request {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

impl Request {
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// A JSON-RPC error. `id` is `None` when the request was too broken to echo.
#[derive(Clone, Debug)]
pub struct Failure {
    pub id: Option<Value>,
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl Failure {
    pub fn new(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            id,
            code,
            message: message.into(),
            data: None,
        }
    }
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
    pub fn with_id(mut self, id: &Value) -> Self {
        if self.id.is_none() {
            self.id = Some(id.clone());
        }
        self
    }
    pub fn into_response(self) -> Value {
        let mut error = json!({
            "code": self.code,
            "message": self.message,
        });
        if let Some(data) = self.data {
            error["data"] = data;
        }
        json!({
            "jsonrpc": "2.0",
            "id": self.id.unwrap_or(Value::Null),
            "error": error,
        })
    }
}

pub fn success(id: &Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// Rejects anything that is not a JSON object carrying `jsonrpc: "2.0"` and a
/// non-empty string method, with an id of string, number or null.
pub fn parse_request(line: &[u8]) -> Result<Request, Failure> {
    let value: Value = match serde_json::from_slice(line) {
        Ok(value) => value,
        Err(_) => return Err(Failure::new(None, PARSE_ERROR, "parse error")),
    };
    let invalid = |message: &str| {
        let id = value
            .as_object()
            .and_then(|object| object.get("id"))
            .filter(|id| matches!(id, Value::Null | Value::String(_) | Value::Number(_)))
            .cloned();
        Failure::new(id, INVALID_REQUEST, message)
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => return Err(invalid("invalid request")),
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(invalid("invalid request"));
    }
    let method = match object.get("method").and_then(Value::as_str) {
        Some(method) if !method.is_empty() => method.to_owned(),
        _ => return Err(invalid("invalid request")),
    };
    let id = match object.get("id") {
        None => None,
        Some(id @ (Value::Null | Value::String(_) | Value::Number(_))) => Some(id.clone()),
        Some(_) => return Err(invalid("invalid request id")),
    };
    Ok(Request {
        id,
        method,
        params: object.get("params").cloned().unwrap_or(Value::Null),
    })
}
