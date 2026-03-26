mod analytics_dashboard;
mod theme_bridge;

use std::sync::Arc;

use gpui::{
    Action, App, Context, EventEmitter, FocusHandle, Focusable, Pixels, Task, Window, actions,
    canvas, px,
};
use ui::prelude::*;
use util::ResultExt as _;
use webview_runtime::{IpcDispatcher, IpcReceiver, Webview, WebviewConfig};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(webview_panel, [ToggleFocus]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<WebViewPanel>(window, cx);
        });
    })
    .detach();
}

pub struct WebViewPanel {
    position: DockPosition,
    size: Pixels,
    focus_handle: FocusHandle,
    webview: Option<Arc<dyn Webview>>,
    ipc_dispatcher: IpcDispatcher,
    ipc_task: Option<Task<()>>,
    is_active: bool,
    _subscriptions: Vec<gpui::Subscription>,
}

impl WebViewPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut ipc_dispatcher = IpcDispatcher::new();

        // Register built-in IPC methods that all webview panels have access to.
        ipc_dispatcher.register("panel.getInfo", |_params| {
            Ok(serde_json::json!({ "name": "Analytics Dashboard", "version": "0.1.0" }))
        });

        // Analytics: read session data from ~/.agentics/sessions/*.jsonl
        ipc_dispatcher.register("analytics.getSessionData", |_params| {
            let sessions_dir = dirs::home_dir()
                .map(|h| h.join(".agentics").join("sessions"))
                .unwrap_or_default();

            let mut sessions = Vec::new();
            if sessions_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|e| e == "jsonl") {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                for line in content.lines() {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(line)
                                    {
                                        sessions.push(val);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(serde_json::json!({ "sessions": sessions }))
        });

        Self {
            position: DockPosition::Bottom,
            size: px(320.),
            focus_handle: cx.focus_handle(),
            webview: None,
            ipc_dispatcher,
            ipc_task: None,
            is_active: false,
            _subscriptions: Vec::new(),
        }
    }

    /// Schedule webview creation for the next frame. Webview creation is
    /// deferred because wry's WebView2 initialization pumps the Win32 message
    /// loop (COM STA), which can re-enter the GPUI event loop and panic if
    /// we're inside an entity update. By extracting the HWND first, then
    /// spawning, and calling `create_webview` outside any entity borrow,
    /// we avoid this re-entrant panic.
    #[cfg(target_os = "windows")]
    fn schedule_webview_creation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.webview.is_some() || self.ipc_task.is_some() {
            return;
        }

        // Extract the HWND and build config while we have access to Window + Context.
        let hwnd = window.get_raw_handle();
        let theme_script = theme_bridge::build_theme_css_script(cx.theme().colors());

        let config = WebviewConfig {
            content: webview_runtime::WebviewContent::Html(
                analytics_dashboard::ANALYTICS_HTML.into(),
            ),
            allow_remote_urls: false,
            allowed_hosts: Vec::new(),
            initialization_scripts: vec![theme_script],
        };

        // Spawn the actual wry creation on the foreground thread. The key is
        // that `create_webview` runs in the async body — not inside `update` —
        // so there is no active entity borrow when wry pumps the message loop.
        let task = cx.spawn(async move |this, cx| {
            // This runs on the GPUI foreground thread, outside any entity update.
            let creation_result = webview_runtime::create_webview(hwnd, config);

            match creation_result {
                Ok((webview, ipc_receiver)) => {
                    let webview: Arc<dyn Webview> = Arc::from(webview);
                    // Show the webview immediately since we're active
                    webview.set_visible(true).log_err();

                    this.update(cx, |this, cx| {
                        this.webview = Some(webview);
                        this.start_ipc_pump(ipc_receiver, cx);
                        log::info!("WebViewPanel: webview created successfully");
                        cx.notify();
                    })
                    .log_err();
                }
                Err(err) => {
                    log::error!("Failed to create webview: {err}");
                }
            }
        });
        // Store the task so it's not dropped (which would cancel it).
        self.ipc_task = Some(task);
    }

    #[cfg(not(target_os = "windows"))]
    fn schedule_webview_creation(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        log::warn!("Webview panels are not yet supported on this platform");
    }

    fn start_ipc_pump(&mut self, receiver: IpcReceiver, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            while let Ok(raw_message) = receiver.recv().await {
                let result = this.update(cx, |this, _cx| {
                    if let Some(response) = this.ipc_dispatcher.dispatch(&raw_message) {
                        if let Ok(json) = serde_json::to_string(&response) {
                            if let Some(webview) = &this.webview {
                                let script = format!(
                                    "window.__zed_ipc._dispatch('{}')",
                                    json.replace('\\', "\\\\").replace('\'', "\\'")
                                );
                                webview.evaluate_script(&script).log_err();
                            }
                        }
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        });
        self.ipc_task = Some(task);
    }

    /// Re-inject theme CSS variables into the webview after a theme change.
    pub fn apply_theme(&self, cx: &App) {
        if let Some(webview) = &self.webview {
            let script = theme_bridge::build_theme_css_script(cx.theme().colors());
            webview.evaluate_script(&script).log_err();
        }
    }
}

impl Focusable for WebViewPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for WebViewPanel {}

impl Panel for WebViewPanel {
    fn persistent_name() -> &'static str {
        "WebViewPanel"
    }

    fn panel_key() -> &'static str {
        "WebViewPanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        self.position
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(
            position,
            DockPosition::Left | DockPosition::Right | DockPosition::Bottom
        )
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.position = position;
        cx.notify();
    }

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.size
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.size = size.unwrap_or(px(320.));
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::ToolWeb)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Analytics Dashboard")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        200
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.is_active = active;
        if active {
            self.schedule_webview_creation(window, cx);
        }
        if let Some(webview) = &self.webview {
            webview.set_visible(active).log_err();
        }
    }
}

impl Render for WebViewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Clone the Arc so the paint closure can call set_bounds without
        // borrowing self. This is safe because set_bounds is a &self method.
        let webview = self.webview.clone();

        v_flex()
            .key_context("WebViewPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                // The canvas element occupies the full panel area. During the
                // paint phase, it positions the OS-level webview to match the
                // canvas bounds (converted to physical pixels).
                canvas(
                    |_bounds, _window, _cx| {},
                    move |bounds, _, _window, _cx| {
                        let Some(webview) = &webview else {
                            return;
                        };
                        // Pass logical coordinates directly — wry handles DPI
                        // scaling internally via the webview's own DPI awareness.
                        let x = bounds.origin.x.as_f32();
                        let y = bounds.origin.y.as_f32();
                        let width = bounds.size.width.as_f32();
                        let height = bounds.size.height.as_f32();

                        if width > 0.0 && height > 0.0 {
                            webview.set_bounds(x, y, width, height).log_err();
                        }
                    },
                )
                .flex_1()
                .size_full(),
            )
    }
}
