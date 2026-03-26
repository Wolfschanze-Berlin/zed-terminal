# System Monitor Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a center-pane tab that runs btop (or a user-configured system monitor) inside an embedded terminal, with bundled binaries, auto-restart, and remote SSH support. Closes #27.

**Architecture:** New standalone `system_monitor` crate that registers a command palette action. The action spawns a `TerminalView` in the center pane via `TerminalPanel::add_center_terminal`, using `Project::create_terminal_task` for automatic remote routing. A GPUI `Global` enforces singleton behavior. A `script/download-btop` script handles binary bundling for releases.

**Tech Stack:** Rust, GPUI, alacritty_terminal (via existing `terminal`/`terminal_view` crates), settings system (`RegisterSetting` derive macro)

---

## File Structure

| File | Responsibility |
|------|---------------|
| **Create:** `crates/system_monitor/Cargo.toml` | Crate manifest with workspace dependencies |
| **Create:** `crates/system_monitor/src/system_monitor.rs` | Settings, global state, action handler, binary resolution, auto-restart |
| **Modify:** `Cargo.toml` (root) | Add `system_monitor` to workspace members and `[workspace.dependencies]` |
| **Modify:** `crates/zed/Cargo.toml` | Add `system_monitor` dependency |
| **Modify:** `crates/zed/src/main.rs` | Call `system_monitor::init(cx)` |
| **Modify:** `crates/settings_content/src/settings_content.rs` | Add `system_monitor` field to `SettingsContent` |
| **Create:** `crates/settings_content/src/system_monitor.rs` | Settings content struct for JSON schema |
| **Modify:** `crates/settings_content/Cargo.toml` | (only if new dependency needed — likely not) |
| **Create:** `script/download-btop` | Shell script to download platform-specific btop binaries |
| **Create:** `script/btop-version` | Pinned btop version |
| **Create:** `script/btop-checksums` | SHA256 checksums for verification |

---

### Task 1: Create the `system_monitor` crate skeleton

**Files:**
- Create: `crates/system_monitor/Cargo.toml`
- Create: `crates/system_monitor/src/system_monitor.rs`
- Modify: `Cargo.toml` (root, workspace members list ~line 91, workspace.dependencies ~line 340)

- [ ] **Step 1: Create `crates/system_monitor/Cargo.toml`**

```toml
[package]
name = "system_monitor"
version = "0.1.0"
edition.workspace = true
publish.workspace = true
license = "GPL-3.0-or-later"

[lints]
workspace = true

[lib]
name = "system_monitor"
path = "src/system_monitor.rs"

[dependencies]
anyhow.workspace = true
gpui.workspace = true
log.workspace = true
project.workspace = true
settings.workspace = true
settings_content.workspace = true
serde.workspace = true
task.workspace = true
terminal_view.workspace = true
ui.workspace = true
workspace.workspace = true
```

- [ ] **Step 2: Create minimal `crates/system_monitor/src/system_monitor.rs`**

```rust
use gpui::App;

pub fn init(_cx: &mut App) {
    // Will be filled in subsequent tasks
}
```

- [ ] **Step 3: Add to root `Cargo.toml` workspace members**

Add `"crates/system_monitor"` to the `members` array (alphabetically near `"crates/ssh_panel"`).

- [ ] **Step 4: Add to root `Cargo.toml` workspace dependencies**

Add to `[workspace.dependencies]` section (alphabetically near `ssh_panel`):

```toml
system_monitor = { path = "crates/system_monitor" }
```

- [ ] **Step 5: Verify the crate compiles**

Run: `rtk cargo check -p system_monitor`
Expected: successful compilation with no errors

- [ ] **Step 6: Commit**

```bash
rtk git add crates/system_monitor/Cargo.toml crates/system_monitor/src/system_monitor.rs Cargo.toml
rtk git commit -m "Add system_monitor crate skeleton"
```

---

### Task 2: Add `SystemMonitorSettings` to the settings system

**Files:**
- Create: `crates/settings_content/src/system_monitor.rs`
- Modify: `crates/settings_content/src/settings_content.rs` (~line 82, struct `SettingsContent`)
- Modify: `crates/settings_content/src/lib.rs` (add `mod system_monitor; pub use system_monitor::*;`)
- Modify: `crates/system_monitor/src/system_monitor.rs`

- [ ] **Step 1: Create `crates/settings_content/src/system_monitor.rs`**

This is the JSON-serializable settings content struct (the "schema" side of settings).

```rust
use serde::{Deserialize, Serialize};
use settings::Settings;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SystemMonitorSettingsContent {
    /// The command to run for the system monitor.
    ///
    /// Default: "btop"
    pub command: Option<String>,

    /// Arguments passed to the system monitor command.
    ///
    /// Default: []
    pub args: Option<Vec<String>>,

    /// Whether to automatically restart the monitor when it exits.
    ///
    /// Default: true
    pub auto_restart: Option<bool>,
}
```

- [ ] **Step 2: Add `system_monitor` field to `SettingsContent`**

In `crates/settings_content/src/settings_content.rs`, add the field to the `SettingsContent` struct (alphabetically near `terminal`):

```rust
    /// Configuration for the system monitor (btop).
    pub system_monitor: Option<SystemMonitorSettingsContent>,
```

- [ ] **Step 3: Add module declaration**

Check how `settings_content` exports its modules. In `crates/settings_content/src/lib.rs` (or equivalent), add:

```rust
mod system_monitor;
pub use system_monitor::*;
```

- [ ] **Step 4: Define `SystemMonitorSettings` in the `system_monitor` crate**

Update `crates/system_monitor/src/system_monitor.rs`:

```rust
use gpui::App;
use settings::{RegisterSetting, Settings};
use settings_content::SystemMonitorSettingsContent;

#[derive(Clone, Debug, RegisterSetting)]
pub struct SystemMonitorSettings {
    pub command: String,
    pub args: Vec<String>,
    pub auto_restart: bool,
}

impl Settings for SystemMonitorSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let settings = content.system_monitor.as_ref();
        SystemMonitorSettings {
            command: settings
                .and_then(|s| s.command.clone())
                .unwrap_or_else(|| "btop".to_string()),
            args: settings
                .and_then(|s| s.args.clone())
                .unwrap_or_default(),
            auto_restart: settings
                .and_then(|s| s.auto_restart)
                .unwrap_or(true),
        }
    }
}

pub fn init(_cx: &mut App) {
    // Will register actions in next task
}
```

- [ ] **Step 5: Verify compilation**

Run: `rtk cargo check -p system_monitor -p settings_content`
Expected: successful compilation

- [ ] **Step 6: Commit**

```bash
rtk git add crates/settings_content/src/system_monitor.rs crates/settings_content/src/settings_content.rs crates/settings_content/src/lib.rs crates/system_monitor/src/system_monitor.rs
rtk git commit -m "Add SystemMonitorSettings to settings system"
```

---

### Task 3: Implement the `OpenSystemMonitor` action and singleton state

**Files:**
- Modify: `crates/system_monitor/src/system_monitor.rs`

- [ ] **Step 1: Add action, global state, and binary resolution**

Replace the contents of `crates/system_monitor/src/system_monitor.rs` with the full implementation:

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use gpui::{Action, App, Context, Entity, Global, WeakEntity};
use settings::{RegisterSetting, Settings};
use settings_content::SystemMonitorSettingsContent;
use task::{RevealStrategy, RevealTarget, Shell, SpawnInTerminal, TaskId};
use terminal_view::{TerminalPanel, TerminalView};
use ui::prelude::*;
use workspace::Workspace;

actions!(system_monitor, [OpenSystemMonitor]);

#[derive(Clone, Debug, RegisterSetting)]
pub struct SystemMonitorSettings {
    pub command: String,
    pub args: Vec<String>,
    pub auto_restart: bool,
}

impl Settings for SystemMonitorSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let settings = content.system_monitor.as_ref();
        SystemMonitorSettings {
            command: settings
                .and_then(|s| s.command.clone())
                .unwrap_or_else(|| "btop".to_string()),
            args: settings
                .and_then(|s| s.args.clone())
                .unwrap_or_default(),
            auto_restart: settings
                .and_then(|s| s.auto_restart)
                .unwrap_or(true),
        }
    }
}

struct SystemMonitorState {
    active_view: Option<WeakEntity<TerminalView>>,
    restart_failures: Vec<Instant>,
}

impl Global for SystemMonitorState {}

impl SystemMonitorState {
    fn new() -> Self {
        Self {
            active_view: None,
            restart_failures: Vec::new(),
        }
    }

    fn should_restart(&mut self) -> bool {
        let now = Instant::now();
        // Remove failures older than 10 seconds
        self.restart_failures
            .retain(|t| now.duration_since(*t).as_secs() < 10);
        if self.restart_failures.len() >= 3 {
            return false;
        }
        self.restart_failures.push(now);
        true
    }

    fn reset_failures(&mut self) {
        self.restart_failures.clear();
    }
}

/// Resolve the btop command to the bundled binary path if available.
/// Falls back to the configured command name for PATH lookup.
fn resolve_command(command: &str) -> String {
    if command != "btop" {
        return command.to_string();
    }

    // Try to find bundled binary relative to the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        let bundled = if cfg!(target_os = "macos") {
            // <app_bundle>/Contents/MacOS/zed -> <app_bundle>/Contents/libexec/btop
            exe_path
                .parent() // MacOS/
                .and_then(|p| p.parent()) // Contents/
                .map(|p| p.join("libexec").join("btop"))
        } else if cfg!(target_os = "windows") {
            // <app_dir>/zed.exe -> <app_dir>/libexec/btop.exe
            exe_path
                .parent()
                .map(|p| p.join("libexec").join("btop.exe"))
        } else {
            // Linux: <app_dir>/zed -> <app_dir>/libexec/btop
            exe_path
                .parent()
                .map(|p| p.join("libexec").join("btop"))
        };

        if let Some(path) = bundled {
            if path.exists() {
                return path.to_string_lossy().to_string();
            }
        }
    }

    // Fallback to PATH lookup
    command.to_string()
}

fn open_system_monitor(
    workspace: &mut Workspace,
    _action: &OpenSystemMonitor,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Check for existing singleton
    if let Some(state) = cx.try_global::<SystemMonitorState>() {
        if let Some(weak_view) = &state.active_view {
            if let Some(view) = weak_view.upgrade() {
                // Focus the existing tab
                workspace.activate_item(&view, true, true, window, cx);
                return;
            }
        }
    }

    let settings = SystemMonitorSettings::get_global(cx);
    let command = resolve_command(&settings.command);
    let args = settings.args.clone();
    let auto_restart = settings.auto_restart;

    let spawn_task = SpawnInTerminal {
        id: TaskId("system-monitor".to_string()),
        full_label: "System Monitor".to_string(),
        label: "System Monitor".to_string(),
        command: Some(command.clone()),
        args: args.clone(),
        command_label: format!("{command} {}", args.join(" ")).trim().to_string(),
        cwd: None,
        env: HashMap::default(),
        use_new_terminal: true,
        allow_concurrent_runs: false,
        reveal: RevealStrategy::Always,
        reveal_target: RevealTarget::Center,
        hide: task::HideStrategy::Never,
        shell: Shell::System,
        show_summary: false,
        show_command: false,
        show_rerun: false,
        save: task::SaveStrategy::Nothing,
    };

    let task = TerminalPanel::add_center_terminal(workspace, window, cx, |project, cx| {
        project.create_terminal_task(spawn_task, cx)
    });

    cx.spawn_in(window, async move |workspace, cx| {
        let terminal_weak = task.await?;

        // Find the TerminalView that wraps this terminal
        workspace.update_in(cx, |workspace, window, cx| {
            // Search panes for the TerminalView with our terminal
            for pane in workspace.panes() {
                let pane = pane.read(cx);
                for item in pane.items() {
                    if let Some(terminal_view) = item.downcast::<TerminalView>() {
                        if terminal_view.read(cx).terminal().downgrade() == terminal_weak {
                            let weak_view = terminal_view.downgrade();

                            // Set custom title
                            terminal_view.update(cx, |view, cx| {
                                view.set_custom_title(Some("System Monitor".to_string()), cx);
                            });

                            // Store singleton state
                            if cx.has_global::<SystemMonitorState>() {
                                SystemMonitorState::update_global(cx, |state, _cx| {
                                    state.active_view = Some(weak_view);
                                    state.reset_failures();
                                });
                            } else {
                                let mut state = SystemMonitorState::new();
                                state.active_view = Some(weak_view);
                                cx.set_global(state);
                            }

                            return;
                        }
                    }
                }
            }
        })?;

        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}

pub fn init(cx: &mut App) {
    cx.set_global(SystemMonitorState::new());

    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(open_system_monitor);
    })
    .detach();
}
```

- [ ] **Step 2: Verify compilation**

Run: `rtk cargo check -p system_monitor`
Expected: successful compilation (there may be import issues to fix based on exact public API — resolve them)

- [ ] **Step 3: Commit**

```bash
rtk git add crates/system_monitor/src/system_monitor.rs
rtk git commit -m "Implement OpenSystemMonitor action with singleton and binary resolution"
```

---

### Task 4: Wire the crate into the main binary

**Files:**
- Modify: `crates/zed/Cargo.toml` (~line 156, near `ssh_panel`)
- Modify: `crates/zed/src/main.rs` (~line 688, near `ssh_panel::init`)

- [ ] **Step 1: Add dependency to `crates/zed/Cargo.toml`**

Add in the `[dependencies]` section (alphabetically near `ssh_panel`):

```toml
system_monitor.workspace = true
```

- [ ] **Step 2: Add `system_monitor::init(cx)` to `main.rs`**

In `crates/zed/src/main.rs`, after `ports_panel::init(cx);` (line 688), add:

```rust
        system_monitor::init(cx);
```

- [ ] **Step 3: Build the full binary**

Run: `rtk cargo check -p zed`
Expected: successful compilation

- [ ] **Step 4: Commit**

```bash
rtk git add crates/zed/Cargo.toml crates/zed/src/main.rs
rtk git commit -m "Wire system_monitor crate into zed binary"
```

---

### Task 5: Add auto-restart on terminal exit

**Files:**
- Modify: `crates/system_monitor/src/system_monitor.rs`

This task adds terminal exit subscription and auto-restart logic to the `open_system_monitor` function. The approach: after finding the `TerminalView`, subscribe to the terminal's exit event. On exit, if `auto_restart` is enabled and the circuit breaker allows it, spawn a new terminal task and replace the tab.

- [ ] **Step 1: Add exit subscription inside `open_system_monitor`**

In the `workspace.update_in` closure in `open_system_monitor`, after storing the singleton state, add a subscription to the terminal's completion event. This requires subscribing to the `Terminal` entity for its exit event.

Add the following after the `cx.set_global(state)` / `state.active_view = Some(weak_view)` block, still inside the same `workspace.update_in` closure:

```rust
                            // Subscribe to terminal exit for auto-restart
                            if auto_restart {
                                let workspace_handle = workspace.weak_handle();
                                cx.subscribe_in(
                                    &terminal_view.read(cx).terminal(),
                                    window,
                                    move |workspace, _terminal, event: &terminal::Event, window, cx| {
                                        if matches!(event, terminal::Event::CloseTerminal) {
                                            schedule_restart(
                                                workspace_handle.clone(),
                                                window,
                                                cx,
                                            );
                                        }
                                    },
                                )
                                .detach();
                            }
```

- [ ] **Step 2: Add the `reopen_system_monitor` function**

Add this function to `system_monitor.rs` (before `pub fn init`):

```rust
fn schedule_restart(
    workspace_handle: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Check circuit breaker
    let should_restart = SystemMonitorState::update_global(cx, |state, _cx| {
        state.should_restart()
    });

    if !should_restart {
        log::warn!("System monitor: too many restart failures, stopping auto-restart");
        return;
    }

    // Wait 1 second then reopen
    cx.spawn_in(window, async move |workspace, mut cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_secs(1))
            .await;

        workspace
            .update_in(&mut cx, |workspace, window, cx| {
                open_system_monitor(workspace, &OpenSystemMonitor, window, cx);
            })
            .log_err();
    })
    .detach();
}
```

- [ ] **Step 3: Add the `terminal` crate dependency**

Add to `crates/system_monitor/Cargo.toml` under `[dependencies]`:

```toml
terminal.workspace = true
```

Add the import at the top of `system_monitor.rs`:

```rust
use terminal;
```

- [ ] **Step 4: Verify compilation**

Run: `rtk cargo check -p system_monitor`
Expected: successful compilation (resolve any Event import issues — check `terminal::Event` or `terminal::TerminalEvent` for the correct type name)

- [ ] **Step 5: Commit**

```bash
rtk git add crates/system_monitor/src/system_monitor.rs crates/system_monitor/Cargo.toml
rtk git commit -m "Add auto-restart with circuit breaker for system monitor"
```

---

### Task 6: Create the `script/download-btop` bundling script

**Files:**
- Create: `script/download-btop`
- Create: `script/btop-version`

- [ ] **Step 1: Create `script/btop-version`**

```
1.4.0
```

- [ ] **Step 2: Create `script/download-btop`**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSION="${1:-$(cat "$SCRIPT_DIR/btop-version")}"
TARGET_DIR="${2:-$SCRIPT_DIR/../target/libexec}"

mkdir -p "$TARGET_DIR"

detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)
            case "$arch" in
                x86_64)  echo "linux-x86_64" ;;
                aarch64) echo "linux-aarch64" ;;
                *)       echo "Unsupported Linux architecture: $arch" >&2; exit 1 ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64)  echo "macos-x86_64" ;;
                arm64)   echo "macos-arm64" ;;
                *)       echo "Unsupported macOS architecture: $arch" >&2; exit 1 ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            echo "windows-x86_64"
            ;;
        *)
            echo "Unsupported OS: $os" >&2
            exit 1
            ;;
    esac
}

PLATFORM="$(detect_platform)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading btop v${VERSION} for ${PLATFORM}..."

case "$PLATFORM" in
    linux-x86_64)
        URL="https://github.com/aristocratos/btop/releases/download/v${VERSION}/btop-x86_64-linux-musl.tbz"
        curl -fsSL "$URL" -o "$TMPDIR/btop.tbz"
        tar -xjf "$TMPDIR/btop.tbz" -C "$TMPDIR"
        cp "$TMPDIR/btop/bin/btop" "$TARGET_DIR/btop"
        chmod +x "$TARGET_DIR/btop"
        ;;
    linux-aarch64)
        URL="https://github.com/aristocratos/btop/releases/download/v${VERSION}/btop-aarch64-linux-musl.tbz"
        curl -fsSL "$URL" -o "$TMPDIR/btop.tbz"
        tar -xjf "$TMPDIR/btop.tbz" -C "$TMPDIR"
        cp "$TMPDIR/btop/bin/btop" "$TARGET_DIR/btop"
        chmod +x "$TARGET_DIR/btop"
        ;;
    macos-x86_64)
        URL="https://github.com/aristocratos/btop/releases/download/v${VERSION}/btop-x86_64-macos.tbz"
        curl -fsSL "$URL" -o "$TMPDIR/btop.tbz"
        tar -xjf "$TMPDIR/btop.tbz" -C "$TMPDIR"
        cp "$TMPDIR/btop/bin/btop" "$TARGET_DIR/btop"
        chmod +x "$TARGET_DIR/btop"
        ;;
    macos-arm64)
        URL="https://github.com/aristocratos/btop/releases/download/v${VERSION}/btop-aarch64-macos.tbz"
        curl -fsSL "$URL" -o "$TMPDIR/btop.tbz"
        tar -xjf "$TMPDIR/btop.tbz" -C "$TMPDIR"
        cp "$TMPDIR/btop/bin/btop" "$TARGET_DIR/btop"
        chmod +x "$TARGET_DIR/btop"
        ;;
    windows-x86_64)
        # btop4win uses a different release URL pattern
        URL="https://github.com/aristocratos/btop4win/releases/download/v${VERSION}/btop4win-x86_64.zip"
        curl -fsSL "$URL" -o "$TMPDIR/btop4win.zip"
        unzip -q "$TMPDIR/btop4win.zip" -d "$TMPDIR/btop4win"
        cp "$TMPDIR/btop4win/btop4win.exe" "$TARGET_DIR/btop.exe"
        ;;
esac

echo "btop v${VERSION} installed to ${TARGET_DIR}"
```

- [ ] **Step 3: Make the script executable**

Run: `chmod +x script/download-btop`

- [ ] **Step 4: Commit**

```bash
rtk git add script/download-btop script/btop-version
rtk git commit -m "Add script/download-btop for bundling pre-built btop binaries"
```

---

### Task 7: Add THIRD_PARTY_LICENSES entry

**Files:**
- Modify: `THIRD_PARTY_LICENSES` (or create if it doesn't exist — check first)

- [ ] **Step 1: Check if the file exists**

Run: `ls -la THIRD_PARTY_LICENSES*` or `ls -la LICENSE*THIRD*`

- [ ] **Step 2: Add btop license entry**

If a `THIRD_PARTY_LICENSES` or equivalent file exists, append to it. If not, create `THIRD_PARTY_LICENSES.md`:

```markdown
## btop

- **Source:** https://github.com/aristocratos/btop
- **License:** GPL-3.0-only
- **Copyright:** Copyright (c) 2021 aristocratos
- **Used as:** Pre-built binary bundled in release artifacts (Linux, macOS)

## btop4win

- **Source:** https://github.com/aristocratos/btop4win
- **License:** GPL-3.0-only
- **Copyright:** Copyright (c) 2021 aristocratos
- **Used as:** Pre-built binary bundled in release artifacts (Windows)
```

- [ ] **Step 3: Commit**

```bash
rtk git add THIRD_PARTY_LICENSES.md
rtk git commit -m "Add btop to THIRD_PARTY_LICENSES"
```

---

### Task 8: Manual verification and smoke test

**Files:** None (verification only)

- [ ] **Step 1: Build the binary**

Run: `rtk cargo build -p zed`
Expected: successful build

- [ ] **Step 2: Run the application and test the command palette**

Run: `cargo run -p zed`

1. Open the command palette (Ctrl+Shift+P / Cmd+Shift+P)
2. Type "Open System Monitor"
3. Verify a new center-pane tab opens running btop (or shows "command not found" if btop is not on PATH)
4. Verify the tab title says "System Monitor"

- [ ] **Step 3: Test singleton behavior**

1. With the system monitor tab already open, open the command palette again
2. Type "Open System Monitor" again
3. Verify it focuses the existing tab instead of opening a second one

- [ ] **Step 4: Test auto-restart**

1. With the system monitor tab open, press `q` to quit btop
2. Verify the terminal restarts btop after ~1 second
3. Quit btop 3 more times rapidly
4. Verify auto-restart stops after the 3rd failure within 10 seconds

- [ ] **Step 5: Test configurable command**

Add to your settings.json:
```json
{
  "system_monitor": {
    "command": "top"
  }
}
```

1. Close the system monitor tab
2. Open "Open System Monitor" again
3. Verify it runs `top` instead of `btop`

- [ ] **Step 6: Commit any fixes from testing**

If any issues were found and fixed during testing, commit them:

```bash
rtk git add -A
rtk git commit -m "Fix issues found during system monitor smoke testing"
```

---

### Task 9: Run clippy and fix warnings

**Files:** Any files modified in previous tasks

- [ ] **Step 1: Run clippy**

Run: `./script/clippy`
Expected: no new warnings from `system_monitor` crate

- [ ] **Step 2: Fix any warnings**

Address all clippy warnings in the `system_monitor` crate.

- [ ] **Step 3: Run rustfmt**

Run: `cargo fmt -p system_monitor`

- [ ] **Step 4: Commit fixes**

```bash
rtk git add crates/system_monitor/
rtk git commit -m "Fix clippy warnings and format system_monitor crate"
```
