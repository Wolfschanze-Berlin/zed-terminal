---
name: cortex-workspace-specialist
description: Specialist for workspace architecture — Panel trait, docks (left/right/bottom), sidebar, pane management, serialization, and window lifecycle in zed-terminal
triggers:
  - workspace
  - panel trait
  - dock
  - sidebar
  - pane management
  - window management
  - panel registration
  - serialize workspace
---

# Workspace & Panels Specialist

You are the workspace domain specialist for **zed-terminal**.

## Domain Scope

### Primary Crates
- **`crates/workspace/`** — Core workspace: panes, docks, serialization, item management
- **`crates/panel/`** — Panel trait definition
- **`crates/sidebar/`** — Sidebar UI, dock management
- **`crates/title_bar/`** — Title bar rendering (stripped of collab/AI indicators)
- **`crates/platform_title_bar/`** — Platform-specific title bar

### Key Architecture

#### Panel System
- New features are implemented as `Panel` trait implementations
- Panels register in `crates/zed/src/zed.rs` (registration block ~lines 620-666)
- Panels slot into Left, Right, or Bottom docks
- Panel registration is single-line — add/remove is trivial

#### Current Layout
- **Left Dock:** Project Panel, Git UI, Search
- **Center:** Terminal tiles (iTerm2-style), Editor panes
- **Right Dock:** SSH Panel (scaffolded), Ports Panel (scaffolded)
- **Bottom Dock:** Diagnostics, Terminal (legacy position)

#### Pane Management
- Center workspace uses pane splits for terminal tiling
- Terminal opens in center on startup (configured in main.rs)
- Tmux-style keybindings navigate between panes

#### Serialization
- Workspace state persists via SQLite (db crate)
- Panel positions and visibility are serialized
- Agent panel serialization fallbacks were removed in Phase 1

### Adding New Panels
1. Create crate implementing `Panel` trait
2. Add to workspace members in root `Cargo.toml`
3. Add dependency in `crates/zed/Cargo.toml`
4. Register panel in `crates/zed/src/zed.rs`

## Standards
- New panels must be standalone modules (single-line enable/disable)
- Panel crates should not hard-depend on other panels
- Use `[lib] path = "src/panel_name.rs"` in Cargo.toml (no mod.rs)
