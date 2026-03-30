# Collab Infrastructure Removal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove collaboration infrastructure (`call`, `channel`, `livekit_client`, `livekit_api`) from the active workspace and strip collab-specific code from active crates.

**Architecture:** Surface clean approach — remove workspace dependency entries and strip imports/usage from 5 active crates (`title_bar`, `file_finder`, `notifications`, `git_ui`, `ui`). The `workspace` crate needs no changes — it defines its own `GlobalAnyActiveCall` abstraction and has no direct `call`/`channel` dependency. With no one registering the global, all collab code paths safely no-op via `Option::None`. Leave crate directories on disk.

**Tech Stack:** Rust, Cargo workspace, GPUI framework

---

### Task 1: Remove collab crate workspace dependencies

**Files:**
- Modify: `Cargo.toml:218-219,302-303` (workspace dependencies)

- [ ] **Step 1: Remove workspace dependency entries**

In root `Cargo.toml`, remove these four lines from `[workspace.dependencies]`:

```toml
# Line 218 - remove:
call = { path = "crates/call" }
# Line 219 - remove:
channel = { path = "crates/channel" }
# Line 302 - remove:
livekit_api = { path = "crates/livekit_api" }
# Line 303 - remove:
livekit_client = { path = "crates/livekit_client" }
```

- [ ] **Step 2: Verify Cargo.toml parses**

Run: `cargo metadata --format-version 1 --no-deps > /dev/null 2>&1 && echo OK`
Expected: OK (metadata parses without error)

Note: `cargo check` will fail at this point because dependent crates still reference these. That's expected — subsequent tasks fix those.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: Remove call, channel, livekit_api, livekit_client from workspace dependencies"
```

---

### Task 2: Strip collab from `title_bar` crate

**Files:**
- Modify: `crates/title_bar/Cargo.toml`
- Delete: `crates/title_bar/src/collab.rs`
- Modify: `crates/title_bar/src/title_bar.rs`

- [ ] **Step 1: Remove collab dependencies from Cargo.toml**

In `crates/title_bar/Cargo.toml`, remove:

From `[features]` `test-support` list (line 19):
```toml
    "call/test-support",
```

From `[dependencies]` (lines 35-36, 44):
```toml
call.workspace = true
channel.workspace = true
livekit_client.workspace = true
```

From `[dev-dependencies]` (line 68):
```toml
call = { workspace = true, features = ["test-support"] }
```

- [ ] **Step 2: Delete collab.rs module file**

Delete `crates/title_bar/src/collab.rs` entirely.

- [ ] **Step 3: Remove module declaration and collab imports from title_bar.rs**

In `crates/title_bar/src/title_bar.rs`, remove:

Line 2:
```rust
pub mod collab;
```

Line 24 (import):
```rust
use call::ActiveCall;
```

Also remove any other `use call::`, `use channel::`, or `use livekit_client::` imports.

- [ ] **Step 4: Remove `screen_share_popover_handle` field from TitleBar struct**

In the `TitleBar` struct definition (around line 147-160), remove:
```rust
    screen_share_popover_handle: PopoverMenuHandle<ContextMenu>,
```

Also remove its initialization in the constructor (around line 443):
```rust
            screen_share_popover_handle: PopoverMenuHandle::default(),
```

- [ ] **Step 5: Remove collaborator list rendering from title bar layout**

In the render method, remove line 233:
```rust
        children.push(self.render_collaborator_list(window, cx).into_any_element());
```

- [ ] **Step 6: Remove call controls rendering from title bar layout**

In the render method, around line 256, change:
```rust
                .children(self.render_call_controls(window, cx))
```
to remove just that line. The surrounding `h_flex()` block should stay — it still renders `.children(self.render_connection_status(status, cx))`, `.child(self.update_version.clone())`, sign-in button, organization menu, and user menu.

- [ ] **Step 7: Clean up unused imports**

Remove any imports that are now unused after removing the collab module references. The compiler will tell you which ones. Common ones to remove: `PopoverMenuHandle`, `ContextMenu` (if only used by screen_share_popover_handle), and any `call`/`channel`/`livekit_client` re-exports.

- [ ] **Step 8: Verify title_bar compiles**

Run: `cargo check -p title_bar 2>&1 | head -30`
Expected: May still fail due to upstream crates. Fix any remaining unused import warnings or errors.

- [ ] **Step 9: Commit**

```bash
git add crates/title_bar/
git commit -m "title_bar: Remove collab module and call/channel/livekit dependencies"
```

---

### Task 3: Strip collab from `file_finder` crate

**Files:**
- Modify: `crates/file_finder/Cargo.toml`
- Modify: `crates/file_finder/src/file_finder.rs`

- [ ] **Step 1: Remove channel dependency from Cargo.toml**

In `crates/file_finder/Cargo.toml`, remove the line:
```toml
channel.workspace = true
```

- [ ] **Step 2: Remove ChannelStore import**

In `crates/file_finder/src/file_finder.rs`, remove line 7:
```rust
use channel::ChannelStore;
```

- [ ] **Step 3: Remove channel_store field from FileFinderDelegate struct**

In the struct definition around line 397, remove:
```rust
    channel_store: Option<Entity<ChannelStore>>,
```

- [ ] **Step 4: Remove channel_store initialization**

In the constructor around lines 859-863, remove:
```rust
        let channel_store = if FileFinderSettings::get_global(cx).include_channels {
            ChannelStore::try_global(cx)
        } else {
            None
        };
```

And in the struct initialization around line 868, remove:
```rust
            channel_store,
```

- [ ] **Step 5: Remove channel matching logic**

Around lines 1001-1040, remove the entire "Add channel matches" block that starts with:
```rust
            // Add channel matches
            if let Some(channel_store) = &self.channel_store {
```

Remove the entire `if let Some(channel_store)` block and all its contents (the channel fuzzy matching logic). Find the closing brace of this block and remove everything between the comment and that brace.

- [ ] **Step 6: Verify file_finder compiles**

Run: `cargo check -p file_finder 2>&1 | head -30`
Expected: Success or fixable unused import warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/file_finder/
git commit -m "file_finder: Remove channel dependency and channel search"
```

---

### Task 4: Strip collab from `notifications` crate

**Files:**
- Modify: `crates/notifications/Cargo.toml`
- Modify: `crates/notifications/src/notification_store.rs`

- [ ] **Step 1: Remove channel dependency from Cargo.toml**

In `crates/notifications/Cargo.toml`, remove:
```toml
channel.workspace = true
```

- [ ] **Step 2: Remove ChannelStore import and field**

In `crates/notifications/src/notification_store.rs`:

Remove line 2:
```rust
use channel::ChannelStore;
```

Remove line 24 from the struct:
```rust
    channel_store: Entity<ChannelStore>,
```

Remove line 95 from the constructor:
```rust
            channel_store: ChannelStore::global(cx),
```

- [ ] **Step 3: Remove channel invitation handling**

Around line 365, the `respond_to_notification` method has a match arm for `Notification::ChannelInvitation`. Remove the entire match arm:

```rust
            Notification::ChannelInvitation { channel_id, .. } => {
                self.channel_store
                    .update(cx, |store, cx| {
                        store.respond_to_channel_invite(ChannelId(channel_id), response, cx)
                    })
                    .detach();
            }
```

- [ ] **Step 4: Verify notifications compiles**

Run: `cargo check -p notifications 2>&1 | head -30`
Expected: Success or fixable warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/notifications/
git commit -m "notifications: Remove channel dependency and channel invitation handling"
```

---

### Task 5: Strip collab from `git_ui` crate

**Files:**
- Modify: `crates/git_ui/Cargo.toml`
- Modify: `crates/git_ui/src/git_panel.rs`

- [ ] **Step 1: Remove call dependency from Cargo.toml**

In `crates/git_ui/Cargo.toml`, remove:
```toml
call.workspace = true
```

- [ ] **Step 2: Stub `potential_co_authors` method**

In `crates/git_ui/src/git_panel.rs`, around line 3298, replace the entire `potential_co_authors` method body with an empty vec return:

Replace:
```rust
    fn potential_co_authors(&self, cx: &App) -> Vec<(String, String)> {
        let mut new_co_authors = Vec::new();
        let project = self.project.read(cx);

        let Some(room) =
            call::ActiveCall::try_global(cx).and_then(|call| call.read(cx).room().cloned())
        else {
            return Vec::default();
        };
        // ... rest of method body ...
        new_co_authors
    }
```

With:
```rust
    fn potential_co_authors(&self, _cx: &App) -> Vec<(String, String)> {
        Vec::new()
    }
```

- [ ] **Step 3: Remove `local_committer` method that takes Room**

Around line 3339, remove the method:
```rust
    fn local_committer(&self, room: &call::Room, cx: &App) -> Option<(String, String)> {
        let user = room.local_participant_user(cx)?;
        let committer = self.local_committer.as_ref()?;
        let email = committer.email.clone()?;
        let name = committer
            .name
            .clone()
            .or_else(|| user.name.clone())
            .unwrap_or_else(|| user.github_login.clone().to_string());
        Some((name, email))
    }
```

Note: Keep the `local_committer` field and `load_local_committer` method on the struct — they don't depend on the `call` crate. Only remove the method that takes `&call::Room`.

- [ ] **Step 4: Stub room-based co-author rendering in Render impl**

Around line 5639, replace the room lookup and co-author detection:

Replace:
```rust
        let room = self.workspace.upgrade().and_then(|_workspace| {
            call::ActiveCall::try_global(cx).and_then(|call| call.read(cx).room().cloned())
        });

        let has_write_access = self.has_write_access(cx);

        let has_co_authors = room.is_some_and(|room| {
            self.load_local_committer(cx);
            let room = room.read(cx);
            room.remote_participants()
                .values()
                .any(|remote_participant| remote_participant.can_write())
        });
```

With:
```rust
        let has_write_access = self.has_write_access(cx);

        let has_co_authors = false;
```

- [ ] **Step 5: Clean up unused imports**

Remove any `use call::` imports at the top of `git_panel.rs`. The compiler will flag remaining unused imports.

- [ ] **Step 6: Verify git_ui compiles**

Run: `cargo check -p git_ui 2>&1 | head -30`
Expected: Success or fixable warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/git_ui/
git commit -m "git_ui: Remove call dependency and stub co-author detection"
```

---

### Task 6: Remove `collab_notification` from `ui` crate

**Files:**
- Delete: `crates/ui/src/components/collab/collab_notification.rs`
- Delete: `crates/ui/src/components/collab.rs` (module aggregator)
- Modify: `crates/ui/src/components.rs`
- Move: `crates/ui/src/components/collab/update_button.rs` → `crates/ui/src/components/update_button.rs`

- [ ] **Step 1: Move update_button.rs out of collab directory**

Copy `crates/ui/src/components/collab/update_button.rs` to `crates/ui/src/components/update_button.rs`.

- [ ] **Step 2: Delete the collab directory and module file**

Delete:
- `crates/ui/src/components/collab/collab_notification.rs`
- `crates/ui/src/components/collab/update_button.rs` (now moved)
- `crates/ui/src/components/collab.rs` (the module aggregator)
- `crates/ui/src/components/collab/` directory

- [ ] **Step 3: Update components.rs module declarations**

In `crates/ui/src/components.rs`, replace line 7:
```rust
mod collab;
```
with:
```rust
mod update_button;
```

- [ ] **Step 4: Update re-exports**

Wherever the `ui` crate's lib file or `components.rs` re-exports from `collab`, update the path. The old re-export was through `collab.rs` which did `pub use collab_notification::*; pub use update_button::*;`. Now:

- Add `pub use update_button::*;` to components.rs (or check if it was already re-exported through the collab module re-export)
- Remove any `pub use collab::*;` or `pub use collab_notification::*;` re-exports
- Grep for `CollabNotification` usage in the workspace — if any active crate uses it, those references need to be removed too

- [ ] **Step 5: Fix any downstream references to CollabNotification**

Run: `cargo check -p ui 2>&1 | head -30`

If other crates import `CollabNotification` from `ui`, those imports need to be removed. Check with grep first.

- [ ] **Step 6: Commit**

```bash
git add crates/ui/
git commit -m "ui: Remove collab_notification component, move update_button out of collab module"
```

---

### Task 7: Remove collab namespaces from zed action test

**Files:**
- Modify: `crates/zed/src/zed.rs:4630-4634`

- [ ] **Step 1: Remove collab namespaces from expected list**

In `crates/zed/src/zed.rs`, in the `test_action_namespaces` test (around line 4617), remove these entries from the `expected_namespaces` vec:

```rust
                "channel_modal",
```
(line 4630)

```rust
                "collab",
```
(line 4633)

```rust
                "collab_panel",
```
(line 4634)

Also remove `"notification_panel"` (line 4671) if the notification panel was part of collab_ui.

- [ ] **Step 2: Commit**

```bash
git add crates/zed/src/zed.rs
git commit -m "zed: Remove collab action namespaces from test expectations"
```

---

### Task 8: Full build verification

**Files:** None (verification only)

- [ ] **Step 1: Run cargo check on the default member**

Run: `cargo check 2>&1 | tail -20`
Expected: Compiles successfully. If there are errors, they will be from transitive references we missed — fix them in this task.

- [ ] **Step 2: Fix any remaining compile errors**

Common issues to watch for:
- Unused imports flagged by the compiler — remove them
- Missing types that were re-exported through `call`/`channel` — check if they come from `client` or `proto` instead
- Feature flags referencing `call/test-support` or `channel/test-support` in other crates

- [ ] **Step 3: Run clippy**

Run: `./script/clippy 2>&1 | tail -30`
Expected: No new warnings. Fix any clippy warnings introduced by the changes.

- [ ] **Step 4: Final commit for any fixups**

```bash
git add -A
git commit -m "chore: Fix remaining collab removal compile errors and warnings"
```

Only commit if there were fixups. Skip if cargo check and clippy passed cleanly.
