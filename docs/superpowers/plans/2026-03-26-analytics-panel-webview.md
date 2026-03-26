# Plan: Analytics Panel with Embedded WebView (agentsview)

## Goal

Replace the native GPUI analytics dashboard with an embedded WebView2 that hosts
[agentsview](https://github.com/wesm/agentsview) — a full-featured AI agent session
browser. The agentsview Go binary runs as a sidecar process on localhost; a WebView2
child window renders its Svelte frontend inside a GPUI center-pane Item.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  GPUI Window (HWND)                                 │
│  ┌───────────────────────────────────────────────┐  │
│  │  Center Pane Tab: "Agentlytics"               │  │
│  │  ┌─────────────────────────────────────────┐  │  │
│  │  │  WebView2 child HWND                    │  │  │
│  │  │  ┌───────────────────────────────────┐  │  │  │
│  │  │  │  agentsview frontend              │  │  │  │
│  │  │  │  http://127.0.0.1:{port}          │  │  │  │
│  │  │  └───────────────────────────────────┘  │  │  │
│  │  └─────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
         ↕ HTTP
┌─────────────────┐
│ agentsview.exe  │  (sidecar, spawned on panel open)
│ Go binary       │  127.0.0.1:{auto-port}
│ SQLite + sync   │  Reads ~/.claude, ~/.cursor, etc.
└─────────────────┘
```

## Layers

### Layer 1: `crates/webview` — WebView2 embedding for GPUI (Windows)

A reusable crate that wraps WebView2 as a GPUI Element. This is the hard
infrastructure work that other panels can reuse later.

**Approach** (following wry's pattern):
1. Create intermediate container HWND (`WS_CHILD | WS_CLIPCHILDREN`)
2. Init WebView2 environment via `webview2-com` crate
3. Create controller parented to container HWND
4. Expose `WebViewElement` that:
   - Reserves layout space via GPUI's flexbox
   - Positions the container HWND to match element bounds during paint
   - Manages visibility (hide when tab is inactive)
   - Forwards focus in/out

**Key API:**
```rust
pub struct WebView {
    // Owns the container HWND, controller, and ICoreWebView2
}

impl WebView {
    pub fn new(parent_hwnd: HWND, url: &str, cx: &mut App) -> Result<Self>;
    pub fn navigate(&self, url: &str) -> Result<()>;
    pub fn set_bounds(&self, bounds: Bounds<Pixels>) -> Result<()>;
    pub fn set_visible(&self, visible: bool) -> Result<()>;
    pub fn focus(&self) -> Result<()>;
}
```

**Dependencies:** `webview2-com`, `windows` (Win32 APIs)

### Layer 2: `crates/sidecar` — Process lifecycle management

Generic sidecar process manager. Spawns a child process, detects port readiness,
and kills on drop.

```rust
pub struct Sidecar {
    child: Child,
    port: u16,
}

impl Sidecar {
    pub async fn spawn(binary: &Path, args: &[&str]) -> Result<Self>;
    pub fn port(&self) -> u16;
    pub fn url(&self) -> String; // http://127.0.0.1:{port}
}

impl Drop for Sidecar {
    fn drop(&mut self) { /* kill child */ }
}
```

### Layer 3: `crates/analytics_panel` — The center-pane Item (already exists)

Refactor to:
1. On open: spawn agentsview sidecar, wait for port
2. Create WebView pointing at `http://127.0.0.1:{port}`
3. Show loading state while sidecar starts
4. On close/drop: kill sidecar

### Layer 4: agentsview binary bundling

Ship the pre-built `agentsview.exe` in the app's resources directory.
Build script downloads or copies it during `cargo build`.

## Implementation Order

### Step 1: WebView2 HWND creation + basic rendering
- Register window class, create container HWND
- Init WebView2 environment and controller
- Navigate to a URL, verify it renders
- No GPUI integration yet — just a standalone test

### Step 2: GPUI Element integration
- Implement `Element` trait for `WebViewElement`
- Track bounds from `request_layout` / `prepaint` / `paint`
- Reposition container HWND to match element bounds
- Handle visibility (show/hide when tab changes)

### Step 3: Focus management
- Forward focus to WebView2 on click / tab-into
- Return focus to GPUI on click-outside / Escape
- Handle WM_SETFOCUS on container HWND

### Step 4: Sidecar process manager
- Spawn agentsview binary with auto-port
- Poll TCP until ready (125ms intervals, 30s timeout)
- Kill on drop
- Handle binary-not-found gracefully

### Step 5: Wire analytics_panel to WebView + sidecar
- Remove native GPUI charts
- Add sidecar spawn on panel open
- Create WebView element pointing at sidecar URL
- Loading state while sidecar starts
- Fallback UI if binary not found

### Step 6: Binary bundling + build integration
- Download agentsview release binary during build
- Place in app resources directory
- Detect path at runtime

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `webview2-com` | 0.38+ | WebView2 COM bindings |
| `windows` | 0.61+ | Win32 APIs (HWND, messages) |
| `raw-window-handle` | 0.6 | Already in workspace |

## Risks

1. **Message loop conflict**: WebView2 init pumps Win32 messages via
   `wait_with_pump`. GPUI also owns the message loop. Wry does this
   successfully, but we need to verify it doesn't deadlock with GPUI.

2. **Z-order**: WebView2 is a native HWND that sits on top of GPUI's
   DirectX surface. GPUI popups/menus cannot overlay it. Acceptable
   for a center-pane tab (no overlapping UI).

3. **DPI scaling**: Must match GPUI's scale factor when setting bounds.

4. **Platform scope**: Windows-only initially. macOS (WKWebView) and
   Linux (WebKitGTK) would be separate implementations behind the
   same trait.

## Out of Scope

- Cross-platform webview (macOS/Linux) — future work
- Generic webview extension API (#26) — this is a concrete panel, not a platform
- IPC bridge / theme injection (#20, #23) — not needed for agentsview
- Security sandbox (#24) — agentsview runs as localhost, trusted
