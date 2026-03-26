use std::sync::Arc;

use collections::HashMap;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    SharedString, Subscription, WeakEntity, actions,
};
use parking_lot::Mutex;
use recent_projects::open_remote_project;
use remote::{RemoteConnectionOptions, SshConnectionOptions};
use ui::prelude::*;
use ui::Tooltip;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(ssh_panel, [ToggleFocus, TogglePanel, RefreshHosts]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<SshPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &TogglePanel, window, cx| {
            if !workspace.toggle_panel_focus::<SshPanel>(window, cx) {
                workspace.close_panel::<SshPanel>(window, cx);
            }
        });
    })
    .detach();
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SshHost {
    pub name: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
}

impl SshHost {
    pub fn display_address(&self) -> String {
        let host = self.hostname.as_deref().unwrap_or(&self.name);
        match &self.user {
            Some(user) => format!("{}@{}", user, host),
            None => host.to_string(),
        }
    }

    pub fn ssh_destination(&self) -> String {
        match &self.user {
            Some(user) => format!("{}@{}", user, self.hostname.as_deref().unwrap_or(&self.name)),
            None => self.hostname.as_deref().unwrap_or(&self.name).to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Clone, Debug)]
pub struct DetectedPort {
    pub port: u16,
    pub process: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SshPanelEvent {
    Connected(SshHost),
    Disconnected(SshHost),
    RemotePortsDetected {
        host_name: String,
        ports: Vec<DetectedPort>,
    },
}

/// Tracks active SSH connections and their associated tunnel processes.
/// Shared between SSH panel and ports panel via Arc<Mutex<>>.
#[derive(Default)]
pub struct SshConnectionStore {
    connections: HashMap<String, ConnectionState>,
}

impl SshConnectionStore {
    pub fn state(&self, host_name: &str) -> ConnectionState {
        self.connections
            .get(host_name)
            .cloned()
            .unwrap_or(ConnectionState::Disconnected)
    }

    pub fn connected_hosts(&self) -> Vec<String> {
        self.connections
            .iter()
            .filter(|(_, state)| **state == ConnectionState::Connected)
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn set_state(&mut self, host_name: &str, state: ConnectionState) {
        if state == ConnectionState::Disconnected {
            self.connections.remove(host_name);
        } else {
            self.connections
                .insert(host_name.to_string(), state);
        }
    }
}

pub struct SshPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    hosts: Vec<SshHost>,
    connection_store: Arc<Mutex<SshConnectionStore>>,
    workspace: WeakEntity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

impl SshPanel {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        active_connection: Option<RemoteConnectionOptions>,
        cx: &mut Context<Self>,
    ) -> Self {
        let hosts = Self::load_ssh_hosts();
        let connection_store = Arc::new(Mutex::new(SshConnectionStore::default()));

        // If we're in a remote workspace, mark the host as connected
        if let Some(RemoteConnectionOptions::Ssh(ssh_opts)) = &active_connection {
            let connected_host_str = ssh_opts.host.to_string();
            let nickname = ssh_opts.nickname.as_deref();

            let matched_name = hosts.iter().find_map(|h| {
                if nickname == Some(h.name.as_str()) {
                    return Some(h.name.clone());
                }
                if h.hostname.as_deref() == Some(&connected_host_str) {
                    return Some(h.name.clone());
                }
                if h.name == connected_host_str {
                    return Some(h.name.clone());
                }
                None
            });

            if let Some(name) = matched_name {
                connection_store
                    .lock()
                    .set_state(&name, ConnectionState::Connected);
            } else {
                connection_store
                    .lock()
                    .set_state(&connected_host_str, ConnectionState::Connected);
            }
        }

        Self {
            focus_handle: cx.focus_handle(),
            width: None,
            hosts,
            connection_store,
            workspace,
            _subscriptions: Vec::new(),
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let workspace_weak = workspace.clone();
        workspace.update_in(&mut cx, |workspace, _window, cx| {
            // Extract remote connection info before creating the panel entity
            // to avoid double-borrowing the workspace
            let active_connection = workspace
                .project()
                .read(cx)
                .remote_client()
                .map(|client| client.read(cx).connection_options());

            cx.new(|cx| Self::new(workspace_weak, active_connection, cx))
        })
    }

    pub fn connection_store(&self) -> &Arc<Mutex<SshConnectionStore>> {
        &self.connection_store
    }

    pub fn hosts(&self) -> &[SshHost] {
        &self.hosts
    }

    pub fn host_by_name(&self, name: &str) -> Option<&SshHost> {
        self.hosts.iter().find(|h| h.name == name)
    }

    fn load_ssh_hosts() -> Vec<SshHost> {
        let Some(home) = dirs::home_dir() else {
            return Vec::new();
        };
        let config_path = home.join(".ssh").join("config");
        let content = match std::fs::read_to_string(&config_path) {
            Ok(content) => content,
            Err(_) => return Vec::new(),
        };
        parse_ssh_config(&content)
    }

    fn refresh_hosts(&mut self, _: &RefreshHosts, _window: &mut Window, cx: &mut Context<Self>) {
        self.hosts = Self::load_ssh_hosts();
        cx.notify();
    }

    fn connect_to_host(&mut self, host_index: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.hosts.get(host_index).cloned() else {
            return;
        };

        let state = self.connection_store.lock().state(&host.name);
        if state != ConnectionState::Disconnected {
            return;
        }

        self.connection_store
            .lock()
            .set_state(&host.name, ConnectionState::Connecting);
        cx.notify();

        // Build SSH connection options for Zed's remote infrastructure
        let ssh_host = host
            .hostname
            .as_deref()
            .unwrap_or(&host.name)
            .to_string();
        let connection_options = SshConnectionOptions {
            host: ssh_host.into(),
            username: host.user.clone(),
            port: host.port,
            nickname: Some(host.name.clone()),
            ..Default::default()
        };

        let workspace = self.workspace.clone();
        let store = self.connection_store.clone();
        let host_for_event = host.clone();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let app_state = workspace.update(cx, |workspace, _cx| {
                workspace.app_state().clone()
            })?;

            let result = open_remote_project(
                RemoteConnectionOptions::Ssh(connection_options),
                vec!["~".into()],
                app_state,
                workspace::OpenOptions {
                    open_new_workspace: Some(true),
                    ..Default::default()
                },
                cx,
            )
            .await;

            match result {
                Ok(()) => {
                    store.lock().set_state(&host_for_event.name, ConnectionState::Connected);
                    this.update(cx, |_this, cx| {
                        cx.emit(SshPanelEvent::Connected(host_for_event));
                        cx.notify();
                    })?;
                }
                Err(err) => {
                    log::error!("Failed to open remote project: {:?}", err);
                    store.lock().set_state(&host_for_event.name, ConnectionState::Disconnected);
                    this.update(cx, |_this, cx| {
                        cx.notify();
                    })?;
                }
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn disconnect_host(&mut self, host_index: usize, cx: &mut Context<Self>) {
        let Some(host) = self.hosts.get(host_index).cloned() else {
            return;
        };

        self.connection_store
            .lock()
            .set_state(&host.name, ConnectionState::Disconnected);
        cx.emit(SshPanelEvent::Disconnected(host));
        cx.notify();
    }

    pub fn detect_remote_ports(&mut self, host_index: usize, cx: &mut Context<Self>) {
        let Some(host) = self.hosts.get(host_index).cloned() else {
            return;
        };

        let host_name = host.name.clone();
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let detected = cx
                .background_spawn(async move {
                    let mut cmd = util::command::new_command("ssh");
                    if let Some(port) = host.port {
                        cmd.arg("-p");
                        cmd.arg(port.to_string());
                    }
                    cmd.arg(&host.name);
                    // Try ss first, fall back to netstat
                    cmd.arg("ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null");
                    cmd.stdout(util::command::Stdio::piped());
                    cmd.stderr(util::command::Stdio::null());

                    let output = cmd.output().await?;
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    anyhow::Ok(parse_listening_ports(&stdout))
                })
                .await?;

            this.update(cx, |_this, cx| {
                cx.emit(SshPanelEvent::RemotePortsDetected {
                    host_name,
                    ports: detected,
                });
                cx.notify();
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}

/// Parse `ss -tlnp` or `netstat -tlnp` output to extract listening ports.
/// Filters out well-known system ports (< 1024).
pub fn parse_listening_ports(output: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in output.lines() {
        // ss format: LISTEN 0 128 0.0.0.0:8080 0.0.0.0:* users:(("node",pid=1234,...))
        // netstat format: tcp 0 0 0.0.0.0:8080 0.0.0.0:* LISTEN 1234/node
        if !line.contains("LISTEN") {
            continue;
        }

        let mut port = None;
        let mut process = None;

        for field in line.split_whitespace() {
            // Look for address:port patterns
            if let Some(colon_pos) = field.rfind(':') {
                if let Ok(p) = field[colon_pos + 1..].parse::<u16>() {
                    if p >= 1024 {
                        port = Some(p);
                    }
                }
            }

            // Extract process name from ss users:((...)) or netstat pid/name
            if field.starts_with("users:") {
                if let Some(start) = field.find("((\"") {
                    if let Some(end) = field[start + 3..].find('"') {
                        process = Some(field[start + 3..start + 3 + end].to_string());
                    }
                }
            } else if field.contains('/') && !field.starts_with('/') {
                if let Some(slash) = field.find('/') {
                    let name = &field[slash + 1..];
                    if !name.is_empty() {
                        process = Some(name.to_string());
                    }
                }
            }
        }

        if let Some(p) = port {
            if seen.insert(p) {
                ports.push(DetectedPort { port: p, process });
            }
        }
    }

    ports.sort_by_key(|d| d.port);
    ports
}

fn parse_ssh_config(content: &str) -> Vec<SshHost> {
    let mut hosts = Vec::new();
    let mut current_names: Vec<String> = Vec::new();
    let mut current_hostname: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_port: Option<u16> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.splitn(2, char::is_whitespace);
        let Some(keyword) = parts.next() else {
            continue;
        };
        let value = parts.next().map(|v| v.trim()).unwrap_or("");

        if keyword.eq_ignore_ascii_case("Host") {
            if !current_names.is_empty() {
                for name in current_names.drain(..) {
                    hosts.push(SshHost {
                        name,
                        hostname: current_hostname.clone(),
                        user: current_user.clone(),
                        port: current_port,
                    });
                }
            }
            current_hostname = None;
            current_user = None;
            current_port = None;
            current_names = value
                .split_whitespace()
                .filter(|h| !h.contains('*') && !h.starts_with('!'))
                .map(|h| h.to_string())
                .collect();
        } else if keyword.eq_ignore_ascii_case("HostName") {
            current_hostname = Some(value.to_string());
        } else if keyword.eq_ignore_ascii_case("User") {
            current_user = Some(value.to_string());
        } else if keyword.eq_ignore_ascii_case("Port") {
            current_port = value.parse().ok();
        }
    }

    if !current_names.is_empty() {
        for name in current_names {
            hosts.push(SshHost {
                name,
                hostname: current_hostname.clone(),
                user: current_user.clone(),
                port: current_port,
            });
        }
    }

    hosts
}

impl Focusable for SshPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for SshPanel {}
impl EventEmitter<SshPanelEvent> for SshPanel {}

impl Panel for SshPanel {
    fn persistent_name() -> &'static str {
        "SshPanel"
    }

    fn panel_key() -> &'static str {
        "SshPanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, _position: DockPosition, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(300.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::Server)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("SSH Connections")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        4
    }
}

impl Render for SshPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        let header = h_flex()
            .w_full()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(colors.border)
            .justify_between()
            .child(
                Label::new("SSH Connections")
                    .size(LabelSize::Small)
                    .color(Color::Default),
            )
            .child(
                IconButton::new("refresh-hosts", ui::IconName::ArrowCircle)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Refresh hosts"))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.refresh_hosts(&RefreshHosts, window, cx);
                    })),
            );

        let body = if self.hosts.is_empty() {
            v_flex().child(
                div()
                    .p_4()
                    .text_color(colors.text_muted)
                    .child("No hosts found. Add entries to ~/.ssh/config to see them here."),
            )
        } else {
            let store = self.connection_store.lock();
            let mut list = v_flex().gap_0p5().py_1();

            for (index, host) in self.hosts.iter().enumerate() {
                let host_name: SharedString = host.name.clone().into();
                let conn_state = store.state(&host.name);

                let subtitle = host.display_address();
                let subtitle = if subtitle == host.name {
                    None
                } else {
                    Some(subtitle)
                };

                let (status_color, status_text) = match &conn_state {
                    ConnectionState::Disconnected => (colors.border, None),
                    ConnectionState::Connecting => {
                        (Color::Warning.color(cx), Some("Connecting..."))
                    }
                    ConnectionState::Connected => (Color::Success.color(cx), Some("Connected")),
                };

                let status_indicator = div()
                    .w(px(8.))
                    .h(px(8.))
                    .rounded_full()
                    .bg(status_color);

                let host_label = h_flex()
                    .gap_2()
                    .items_center()
                    .child(status_indicator)
                    .child(
                        v_flex()
                            .child(Label::new(host_name).size(LabelSize::Default))
                            .when_some(subtitle, |this, subtitle| {
                                this.child(
                                    Label::new(subtitle)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                            }),
                    );

                let is_connected = conn_state == ConnectionState::Connected;
                let is_connecting = conn_state == ConnectionState::Connecting;

                let mut entry = div()
                    .id(("ssh-host", index))
                    .w_full()
                    .px_3()
                    .py_1()
                    .hover(|style| style.bg(colors.ghost_element_hover))
                    .active(|style| style.bg(colors.ghost_element_active))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .child(host_label)
                            .when_some(status_text, |el, text| {
                                el.child(
                                    Label::new(text)
                                        .size(LabelSize::XSmall)
                                        .color(if is_connected {
                                            Color::Success
                                        } else {
                                            Color::Warning
                                        }),
                                )
                            }),
                    );

                if is_connected {
                    entry = entry.child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .px_3()
                            .child(
                                Button::new(("disconnect", index), "Disconnect")
                                    .style(ButtonStyle::Subtle)
                                    .label_size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.disconnect_host(index, cx);
                                    })),
                            ),
                    );
                } else if !is_connecting {
                    entry = entry
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.connect_to_host(index, window, cx);
                        }));
                }

                list = list.child(entry);
            }
            list
        };

        v_flex()
            .key_context("SshPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(header)
            .child(body)
    }
}
