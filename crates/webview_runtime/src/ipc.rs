use std::sync::Arc;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Webview;

// ── Channel types ──────────────────────────────────────────────────────────

pub type IpcSender = async_channel::Sender<String>;
pub type IpcReceiver = async_channel::Receiver<String>;

pub(crate) fn create_channel() -> (IpcSender, IpcReceiver) {
    async_channel::bounded(256)
}

// ── JSON-RPC 2.0 wire types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(i64),
    Str(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcError {
    pub const PARSE_ERROR: i32 = -32700;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const PERMISSION_DENIED: i32 = -32000;

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: Self::METHOD_NOT_FOUND,
            message: format!("Method not found: {method}"),
        }
    }

    pub fn permission_denied(method: &str) -> Self {
        Self {
            code: Self::PERMISSION_DENIED,
            message: format!("Permission denied: {method}"),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: Self::INTERNAL_ERROR,
            message: message.into(),
        }
    }
}

// ── Rust→JS event emitter ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct IpcEvent {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

impl IpcEvent {
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        }
    }
}

pub struct IpcEmitter {
    webview: Arc<dyn Webview>,
}

impl IpcEmitter {
    pub fn new(webview: Arc<dyn Webview>) -> Self {
        Self { webview }
    }

    /// Emit a JSON-RPC notification (no id) to the webview.
    /// The JS side receives it via `window.__zed_ipc.on(event, callback)`.
    pub fn emit(&self, event: &str, params: serde_json::Value) -> anyhow::Result<()> {
        let notification = IpcEvent::new(event, params);
        let json = serde_json::to_string(&notification)?;
        let escaped = json.replace('\\', "\\\\").replace('\'', "\\'");
        self.webview
            .evaluate_script(&format!("window.__zed_ipc._dispatch('{escaped}')"))
    }
}

// ── Dispatcher ─────────────────────────────────────────────────────────────

type HandlerFn = Box<dyn Fn(serde_json::Value) -> anyhow::Result<serde_json::Value>>;

/// Routes incoming JSON-RPC requests to registered method handlers.
///
/// Lives on the GPUI foreground thread alongside the `WebViewPanel` entity.
pub struct IpcDispatcher {
    handlers: HashMap<String, HandlerFn>,
}

impl IpcDispatcher {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a synchronous handler for `method`.
    /// The handler receives `params` and returns a JSON result or an error.
    pub fn register<F>(&mut self, method: impl Into<String>, handler: F)
    where
        F: Fn(serde_json::Value) -> anyhow::Result<serde_json::Value> + 'static,
    {
        self.handlers.insert(method.into(), Box::new(handler));
    }

    /// Process a raw JSON string received from the webview IPC channel.
    ///
    /// Returns `Some(response)` for JSON-RPC requests (which have an `id`),
    /// or `None` for notifications and malformed messages.
    pub fn dispatch(&self, raw_json: &str) -> Option<JsonRpcResponse> {
        let request: JsonRpcRequest = match serde_json::from_str(raw_json) {
            Ok(req) => req,
            Err(err) => {
                log::warn!("webview IPC: failed to parse JSON-RPC request: {err}");
                return None;
            }
        };

        let result = match self.handlers.get(&request.method) {
            Some(handler) => handler(request.params),
            None => {
                return Some(JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError::method_not_found(&request.method)),
                });
            }
        };

        Some(match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id,
                result: Some(value),
                error: None,
            },
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError::internal(err.to_string())),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_known_method_returns_result() {
        let mut dispatcher = IpcDispatcher::new();
        dispatcher.register("echo", |params| Ok(params));

        let response = dispatcher
            .dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":{"value":42}}"#)
            .expect("should return a response");

        assert!(response.error.is_none());
        let result = response.result.expect("should have result");
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn dispatch_unknown_method_returns_error() {
        let dispatcher = IpcDispatcher::new();

        let response = dispatcher
            .dispatch(r#"{"jsonrpc":"2.0","id":2,"method":"nonexistent","params":{}}"#)
            .expect("should return a response");

        assert!(response.result.is_none());
        let error = response.error.expect("should have error");
        assert_eq!(error.code, JsonRpcError::METHOD_NOT_FOUND);
    }

    #[test]
    fn dispatch_malformed_json_returns_none() {
        let dispatcher = IpcDispatcher::new();
        assert!(dispatcher.dispatch("not json at all").is_none());
    }

    #[test]
    fn dispatch_handler_error_returns_internal_error() {
        let mut dispatcher = IpcDispatcher::new();
        dispatcher.register("fail", |_| anyhow::bail!("something broke"));

        let response = dispatcher
            .dispatch(r#"{"jsonrpc":"2.0","id":3,"method":"fail","params":{}}"#)
            .expect("should return a response");

        let error = response.error.expect("should have error");
        assert_eq!(error.code, JsonRpcError::INTERNAL_ERROR);
        assert!(error.message.contains("something broke"));
    }
}
