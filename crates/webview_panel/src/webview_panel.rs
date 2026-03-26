mod analytics_dashboard;
mod theme_bridge;

use std::sync::Arc;

use gpui::{App, Context, EventEmitter, FocusHandle, Focusable, Task, Window, actions, canvas};
use ui::prelude::*;
use util::ResultExt as _;
use webview_runtime::{IpcDispatcher, IpcReceiver, Webview, WebviewConfig};
use workspace::{
    Workspace,
    item::{Item, ItemEvent},
};

actions!(webview_panel, [OpenAnalyticsDashboard]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &OpenAnalyticsDashboard, window, cx| {
            let item = cx.new(|cx| WebViewPanel::new(window, cx));
            workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);
        });
    })
    .detach();
}

pub struct WebViewPanel {
    focus_handle: FocusHandle,
    webview: Option<Arc<dyn Webview>>,
    ipc_dispatcher: IpcDispatcher,
    ipc_task: Option<Task<()>>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl WebViewPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut ipc_dispatcher = IpcDispatcher::new();

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

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            webview: None,
            ipc_dispatcher,
            ipc_task: None,
            _subscriptions: Vec::new(),
        };

        this.schedule_webview_creation(window, cx);
        this
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

        let task = cx.spawn(async move |this, cx| {
            let creation_result = webview_runtime::create_webview(hwnd, config);

            match creation_result {
                Ok((webview, ipc_receiver)) => {
                    let webview: Arc<dyn Webview> = Arc::from(webview);
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

impl EventEmitter<ItemEvent> for WebViewPanel {}

impl Item for WebViewPanel {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Analytics Dashboard".into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<ui::Icon> {
        Some(ui::Icon::new(ui::IconName::ToolWeb))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(event.clone());
    }
}

impl Render for WebViewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let webview = self.webview.clone();

        v_flex()
            .key_context("WebViewPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                canvas(
                    |_bounds, _window, _cx| {},
                    move |bounds, _, _window, _cx| {
                        let Some(webview) = &webview else {
                            return;
                        };
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
