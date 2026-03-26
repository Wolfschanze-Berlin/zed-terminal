---
generated_by: verrueckt-cortex
generated_at: "2026-03-26"
git_hash: fd20b20ad5
scan_type: bootstrap
---

# Project Context: Zed Terminal

## Identity

| Field | Value |
|-------|-------|
| Name | zed-terminal |
| Type | Desktop application (terminal-first fork of Zed editor) |
| Language | Rust (edition 2024) |
| Toolchain | Rust 1.93 (stable), minimal profile + rustfmt, clippy, rust-analyzer |
| Framework | GPUI (custom GPU-accelerated UI framework) |
| Terminal backend | alacritty_terminal |
| Repository | git@github.com:Wolfschanze-Berlin/zed-terminal.git |
| License | GPL-3.0-or-later (main), AGPL (some crates), Apache (libraries) |

## Vision

GPU-accelerated, terminal-first application built on Zed's GPUI framework. Terminals tile in the center pane (iTerm2-style). Full Zed editor available as split panes alongside terminals. All AI code-generation and collaboration features are stripped. Enhanced SSH with ports panel, connection manager, and remote file browsing. GitHub integration and QMD knowledge layer planned for later phases.

## Architecture

### Layout

```
Title Bar (minimal)
├── Left Dock: [Project Panel] [Git] [History]
├── Center: Terminal Tiles (iTerm2-style splits, editor panes alongside)
├── Right Dock: [SSH Manager] [Ports] [GitHub] [Analytics]
└── Status Bar (connection status, port forwards)
```

### Approach

**Surgical Removal** of AI/collab crates from upstream Zed. The Panel trait is fully decoupled; AI dependency graph flows one-way (`zed -> agent_ui -> agent -> language_model`). No reverse deps from core. New features slot in as standard Panel implementations.

### Crate Organization

- **Total crates in workspace:** ~155 active members (down from ~225 upstream)
- **Total crate directories:** 227 (includes ~70 stripped but not deleted crates)
- **Default member:** `crates/zed` (main binary)
- **Entry point:** `crates/zed/src/main.rs`

### Key Crate Domains

| Domain | Crates | Notes |
|--------|--------|-------|
| **Core UI** | `gpui`, `gpui_platform`, `gpui_windows`, `gpui_wgpu`, `gpui_macros`, `gpui_util` | Custom GPU-accelerated UI framework |
| **Terminal** | `terminal`, `terminal_view` | alacritty_terminal backend, terminal element rendering |
| **SSH/Remote** | `ssh_panel`, `ports_panel`, `remote`, `remote_connection`, `remote_server` | New panels (scaffolded), plus upstream remote support |
| **Editor** | `editor`, `multi_buffer`, `language`, `lsp`, `snippet`, `snippet_provider` | Full editor retained as split panes |
| **Workspace** | `workspace`, `sidebar`, `panel`, `title_bar`, `platform_title_bar` | Dock/panel system, window management |
| **Git** | `git`, `git_ui`, `git_graph`, `git_hosting_providers` | Git integration retained |
| **Project** | `project`, `project_panel`, `worktree`, `fs` | File system, project tree |
| **Settings** | `settings`, `settings_ui`, `settings_json`, `settings_macros` | Configuration system |
| **Search** | `search`, `file_finder`, `fuzzy`, `outline`, `project_symbols` | Code navigation |
| **Database** | `db`, `sqlez`, `sqlez_macros` | SQLite-based persistence |
| **Diagnostics** | `diagnostics`, `language_tools` | LSP diagnostics |
| **Theme** | `theme`, `theme_selector`, `theme_extension`, `ui`, `icons` | Theming system |
| **Extensions** | `extension`, `extension_host`, `extensions_ui`, `extension_api` | Extension system retained |

### Stripped Crate Categories (~70 crates removed from workspace)

- **AI/Agent System (33):** agent, copilot, language_model, edit_prediction, etc.
- **Collaboration (6):** collab, call, channel, livekit
- **Editor-heavy (15+):** outline_panel, debugger_ui, repl, vim, storybook
- **Other:** Various AI provider crates (anthropic, openai, bedrock, etc.)

## Build System

| Tool | Details |
|------|---------|
| Build | `cargo build` (default member: zed) |
| Clippy | `./script/clippy` (NOT `cargo clippy` directly) |
| Test | `cargo test` with `test-support` feature flags |
| Formatter | `rustfmt` (workspace config in `rustfmt.toml`) |
| JS/TS Runtime | `bun --bun` (NEVER use `node` — always `bun --bun` for all JS/TS execution) |
| CI | Scripts in `script/` directory |
| Containerization | Docker (collab server), Nix flake |

## Development Phases

### Phase 1: Strip & Optimize (IN PROGRESS)
- Remove ~75 AI/collab/editor-heavy crates (**DONE**)
- Clean up compile errors in kept crates (**DONE**)
- Apply startup optimizations and terminal rendering perf fixes (**DONE**)
- Add tmux-style split/navigate keybindings (**DONE**)
- Open terminal in center pane on startup (**DONE**)
- Scaffold `ssh_panel` and `ports_panel` crates (**DONE**)

### Phase 2+ (PLANNED)
- SSH panel: connection manager, auto-forwarding, persistent tunnels
- Ports panel: port forwarding UI
- GitHub integration: activity feeds, PR status, issue tracking
- QMD knowledge layer: semantic search over terminal history
- Agentlytics dashboard: read-only AI tool usage tracking

## Key Design Decisions

1. **Modular crates:** All new crates must be standalone modules with single-line enable/disable in Cargo.toml
2. **No AI code-gen:** All AI code generation and inline assistance stripped; only retained AI dependency is OpenAI embeddings API for QMD search (future)
3. **Panel architecture:** New features implemented as `Panel` trait implementations in existing dock system
4. **Terminal-first:** Terminal opens in center pane on startup (not editor); tmux-style keybindings for split/navigate

## Platform Support

- **Primary target:** Windows (current development platform)
- **Upstream targets:** macOS, Linux (X11 + Wayland), Web (wasm32)
- **WASM targets:** `wasm32-wasip2` (extensions), `wasm32-unknown-unknown` (web)
- **Cross-compilation:** `x86_64-unknown-linux-musl` (remote server)

## Specs & Plans

- Design spec: `docs/superpowers/specs/2026-03-24-zed-terminal-design.md`
- Phase 1 plan: `docs/superpowers/plans/2026-03-24-phase1-strip-and-optimize.md`
