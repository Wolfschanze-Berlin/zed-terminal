use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, WeakEntity, actions,
};
use ui::{Tooltip, prelude::*};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(ports_panel, [ToggleFocus, TogglePanel, AddForward, RemoveForward]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<PortsPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &TogglePanel, window, cx| {
            if !workspace.toggle_panel_focus::<PortsPanel>(window, cx) {
                workspace.close_panel::<PortsPanel>(window, cx);
            }
        });
    })
    .detach();
}

#[derive(Clone, Debug)]
struct PortForward {
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    status: ForwardStatus,
}

#[derive(Clone, Debug, PartialEq)]
enum ForwardStatus {
    Active,
    Connecting,
    Failed(String),
}

pub struct PortsPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    forwards: Vec<PortForward>,
    show_add_form: bool,
    _subscriptions: Vec<Subscription>,
}

impl PortsPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            width: None,
            forwards: Vec::new(),
            show_add_form: false,
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

    fn toggle_add_form(&mut self, cx: &mut Context<Self>) {
        self.show_add_form = !self.show_add_form;
        cx.notify();
    }

    fn add_mock_forward(&mut self, cx: &mut Context<Self>) {
        let local_port = 8080 + self.forwards.len() as u16;
        self.forwards.push(PortForward {
            local_port,
            remote_host: "remote".into(),
            remote_port: 80,
            status: ForwardStatus::Active,
        });
        self.show_add_form = false;
        cx.notify();
    }

    fn remove_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.forwards.len() {
            self.forwards.remove(index);
            cx.notify();
        }
    }
}

impl Focusable for PortsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PortsPanel {}

impl Panel for PortsPanel {
    fn persistent_name() -> &'static str {
        "PortsPanel"
    }

    fn panel_key() -> &'static str {
        "PortsPanel"
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
        Some("Port Forwards")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        5
    }
}

impl Render for PortsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_forwards = !self.forwards.is_empty();
        let show_form = self.show_add_form;

        // Build header
        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("Port Forwards")
                    .size(LabelSize::Default)
                    .color(Color::Default),
            )
            .child(
                IconButton::new("add-forward", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Add Forward"))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_add_form(cx);
                    })),
            );

        let mut panel = v_flex()
            .key_context("PortsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(header);

        if show_form {
            let form = v_flex()
                .p_2()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new("New Port Forward")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    h_flex().gap_1().child(
                        Label::new("localhost:8080 \u{2192} remote:80")
                            .size(LabelSize::Small)
                            .color(Color::Default),
                    ),
                )
                .child(
                    h_flex().gap_2().child(
                        Button::new("submit-forward", "Forward")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.add_mock_forward(cx);
                            })),
                    ),
                );
            panel = panel.child(form);
        }

        if has_forwards {
            let forwards = self.forwards.clone();
            let mut rows = v_flex().id("forwards-list").flex_1().overflow_y_scroll();

            for (index, forward) in forwards.iter().enumerate() {
                let status_color = match &forward.status {
                    ForwardStatus::Active => Color::Success,
                    ForwardStatus::Connecting => Color::Warning,
                    ForwardStatus::Failed(_) => Color::Error,
                };

                let label = format!(
                    "localhost:{} \u{2192} {}:{}",
                    forward.local_port, forward.remote_host, forward.remote_port
                );

                let status_label = match &forward.status {
                    ForwardStatus::Active => "Active".to_string(),
                    ForwardStatus::Connecting => "Connecting".to_string(),
                    ForwardStatus::Failed(reason) => format!("Failed: {reason}"),
                };

                let hover_bg = cx.theme().colors().ghost_element_hover;

                let row = h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .hover(move |style| style.bg(hover_bg))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_1()
                            .child(
                                div()
                                    .w(px(8.))
                                    .h(px(8.))
                                    .rounded_full()
                                    .bg(status_color.color(cx)),
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(label).size(LabelSize::Small))
                                    .child(
                                        Label::new(status_label)
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            ),
                    )
                    .child(
                        IconButton::new(("remove-forward", index), IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Remove Forward"))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.remove_forward(index, cx);
                            })),
                    );

                rows = rows.child(row);
            }

            panel = panel.child(rows);
        } else {
            panel = panel.child(
                div()
                    .p_4()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(
                        Label::new(
                            "No active port forwards. Connect to a remote host to manage port forwarding.",
                        )
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
            );
        }

        panel
    }
}
