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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_and_non_object_frames() {
        assert_eq!(parse_request(b"{ not json").unwrap_err().code, PARSE_ERROR);
        assert_eq!(parse_request(b"[1,2]").unwrap_err().code, INVALID_REQUEST);
        assert_eq!(
            parse_request(br#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#)
                .unwrap_err()
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            parse_request(br#"{"jsonrpc":"2.0","id":1}"#)
                .unwrap_err()
                .code,
            INVALID_REQUEST
        );
    }

    #[test]
    fn keeps_the_id_of_a_broken_request_and_rejects_object_ids() {
        assert_eq!(
            parse_request(br#"{"jsonrpc":"2.0","id":1,"method":""}"#)
                .unwrap_err()
                .id,
            Some(json!(1))
        );
        assert_eq!(
            parse_request(br#"{"jsonrpc":"2.0","id":"abc","method":12}"#)
                .unwrap_err()
                .id,
            Some(json!("abc"))
        );
        let error =
            parse_request(br#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#).unwrap_err();
        assert_eq!(error.code, INVALID_REQUEST);
        assert_eq!(error.message, "invalid request id");
        assert_eq!(error.id, None);
    }

    #[test]
    fn accepts_string_number_and_null_ids_and_treats_no_id_as_a_notification() {
        let lines: [&[u8]; 3] = [
            br#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#,
            br#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
            br#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
        ];
        for line in lines {
            let request = parse_request(line).unwrap();
            assert!(!request.is_notification());
        }
        let request =
            parse_request(br#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#).unwrap();
        assert!(request.is_notification());
        assert_eq!(request.params, Value::Null);
    }

    #[test]
    fn responses_carry_result_or_error_but_never_both() {
        let ok = success(&json!(1), json!({"pong": true}));
        assert_eq!(ok["result"]["pong"], true);
        assert!(ok.get("error").is_none());
        let err = Failure::new(Some(json!("x")), METHOD_NOT_FOUND, "nope")
            .with_data(json!({"supported": ["2025-11-25"]}))
            .into_response();
        assert_eq!(err["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(err["error"]["data"]["supported"][0], "2025-11-25");
        assert!(err.get("result").is_none());
    }
}
