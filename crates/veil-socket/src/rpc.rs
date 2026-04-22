//! JSON-RPC 2.0 wire-format types, error codes, and helper constructors.

/// A JSON-RPC 2.0 request.
///
/// The `id` field is `Option<serde_json::Value>` because JSON-RPC allows
/// string, number, or null IDs. Notifications have no `id`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// The method name.
    pub method: String,
    /// Parameters for the method. Defaults to `null` if omitted.
    #[serde(default)]
    pub params: serde_json::Value,
    /// Request ID. `None` for notifications.
    pub id: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 success response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// The result value.
    pub result: serde_json::Value,
    /// Matches the request ID.
    pub id: serde_json::Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorResponse {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// The error object.
    pub error: RpcError,
    /// Matches the request ID (or `null` for parse errors).
    pub id: serde_json::Value,
}

/// The `error` object inside an error response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RpcError {
    /// Error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── Standard JSON-RPC 2.0 error codes ────────────────────────────────────────

/// Invalid JSON was received.
pub const PARSE_ERROR: i64 = -32700;
/// The JSON sent is not a valid Request object.
pub const INVALID_REQUEST: i64 = -32600;
/// The method does not exist or is not available.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// Invalid method parameters.
pub const INVALID_PARAMS: i64 = -32602;
/// Internal JSON-RPC error.
pub const INTERNAL_ERROR: i64 = -32603;
/// Application-defined: the requested workspace was not found.
pub const WORKSPACE_NOT_FOUND: i64 = -32000;
/// Application-defined: method exists but is not yet implemented.
pub const NOT_IMPLEMENTED: i64 = -32001;

// ── Response constructors ─────────────────────────────────────────────────────

impl Response {
    /// Construct a success response.
    #[allow(unused_variables)]
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        todo!("implement Response::ok")
    }
}

// ── ErrorResponse constructors ────────────────────────────────────────────────

impl ErrorResponse {
    /// Construct an error response with an explicit code and message.
    #[allow(unused_variables)]
    pub fn new(id: serde_json::Value, code: i64, message: impl Into<String>) -> Self {
        todo!("implement ErrorResponse::new")
    }

    /// `-32700 Parse error` — used when a line cannot be parsed as JSON.
    /// The id is `null` because we cannot identify the request.
    pub fn parse_error() -> Self {
        todo!("implement ErrorResponse::parse_error")
    }

    /// `-32601 Method not found`.
    #[allow(unused_variables)]
    pub fn method_not_found(id: serde_json::Value, method: &str) -> Self {
        todo!("implement ErrorResponse::method_not_found")
    }

    /// `-32602 Invalid params`.
    #[allow(unused_variables)]
    pub fn invalid_params(id: serde_json::Value, detail: impl Into<String>) -> Self {
        todo!("implement ErrorResponse::invalid_params")
    }

    /// `-32603 Internal error`.
    #[allow(unused_variables)]
    pub fn internal_error(id: serde_json::Value, detail: impl Into<String>) -> Self {
        todo!("implement ErrorResponse::internal_error")
    }

    /// `-32000 Workspace not found` (application-defined).
    #[allow(unused_variables)]
    pub fn workspace_not_found(id: serde_json::Value, ws_id: u64) -> Self {
        todo!("implement ErrorResponse::workspace_not_found")
    }
}

// ── RpcOutcome ────────────────────────────────────────────────────────────────

/// What a method handler returns. Converted to wire bytes by the connection handler.
pub enum RpcOutcome {
    /// Successful result value.
    Ok(serde_json::Value),
    /// Error response to send back.
    Err(ErrorResponse),
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Unit 1: JSON-RPC 2.0 types ────────────────────────────────────────────

    #[test]
    fn request_round_trip() {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            method: "workspace.list".to_string(),
            params: json!({"key": "value"}),
            id: Some(json!(42)),
        };
        let serialized = serde_json::to_string(&req).expect("serialize");
        let deserialized: Request = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.jsonrpc, "2.0");
        assert_eq!(deserialized.method, "workspace.list");
        assert_eq!(deserialized.params, json!({"key": "value"}));
        assert_eq!(deserialized.id, Some(json!(42)));
    }

    #[test]
    fn request_missing_id_deserializes_as_none() {
        let json_str = r#"{"jsonrpc":"2.0","method":"workspace.list"}"#;
        let req: Request = serde_json::from_str(json_str).expect("deserialize");
        assert_eq!(req.id, None);
    }

    #[test]
    fn request_params_defaults_to_null() {
        let json_str = r#"{"jsonrpc":"2.0","method":"workspace.list","id":1}"#;
        let req: Request = serde_json::from_str(json_str).expect("deserialize");
        assert_eq!(req.params, serde_json::Value::Null);
    }

    #[test]
    fn response_serializes_jsonrpc_field() {
        let resp = Response::ok(json!(1), json!({"result": "ok"}));
        let serialized = serde_json::to_string(&resp).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(value["jsonrpc"], "2.0");
    }

    #[test]
    fn error_response_serializes_code_and_message() {
        let err = ErrorResponse::new(json!(1), -32600, "Invalid Request");
        let serialized = serde_json::to_string(&err).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(value["error"]["code"], -32600_i64);
        assert_eq!(value["error"]["message"], "Invalid Request");
    }

    #[test]
    fn error_data_omitted_when_none() {
        let err = RpcError { code: -32600, message: "oops".to_string(), data: None };
        let serialized = serde_json::to_string(&err).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("deserialize");
        assert!(value.get("data").is_none(), "data key should be absent when None, got: {value}");
    }

    #[test]
    fn parse_error_has_correct_code() {
        let err = ErrorResponse::parse_error();
        assert_eq!(err.error.code, PARSE_ERROR);
    }

    #[test]
    fn method_not_found_embeds_method_name() {
        let err = ErrorResponse::method_not_found(json!(1), "workspace.frobnicate");
        assert!(
            err.error.message.contains("workspace.frobnicate"),
            "message should contain method name, got: {}",
            err.error.message
        );
    }

    #[test]
    fn workspace_not_found_has_application_code() {
        let err = ErrorResponse::workspace_not_found(json!(1), 42);
        assert_eq!(err.error.code, WORKSPACE_NOT_FOUND);
    }
}
