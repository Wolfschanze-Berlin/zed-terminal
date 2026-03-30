# Collab Infrastructure Removal — Design Spec

**Date:** 2026-03-30
**Scope:** Surface clean — remove collab crates from workspace, strip references from active crates, stub methods, leave directories on disk.

## Background

The `collab` and `collab_ui` crates were already excluded from workspace members during Phase 1. However, four supporting crates (`call`, `channel`, `livekit_client`, `livekit_api`) remain as active workspace members, and several active crates still contain collab-specific code. This work completes the collab removal.

## Crates to Remove from Workspace

Remove from `[workspace.members]` and `[workspace.dependencies]` in root `Cargo.toml`:

| Crate | Purpose | Current Status |
|-------|---------|----------------|
| `call` | ActiveCall, Room, LiveKit integration | Active member |
| `channel` | ChannelStore, ChannelBuffer | Active member |
| `livekit_client` | WebRTC client | Active member |
| `livekit_api` | Vendored LiveKit protocol defs | Active member |

Directories remain on disk (consistent with `collab`/`collab_ui` treatment).

## Active Crates Requiring Modifications

### 1. `title_bar`

**Dependencies to remove:** `call`, `channel`, `livekit_client` (normal + dev)

**Code changes:**
- Delete `src/collab.rs` (732 lines) — contains `toggle_mute`, `toggle_deafen`, `toggle_screen_sharing`, `render_collaborator_list`, `render_call_controls`
- Remove `pub mod collab;` from `title_bar.rs`
- Remove two render calls from the title bar layout:
  - `self.render_collaborator_list(window, cx)` (collaborator avatar facepile)
  - `self.render_call_controls(window, cx)` (mute/deafen/screen-share buttons)
- Remove the `screen_share_popover_handle` field from `TitleBar` struct if it becomes unused
- Clean up unused imports (`call::*`, `channel::*`, `livekit_client::*`)

**Result:** Clean minimal title bar with project info on the left, controls (update indicator, user menu) on the right.

### 2. `workspace`

**Dependencies to remove:** None (workspace crate has no direct `call`/`channel` dependency — it uses `client::proto` types)

**Code changes — stub the following methods in `workspace.rs`:**

| Method | Stub behavior |
|--------|---------------|
| `collaborator_left()` | No-op |
| `start_following()` | Return `None` |
| `follow_next_collaborator()` | No-op |
| `follow()` | No-op |
| `unfollow()` | Return `None` |
| `unfollow_in_pane()` | Return `None` |
| `is_being_followed()` | Return `false` |
| `active_view_for_follower()` | Return `None` |
| `handle_follow()` | Return default `proto::FollowResponse` |
| `handle_update_followers()` | No-op |
| `process_leader_update()` | Return `Ok(())` |
| `add_view_from_leader()` | Return `Ok(())` |
| `update_active_view_for_followers()` | No-op |
| `active_item_for_followers()` | Return `(None, None)` |
| `update_followers()` | Return `None` |
| `leader_for_pane()` | Return `None` |
| `leader_updated()` | Return `None` |

**Action declarations to keep as no-ops:** `ShareProject`, `ScreenShare`, `Mute`, `Deafen`, `LeaveCall`, `CopyRoomId`, `OpenChannelNotes`, `OpenChannelNotesById`, `FollowNextCollaborator`

Keep the `follower_states`, `last_leaders_by_pane`, and `leader_updates_tx` fields but they will remain empty/unused. This avoids cascading struct changes through code that constructs `Workspace`.

**`is_via_collab()` references:** This method lives on `Project`, not `Workspace`. It checks `project.is_via_collab()` which returns whether the project was opened via collaboration. Since collab connections can no longer happen, this will always return `false` at runtime. No code changes needed — the method stays, it just never triggers.

### 3. `file_finder`

**Dependencies to remove:** `channel`

**Code changes:**
- Remove `use channel::ChannelStore;` import
- Remove `channel_store: Option<Entity<ChannelStore>>` field
- Remove the `ChannelStore::try_global(cx)` call — replace with `None` assignment or remove the field entirely

### 4. `notifications`

**Dependencies to remove:** `channel`

**Code changes:**
- Remove `use channel::ChannelStore;` import
- Remove `channel_store: Entity<ChannelStore>` field from `NotificationStore`
- Remove the `ChannelStore::global(cx)` call in the constructor
- Make any channel-dependent notification logic conditional or remove it — notifications should still function for non-collab events

### 5. `git_ui`

**Dependencies to remove:** `call`

**Code changes:**
- Remove `call::ActiveCall::try_global(cx)` calls in `git_panel.rs` (lines 3303, 5640)
- Remove `local_committer()` method that takes `&call::Room` (line 3339)
- Stub the co-author detection: `co_authors_for_project()` returns empty `Vec` when there's no active call room (which is always now)
- Remove the room-based co-author facepile rendering that depends on `call::Room`

### 6. `ui`

**Dependencies to remove:** None (no `call`/`channel` deps in Cargo.toml)

**Code changes:**
- Check `src/components/collab.rs` module — contains `collab_notification` and `update_button` sub-modules
- Keep `update_button` (app update notifications, not collab-specific)
- Remove `collab_notification` sub-module (collab-only UI component)
- Move `update_button` out of the `collab` parent module and remove the `collab` module file

### 7. `zed` (main binary)

**Code changes:**
- Remove `"collab"`, `"collab_panel"`, `"channel_modal"` from the action namespace validation test (lines ~4630-4634 in `zed.rs`)

## What Stays Untouched

- **Crate directories on disk:** `call/`, `channel/`, `livekit_client/`, `livekit_api/`, `collab/`, `collab_ui/`
- **Proto definitions:** `call.proto`, `channel.proto` remain in `crates/proto/proto/`
- **`project.is_via_collab()`:** Method stays, always returns `false` at runtime since no collab connections can be established
- **`client` crate:** Contains proto types and `ChannelId` used broadly — no changes needed
- **Editor collaborator rendering:** References `project.is_via_collab()` which returns `false`, so collaborator rendering code paths are dead but harmless

## Verification

After all changes:
1. `cargo check` must pass for the default member (`crates/zed`)
2. `./script/clippy` must pass
3. The application must launch with a clean title bar (no collab controls)
4. No runtime panics from missing `ActiveCall::global()` or `ChannelStore::global()` calls
