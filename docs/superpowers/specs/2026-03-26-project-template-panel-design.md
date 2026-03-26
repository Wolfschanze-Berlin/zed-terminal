# Project Templates Panel — Design Spec

**Date:** 2026-03-26
**Status:** Approved

## Overview

A central pane view for managing GitHub template repositories. Users can list their template repos, create new templates, create projects from existing templates, and edit templates by opening them in a new window.

## Architecture

### Crate

New crate: `project_templates`
- Standalone module (single-line enable/disable in Cargo.toml)
- Library root: `src/project_templates.rs` (specified via `[lib] path`)

### Dependencies

- `gpui` — UI framework
- `workspace` — Item trait, ModalView trait, window management
- `ui` — shared UI components (buttons, labels, inputs)
- `fs` — git clone via system `git` CLI
- `util` — path helpers

No dependency on `http_client`, `git_hosting_providers`, or `credentials_provider`. All GitHub interaction goes through the `gh` CLI.

### Trait Implementations

`ProjectTemplates` struct implements:
- `Render` — renders the template list UI
- `Focusable` — keyboard focus support
- `EventEmitter<ItemEvent>` — workspace item events
- `Item` — central pane tab behavior (tab title, icon, tooltip)

Two modal structs implement `ModalView`:
- `CreateTemplateModal` — form for creating a new template repo
- `UseTemplateModal` — form for creating a project from a template

### Registration

- `init(cx)` called from `crates/zed/src/main.rs`
- Registers `OpenProjectTemplates` action
- Item loaded in `crates/zed/src/zed.rs` `initialize_workspace_panels()`
- View menu entry: "Project Templates"
- Command palette: "project templates: Open"

## UI Layout

### Main View (Central Pane Tab)

```
┌─────────────────────────────────────────────────────────┐
│ ⬡ Project Templates          3 templates   [+ Create] [↻]│
├─────────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────────────────────────┐ │
│ │ zig-game-template    [public] [template]            │ │
│ │ Zig game development starter with raylib bindings   │ │
│ │ Updated 3 days ago · Zig · ★ 12                     │ │
│ │                          [Use Template] [Edit]      │ │
│ └─────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ rust-cli-template    [public] [template]            │ │
│ │ Rust CLI starter with clap, tracing, and CI         │ │
│ │ Updated 1 week ago · Rust · ★ 5                     │ │
│ │                          [Use Template] [Edit]      │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                         │
│   Templates are fetched from your GitHub account.       │
│   Click "Create Template" to make a new one.            │
└─────────────────────────────────────────────────────────┘
```

**Header toolbar:**
- Title: "Project Templates" with icon
- Template count badge
- "+ Create Template" button (primary)
- "↻ Refresh" button (secondary)

**Template cards:** Each card displays:
- Repository name (linked style)
- Visibility badge: public (neutral) or private (red)
- "template" badge (yellow)
- Description text
- Metadata line: last updated, primary language, star count
- Actions: "Use Template" (green), "Edit" (neutral)

**Empty state:** Friendly message when no templates exist, with guidance to create one.

**Loading state:** Spinner replacing the card list while fetching.

### Create Template Modal

Fields:
- **Repository Name** — text input (required)
- **Visibility** — toggle: Public (default) / Private

Actions:
- Cancel (dismisses modal)
- Create (disabled until name is non-empty, shows spinner during operation)

On success:
1. Creates GitHub repo via `gh repo create <name> --public/--private`
2. Marks as template via `gh api repos/{owner}/{name} -X PATCH -f is_template=true`
3. Clones to `~/workspaces/zig_workspace/templates/<name>`
4. Opens cloned directory in a new window
5. Refreshes template list

### Use Template Modal

Shows which template is being used (read-only label).

Fields:
- **Project Name** — text input (required)
- **Visibility** — toggle: Public / Private (default)

Actions:
- Cancel (dismisses modal)
- Create Project (disabled until name is non-empty, shows spinner during operation)

On success:
1. Creates and clones repo from template via `gh repo create <name> --template <owner>/<template> --public/--private --clone` in the `~/workspaces/zig_workspace/` directory
2. Opens cloned directory in a new window

### Edit Template

No modal — direct action from the template card's "Edit" button.

Flow:
1. Check if `~/workspaces/zig_workspace/templates/<name>` exists locally
2. If not, clone it first (show progress)
3. Open the directory in a new window

User edits files, commits, and pushes normally in the new window.

## Data Flow

### GitHub CLI Commands

| Action | Command |
|--------|---------|
| List templates | `gh repo list --json name,description,isTemplate,visibility,primaryLanguage,stargazerCount,updatedAt,url --limit 100` (filter `isTemplate == true`) |
| Create template | `gh repo create <name> --public/--private` then `gh api repos/{owner}/{name} -X PATCH -f is_template=true` |
| Create from template | `gh repo create <name> --template <owner>/<template> --public/--private --clone` (run in target directory) |
| Clone | `git clone <url> <path>` (via `fs::git_clone`) |

### Authentication

Resolution order:
1. `GITHUB_TOKEN` environment variable (if set, `gh` uses it automatically)
2. `gh auth status` — if user has run `gh auth login`, commands work with stored credentials
3. If neither: display inline error "GitHub auth required — run `gh auth login` in a terminal"

### Async Execution

- All `gh` and `git` commands run via `cx.background_spawn()` to avoid blocking the UI thread
- Foreground task spawned with `cx.spawn()` awaits the result, updates state, calls `cx.notify()`
- Buttons show spinner text during operations ("Creating...", "Cloning...")

## Error Handling

All errors display inline — in the modal (below the form) or as a banner in the main view.

| Condition | Message |
|-----------|---------|
| `gh` CLI not found | "Install GitHub CLI: https://cli.github.com" |
| Auth failure | "Run `gh auth login` in a terminal" |
| Repo name conflict | "Repository already exists" |
| Network error | "Could not reach GitHub — check your connection" |
| Clone failure | "Clone failed: `<stderr>`" |

## State

```rust
struct ProjectTemplates {
    templates: Vec<TemplateRepo>,
    loading: bool,
    error: Option<SharedString>,
    focus_handle: FocusHandle,
}

struct TemplateRepo {
    name: SharedString,
    description: Option<SharedString>,
    visibility: Visibility,  // Public | Private
    language: Option<SharedString>,
    stars: u32,
    updated_at: SharedString,
    clone_url: SharedString,
}

enum Visibility {
    Public,
    Private,
}
```

## Paths

| Purpose | Path |
|---------|------|
| Templates directory | `~/workspaces/zig_workspace/templates/` |
| New projects | `~/workspaces/zig_workspace/<project-name>/` |

## Files to Create/Modify

### New files
- `crates/project_templates/Cargo.toml`
- `crates/project_templates/src/project_templates.rs` — main view (Item impl, rendering, gh CLI integration)
- `crates/project_templates/src/create_template_modal.rs` — ModalView for creating templates
- `crates/project_templates/src/use_template_modal.rs` — ModalView for using templates

### Modified files
- `Cargo.toml` (workspace) — add `project_templates` to members and dependencies
- `crates/zed/Cargo.toml` — add `project_templates` dependency
- `crates/zed/src/main.rs` — add `project_templates::init(cx)` call
- `crates/zed/src/zed.rs` — import, load, and register the item
- `crates/zed/src/zed/app_menus.rs` — add View menu entry
