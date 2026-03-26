---
name: new-panel-crate
description: Scaffold a new GPUI Panel crate for zed-terminal. Use this skill whenever the user wants to create a new panel, add a new sidebar feature, build a new dock panel, create a new crate that shows UI in the left/right/bottom dock, or mentions creating something like "github panel", "metrics panel", "notes panel", etc. Even if they don't say "panel" explicitly — if they want a new feature that lives in the sidebar or dock area, this skill applies.
---

# New Panel Crate Scaffold

This skill creates a fully wired Panel crate for zed-terminal following the established patterns from `ssh_panel`, `ports_panel`, and other panels. Every panel in this project follows an identical structure, and skipping any step causes either compile errors or runtime registration failures.

## Why this matters

The Panel trait has ~12 required methods, plus you need Cargo.toml wiring in 3 places, init() registration, a load() async factory, Render implementation, action definitions, and workspace panel registration in `zed.rs`. Missing any one of these causes either a compile error or a silent failure where the panel never appears.

## Step-by-step scaffold

### 1. Create the crate directory and Cargo.toml

```
crates/<panel_name>/Cargo.toml
crates/<panel_name>/src/<panel_name>.rs
```

The Cargo.toml must follow this exact structure (the `[lib] path` convention is a project rule — no `lib.rs`):

```toml
[package]
name = "<panel_name>"
version = "0.1.0"
edition.workspace = true
publish.workspace = true
license = "GPL-3.0-or-later"

[lints]
workspace = true

[lib]
name = "<panel_name>"
path = "src/<panel_name>.rs"

[dependencies]
anyhow.workspace = true
gpui.workspace = true
log.workspace = true
ui.workspace = true
workspace.workspace = true
```

Add more dependencies as needed (e.g., `serde`, `collections`, `settings`), but start minimal.

### 2. Register in the workspace Cargo.toml

Three changes in `Cargo.toml` (root):

1. Add `"crates/<panel_name>"` to the `[workspace] members` array (keep alphabetical order)
2. Add `<panel_name> = { path = "crates/<panel_name>" }` to `[workspace.dependencies]` (keep alphabetical order)
3. Add `<panel_name>.workspace = true` to `crates/zed/Cargo.toml` under `[dependencies]` (keep alphabetical order)

### 3. Write the panel source file

The source file (`src/<panel_name>.rs`) needs these components in order:

```rust
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, WeakEntity, actions,
};
use ui::{Tooltip, prelude::*};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

// Actions — ToggleFocus is required for the dock icon to work
actions!(<panel_name>, [ToggleFocus]);

// init() — registers workspace actions for toggling
pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<YourPanel>(window, cx);
        });
    })
    .detach();
}

// The panel struct
pub struct YourPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    workspace: WeakEntity<Workspace>,
}

impl YourPanel {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            width: None,
            workspace,
        }
    }

    // load() is the async factory called from zed.rs
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |_workspace, _window, cx| {
            let weak = workspace.clone();
            cx.new(|cx| Self::new(weak, cx))
        })
    }
}

// Required trait impls
impl Focusable for YourPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for YourPanel {}

impl Panel for YourPanel {
    fn persistent_name() -> &'static str { "YourPanel" }
    fn panel_key() -> &'static str { "YourPanel" }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right  // or Left, Bottom
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
        Some(ui::IconName::Notebook)  // Pick an appropriate icon
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Your Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        5  // Higher = further down in the dock icon list
    }
}

impl Render for YourPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        v_flex()
            .key_context("YourPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(colors.panel_background)
            .child(
                // Header
                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(Label::new("Your Panel").size(LabelSize::Small))
            )
            .child(
                // Body
                div().p_4().child("Panel content goes here")
            )
    }
}
```

### 4. Wire into the application

In `crates/zed/src/main.rs`, add the init call (near the other panel inits around line ~687):
```rust
<panel_name>::init(cx);
```

In `crates/zed/src/zed.rs`, add the panel load in `initialize_panels()`:
1. Add `use <panel_name>::YourPanel;` to the imports
2. Add the load call inside `initialize_panels()`:
```rust
let your_panel = YourPanel::load(workspace_handle.clone(), cx.clone());
```
3. Add it to the `futures::join!()` call or as a sequential load after it

### 5. Verify

Run `rtk cargo check` to confirm everything compiles. The panel should now appear in the dock.

## Gotchas

- The `persistent_name()` and `panel_key()` must return a unique string — if they collide with another panel, serialization breaks silently.
- Always implement `EventEmitter<PanelEvent>` even if you emit no custom events — the Panel trait bound requires it.
- The `load()` method must be `pub async fn` taking `WeakEntity<Workspace>` and `AsyncWindowContext` — this exact signature is what `add_panel_when_ready()` in `zed.rs` expects.
- If your panel depends on another panel's events (like PortsPanel depends on SshPanel), load it sequentially AFTER the dependency, not in the `futures::join!()` block.
