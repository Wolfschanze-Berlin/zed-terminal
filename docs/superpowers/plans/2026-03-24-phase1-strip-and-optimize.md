# Phase 1: Strip & Optimize — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove ~75 AI/collab/editor-heavy crates from Zed, clean up kept-crate dependencies, apply startup optimizations and Tier 1 terminal rendering performance fixes. Result: a stripped-down, faster-starting Zed that compiles, runs, and passes all smoke tests.

**Architecture:** Surgical removal from the existing Zed codebase. Remove crates from Cargo.toml workspace members, remove init() calls in main.rs/zed.rs, remove panel registrations, then clean up compile errors in kept crates. Performance fixes target specific hot paths identified by line-by-line code analysis.

**Tech Stack:** Rust, GPUI framework, alacritty_terminal, Cargo workspace

**Spec:** `docs/superpowers/specs/2026-03-24-zed-terminal-design.md`

---

## File Map

**Files to modify (primary):**
- `Cargo.toml` — Remove ~75 workspace members
- `crates/zed/Cargo.toml` — Remove ~40 dependency declarations
- `crates/zed/src/main.rs` — Remove AI/collab init() calls (lines 650-688, 701, 714, 725, 735-737, 740, 744, 748), reduce Rayon stack (line 307), defer block_on (lines 576-578)
- `crates/zed/src/zed.rs` — Remove panel registrations (lines 627, 630-636, 661), remove outline/debug panels (lines 636, 655, 660)
- `crates/terminal_view/src/terminal_element.rs` — Replace O(n^3) merge (lines 250-280), remove cell clones (line 1107), increase String capacity (line 99)
- `crates/terminal/src/terminal.rs` — Reduce default scrollback (line 342)

**Files to modify (kept-crate cleanup — compile errors):**
- `crates/editor/src/editor.rs` — Remove edit_prediction/copilot references
- `crates/editor/src/element.rs` — Remove edit prediction rendering
- `crates/editor/src/mouse_context_menu.rs` — Remove AI context menu items
- `crates/project/src/project.rs` — Hardcode DisableAiSettings to always-disabled, remove AgentRegistryStore
- `crates/title_bar/src/` — Remove collab/AI status indicators
- `crates/workspace/src/` — Remove agent panel serialization fallbacks
- `crates/terminal_view/src/terminal_view.rs` — Remove terminal slash command registration

**No new files created in Phase 1.**

---

## Task 1: Capture Performance Baselines

**Files:**
- None (measurement only)

- [ ] **Step 1: Build current Zed and measure startup time**

```bash
cd D:/zed-terminal
cargo build --release 2>&1 | tail -5
# Then run and note time-to-first-paint using miniprofiler
```

- [ ] **Step 2: Record baseline metrics**

Create a temporary file to track measurements:
```bash
cat > /tmp/zed-terminal-baselines.md << 'EOF'
# Zed Terminal Phase 1 Baselines
- Build time (release): ___
- Startup time (first paint): ___
- Memory at rest (1 terminal): ___
- Terminal frame time (200x120, idle): ___
- Terminal frame time (200x120, scrolling): ___
- Crate count: 225
EOF
```

- [ ] **Step 3: Commit baseline doc**

No commit — this is a reference file only.

---

## Task 2: Remove AI/Agent Crates from Workspace (Batch 1 — 33 crates)

**Files:**
- Modify: `Cargo.toml` (root workspace members list)

- [ ] **Step 1: Remove AI/Agent crate members from root Cargo.toml**

Open `Cargo.toml` and remove these entries from the `[workspace] members` array:

```
"crates/agent"
"crates/agent_servers"
"crates/agent_settings"
"crates/agent_ui"
"crates/acp_thread"
"crates/acp_tools"
"crates/ai_onboarding"
"crates/anthropic"
"crates/bedrock"
"crates/cloud_llm_client"
"crates/codestral"
"crates/copilot"
"crates/copilot_chat"
"crates/copilot_ui"
"crates/deepseek"
"crates/google_ai"
"crates/lmstudio"
"crates/mistral"
"crates/ollama"
"crates/open_router"
"crates/x_ai"
"crates/vercel"
"crates/opencode"
"crates/language_model"
"crates/language_models"
"crates/edit_prediction"
"crates/edit_prediction_cli"
"crates/edit_prediction_context"
"crates/edit_prediction_types"
"crates/edit_prediction_ui"
"crates/web_search"
"crates/web_search_providers"
"crates/assistant_slash_command"
"crates/assistant_slash_commands"
"crates/assistant_text_thread"
"crates/context_server"
```

- [ ] **Step 2: Try to compile to see what breaks**

```bash
cargo check 2>&1 | head -50
```

Expected: Compilation errors in crates that depend on removed crates. This is expected — we fix them in later tasks.

- [ ] **Step 3: Commit the workspace removal**

```bash
git add Cargo.toml
git commit -m "Remove 35 AI/agent crates from workspace members"
```

---

## Task 3: Remove Collaboration Crates from Workspace (Batch 2 — 6 crates)

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Remove collab crate members**

Remove from `[workspace] members`:
```
"crates/collab"
"crates/collab_ui"
"crates/call"
"crates/channel"
"crates/livekit_api"
"crates/livekit_client"
```

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml
git commit -m "Remove 6 collaboration crates from workspace members"
```

---

## Task 4: Remove Editor-Heavy and Other Crates (Batch 3 — ~35 crates)

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Remove editor-heavy feature crates**

Remove from `[workspace] members`:
```
"crates/outline_panel"
"crates/debugger_ui"
"crates/dap"
"crates/dap_adapters"
"crates/debug_adapter_extension"
"crates/repl"
"crates/vim"
"crates/vim_mode_setting"
"crates/component"
"crates/component_preview"
"crates/storybook"
"crates/story"
"crates/zeta_prompt"
"crates/prompt_store"
"crates/rules_library"
```

- [ ] **Step 2: Remove other unnecessary crates**

Remove from `[workspace] members`:
```
"crates/eval"
"crates/eval_cli"
"crates/eval_utils"
"crates/schema_generator"
"crates/docs_preprocessor"
"crates/feedback"
"crates/onboarding"
"crates/language_onboarding"
"crates/streaming_diff"
"crates/action_log"
"crates/audio"
```

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "Remove 26 editor-heavy and utility crates from workspace members"
```

---

## Task 5: Remove Dependencies from Zed Crate Cargo.toml

**Files:**
- Modify: `crates/zed/Cargo.toml`

- [ ] **Step 1: Remove dependency declarations for all removed crates**

Open `crates/zed/Cargo.toml` and remove all `[dependencies]` entries for crates that were removed from the workspace in Tasks 2-4. These include entries like:
```toml
agent.workspace = true
agent_ui.workspace = true
acp_tools.workspace = true
# ... etc for all ~40 removed dependencies
```

Search for each removed crate name and delete its dependency line.

- [ ] **Step 2: Also check and clean `[dev-dependencies]` and `[build-dependencies]`**

Remove any references to removed crates in those sections too.

- [ ] **Step 3: Commit**

```bash
git add crates/zed/Cargo.toml
git commit -m "Remove dependencies on stripped crates from zed crate"
```

---

## Task 6: Remove AI/Collab Init Calls from main.rs

**Files:**
- Modify: `crates/zed/src/main.rs`

- [ ] **Step 1: Remove AI init calls (lines 650-688)**

Remove or comment out these blocks:
```rust
// DELETE lines 650-662 (copilot_chat::init block)
// DELETE line 664 (copilot_ui::init)
// DELETE line 665 (language_model::init)
// DELETE line 666 (language_models::init)
// DELETE line 667 (acp_tools::init)
// DELETE line 670 (edit_prediction_ui::init)
// DELETE line 671 (web_search::init)
// DELETE line 672 (web_search_providers::init)
// DELETE line 674 (edit_prediction_registry::init)
// DELETE lines 675-680 (PromptBuilder and AgentRegistryStore)
// DELETE lines 681-688 (agent_ui::init)
```

Keep `snippet_provider::init(cx)` (line 673) — it's not AI-related.

- [ ] **Step 2: Remove collab/debug/vim/other init calls**

```rust
// DELETE line 690 (repl::init)
// DELETE line 698 (repl::notebook::init)
// DELETE line 701 (audio::init)
// DELETE line 708 (outline::init — outline_panel was removed)
// DELETE line 711 (outline_panel::init)
// DELETE line 714 (channel::init)
// DELETE line 725 (vim::init)
// DELETE line 735 (call::init)
// DELETE line 736 (notifications::init)
// DELETE line 737 (collab_ui::init)
// DELETE line 740 (feedback::init)
// DELETE line 744 (onboarding::init)
// DELETE line 748 (edit_prediction::init)
```

- [ ] **Step 3: Remove corresponding `use` imports at top of file**

Search for `use agent_ui`, `use copilot`, `use collab_ui`, `use language_model`, `use edit_prediction`, `use vim`, `use call`, `use channel`, `use feedback`, `use onboarding`, `use audio`, `use repl`, `use outline_panel`, `use debugger_ui`, etc. and remove them.

- [ ] **Step 4: Commit**

```bash
git add crates/zed/src/main.rs
git commit -m "Remove AI, collab, debug, and vim init calls from main.rs"
```

---

## Task 7: Remove Panel Registrations from zed.rs

**Files:**
- Modify: `crates/zed/src/zed.rs`

- [ ] **Step 1: Remove panel load calls in initialize_panels() (lines 620-666)**

In the `initialize_panels` function, remove:
```rust
// DELETE line 627: let outline_panel = OutlinePanel::load(...)
// DELETE lines 630-635: let channels_panel = collab_ui::collab_panel::CollabPanel::load(...)
//                       let notification_panel = collab_ui::notification_panel::NotificationPanel::load(...)
// DELETE line 636: let debug_panel = DebugPanel::load(...)
```

- [ ] **Step 2: Remove corresponding add_panel_when_ready calls in futures::join!**

In the `futures::join!` block, remove:
```rust
// DELETE line 655: add_panel_when_ready(outline_panel, ...)
// DELETE line 658: add_panel_when_ready(channels_panel, ...)
// DELETE line 659: add_panel_when_ready(notification_panel, ...)
// DELETE line 660: add_panel_when_ready(debug_panel, ...)
// DELETE line 661: initialize_agent_panel(...)
```

Keep: project_panel, terminal_panel, git_panel.

- [ ] **Step 3: Remove the setup_or_teardown_ai_panel function (line 668+)**

This function manages the agent panel lifecycle — delete it entirely.

- [ ] **Step 4: Remove the initialize_agent_panel function**

Find and delete `fn initialize_agent_panel(...)` — it's no longer called.

- [ ] **Step 5: Remove unused imports**

Remove `use collab_ui`, `use outline_panel`, `use debugger_ui`, `use agent_ui`, etc. from the top of the file.

- [ ] **Step 6: Commit**

```bash
git add crates/zed/src/zed.rs
git commit -m "Remove Agent, Collab, Outline, Debug panel registrations"
```

---

## Task 8: Clean Up Editor Crate (Remove AI References)

**Files:**
- Modify: `crates/editor/src/editor.rs`
- Modify: `crates/editor/src/element.rs`
- Modify: `crates/editor/src/mouse_context_menu.rs`
- Modify: `crates/editor/Cargo.toml`

- [ ] **Step 1: Try to compile and collect all editor errors**

```bash
cargo check -p editor 2>&1 | grep "error\[" | head -30
```

- [ ] **Step 2: Remove edit_prediction references from editor.rs**

Search for `edit_prediction` in `editor.rs` and remove or stub out:
- Edit prediction ghost text rendering
- Copilot suggestion acceptance/rejection actions
- AI context menu items

For each reference, either delete the block or replace with a no-op. The `DisableAiSettings` code paths already exist — hardcode the "disabled" branch.

- [ ] **Step 3: Remove copilot references from editor.rs**

Search for `copilot` and remove inline suggestion rendering, copilot status checks, etc.

- [ ] **Step 4: Remove language_model references**

Search for `language_model` and remove any AI model selection or inline assist triggers.

- [ ] **Step 5: Clean up Cargo.toml dependencies**

Remove `edit_prediction`, `copilot`, `language_model`, etc. from `crates/editor/Cargo.toml`.

- [ ] **Step 6: Compile and fix remaining errors iteratively**

```bash
cargo check -p editor 2>&1 | grep "error\[" | head -20
```

Repeat until editor compiles clean.

- [ ] **Step 7: Commit**

```bash
git add crates/editor/
git commit -m "Remove AI/copilot/edit-prediction references from editor crate"
```

---

## Task 9: Clean Up Project Crate

**Files:**
- Modify: `crates/project/src/project.rs`
- Modify: `crates/project/Cargo.toml`

- [ ] **Step 1: Hardcode DisableAiSettings to always-disabled**

Find `DisableAiSettings` in `project.rs`. Change it so `disable_ai` always returns `true`. Remove the settings toggle — AI is always disabled in this fork.

- [ ] **Step 2: Remove AgentRegistryStore**

Find `AgentRegistryStore` references and remove them. This was initialized in main.rs (already removed) but the type may still be referenced in project.

- [ ] **Step 3: Remove context_server references**

Search for `context_server` and remove.

- [ ] **Step 4: Clean Cargo.toml**

Remove dependencies on removed crates.

- [ ] **Step 5: Compile and fix iteratively**

```bash
cargo check -p project 2>&1 | grep "error\[" | head -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/project/
git commit -m "Hardcode AI-disabled in project crate, remove agent registry"
```

---

## Task 10: Clean Up Title Bar, Workspace, and Terminal View Crates

**Files:**
- Modify: `crates/title_bar/src/` (multiple files)
- Modify: `crates/workspace/src/` (serialization files)
- Modify: `crates/terminal_view/src/terminal_view.rs`
- Modify: Cargo.toml files for each crate

- [ ] **Step 1: Clean title_bar crate**

Remove collab_ui, copilot_ui, language_model references. Remove AI/collab status indicators from the title bar rendering.

```bash
cargo check -p title_bar 2>&1 | grep "error\[" | head -20
```

Fix until clean.

- [ ] **Step 2: Clean workspace crate**

Remove agent panel serialization/deserialization fallbacks. Search for `AgentPanel` references.

```bash
cargo check -p workspace 2>&1 | grep "error\[" | head -20
```

- [ ] **Step 3: Clean terminal_view crate**

Remove `TerminalSlashCommand` registration in `terminal_view.rs`. This was the AI assistant's ability to run terminal commands.

```bash
cargo check -p terminal_view 2>&1 | grep "error\[" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/title_bar/ crates/workspace/ crates/terminal_view/
git commit -m "Clean up title_bar, workspace, terminal_view AI/collab references"
```

---

## Task 11: Full Compilation and Smoke Test

**Files:**
- None (verification only)

- [ ] **Step 1: Full workspace compilation**

```bash
cargo check 2>&1 | tail -20
```

Expected: No errors. If errors remain, fix them in the relevant crate and commit.

- [ ] **Step 2: Build release binary**

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 3: Run smoke tests**

Launch the built binary and verify:
1. Window opens
2. Terminal appears (may still be in bottom panel — Phase 2 moves it to center)
3. Can type in terminal and see output
4. Can open a file in editor
5. Project panel shows file tree
6. Git panel shows status
7. Settings load (Ctrl+,)
8. Extensions load (check syntax highlighting on a .rs file)

- [ ] **Step 4: Commit any remaining fixes**

```bash
git add -A
git commit -m "Fix remaining compilation issues after crate removal"
```

---

## Task 12: Startup Optimization — Reduce Rayon Stack Size

**Files:**
- Modify: `crates/zed/src/main.rs:307`

- [ ] **Step 1: Change Rayon thread stack from 10MB to 2MB**

At line 307, change:
```rust
// BEFORE
.stack_size(10 * 1024 * 1024)

// AFTER
.stack_size(2 * 1024 * 1024)
```

- [ ] **Step 2: Verify it still runs**

```bash
cargo build --release && ./target/release/zed
```

If any thread hits a stack overflow, increase to 4MB.

- [ ] **Step 3: Commit**

```bash
git add crates/zed/src/main.rs
git commit -m "Reduce Rayon thread stack from 10MB to 2MB (saves ~40-80MB)"
```

---

## Task 13: Startup Optimization — Reduce Default Scrollback

**Files:**
- Modify: `crates/terminal/src/terminal.rs:342`

- [ ] **Step 1: Reduce default scrollback from 10,000 to 5,000 lines**

At line 342:
```rust
// BEFORE
const DEFAULT_SCROLL_HISTORY_LINES: usize = 10_000;

// AFTER
const DEFAULT_SCROLL_HISTORY_LINES: usize = 5_000;
```

- [ ] **Step 2: Commit**

```bash
git add crates/terminal/src/terminal.rs
git commit -m "Reduce default terminal scrollback to 5000 lines (saves ~15-30MB/terminal)"
```

---

## Task 14: Terminal Perf — Replace O(n^3) Background Region Merge

**Files:**
- Modify: `crates/terminal_view/src/terminal_element.rs:250-280`

- [ ] **Step 1: Replace merge_background_regions with O(n log n) sort+sweep**

Replace the function at lines 250-280 with:

```rust
fn merge_background_regions(mut regions: Vec<BackgroundRegion>) -> Vec<BackgroundRegion> {
    if regions.len() <= 1 {
        return regions;
    }

    // Sort by color, then by start_line, then by start_col
    regions.sort_unstable_by(|a, b| {
        a.color
            .cmp(&b.color)
            .then(a.start_line.cmp(&b.start_line))
            .then(a.start_col.cmp(&b.start_col))
    });

    let mut merged: Vec<BackgroundRegion> = Vec::with_capacity(regions.len());
    merged.push(regions[0].clone());

    for region in regions.into_iter().skip(1) {
        let last = merged.last_mut().expect("merged is non-empty");
        if last.can_merge_with(&region) {
            last.merge_with(&region);
        } else {
            merged.push(region);
        }
    }

    merged
}
```

This sorts regions first (O(n log n)), then does a single pass to merge adjacent compatible regions (O(n)). Total: O(n log n) vs the original O(n^3).

Note: `BackgroundRegion` needs to have a color field that implements `Ord`. If the color type does not implement `Ord`, derive or implement it. Check what `BackgroundRegion.color` is — if it's an RGBA type, compare by converting to a tuple of u8 values.

- [ ] **Step 2: Verify terminal still renders correctly**

Build and run. Open a terminal. Run `ls --color` or similar colored output. Verify backgrounds render correctly.

- [ ] **Step 3: Commit**

```bash
git add crates/terminal_view/src/terminal_element.rs
git commit -m "Replace O(n^3) background merge with O(n log n) sort+sweep"
```

---

## Task 15: Terminal Perf — Increase BatchedTextRun String Capacity

**Files:**
- Modify: `crates/terminal_view/src/terminal_element.rs:99`

- [ ] **Step 1: Increase pre-allocated String capacity**

Find the `BatchedTextRun` creation (around line 99) where `String::with_capacity(100)` appears. Change to:

```rust
// BEFORE
String::with_capacity(100)

// AFTER
String::with_capacity(256)
```

- [ ] **Step 2: Commit**

```bash
git add crates/terminal_view/src/terminal_element.rs
git commit -m "Increase BatchedTextRun String capacity from 100 to 256"
```

---

## Task 16: Terminal Perf — Remove Cell Clones in Viewport Filter

**Files:**
- Modify: `crates/terminal_view/src/terminal_element.rs` (around line 1107)

- [ ] **Step 1: Find the viewport filtering code**

Look for the `.cloned()` call in the prepaint viewport filtering logic (around line 1107). It looks like:

```rust
.flat_map(|(_, line_cells)| line_cells)
.cloned()
```

- [ ] **Step 2: Remove .cloned() and update layout_grid() to accept references**

Change the iterator to pass references instead of clones. This requires updating the `layout_grid()` function signature to accept `impl Iterator<Item = &IndexedCell>` instead of `impl Iterator<Item = IndexedCell>`.

This is a larger refactor — the function signature change may cascade to how cells are accessed inside `layout_grid()`. Read the function body (lines 331-500) to understand what borrows are needed.

If the refactor is too invasive, an alternative quick fix: use `Cow<'_, IndexedCell>` to defer cloning until mutation is needed.

- [ ] **Step 3: Verify terminal renders correctly**

Build and run. Open terminal, run colored output, scroll up and down.

- [ ] **Step 4: Commit**

```bash
git add crates/terminal_view/src/terminal_element.rs
git commit -m "Remove unnecessary cell clones in viewport filtering"
```

---

## Task 17: Final Phase 1 Verification

**Files:**
- None (verification only)

- [ ] **Step 1: Full release build**

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 2: Measure post-optimization metrics**

Compare against baselines from Task 1:
- Build time (release)
- Startup time (first paint)
- Memory at rest (1 terminal)
- Terminal frame time (200x120, idle)
- Terminal frame time (200x120, scrolling)
- Crate count (should be ~150, down from 225)

- [ ] **Step 3: Run full smoke test suite**

1. Window opens
2. Terminal works (type, output, scrollback)
3. Editor opens files (syntax highlighting works)
4. Project panel shows file tree
5. Git panel shows status
6. Settings load and persist
7. Extensions load
8. SSH connection works (if remote host available)
9. Terminal splits work
10. No AI/collab UI elements visible anywhere

- [ ] **Step 4: Commit final state**

```bash
git add -A
git commit -m "Phase 1 complete: stripped Zed Terminal with startup and rendering optimizations"
```

---

## Summary

| Task | Description | Key Files |
|------|-------------|-----------|
| 1 | Capture baselines | (measurement) |
| 2 | Remove 35 AI/agent crates | Cargo.toml |
| 3 | Remove 6 collab crates | Cargo.toml |
| 4 | Remove 26 editor-heavy/utility crates | Cargo.toml |
| 5 | Remove deps from zed crate | crates/zed/Cargo.toml |
| 6 | Remove init() calls from main.rs | crates/zed/src/main.rs |
| 7 | Remove panel registrations from zed.rs | crates/zed/src/zed.rs |
| 8 | Clean editor crate | crates/editor/src/*.rs |
| 9 | Clean project crate | crates/project/src/project.rs |
| 10 | Clean title_bar, workspace, terminal_view | Multiple crates |
| 11 | Full compilation + smoke test | (verification) |
| 12 | Reduce Rayon stack (10MB→2MB) | main.rs:307 |
| 13 | Reduce scrollback (10k→5k) | terminal.rs:342 |
| 14 | O(n^3)→O(n log n) background merge | terminal_element.rs:250-280 |
| 15 | Increase String capacity (100→256) | terminal_element.rs:99 |
| 16 | Remove cell clones in viewport filter | terminal_element.rs:1107 |
| 17 | Final verification + metrics | (verification) |
