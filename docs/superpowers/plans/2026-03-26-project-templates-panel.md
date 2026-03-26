# Project Templates Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a central pane view that lists GitHub template repos, lets users create new templates, create projects from templates, and edit existing templates.

**Architecture:** New standalone `project_templates` crate implementing the `Item` trait for a central pane tab. All GitHub operations delegate to the `gh` CLI. Two `ModalView` implementations handle the create-template and use-template forms. Opens new windows via `workspace::open_paths()`.

**Tech Stack:** Rust, GPUI, workspace Item/ModalView traits, `gh` CLI, system `git` CLI

---

## File Structure

| File | Responsibility |
|------|---------------|
| `crates/project_templates/Cargo.toml` | Crate manifest with dependencies |
| `crates/project_templates/src/project_templates.rs` | Main view: `ProjectTemplates` struct, `Item` impl, `Render`, template list rendering, `gh` CLI integration, `init()` |
| `crates/project_templates/src/create_template_modal.rs` | `CreateTemplateModal` struct, `ModalView` impl, form UI, async create+clone+open logic |
| `crates/project_templates/src/use_template_modal.rs` | `UseTemplateModal` struct, `ModalView` impl, form UI, async create-from-template+open logic |
| `Cargo.toml` (workspace root) | Add `project_templates` to workspace members and dependencies |
| `crates/zed/Cargo.toml` | Add `project_templates` dependency |
| `crates/zed/src/main.rs` | Add `project_templates::init(cx)` call |
| `crates/zed/src/zed/app_menus.rs` | Add "Project Templates" to View menu |

---

### Task 1: Scaffold the `project_templates` crate

**Files:**
- Create: `crates/project_templates/Cargo.toml`
- Create: `crates/project_templates/src/project_templates.rs`
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/zed/Cargo.toml`

- [ ] **Step 1: Create `crates/project_templates/Cargo.toml`**

```toml
[package]
name = "project_templates"
version = "0.1.0"
edition.workspace = true
publish.workspace = true
license = "GPL-3.0-or-later"

[lints]
workspace = true

[lib]
name = "project_templates"
path = "src/project_templates.rs"

[dependencies]
anyhow.workspace = true
gpui.workspace = true
serde.workspace = true
serde_json.workspace = true
ui.workspace = true
workspace.workspace = true
```

- [ ] **Step 2: Create minimal `crates/project_templates/src/project_templates.rs`**

```rust
use gpui::{
    actions, div, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window,
};
use ui::prelude::*;
use workspace::{Item, Workspace};

actions!(project_templates, [Open]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &Open, window, cx| {
            let view = cx.new(|cx| ProjectTemplates::new(window, cx));
            workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
        });
    })
    .detach();
}

pub struct ProjectTemplates {
    focus_handle: FocusHandle,
}

impl ProjectTemplates {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<()> for ProjectTemplates {}

impl Focusable for ProjectTemplates {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for ProjectTemplates {
    type Event = ();

    fn to_item_events(_: &Self::Event, _: &mut dyn FnMut(workspace::item::ItemEvent)) {}

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Project Templates".into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<ui::Icon> {
        Some(ui::Icon::new(ui::IconName::FileTree))
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        None
    }
}

impl Render for ProjectTemplates {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("project-templates")
            .key_context("ProjectTemplates")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child("Project Templates — loading...")
    }
}
```

- [ ] **Step 3: Add `project_templates` to workspace `Cargo.toml`**

In the root `Cargo.toml`, add to `[workspace.members]`:
```toml
"crates/project_templates",
```

And add to `[workspace.dependencies]`:
```toml
project_templates = { path = "crates/project_templates" }
```

- [ ] **Step 4: Add dependency to `crates/zed/Cargo.toml`**

In `crates/zed/Cargo.toml` under `[dependencies]`:
```toml
project_templates.workspace = true
```

- [ ] **Step 5: Wire into `main.rs`**

In `crates/zed/src/main.rs`, add after the `ports_panel::init(cx);` line (around line 688):
```rust
project_templates::init(cx);
```

- [ ] **Step 6: Add View menu entry**

In `crates/zed/src/zed/app_menus.rs`, in the `view_items` vec after the diagnostics separator, add:
```rust
MenuItem::action("Project Templates", project_templates::Open),
```

- [ ] **Step 7: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully

Run: `cargo build -p zed`
Expected: Compiles successfully. The app should launch, and "Project Templates" should appear in the View menu. Opening it shows the placeholder text in a center pane tab.

- [ ] **Step 8: Commit**

```bash
git add crates/project_templates/ Cargo.toml crates/zed/Cargo.toml crates/zed/src/main.rs crates/zed/src/zed/app_menus.rs
git commit -m "project_templates: Scaffold crate with Item impl and View menu entry"
```

---

### Task 2: Implement `gh` CLI integration and template listing

**Files:**
- Modify: `crates/project_templates/src/project_templates.rs`

- [ ] **Step 1: Add types and `gh` CLI wrapper**

Add these types and functions to `project_templates.rs`, above the `ProjectTemplates` struct:

```rust
use std::process::Command;

#[derive(Debug, Clone, serde::Deserialize)]
struct GhRepo {
    name: String,
    description: Option<String>,
    #[serde(rename = "isTemplate")]
    is_template: bool,
    visibility: String,
    #[serde(rename = "primaryLanguage")]
    primary_language: Option<GhLanguage>,
    #[serde(rename = "stargazerCount")]
    stargazer_count: u32,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    url: String,
    owner: GhOwner,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GhLanguage {
    name: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GhOwner {
    login: String,
}

#[derive(Debug, Clone)]
pub struct TemplateRepo {
    pub name: SharedString,
    pub description: Option<SharedString>,
    pub visibility: Visibility,
    pub language: Option<SharedString>,
    pub stars: u32,
    pub updated_at: SharedString,
    pub clone_url: SharedString,
    pub owner: SharedString,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    Public,
    Private,
}

fn fetch_template_repos() -> anyhow::Result<Vec<TemplateRepo>> {
    let output = Command::new("gh")
        .args([
            "repo",
            "list",
            "--json",
            "name,description,isTemplate,visibility,primaryLanguage,stargazerCount,updatedAt,url,owner",
            "--limit",
            "100",
        ])
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("Install GitHub CLI: https://cli.github.com")
            } else {
                anyhow::anyhow!("Failed to run gh: {}", error)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("auth") || stderr.contains("login") {
            anyhow::bail!("Run `gh auth login` in a terminal");
        }
        anyhow::bail!("gh repo list failed: {}", stderr);
    }

    let repos: Vec<GhRepo> = serde_json::from_slice(&output.stdout)?;
    let templates = repos
        .into_iter()
        .filter(|repo| repo.is_template)
        .map(|repo| TemplateRepo {
            name: repo.name.into(),
            description: repo.description.map(Into::into),
            visibility: if repo.visibility == "PUBLIC" {
                Visibility::Public
            } else {
                Visibility::Private
            },
            language: repo.primary_language.map(|lang| lang.name.into()),
            stars: repo.stargazer_count,
            updated_at: repo.updated_at.into(),
            clone_url: repo.url.into(),
            owner: repo.owner.login.into(),
        })
        .collect();

    Ok(templates)
}
```

- [ ] **Step 2: Update `ProjectTemplates` struct with state**

Replace the existing struct and constructor:

```rust
pub struct ProjectTemplates {
    templates: Vec<TemplateRepo>,
    loading: bool,
    error: Option<SharedString>,
    focus_handle: FocusHandle,
    _fetch_task: Option<Task<()>>,
}

impl ProjectTemplates {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            templates: Vec::new(),
            loading: false,
            error: None,
            focus_handle: cx.focus_handle(),
            _fetch_task: None,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        cx.notify();

        let task = cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async { fetch_template_repos() }).await;

            this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(templates) => {
                        this.templates = templates;
                        this.error = None;
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        });
        self._fetch_task = Some(task);
    }
}
```

Add `use log::ResultExt as _;` to imports (the `.log_err()` method).

- [ ] **Step 3: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add crates/project_templates/src/project_templates.rs
git commit -m "project_templates: Add gh CLI integration and template fetching"
```

---

### Task 3: Render template list with cards

**Files:**
- Modify: `crates/project_templates/src/project_templates.rs`

- [ ] **Step 1: Add Refresh action and register it**

Add after the existing `actions!` macro:

```rust
actions!(project_templates, [Open, Refresh]);
```

Update the `init()` function to also register the Refresh action:

```rust
pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &Open, window, cx| {
            let view = cx.new(|cx| ProjectTemplates::new(window, cx));
            workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
        });
    })
    .detach();
}
```

Note: `Refresh` is handled as an `on_action` in the Render impl, not as a workspace action.

- [ ] **Step 2: Replace the `Render` impl with the full UI**

```rust
impl Render for ProjectTemplates {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("project-templates")
            .key_context("ProjectTemplates")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Refresh, _window, cx| {
                this.refresh(cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().colors().background)
            .child(self.render_toolbar(cx))
            .child(self.render_content(cx))
    }
}

impl ProjectTemplates {
    fn render_toolbar(&self, cx: &Context<Self>) -> impl IntoElement {
        let template_count = self.templates.len();

        h_flex()
            .px_4()
            .py_2()
            .gap_3()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Label::new("Project Templates")
                            .size(LabelSize::Large)
                            .weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        Label::new(format!("{} templates", template_count))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("create-template", "Create Template")
                            .icon(IconName::Plus)
                            .icon_position(IconPosition::Start)
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_create_template_modal(window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("refresh", IconName::ArrowCircle)
                            .tooltip(Tooltip::text("Refresh"))
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.refresh(cx);
                            })),
                    ),
            )
    }

    fn render_content(&self, cx: &Context<Self>) -> impl IntoElement {
        let content = v_flex().px_4().py_3().gap_2().flex_1().size_full();

        if self.loading {
            return content
                .items_center()
                .justify_center()
                .child(Label::new("Loading templates...").color(Color::Muted));
        }

        if let Some(error) = &self.error {
            return content.child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().status().error_background)
                    .child(Label::new(error.clone()).color(Color::Error)),
            );
        }

        if self.templates.is_empty() {
            return content.items_center().justify_center().child(
                v_flex().gap_1().items_center().child(
                    Label::new("No template repositories found.").color(Color::Muted),
                ).child(
                    Label::new("Click \"Create Template\" to make your first one.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            );
        }

        let mut list = content;
        for template in &self.templates {
            list = list.child(self.render_template_card(template, cx));
        }
        list
    }

    fn render_template_card(
        &self,
        template: &TemplateRepo,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let name = template.name.clone();
        let name_for_use = template.name.clone();
        let name_for_edit = template.name.clone();
        let owner = template.owner.clone();
        let clone_url = template.clone_url.clone();

        div()
            .px_4()
            .py_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().surface_background)
            .flex()
            .items_center()
            .justify_between()
            .child(
                v_flex()
                    .gap_1()
                    .flex_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Label::new(name)
                                    .weight(FontWeight::SEMIBOLD)
                                    .color(Color::Accent),
                            )
                            .child(
                                Label::new(match template.visibility {
                                    Visibility::Public => "public",
                                    Visibility::Private => "private",
                                })
                                .size(LabelSize::XSmall)
                                .color(match template.visibility {
                                    Visibility::Public => Color::Muted,
                                    Visibility::Private => Color::Error,
                                }),
                            ),
                    )
                    .when_some(template.description.as_ref(), |el, desc| {
                        el.child(Label::new(desc.clone()).size(LabelSize::Small).color(Color::Muted))
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .when_some(template.language.as_ref(), |el, lang| {
                                el.child(Label::new(lang.clone()).size(LabelSize::XSmall).color(Color::Muted))
                            })
                            .when(template.stars > 0, |el| {
                                el.child(
                                    Label::new(format!("★ {}", template.stars))
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(
                            SharedString::from(format!("use-{}", name_for_use)),
                            "Use Template",
                        )
                        .style(ButtonStyle::Tinted(TintColor::Positive))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_use_template_modal(
                                name_for_use.clone(),
                                owner.clone(),
                                window,
                                cx,
                            );
                        })),
                    )
                    .child(
                        Button::new(
                            SharedString::from(format!("edit-{}", name_for_edit)),
                            "Edit",
                        )
                        .style(ButtonStyle::Subtle)
                        .on_click(cx.listener(move |this, _, _window, cx| {
                            this.edit_template(name_for_edit.clone(), clone_url.clone(), cx);
                        })),
                    ),
            )
    }

    fn open_create_template_modal(&self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Implemented in Task 4
    }

    fn open_use_template_modal(
        &self,
        _template_name: SharedString,
        _owner: SharedString,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Implemented in Task 5
    }

    fn edit_template(
        &self,
        _name: SharedString,
        _clone_url: SharedString,
        _cx: &mut Context<Self>,
    ) {
        // Implemented in Task 6
    }
}
```

- [ ] **Step 3: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully. The view renders a toolbar with Create Template and Refresh buttons, template cards with Use Template and Edit buttons, and loading/error/empty states.

- [ ] **Step 4: Commit**

```bash
git add crates/project_templates/src/project_templates.rs
git commit -m "project_templates: Render template list with cards, toolbar, and states"
```

---

### Task 4: Implement `CreateTemplateModal`

**Files:**
- Create: `crates/project_templates/src/create_template_modal.rs`
- Modify: `crates/project_templates/src/project_templates.rs`

- [ ] **Step 1: Create `create_template_modal.rs`**

```rust
use anyhow::Context as _;
use gpui::{
    div, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window,
};
use ui::prelude::*;
use workspace::{DismissDecision, ModalView, Workspace};

use crate::Visibility;

pub struct CreateTemplateModal {
    repo_name: String,
    visibility: Visibility,
    error: Option<SharedString>,
    creating: bool,
    focus_handle: FocusHandle,
    _create_task: Option<Task<()>>,
}

impl CreateTemplateModal {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            repo_name: String::new(),
            visibility: Visibility::Public,
            error: None,
            creating: false,
            focus_handle: cx.focus_handle(),
            _create_task: None,
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.repo_name.trim().to_string();
        if name.is_empty() {
            return;
        }

        self.creating = true;
        self.error = None;
        cx.notify();

        let visibility_flag = match self.visibility {
            Visibility::Public => "--public",
            Visibility::Private => "--private",
        };

        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn({
                    let name = name.clone();
                    let visibility_flag = visibility_flag.to_string();
                    async move { create_template_repo(&name, &visibility_flag) }
                })
                .await;

            this.update(cx, |this, cx| {
                this.creating = false;
                match result {
                    Ok(()) => {
                        cx.emit(CreateTemplateEvent::Created { name: name.into() });
                        cx.emit(DismissEvent);
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                        cx.notify();
                    }
                }
            })
            .log_err();
        });
        self._create_task = Some(task);
    }

    fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.visibility = match self.visibility {
            Visibility::Public => Visibility::Private,
            Visibility::Private => Visibility::Public,
        };
        cx.notify();
    }
}

pub enum CreateTemplateEvent {
    Created { name: SharedString },
}

impl EventEmitter<DismissEvent> for CreateTemplateModal {}
impl EventEmitter<CreateTemplateEvent> for CreateTemplateModal {}

impl ModalView for CreateTemplateModal {
    fn on_before_dismiss(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> DismissDecision {
        DismissDecision::Dismiss(true)
    }
}

impl Focusable for CreateTemplateModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CreateTemplateModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_create = !self.repo_name.trim().is_empty() && !self.creating;

        v_flex()
            .id("create-template-modal")
            .key_context("CreateTemplateModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w(px(400.0))
            .p_4()
            .gap_4()
            .bg(cx.theme().colors().elevated_surface_background)
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("Create New Template")
                    .size(LabelSize::Large)
                    .weight(FontWeight::SEMIBOLD),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Repository Name").size(LabelSize::Small).color(Color::Muted))
                    .child(self.render_name_input(cx)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Visibility").size(LabelSize::Small).color(Color::Muted))
                    .child(self.render_visibility_toggle(cx)),
            )
            .when_some(self.error.as_ref(), |el, error| {
                el.child(Label::new(error.clone()).color(Color::Error).size(LabelSize::Small))
            })
            .child(self.render_actions(can_create, cx))
    }
}

impl CreateTemplateModal {
    fn render_name_input(&self, cx: &Context<Self>) -> impl IntoElement {
        // Simple text display — GPUI text inputs require an Editor entity which
        // adds complexity. For the initial implementation, use a minimal approach.
        // The actual text input will use a single-line Editor entity.
        div()
            .px_3()
            .py_1p5()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().editor_background)
            .child(
                Label::new(if self.repo_name.is_empty() {
                    "my-new-template".into()
                } else {
                    SharedString::from(self.repo_name.clone())
                })
                .size(LabelSize::Small)
                .color(if self.repo_name.is_empty() {
                    Color::Muted
                } else {
                    Color::Default
                }),
            )
    }

    fn render_visibility_toggle(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex().child(
            Button::new(
                "visibility-toggle",
                match self.visibility {
                    Visibility::Public => "Public",
                    Visibility::Private => "Private",
                },
            )
            .style(ButtonStyle::Filled)
            .on_click(cx.listener(|this, _, _window, cx| {
                this.toggle_visibility(cx);
            })),
        )
    }

    fn render_actions(&self, can_create: bool, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .justify_end()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(
                Button::new("cancel", "Cancel")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|_, _, _window, cx| {
                        cx.emit(DismissEvent);
                    })),
            )
            .child(
                Button::new(
                    "create",
                    if self.creating { "Creating..." } else { "Create" },
                )
                .style(ButtonStyle::Filled)
                .disabled(!can_create)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.create(window, cx);
                })),
            )
    }
}

fn create_template_repo(name: &str, visibility_flag: &str) -> anyhow::Result<()> {
    let output = std::process::Command::new("gh")
        .args(["repo", "create", name, visibility_flag])
        .output()
        .context("Failed to run gh — is GitHub CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            anyhow::bail!("Repository already exists");
        }
        anyhow::bail!("gh repo create failed: {}", stderr);
    }

    // Get the authenticated user's login for the API call
    let whoami = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .context("Failed to get GitHub username")?;

    let owner = String::from_utf8_lossy(&whoami.stdout).trim().to_string();

    let mark_output = std::process::Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/{}", owner, name),
            "-X",
            "PATCH",
            "-f",
            "is_template=true",
        ])
        .output()
        .context("Failed to mark repo as template")?;

    if !mark_output.status.success() {
        let stderr = String::from_utf8_lossy(&mark_output.stderr);
        anyhow::bail!("Failed to mark as template: {}", stderr);
    }

    Ok(())
}
```

- [ ] **Step 2: Add module declaration and wire modal into `project_templates.rs`**

Add at the top of `project_templates.rs`:
```rust
mod create_template_modal;

use create_template_modal::{CreateTemplateEvent, CreateTemplateModal};
```

Replace the stub `open_create_template_modal`:
```rust
fn open_create_template_modal(&self, window: &mut Window, cx: &mut Context<Self>) {
    let workspace = cx.window_handle().downcast::<Workspace>();
    if let Some(workspace) = workspace {
        workspace
            .update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    CreateTemplateModal::new(window, cx)
                });
                if let Some(modal) = workspace.active_modal::<CreateTemplateModal>(cx) {
                    cx.subscribe(&modal, |workspace, _, event: &CreateTemplateEvent, cx| {
                        match event {
                            CreateTemplateEvent::Created { name } => {
                                // Clone and open in new window handled in Task 6
                            }
                        }
                    })
                    .detach();
                }
            })
            .log_err();
    }
}
```

Note: The `workspace.active_modal()` method is on the `ModalLayer`. Access it through the workspace. We may need to adjust the exact API based on what's available — check `workspace.rs` for modal subscription patterns. The subscribe pattern allows us to react to the `Created` event.

- [ ] **Step 3: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add crates/project_templates/src/create_template_modal.rs crates/project_templates/src/project_templates.rs
git commit -m "project_templates: Add CreateTemplateModal with gh CLI repo creation"
```

---

### Task 5: Implement `UseTemplateModal`

**Files:**
- Create: `crates/project_templates/src/use_template_modal.rs`
- Modify: `crates/project_templates/src/project_templates.rs`

- [ ] **Step 1: Create `use_template_modal.rs`**

```rust
use anyhow::Context as _;
use gpui::{
    div, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window,
};
use ui::prelude::*;
use workspace::{DismissDecision, ModalView};

use crate::Visibility;

pub struct UseTemplateModal {
    template_name: SharedString,
    template_owner: SharedString,
    project_name: String,
    visibility: Visibility,
    error: Option<SharedString>,
    creating: bool,
    focus_handle: FocusHandle,
    _create_task: Option<Task<()>>,
}

impl UseTemplateModal {
    pub fn new(
        template_name: SharedString,
        template_owner: SharedString,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            template_name,
            template_owner,
            project_name: String::new(),
            visibility: Visibility::Private,
            error: None,
            creating: false,
            focus_handle: cx.focus_handle(),
            _create_task: None,
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project_name = self.project_name.trim().to_string();
        if project_name.is_empty() {
            return;
        }

        self.creating = true;
        self.error = None;
        cx.notify();

        let template_ref = format!("{}/{}", self.template_owner, self.template_name);
        let visibility_flag = match self.visibility {
            Visibility::Public => "--public",
            Visibility::Private => "--private",
        };

        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn({
                    let project_name = project_name.clone();
                    let template_ref = template_ref.clone();
                    let visibility_flag = visibility_flag.to_string();
                    async move {
                        create_from_template(&project_name, &template_ref, &visibility_flag)
                    }
                })
                .await;

            this.update(cx, |this, cx| {
                this.creating = false;
                match result {
                    Ok(()) => {
                        cx.emit(UseTemplateEvent::Created {
                            name: project_name.into(),
                        });
                        cx.emit(DismissEvent);
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                        cx.notify();
                    }
                }
            })
            .log_err();
        });
        self._create_task = Some(task);
    }

    fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.visibility = match self.visibility {
            Visibility::Public => Visibility::Private,
            Visibility::Private => Visibility::Public,
        };
        cx.notify();
    }
}

pub enum UseTemplateEvent {
    Created { name: SharedString },
}

impl EventEmitter<DismissEvent> for UseTemplateModal {}
impl EventEmitter<UseTemplateEvent> for UseTemplateModal {}

impl ModalView for UseTemplateModal {
    fn on_before_dismiss(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> DismissDecision {
        DismissDecision::Dismiss(true)
    }
}

impl Focusable for UseTemplateModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UseTemplateModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_create = !self.project_name.trim().is_empty() && !self.creating;

        v_flex()
            .id("use-template-modal")
            .key_context("UseTemplateModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w(px(400.0))
            .p_4()
            .gap_4()
            .bg(cx.theme().colors().elevated_surface_background)
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        Label::new("New Project from Template")
                            .size(LabelSize::Large)
                            .weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(Label::new("Using:").size(LabelSize::Small).color(Color::Muted))
                            .child(
                                Label::new(self.template_name.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Accent),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Project Name").size(LabelSize::Small).color(Color::Muted))
                    .child(self.render_name_input(cx)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Visibility").size(LabelSize::Small).color(Color::Muted))
                    .child(self.render_visibility_toggle(cx)),
            )
            .when_some(self.error.as_ref(), |el, error| {
                el.child(Label::new(error.clone()).color(Color::Error).size(LabelSize::Small))
            })
            .child(self.render_actions(can_create, cx))
    }
}

impl UseTemplateModal {
    fn render_name_input(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1p5()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().editor_background)
            .child(
                Label::new(if self.project_name.is_empty() {
                    "my-new-project".into()
                } else {
                    SharedString::from(self.project_name.clone())
                })
                .size(LabelSize::Small)
                .color(if self.project_name.is_empty() {
                    Color::Muted
                } else {
                    Color::Default
                }),
            )
    }

    fn render_visibility_toggle(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex().child(
            Button::new(
                "visibility-toggle",
                match self.visibility {
                    Visibility::Public => "Public",
                    Visibility::Private => "Private",
                },
            )
            .style(ButtonStyle::Filled)
            .on_click(cx.listener(|this, _, _window, cx| {
                this.toggle_visibility(cx);
            })),
        )
    }

    fn render_actions(&self, can_create: bool, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .justify_end()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(
                Button::new("cancel", "Cancel")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|_, _, _window, cx| {
                        cx.emit(DismissEvent);
                    })),
            )
            .child(
                Button::new(
                    "create",
                    if self.creating {
                        "Creating..."
                    } else {
                        "Create Project"
                    },
                )
                .style(ButtonStyle::Filled)
                .disabled(!can_create)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.create(window, cx);
                })),
            )
    }
}

fn create_from_template(
    project_name: &str,
    template_ref: &str,
    visibility_flag: &str,
) -> anyhow::Result<()> {
    let workspace_dir = dirs::home_dir()
        .context("Could not determine home directory")?
        .join("workspaces")
        .join("zig_workspace");

    std::fs::create_dir_all(&workspace_dir)
        .context("Failed to create workspace directory")?;

    let output = std::process::Command::new("gh")
        .current_dir(&workspace_dir)
        .args([
            "repo",
            "create",
            project_name,
            "--template",
            template_ref,
            visibility_flag,
            "--clone",
        ])
        .output()
        .context("Failed to run gh — is GitHub CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            anyhow::bail!("Repository already exists");
        }
        anyhow::bail!("gh repo create failed: {}", stderr);
    }

    Ok(())
}
```

- [ ] **Step 2: Add module declaration and wire modal into `project_templates.rs`**

Add module declaration:
```rust
mod use_template_modal;

use use_template_modal::{UseTemplateEvent, UseTemplateModal};
```

Replace the stub `open_use_template_modal`:
```rust
fn open_use_template_modal(
    &self,
    template_name: SharedString,
    owner: SharedString,
    window: &mut Window,
    cx: &mut Context<Self>,
) {
    let workspace = cx.window_handle().downcast::<Workspace>();
    if let Some(workspace) = workspace {
        workspace
            .update(cx, |workspace, cx| {
                let name = template_name.clone();
                let owner = owner.clone();
                workspace.toggle_modal(window, cx, move |window, cx| {
                    UseTemplateModal::new(name, owner, window, cx)
                });
                if let Some(modal) = workspace.active_modal::<UseTemplateModal>(cx) {
                    cx.subscribe(&modal, |workspace, _, event: &UseTemplateEvent, cx| {
                        match event {
                            UseTemplateEvent::Created { name } => {
                                // Open new window handled in Task 6
                            }
                        }
                    })
                    .detach();
                }
            })
            .log_err();
    }
}
```

- [ ] **Step 3: Add `dirs` dependency to `Cargo.toml`**

In `crates/project_templates/Cargo.toml` under `[dependencies]`:
```toml
dirs.workspace = true
```

- [ ] **Step 4: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add crates/project_templates/
git commit -m "project_templates: Add UseTemplateModal for creating projects from templates"
```

---

### Task 6: Implement Edit flow and new-window opening

**Files:**
- Modify: `crates/project_templates/src/project_templates.rs`
- Modify: `crates/project_templates/src/create_template_modal.rs`
- Modify: `crates/project_templates/src/use_template_modal.rs`
- Modify: `crates/project_templates/Cargo.toml`

- [ ] **Step 1: Add `app_state` dependency pattern**

The `workspace::open_paths()` global function needs `Arc<AppState>`. We need to pass it through. Update `Cargo.toml` to add the `app_state` dependency:

In `crates/project_templates/Cargo.toml` under `[dependencies]`:
```toml
util.workspace = true
```

- [ ] **Step 2: Add helper function for opening a directory in a new window**

Add to `project_templates.rs`:

```rust
use std::path::PathBuf;
use workspace::OpenOptions;

fn open_directory_in_new_window(path: PathBuf, cx: &mut App) {
    let app_state = workspace::AppState::global(cx);
    workspace::open_paths(
        &[path],
        app_state,
        OpenOptions::default(),
        cx,
    )
    .detach_and_log_err(cx);
}
```

Note: The exact way to get `AppState` may differ. Check if `AppState::global(cx)` exists, or if it needs to be passed through `init()`. If `AppState` isn't accessible globally, store it in `ProjectTemplates` during construction and pass it from `init()`.

- [ ] **Step 3: Implement `edit_template`**

Replace the stub in `project_templates.rs`:

```rust
fn edit_template(
    &mut self,
    name: SharedString,
    clone_url: SharedString,
    cx: &mut Context<Self>,
) {
    let templates_dir = match dirs::home_dir() {
        Some(home) => home.join("workspaces").join("zig_workspace").join("templates"),
        None => {
            self.error = Some("Could not determine home directory".into());
            cx.notify();
            return;
        }
    };

    let template_path = templates_dir.join(name.as_ref());

    if template_path.exists() {
        open_directory_in_new_window(template_path, cx);
        return;
    }

    self.loading = true;
    cx.notify();

    let clone_url = clone_url.to_string();
    let task = cx.spawn(async move |this, cx| {
        let result = cx
            .background_spawn({
                let templates_dir = templates_dir.clone();
                let clone_url = clone_url.clone();
                async move {
                    std::fs::create_dir_all(&templates_dir)?;
                    let output = std::process::Command::new("git")
                        .args(["clone", &clone_url])
                        .current_dir(&templates_dir)
                        .output()?;
                    if !output.status.success() {
                        anyhow::bail!(
                            "Clone failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    anyhow::Ok(())
                }
            })
            .await;

        this.update(cx, |this, cx| {
            this.loading = false;
            match result {
                Ok(()) => {
                    open_directory_in_new_window(template_path, cx);
                }
                Err(error) => {
                    this.error = Some(error.to_string().into());
                }
            }
            cx.notify();
        })
        .log_err();
    });
    self._fetch_task = Some(task);
}
```

- [ ] **Step 4: Wire the Create Template modal to clone and open**

In `project_templates.rs`, update the `CreateTemplateEvent::Created` handler inside `open_create_template_modal`:

```rust
CreateTemplateEvent::Created { name } => {
    let templates_dir = dirs::home_dir()
        .map(|home| home.join("workspaces").join("zig_workspace").join("templates"));

    if let Some(templates_dir) = templates_dir {
        let name = name.clone();
        // Clone the newly created template repo
        cx.spawn_in(window, async move |workspace_handle, cx| {
            let clone_result = cx.background_spawn({
                let templates_dir = templates_dir.clone();
                let name = name.clone();
                async move {
                    std::fs::create_dir_all(&templates_dir)?;
                    let output = std::process::Command::new("gh")
                        .args(["repo", "clone", name.as_ref()])
                        .current_dir(&templates_dir)
                        .output()?;
                    if !output.status.success() {
                        anyhow::bail!(
                            "Clone failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    anyhow::Ok(())
                }
            }).await;

            if clone_result.is_ok() {
                let template_path = templates_dir.join(name.as_ref());
                workspace_handle.update(&mut cx, |_, cx| {
                    open_directory_in_new_window(template_path, cx);
                }).log_err();
            }
        }).detach();
    }
}
```

- [ ] **Step 5: Wire the Use Template modal to open new window**

Update the `UseTemplateEvent::Created` handler inside `open_use_template_modal`:

```rust
UseTemplateEvent::Created { name } => {
    let project_dir = dirs::home_dir()
        .map(|home| home.join("workspaces").join("zig_workspace").join(name.as_ref()));

    if let Some(project_dir) = project_dir {
        open_directory_in_new_window(project_dir, cx);
    }
}
```

- [ ] **Step 6: Refresh template list after creation**

In both `CreateTemplateEvent::Created` and after the clone completes, find the `ProjectTemplates` item in the workspace pane and call `refresh`. The simplest approach is to also subscribe to the modal events from `ProjectTemplates` itself. Update `open_create_template_modal` to refresh after creation:

Add a refresh call inside the `Created` event handler:
```rust
// After opening the new window, refresh the template list
// Find the ProjectTemplates item and refresh it
for item in workspace.active_pane().read(cx).items() {
    if let Some(pt) = item.downcast::<ProjectTemplates>() {
        pt.update(cx, |pt, cx| pt.refresh(cx));
        break;
    }
}
```

- [ ] **Step 7: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully

Run: `cargo build -p zed`
Expected: Full binary compiles. All three flows (create template, use template, edit template) should be functional.

- [ ] **Step 8: Commit**

```bash
git add crates/project_templates/
git commit -m "project_templates: Implement edit flow and new-window opening for all actions"
```

---

### Task 7: Add text input to modals using Editor entity

**Files:**
- Modify: `crates/project_templates/src/create_template_modal.rs`
- Modify: `crates/project_templates/src/use_template_modal.rs`
- Modify: `crates/project_templates/Cargo.toml`

The modals in Tasks 4-5 use placeholder Label rendering for the text input. This task replaces them with proper single-line `Editor` entities for real keyboard input.

- [ ] **Step 1: Add `editor` dependency**

In `crates/project_templates/Cargo.toml` under `[dependencies]`:
```toml
editor.workspace = true
```

- [ ] **Step 2: Update `CreateTemplateModal` to use Editor**

In `create_template_modal.rs`, add imports:
```rust
use editor::{Editor, EditorMode};
```

Replace the `repo_name: String` field with an Editor entity:
```rust
pub struct CreateTemplateModal {
    name_editor: Entity<Editor>,
    visibility: Visibility,
    error: Option<SharedString>,
    creating: bool,
    focus_handle: FocusHandle,
    _create_task: Option<Task<()>>,
}
```

Update the constructor:
```rust
pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let name_editor = cx.new(|cx| {
        let mut editor = Editor::single_line(window, cx);
        editor.set_placeholder_text("my-new-template", cx);
        editor
    });

    Self {
        name_editor,
        visibility: Visibility::Public,
        error: None,
        creating: false,
        focus_handle: cx.focus_handle(),
        _create_task: None,
    }
}
```

Update `create()` to read from the editor:
```rust
fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let name = self.name_editor.read(cx).text(cx).trim().to_string();
    if name.is_empty() {
        return;
    }
    // ... rest unchanged, just use `name` instead of `self.repo_name`
}
```

Replace `render_name_input`:
```rust
fn render_name_input(&self, cx: &Context<Self>) -> impl IntoElement {
    div()
        .px_1()
        .py_0p5()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border_variant)
        .bg(cx.theme().colors().editor_background)
        .child(self.name_editor.clone())
}
```

Update the `can_create` check in `render()`:
```rust
let name_text = self.name_editor.read(cx).text(cx);
let can_create = !name_text.trim().is_empty() && !self.creating;
```

Update `Focusable` to focus the editor:
```rust
impl Focusable for CreateTemplateModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name_editor.focus_handle(cx)
    }
}
```

- [ ] **Step 3: Apply same pattern to `UseTemplateModal`**

Same changes as Step 2 but for `use_template_modal.rs`:
- Replace `project_name: String` with `name_editor: Entity<Editor>`
- Update constructor to create single-line Editor with placeholder `"my-new-project"`
- Update `create()` to read from editor
- Replace `render_name_input` to render editor entity
- Update `Focusable` to focus the editor

- [ ] **Step 4: Build and verify**

Run: `cargo build -p project_templates`
Expected: Compiles successfully. Modals now have real text input that accepts keyboard input.

- [ ] **Step 5: Commit**

```bash
git add crates/project_templates/
git commit -m "project_templates: Replace placeholder inputs with Editor entities for real text input"
```

---

### Task 8: Manual integration test

**Files:** None (testing only)

- [ ] **Step 1: Build the full binary**

Run: `cargo build -p zed`
Expected: Compiles successfully

- [ ] **Step 2: Launch and verify View menu**

Launch the app. Open View menu → "Project Templates" should appear. Click it.
Expected: A new tab opens in the center pane titled "Project Templates".

- [ ] **Step 3: Verify template listing**

If `gh` is authenticated, the panel should list your template repos (or show empty state if none exist). If not, it should show the auth error message.

- [ ] **Step 4: Test Create Template flow**

Click "+ Create Template" → modal should appear with name input and visibility toggle. Enter a test name, click Create. Verify: repo is created on GitHub, marked as template, cloned locally, new window opens.

- [ ] **Step 5: Test Use Template flow**

Click "Use Template" on an existing template → modal should appear showing template name. Enter project name, click "Create Project". Verify: repo is created from template on GitHub, cloned locally, new window opens.

- [ ] **Step 6: Test Edit flow**

Click "Edit" on a template → should clone if not already local, then open in new window. Click "Edit" again → should open immediately (already cloned).

- [ ] **Step 7: Verify error states**

Test with `gh` not authenticated, with network disconnected, with duplicate repo names. Verify inline error messages appear correctly.

- [ ] **Step 8: Commit any fixes**

If any issues were found and fixed during testing:
```bash
git add -A
git commit -m "project_templates: Fix issues found during integration testing"
```
