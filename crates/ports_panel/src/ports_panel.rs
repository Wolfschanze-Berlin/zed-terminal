use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, WeakEntity, actions,
};
use ui::prelude::*;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(ports_panel, [ToggleFocus, TogglePanel]);

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

pub struct PortsPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl PortsPanel {
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
        v_flex()
            .key_context("PortsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(div().p_4().child("Port Forwards"))
            .child(
                div()
                    .p_4()
                    .text_color(cx.theme().colors().text_muted)
                    .child("No active port forwards."),
            )
    }
}
