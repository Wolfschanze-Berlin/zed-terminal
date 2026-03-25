use std::path::PathBuf;
use std::sync::Arc;

use collections::HashSet;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, WeakEntity, actions,
};
use remote::{RemoteConnectionOptions, SshConnectionOptions};
use ui::prelude::*;
use ui::Tooltip;
use workspace::{
    AppState, OpenOptions, Workspace,
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

struct SshHost {
    name: String,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
}

pub struct SshPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    hosts: Vec<SshHost>,
    connected_hosts: HashSet<String>,
    workspace: WeakEntity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

impl SshPanel {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let hosts = Self::load_ssh_hosts();
        Self {
            focus_handle: cx.focus_handle(),
            width: None,
            hosts,
            connected_hosts: HashSet::default(),
            workspace,
            _subscriptions: Vec::new(),
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let workspace_weak = workspace.clone();
        workspace.update_in(&mut cx, |_workspace, _window, cx| {
            cx.new(|cx| Self::new(workspace_weak, cx))
        })
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
        let Some(host) = self.hosts.get(host_index) else {
            return;
        };

        let hostname = host
            .hostname
            .clone()
            .unwrap_or_else(|| host.name.clone());

        let connection_options = SshConnectionOptions {
            host: hostname.into(),
            username: host.user.clone(),
            port: host.port,
            ..Default::default()
        };

        let host_name = host.name.clone();
        let remote_options = RemoteConnectionOptions::Ssh(connection_options);

        let app_state = match self.workspace.read_with(cx, |workspace, _cx| {
            workspace.app_state().clone()
        }) {
            Ok(state) => state,
            Err(error) => {
                log::error!("SSH panel: workspace no longer available: {}", error);
                return;
            }
        };

        self.connected_hosts.insert(host_name.clone());
        cx.notify();

        Self::open_ssh_connection(remote_options, app_state, host_name, cx);
    }

    fn open_ssh_connection(
        connection_options: RemoteConnectionOptions,
        app_state: Arc<AppState>,
        host_name: String,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let open_options = OpenOptions::default();
            let paths: Vec<PathBuf> = vec![];

            let result = recent_projects::open_remote_project(
                connection_options,
                paths,
                app_state,
                open_options,
                cx,
            )
            .await;

            if let Err(error) = &result {
                log::error!(
                    "SSH connection failed for host {}: {:?}",
                    host_name,
                    error
                );
                if let Err(update_error) = this.update(cx, |this, cx| {
                    this.connected_hosts.remove(&host_name);
                    cx.notify();
                }) {
                    log::warn!("Failed to update SSH panel state: {}", update_error);
                }
            }

            result
        })
        .detach_and_log_err(cx);
    }

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
            let mut list = v_flex().gap_0p5().py_1();
            for (index, host) in self.hosts.iter().enumerate() {
                let host_name: SharedString = host.name.clone().into();
                let is_connected = self.connected_hosts.contains(&host.name);

                let subtitle = match (&host.user, &host.hostname) {
                    (Some(user), Some(hostname)) => Some(format!("{}@{}", user, hostname)),
                    (None, Some(hostname)) => Some(hostname.clone()),
                    (Some(user), None) => Some(format!("{}@{}", user, host.name)),
                    (None, None) => None,
                };

                let status_indicator = div()
                    .w(px(8.))
                    .h(px(8.))
                    .rounded_full()
                    .when(is_connected, |el| el.bg(Color::Success.color(cx)))
                    .when(!is_connected, |el| el.bg(colors.border));

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

                let mut entry = div()
                    .id(("ssh-host", index))
                    .w_full()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.ghost_element_hover))
                    .active(|style| style.bg(colors.ghost_element_active))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .child(host_label),
                    );

                if is_connected {
                    entry = entry.child(
                        h_flex().child(
                            Label::new("Connected")
                                .size(LabelSize::XSmall)
                                .color(Color::Success),
                        ),
                    );
                } else {
                    entry = entry.on_click(cx.listener(move |this, _event, window, cx| {
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
