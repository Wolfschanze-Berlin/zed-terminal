use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, WeakEntity, actions,
};
use ui::prelude::*;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(ssh_panel, [ToggleFocus, TogglePanel]);

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

pub struct SshPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl SshPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            width: None,
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
        v_flex()
            .key_context("SshPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(div().p_4().child("SSH Connections"))
            .child(
                div()
                    .p_4()
                    .text_color(cx.theme().colors().text_muted)
                    .child("No connections configured. Edit ~/.ssh/config to add hosts."),
            )
    }
}
