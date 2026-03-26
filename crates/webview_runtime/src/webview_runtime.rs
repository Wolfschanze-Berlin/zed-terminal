pub mod ipc;

#[cfg(target_os = "windows")]
mod platform_windows;

pub use ipc::{IpcDispatcher, IpcReceiver, IpcSender};

/// What content the webview should load initially.
pub enum WebviewContent {
    /// Load content from a URL (http://, https://, file://).
    /// Note: `data:` URLs are NOT supported by wry — use `Html` instead.
    Url(String),
    /// Load inline HTML content directly.
    Html(String),
}

/// Configuration for creating a new webview instance.
pub struct WebviewConfig {
    /// What to load in the webview.
    pub content: WebviewContent,
    /// Whether the webview is allowed to navigate to remote URLs.
    pub allow_remote_urls: bool,
    /// Allowlist of hostnames the webview may navigate to (only checked when
    /// `allow_remote_urls` is true). An empty list means all hosts are allowed.
    pub allowed_hosts: Vec<String>,
    /// Extra JavaScript to inject before any page script runs.
    pub initialization_scripts: Vec<String>,
}

/// Platform-independent interface to a live webview instance.
///
/// All methods must be called from the GPUI foreground thread.
pub trait Webview: Send + 'static {
    /// Reposition and resize the webview to match the given logical-pixel rect
    /// relative to the parent window's client area. DPI scaling is handled
    /// internally by the platform webview (wry).
    fn set_bounds(&self, x: f32, y: f32, width: f32, height: f32) -> anyhow::Result<()>;

    /// Show or hide the webview without destroying it.
    fn set_visible(&self, visible: bool) -> anyhow::Result<()>;

    /// Evaluate arbitrary JavaScript in the webview's main frame.
    fn evaluate_script(&self, script: &str) -> anyhow::Result<()>;
}

/// Create a new webview attached to the given parent window.
///
/// Returns a `(Box<dyn Webview>, IpcReceiver)` pair. The receiver yields raw JSON
/// strings posted by the page via `window.ipc.postMessage(jsonString)`.
#[cfg(target_os = "windows")]
pub fn create_webview(
    parent_hwnd: windows::Win32::Foundation::HWND,
    config: WebviewConfig,
) -> anyhow::Result<(Box<dyn Webview>, IpcReceiver)> {
    platform_windows::create(parent_hwnd, config)
}

/// Webview panels are not yet supported on non-Windows platforms.
/// This function is provided so that code referencing `create_webview` can
/// compile cross-platform, but it will always return an error at runtime.
#[cfg(not(target_os = "windows"))]
pub fn create_webview(
    _config: WebviewConfig,
) -> anyhow::Result<(Box<dyn Webview>, IpcReceiver)> {
    anyhow::bail!("Webview panels are not yet supported on this platform")
}

/// The JavaScript bridge injected into every webview as an initialization script.
/// Provides `window.__zed_ipc.postMessage(json)` for JS→Rust calls and
/// `window.__zed_ipc._dispatch(json)` for Rust→JS event delivery.
pub const IPC_BRIDGE_SCRIPT: &str = r#"
(function() {
    'use strict';
    var _requestId = 0;
    var _pending = {};
    var _listeners = {};

    window.__zed_ipc = {
        invoke: function(method, params) {
            return new Promise(function(resolve, reject) {
                var id = ++_requestId;
                _pending[id] = { resolve: resolve, reject: reject };
                window.ipc.postMessage(JSON.stringify({
                    jsonrpc: '2.0',
                    id: id,
                    method: method,
                    params: params || {}
                }));
            });
        },

        on: function(event, callback) {
            if (!_listeners[event]) _listeners[event] = [];
            _listeners[event].push(callback);
            return function() {
                var arr = _listeners[event];
                if (arr) {
                    var idx = arr.indexOf(callback);
                    if (idx >= 0) arr.splice(idx, 1);
                }
            };
        },

        _dispatch: function(json) {
            try {
                var msg = JSON.parse(json);
                if (msg.id != null && _pending[msg.id]) {
                    var p = _pending[msg.id];
                    delete _pending[msg.id];
                    if (msg.error) {
                        p.reject(new Error(msg.error.message || 'IPC error'));
                    } else {
                        p.resolve(msg.result);
                    }
                } else if (msg.method && _listeners[msg.method]) {
                    var handlers = _listeners[msg.method];
                    for (var i = 0; i < handlers.length; i++) {
                        try { handlers[i](msg.params); } catch(e) {
                            console.error('[zed-ipc] listener error:', e);
                        }
                    }
                }
            } catch(e) {
                console.error('[zed-ipc] dispatch parse error:', e);
            }
        }
    };
})();
"#;
