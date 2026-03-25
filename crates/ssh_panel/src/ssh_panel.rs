use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, WeakEntity, actions,
};
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

struct SshHost {
    name: String,
    hostname: Option<String>,
    user: Option<String>,
}

pub struct SshPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    hosts: Vec<SshHost>,
    _subscriptions: Vec<Subscription>,
}

impl SshPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let hosts = Self::load_ssh_hosts();
        Self {
            focus_handle: cx.focus_handle(),
            width: None,
            hosts,
            _subscriptions: Vec::new(),
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |_workspace, _window, cx| {
            cx.new(|cx| Self::new(cx))
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

    fn connect_to_host(&mut self, host_name: &str, _window: &mut Window, _cx: &mut Context<Self>) {
        log::info!("SSH connect requested for host: {}", host_name);
    }

}

fn parse_ssh_config(content: &str) -> Vec<SshHost> {
    let mut hosts = Vec::new();
    let mut current_names: Vec<String> = Vec::new();
    let mut current_hostname: Option<String> = None;
    let mut current_user: Option<String> = None;

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
                    });
                }
            }
            current_hostname = None;
            current_user = None;
            current_names = value
                .split_whitespace()
                .filter(|h| !h.contains('*') && !h.starts_with('!'))
                .map(|h| h.to_string())
                .collect();
        } else if keyword.eq_ignore_ascii_case("HostName") {
            current_hostname = Some(value.to_string());
        } else if keyword.eq_ignore_ascii_case("User") {
            current_user = Some(value.to_string());
        }
    }

    if !current_names.is_empty() {
        for name in current_names {
            hosts.push(SshHost {
                name,
                hostname: current_hostname.clone(),
                user: current_user.clone(),
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

        // Build header
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

        // Build body
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
                let host_name_for_click = host.name.clone();

                let subtitle = match (&host.user, &host.hostname) {
                    (Some(user), Some(hostname)) => Some(format!("{}@{}", user, hostname)),
                    (None, Some(hostname)) => Some(hostname.clone()),
                    (Some(user), None) => Some(format!("{}@{}", user, host.name)),
                    (None, None) => None,
                };

                let entry = div()
                    .id(("ssh-host", index))
                    .w_full()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.ghost_element_hover))
                    .active(|style| style.bg(colors.ghost_element_active))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.connect_to_host(&host_name_for_click, window, cx);
                    }))
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
