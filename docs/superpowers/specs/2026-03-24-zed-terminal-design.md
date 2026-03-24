# Zed Terminal — Terminal-First Fork Design Spec

## Vision

Zed Terminal is a GPU-accelerated, terminal-first application built on Zed's GPUI framework. Terminals tile in the center pane (iTerm2-style). The full Zed editor is available as split panes alongside terminals. All AI/collaboration features are stripped. Enhanced SSH with a ports panel, connection manager, auto-forwarding, persistent tunnels, and remote file browsing. A read-only Agentlytics-style dashboard tracks external AI tool usage. GitHub integration provides activity feeds, PR status, and issue tracking. A Rust-native QMD knowledge layer powered by OpenAI embeddings enables semantic search across terminal history, SSH notes, and workspace metadata.

## Approach

**Surgical Removal (Approach 1):** Keep the existing workspace architecture intact. Remove AI/collab crates from `Cargo.toml` workspace members, remove initialization calls in `main.rs` and `zed.rs`, remove panel registrations. Build new features as Panel trait implementations in existing docks.

**Rationale:**
- The Panel trait is fully decoupled — removing panels requires ~20 lines changed in `zed.rs:620-666`
- AI dependency graph flows one-way: `zed` -> `agent_ui` -> `agent` -> `language_model`. No reverse deps from core.
- The `disable_ai` setting (`project::DisableAiSettings`) already proves the codebase was designed for this separation.
- New features slot into the dock system as standard Panel implementations.

---

## 1. Architecture

### 1.1 Target Layout

```
┌─────────────────────────────────────────────────┐
│ Title Bar (minimal)                             │
├────────┬─────────────────────┬──────────────────┤
│ Left   │ Terminal Tiles      │ Right            │
│ Dock   │ (iTerm2-style)      │ Dock             │
│        │ ┌──────┬──────┐    │                  │
│ [Proj] │ │ T1   │ T2   │    │ [SSH Manager]    │
│ [Git]  │ │      │      │    │ [Ports]          │
│ [Hist] │ ├──────┴──────┤    │ [GitHub]         │
│        │ │ T3 (editor) │    │ [Analytics]      │
│        │ └─────────────┘    │                  │
├────────┴─────────────────────┴──────────────────┤
│ Status Bar (connection status, port forwards)   │
└─────────────────────────────────────────────────┘
```

### 1.2 Crates to Remove (~75 crates)

**AI/Agent System (33 crates):**
- agent, agent_servers, agent_settings, agent_ui
- acp_thread, acp_tools, ai_onboarding
- anthropic, bedrock, cloud_llm_client, codestral
- copilot, copilot_chat, copilot_ui
- deepseek, google_ai, lmstudio, mistral, ollama, open_router, x_ai, vercel, opencode
- language_model, language_models
- edit_prediction, edit_prediction_cli, edit_prediction_context, edit_prediction_types, edit_prediction_ui
- web_search, web_search_providers

**Collaboration (6 crates):**
- collab, collab_ui, call, channel, livekit_api, livekit_client

**Editor-heavy features (15+ crates):**
- outline_panel, debugger_ui, dap, dap_adapters, debug_adapter_extension
- repl, vim, vim_mode_setting
- component, component_preview, storybook, story
- zeta_prompt, prompt_store, rules_library

**Other removals (~20 crates):**
- assistant_slash_command, assistant_slash_commands, assistant_text_thread
- context_server, eval, eval_cli, eval_utils
- schema_generator, docs_preprocessor
- feedback, onboarding, language_onboarding
- streaming_diff, action_log, audio

**Impact:**
- Compile time: ~40% reduction
- Binary size: ~30-40% reduction
- Startup memory: -50-100MB
- Startup time: -50-100ms (fewer init() calls)

### 1.3 Crates to Keep (~150 crates)

- **Core:** gpui, gpui_*, workspace, editor, project, worktree, settings, theme, ui
- **Terminal:** terminal, terminal_view (enhanced)
- **Git:** git, git_ui, git_hosting_providers
- **SSH/Remote:** remote, remote_connection, remote_server, askpass, recent_projects
- **Extensions:** extension, extension_api, extension_host, extensions_ui (languages + themes)
- **Infrastructure:** fs, lsp, language, languages, task, paths, cli, client, http_client, etc.

### 1.4 Crates to Add (~7 new crates)

| Crate | Purpose | Dock Position |
|-------|---------|---------------|
| ssh_panel | SSH connection manager | Right |
| ports_panel | Port forwarding management | Right |
| analytics_panel | Agentlytics-style AI usage dashboard | Right |
| history_panel | Folder history + session restore + bookmarks | Left |
| github_panel | GitHub activity feed, PRs, issues | Right |
| qmd_store | Rust-native vector DB (Tantivy + sqlite-vec + OpenAI embeddings) | N/A (library) |

---

## 2. Performance Optimizations

### 2.1 Terminal Rendering

**Current state:** ~42-56ms per frame for a 200x120 terminal (needs <16.7ms for 60fps).

**Root cause analysis (from line-by-line code review):**

| Rank | Bottleneck | Per-Frame Cost | File:Line |
|------|-----------|---------------|-----------|
| #1 | `paint_glyph()` called 24,000x sequentially | ~24ms | window.rs:3332 |
| #2 | Text shaping per batch (2,400 calls, cached on reuse) | 8-12ms (2ms cached) | terminal_element.rs:150 |
| #3 | O(n^3) background region merging (nested while + Vec::remove) | 5-10ms | terminal_element.rs:251-280 |
| #4 | Full grid cell cloning in make_content() | 5-10ms | terminal.rs:1635-1638 |
| #5 | Cell clones in viewport filtering (.cloned()) | 5-10ms | terminal_element.rs:1107 |
| #6 | Cursor blink forces full terminal redraw | 2 extra frames/sec | terminal_view.rs:268 |

**Tier 1 fixes (week 1, ~10-18ms savings):**

1. **Replace O(n^3) background merge with O(n log n) sort+sweep** (`terminal_element.rs:251-280`)
   - Current: nested while loops with `Vec::remove()` causing O(n) shifts per removal
   - Fix: sort regions by position, single-pass merge adjacent. 5-10ms -> <1ms.

2. **Stop cloning cells in viewport filter** (`terminal_element.rs:1107`)
   - Current: `.cloned()` on visible cell iterator
   - Fix: pass `&IndexedCell` references to `layout_grid()`. 5-10ms -> <1ms.

3. **Pre-allocate larger String capacity** (`terminal_element.rs:99`)
   - Current: `String::with_capacity(100)` per BatchedTextRun
   - Fix: capacity 256 (typical line length). Minor but cumulative.

**Tier 2 fixes (weeks 2-3, ~15-25ms savings):**

4. **Dirty-line tracking** (`terminal.rs` + `terminal_element.rs`)
   - Current: iterate all 24,000 visible cells every frame even if 1 line changed
   - Fix: track which terminal lines changed since last frame (alacritty has damage info). Only re-layout/paint changed lines. 80% reduction in typical use.

5. **Leverage GPUI's LineLayout cache** (`terminal_element.rs:150`)
   - Current: `shape_line()` called per batch, cache exists but terminal rebuilds all runs
   - Fix: with dirty-line tracking, unchanged lines hit the 2-tier cache (current frame + previous frame). 2,400 calls -> ~200 for typical editing.

6. **Cursor-only paint path** (`terminal_view.rs:268`)
   - Current: `cx.observe(&blink_manager, |_, _, cx| cx.notify())` triggers full redraw
   - Fix: paint cursor as overlay quad, skip full layout_grid() on blink.

7. **Reference-counted grid cells** (`terminal.rs:1635`)
   - Current: `ic.cell.clone()` for every cell in `make_content()`
   - Fix: use `Cow<Cell>` or `Arc<Cell>` to avoid deep cloning.

**Tier 3 fixes (weeks 4+, ~19ms savings):**

8. **Batch paint_glyph() loop** (`window.rs:3332-3399`)
   - Current: 24,000 sequential calls, each doing subpixel computation + atlas lookup + sprite insertion
   - Fix: pre-compute all glyph positions in a batch, insert sprites in bulk. ~24ms -> ~5ms.

**Target:** <10ms per frame for 200x120 terminal (60fps with headroom).

### 2.2 SSH/Remote Performance

| Fix | File:Line | Impact |
|-----|-----------|--------|
| Add zstd compression to RPC wire protocol | protocol.rs:43-49 | 15-30% bandwidth reduction |
| BufReader for message reads (eliminate double syscall) | transport.rs:98-106 | Fewer syscalls per message |
| Message batching with 1ms coalescing window | remote_client.rs:1686-1692 | Fewer TCP packets |
| Windows: persistent SSH connection pool | ssh.rs:204-254 | Eliminate 200-500ms per command |
| Configurable idle timeout (currently hardcoded 10min) | server.rs:301 | User control |
| Batch flush on server (currently flush per message) | server.rs:405 | Better TCP coalescing |
| Scale binary upload timeout with file size | ssh.rs:797-802 | Prevent timeout on slow networks |

**Windows SSH critical fix:**
Windows OpenSSH lacks ControlMaster (Win32-OpenSSH#405). Current workaround spawns new SSH connection per command (~200-500ms latency). Fix: implement a persistent connection pool that keeps SSH subprocesses alive and reuses them for subsequent commands.

### 2.3 Startup Performance

**Current:** 200-800ms before window appears. **Target:** <100ms.

| Fix | File:Line | Time Saved | Memory Saved |
|-----|-----------|------------|--------------|
| Remove 33 AI crate init() calls | main.rs:657-688 | -50-100ms | -50MB |
| Remove 6 collab crate init() calls | main.rs:737 | -10-20ms | -20MB |
| Defer block_on for telemetry IDs | main.rs:576-578 | -100-200ms | 0 |
| Lazy-load built-in themes (keep active only) | main.rs:642 | -50-100ms | -20MB |
| Reduce Rayon thread stack 10MB -> 2MB | main.rs:307 | 0 | -40-80MB |
| Reduce default scrollback 10k -> 5k lines | terminal.rs:342 | 0 | -80MB/terminal |
| Lazy-load extension host | main.rs:508 | -20-50ms | -20MB |

**Total potential:** -230-570ms startup, -230-270MB RAM.

---

## 3. New Features

### 3.1 Terminal-First Tiling Layout

**Leverages existing infrastructure:** `PaneGroup` already supports recursive H/V splits. `TerminalPanel` already has `center: PaneGroup` field.

**Changes:**
- On startup, center pane opens terminal (not editor welcome screen)
- Keyboard splitting: `Ctrl+D` horizontal, `Ctrl+Shift+D` vertical
- Pane navigation: `Ctrl+[arrow]` move focus, `Ctrl+Shift+[arrow]` resize
- Opening files creates a split pane alongside terminals
- Full layout persistence via existing `persistence.rs` serialization

**Implementation:** Modify `initialize_workspace()` in `zed.rs` to spawn terminal in center pane on new workspace. Add keybindings for split/navigate/resize actions.

### 3.2 SSH Connection Manager Panel

New crate: `ssh_panel` implementing `Panel` trait. Right dock.

**Features:**
- Connection list with status indicators (connected/disconnected/error)
- Quick connect with auto-complete from `~/.ssh/config` (parsed by `recent_projects/src/ssh_config.rs`)
- Connection groups (project/environment)
- One-click connect -> opens remote terminal in center pane
- Real-time latency indicator from heartbeat data
- Inline config editing (port, user, key, jump hosts)

**Data:** Settings stored in `~/.config/zed-terminal/ssh_connections.json`. Connection state from `RemoteClient` entity observation.

### 3.3 Ports Panel

New crate: `ports_panel` implementing `Panel` trait. Right dock.

**Features:**
- Active forwards list with local:remote mapping
- Add/remove forwards without reconnecting
- Auto-detect: periodic `ss -tlnp` over SSH RPC, prompt to forward new listeners
- Persistent tunnels: survive reconnection, stored in workspace settings
- Status indicators: green (active), yellow (connecting), red (failed)
- One-click open forwarded HTTP port in browser

**Implementation:**
- Extend `SshConnectionOptions.port_forwards` to be mutable at runtime
- Use existing `build_forward_ports_command()` (`remote_client.rs:1363`)
- Auto-detect via periodic remote command execution
- Persistent forwards stored in workspace DB

### 3.4 Folder History + Session Restore + Bookmarks Panel

New crate: `history_panel` implementing `Panel` trait. Left dock.

**Features:**
- Recent folders: last opened date, SSH host badge, path, frequency
- Session restore: full terminal layout, open files, SSH connections, port forwards
- Bookmarks: pinned folders (local + remote) with custom names
- Quick switch: fuzzy search across history and bookmarks
- Metadata: commands run, time spent, last activity

**Storage:** SQLite via existing `WorkspaceDb`/`sqlez` pattern.

### 3.5 Analytics Dashboard Panel

New crate: `analytics_panel` implementing `Panel` trait. Right dock.

**Agentlytics-style read-only dashboard:**
- Session metrics: total sessions, tokens in/out, estimated cost, cache ratio
- Activity heatmap: GitHub-style contribution graph
- Editor breakdown: sessions per editor (Cursor, VS Code, Claude Code, etc.)
- Trend charts: monthly usage, peak hours, weekday patterns
- Model usage: top models, token distribution

**Data source:** Reads from `~/.agentics/` or similar directory. No AI in the app.

**Rendering:** GPUI native elements (div, text, colored rects for charts). No web view.

### 3.6 GitHub Integration Panel

New crate: `github_panel` implementing `Panel` trait. Right dock.

**Features:**
- Activity feed: notifications, PR reviews requested, issue mentions, releases
- PR status: open PRs with CI checks, review state, merge conflicts
- Issue tracker: assigned issues with labels, milestones, priority
- Repository dashboard: stars, forks, recent commits for watched repos
- Quick actions: merge PR, comment, approve, close issue from panel
- Watch list: follow repos/users/orgs for activity

**Implementation:**
- GitHub REST/GraphQL API via `http_client` crate + optional `gh` CLI
- OAuth token from `gh auth status` or settings
- Polling interval: configurable (default 60s)
- SQLite cache for offline viewing
- Notification badge on panel icon

### 3.7 QMD Knowledge Layer

New crate: `qmd_store` (library, not a panel).

**Rust-native implementation (QMD is TypeScript, not embeddable):**
- **Tantivy** for BM25 full-text search (pure Rust, 2x faster than Lucene)
- **sqlite-vec** for vector search via rusqlite (same approach QMD uses)
- **OpenAI embeddings API** for vectorization via `http_client`
- **comrak** for markdown parsing and chunk extraction

**What it indexes:**
- Terminal command history + output (auto-indexed)
- SSH connection notes and troubleshooting
- Folder/workspace notes and metadata
- Agentlytics session logs

**Chunking strategy (inspired by QMD):**
- ~900-token chunks with 15% overlap
- Markdown-aware boundaries (headings, paragraphs, code blocks)

**Query interface:**
- Command palette: `Ctrl+K` semantic search across all indexed content
- Results contextualized: terminal commands replay, SSH notes open connections, files open in editor

**Storage:** `~/.zed-terminal/qmd/` with SQLite + Tantivy index.

---

## 4. Implementation Order

### Phase 1: Strip & Optimize (Foundation)
1. Remove 75 AI/collab/editor-heavy crates from Cargo.toml
2. Remove init() calls in main.rs and zed.rs
3. Remove panel registrations for Agent, Collab, Notification panels
4. Verify the app compiles and runs as a stripped-down Zed
5. Apply startup optimizations (defer block_on, lazy themes, reduce Rayon stack)
6. Apply Tier 1 terminal rendering fixes (O(n^3) merge, cell clone refs)

### Phase 2: Terminal-First Layout
7. Change default center pane to terminal on new workspace
8. Add keyboard-driven split/navigate/resize actions
9. Editor opens as split pane alongside terminals
10. Enhance layout persistence for terminal-centric sessions

### Phase 3: SSH Enhancements
11. Build ssh_panel (connection manager)
12. Build ports_panel (port forwarding)
13. Apply SSH performance fixes (compression, batching, Windows pool)
14. Auto-detect remote listening ports
15. Persistent tunnels that survive reconnection

### Phase 4: Knowledge & History
16. Build history_panel (folder history + session restore + bookmarks)
17. Build qmd_store (Tantivy + sqlite-vec + OpenAI embeddings)
18. Index terminal history, SSH notes, workspace metadata
19. Command palette semantic search integration

### Phase 5: Dashboards
20. Build analytics_panel (Agentlytics dashboard)
21. Build github_panel (activity feed, PRs, issues)

### Phase 6: Deep Performance
22. Apply Tier 2 terminal rendering fixes (dirty-line tracking, cache leverage)
23. Apply Tier 3 terminal rendering fixes (batch paint_glyph)
24. Profile and iterate

---

## 5. Testing Strategy

- **Compilation gate:** After Phase 1, app must compile and run with all crates removed
- **Terminal rendering benchmarks:** Measure frame time for 200x120 terminal before/after each optimization
- **SSH latency benchmarks:** Measure keystroke roundtrip time over SSH on LAN and high-latency networks
- **Startup time measurement:** Use existing miniprofiler/ztracing infrastructure
- **Panel integration tests:** Each new panel gets a test that verifies Panel trait compliance

---

## 6. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Hidden dependencies between removed and kept crates | Medium | High | Compile early, fix incrementally |
| GPUI dirty-region tracking requires deep framework changes | Medium | Medium | Start with terminal-level dirty tracking, defer GPUI changes |
| Windows SSH performance harder to fix than Unix | High | Medium | Connection pool is a workaround, not a fix for Win32-OpenSSH |
| OpenAI embedding costs for QMD | Low | Low | Embeddings are cheap; batch during idle time |
| GitHub API rate limits | Medium | Low | Cache aggressively, respect rate limit headers |
