//! DeepSeek Harness (DSH) SDK JSON-RPC Protocol Definitions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl DshRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<DshRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshRpcIncomingRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone)]
pub enum DshIncomingMessage {
    Response(DshRpcResponse),
    Request(DshRpcIncomingRequest),
    Notification(DshRpcNotification),
    Raw(Value),
}

pub fn parse_incoming(line: &str) -> Result<DshIncomingMessage, String> {
    let val: Value = serde_json::from_str(line).map_err(|e| format!("Invalid JSON: {e}"))?;
    if val.get("id").is_some() && (val.get("result").is_some() || val.get("error").is_some()) {
        if let Ok(resp) = serde_json::from_value::<DshRpcResponse>(val.clone()) {
            return Ok(DshIncomingMessage::Response(resp));
        }
    }
    if val.get("id").is_some() && val.get("method").is_some() {
        if let Ok(request) = serde_json::from_value::<DshRpcIncomingRequest>(val.clone()) {
            return Ok(DshIncomingMessage::Request(request));
        }
    }
    if val.get("id").is_none() && val.get("method").is_some() {
        if let Ok(notif) = serde_json::from_value::<DshRpcNotification>(val.clone()) {
            return Ok(DshIncomingMessage::Notification(notif));
        }
    }
    Ok(DshIncomingMessage::Raw(val))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_rpc_request_serialization() {
        let req = DshRpcRequest::new(1, "initialize", Some(json!({"client": "test"})));
        let serialized = serde_json::to_string(&req).unwrap();
        assert!(serialized.contains(r#""jsonrpc":"2.0""#));
        assert!(serialized.contains(r#""id":1"#));
        assert!(serialized.contains(r#""method":"initialize""#));
        assert!(serialized.contains(r#""client":"test""#));
    }

    #[test]
    fn test_cancel_request_carries_the_active_prompt_id() {
        let req = DshRpcRequest::new(9, "$/cancelRequest", Some(json!({"id": 42})));
        let serialized = serde_json::to_string(&req).unwrap();
        assert!(serialized.contains(r#""method":"$/cancelRequest""#));
        assert!(serialized.contains(r#""id":42"#));
        assert!(serialized.contains(r#""id":9"#));
    }

    #[test]
    fn test_parse_incoming_response_and_error() {
        let success_json = r#"{"jsonrpc":"2.0","id":42,"result":{"status":"ok"}}"#;
        match parse_incoming(success_json).expect("should parse successfully") {
            DshIncomingMessage::Response(resp) => {
                assert_eq!(resp.id, Some(42));
                assert_eq!(resp.result.unwrap()["status"], "ok");
                assert!(resp.error.is_none());
            }
            other => panic!("Expected Response, got {:?}", other),
        }

        let error_json =
            r#"{"jsonrpc":"2.0","id":43,"error":{"code":-32601,"message":"Method not found"}}"#;
        match parse_incoming(error_json).expect("should parse successfully") {
            DshIncomingMessage::Response(resp) => {
                assert_eq!(resp.id, Some(43));
                assert!(resp.result.is_none());
                let err = resp.error.expect("expected error object");
                assert_eq!(err.code, -32601);
                assert_eq!(err.message, "Method not found");
            }
            other => panic!("Expected Response with error, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_incoming_notification() {
        let notif_json =
            r#"{"jsonrpc":"2.0","method":"session/messageDelta","params":{"delta":"hello"}}"#;
        match parse_incoming(notif_json).expect("should parse successfully") {
            DshIncomingMessage::Notification(notif) => {
                assert_eq!(notif.method, "session/messageDelta");
                assert_eq!(notif.params.unwrap()["delta"], "hello");
            }
            other => panic!("Expected Notification, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_incoming_raw_and_invalid() {
        let raw_json = r#"{"arbitrary":"data"}"#;
        match parse_incoming(raw_json).expect("should parse successfully") {
            DshIncomingMessage::Raw(val) => {
                assert_eq!(val["arbitrary"], "data");
            }
            other => panic!("Expected Raw, got {:?}", other),
        }

        assert!(parse_incoming("invalid json string").is_err());
    }

    #[test]
    fn test_parse_incoming_request() {
        let request_json =
            r#"{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{}}"#;
        match parse_incoming(request_json).expect("should parse request") {
            DshIncomingMessage::Request(request) => {
                assert_eq!(request.id, json!(7));
                assert_eq!(request.method, "session/request_permission");
            }
            other => panic!("Expected Request, got {:?}", other),
        }
    }
}
