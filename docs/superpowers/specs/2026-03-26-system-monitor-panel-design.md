# System Monitor Panel Design

## Overview

A center-pane tab that runs btop (or a user-configured system monitor command) inside an embedded terminal. Bundled pre-built btop binaries ship with the app for Windows, macOS, and Linux. When connected to a remote server via SSH, the monitor runs on the remote — showing remote resources, not local.

## Requirements

- Opens as a tab in the center pane (alongside terminal tiles), not a dock panel
- Launched via command palette: "Open System Monitor"
- No default keybinding (users can bind their own)
- Singleton: only one system monitor tab at a time; re-opening focuses the existing tab
- Auto-restarts on exit/crash with circuit breaker (3 failures in 10s stops retrying)
- Configurable command defaults to bundled btop; users can override to htop/top/gotop/etc.
- Remote-aware: uses existing `Project::create_terminal_task` which routes to remote server when SSH is active

## Architecture

### New Crate: `system_monitor`

```
crates/system_monitor/
├── Cargo.toml
└── src/
    └── system_monitor.rs
```

**Dependencies:** `gpui`, `ui`, `workspace`, `terminal`, `terminal_view`, `project`, `settings`, `serde`, `anyhow`, `log`

The crate does not implement the `Panel` trait. It registers a command palette action (`OpenSystemMonitor`) that spawns a `TerminalView` in the center pane via `TerminalPanel::add_center_terminal`, configured with the system monitor command.

### Core Components

**`SystemMonitorSettings`** — registered via the settings system:

```json
{
  "system_monitor": {
    "command": "btop",
    "args": [],
    "auto_restart": true
  }
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | `String` | `"btop"` | Executable name or path. Default resolves to bundled binary; override uses PATH lookup. |
| `args` | `Vec<String>` | `[]` | Arguments passed to the command. |
| `auto_restart` | `bool` | `true` | Restart the process on exit/crash. |

**`SystemMonitorState`** — a GPUI `Global` tracking the active tab:

- Holds a `WeakEntity<TerminalView>` pointing to the current system monitor tab (if any)
- When `OpenSystemMonitor` fires: check if the weak entity is still alive and focus it; otherwise create a new one

**Actions:**

- `system_monitor::OpenSystemMonitor` — opens or focuses the system monitor tab

**`init(cx: &mut App)`** — registers the action on `Workspace` via `cx.observe_new`:

1. Registers `OpenSystemMonitor` action handler
2. Registers `SystemMonitorSettings` with the settings system

### Command Resolution

When creating the terminal, the command is resolved as follows:

1. Read `SystemMonitorSettings` for the configured command and args
2. If command is `"btop"` (default), resolve to the bundled binary path:
   - macOS: `<app_bundle>/Contents/libexec/btop`
   - Linux: `<app_dir>/libexec/btop`
   - Windows: `<app_dir>/libexec/btop.exe`
3. If the bundled binary doesn't exist (dev builds), fall back to PATH lookup for `"btop"`
4. If command is anything other than `"btop"`, use it as-is (PATH lookup)
5. Pass the resolved command as `Shell::WithArguments { program, args, title_override: Some("System Monitor") }` via a `SpawnInTerminal` struct

### Remote SSH Behavior

No special handling needed. The command is passed through `Project::create_terminal_task`, which checks `self.remote_client.is_some()`. When an SSH connection is active, the command executes on the remote server.

On remote servers, the bundled binary resolution is skipped — the command runs as-is on the remote PATH. If btop is not installed on the remote, the user sees the shell's "command not found" error and can either install btop on the remote or configure an alternative command.

### Auto-Restart

When `auto_restart` is enabled:

1. Subscribe to the terminal's exit event
2. On exit, wait 1 second, then spawn a new `Terminal` entity with the same command and replace the existing terminal in the `TerminalView` (the tab itself stays in place — no close/reopen flicker)
3. Track consecutive failures: if 3 restarts fail within 10 seconds, stop retrying and let the error remain visible in the terminal output
4. Reset the failure counter when the process runs successfully for more than 10 seconds

### Tab Behavior

| Property | Value |
|----------|-------|
| Title | "System Monitor" |
| Icon | `IconName::Pulse` (or nearest system/activity icon) |
| Closable | Yes (kills the process) |
| Draggable | Yes (standard pane tab behavior) |
| Splittable | Yes (can be split alongside terminals) |
| Persisted | No (not saved/restored across sessions) |

## Binary Bundling

### Source Repositories

| Platform | Repository | Binary |
|----------|-----------|--------|
| Linux (x86_64) | [aristocratos/btop](https://github.com/aristocratos/btop/releases) | `btop-x86_64-linux-musl.tbz` |
| macOS (universal) | [aristocratos/btop](https://github.com/aristocratos/btop/releases) | macOS release archive |
| Windows (x86_64) | [aristocratos/btop4win](https://github.com/aristocratos/btop4win) | `btop4win-x86_64.zip` |

### Download Script

`script/download-btop` — a shell script that:

1. Accepts a version argument (e.g. `v1.4.0`) or reads from a pinned version file
2. Downloads the correct binary for the current (or target) platform
3. Extracts and places it in `target/libexec/btop[.exe]`
4. Verifies checksum against a committed checksums file
5. Runs as part of release packaging, not during `cargo build`

During development, the bundled binary won't exist. The crate falls back to PATH lookup.

### Version Pinning

A file `script/btop-version` contains the pinned version (e.g. `1.4.0`). Updating this file and re-running the download script is all that's needed to upgrade.

### License Compliance

btop is GPL-3.0-only. zed-terminal is GPL-3.0-or-later. Bundling is license-compatible. A `THIRD_PARTY_LICENSES` entry documents:

- btop (Linux/macOS): GPL-3.0-only, Copyright aristocratos
- btop4win (Windows): GPL-3.0-only, Copyright aristocratos

## Integration Points

### `crates/zed/src/main.rs`

Add to init sequence:
```rust
system_monitor::init(cx);
```

### `crates/zed/Cargo.toml`

Add workspace dependency:
```toml
system_monitor.workspace = true
```

Single-line disable by commenting out (per project convention for modular crates).

### No changes to:

- `initialize_panels` in `zed.rs` (not a dock panel)
- Sidebar icons (command palette only)
- Keymap defaults (no default binding)

## Testing Strategy

- **Unit tests:** Settings deserialization, bundled binary path resolution per platform, circuit breaker logic (failure counting and reset)
- **Integration tests:** Action registration, singleton enforcement (open twice = one tab), auto-restart on simulated exit
- **Manual testing:** Verify btop renders correctly in the terminal tab on each platform, verify remote SSH routes to remote server
