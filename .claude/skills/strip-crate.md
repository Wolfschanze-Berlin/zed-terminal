---
name: strip-crate
description: Safely remove or disable a crate from the zed-terminal workspace. Use this skill whenever the user wants to strip a crate, remove a dependency, disable a feature module, clean up unused crates, or reduce the workspace size. Also use when encountering "unregistered global/setting" runtime crashes after removing code — this skill covers the gotchas that cause those crashes.
---

# Safely Strip a Crate from zed-terminal

Removing a crate from this workspace is deceptively tricky. The Zed codebase uses runtime-registered globals and settings that cause panics if their init() functions are called but the crate is removed, or vice versa. This skill covers the safe removal process.

## Why careful ordering matters

The Zed runtime registers globals via `cx.set_global()` and settings via `settings::register::<T>()`. If code elsewhere tries to `read_global::<T>()` and that type was never registered (because you removed the crate that registers it), you get a runtime panic — not a compile error. This has bitten this project multiple times during the Phase 1 crate stripping.

## Step-by-step removal

### 1. Identify all references to the crate

Search for the crate name across the workspace:
```bash
rtk grep "<crate_name>" --type rust
rtk grep "<crate_name>" Cargo.toml crates/zed/Cargo.toml
```

Also search for the types the crate exports — especially settings types and global types:
```bash
rtk grep "use <crate_name>::" --type rust
```

### 2. Remove the init() call first

In `crates/zed/src/main.rs`, comment out or remove the `<crate_name>::init(cx);` line. This is the safest first step because it prevents the crate from registering anything at runtime.

### 3. Remove panel loading (if it's a panel)

In `crates/zed/src/zed.rs`, remove:
- The `use` import for the panel type
- The `load()` call
- The `add_panel_when_ready()` call
- Any `futures::join!()` participation

### 4. Remove from Cargo dependencies

Remove in this order:
1. `crates/zed/Cargo.toml` — remove the `<crate_name>.workspace = true` line
2. Any other crates that depend on it (check with `rtk grep "<crate_name>" crates/*/Cargo.toml`)
3. `Cargo.toml` (root) — remove from `[workspace.dependencies]`
4. `Cargo.toml` (root) — remove from `[workspace] members` array

### 5. Handle dangling references

After removing the dependency, run `rtk cargo check` and fix compile errors. Common patterns:

- **Settings references**: If other crates read settings defined by the removed crate, you need to either inline the setting type or provide a stub.
- **Action references**: If keybindings reference actions from the removed crate, remove those keybinding entries from `assets/keymaps/default-*.json`.
- **Global state**: If any remaining code calls `cx.global::<TypeFromRemovedCrate>()`, you must either remove that code or provide an alternative.

### 6. Verify at runtime

Compile-time success is not enough. Run the application and exercise the features that were adjacent to the removed crate. Runtime panics from missing globals/settings will show up as:
- "No global of type X has been set"
- "Setting X has not been registered"

### 7. Optionally remove the directory

You can either:
- Delete the `crates/<crate_name>/` directory entirely
- Keep it but just remove it from workspace members (this is what Phase 1 did for ~70 crates)

Keeping directories around is safe — they just won't compile as part of the workspace.

## Quick disable (reversible)

If you want to temporarily disable rather than fully remove:

1. Comment out the crate from `[workspace] members` in root `Cargo.toml`
2. Comment out `<crate_name>.workspace = true` from `crates/zed/Cargo.toml`
3. Comment out the `init()` call in `main.rs`
4. Wrap any imports with `#[cfg(feature = "...")]` if needed

This is easily reversible by uncommenting.

## Known gotchas from this project

- `language_model` crate removal required stubbing `LanguageModelRegistry` global — several UI crates checked for its existence
- `collab` removal required removing the `call` crate too — they have circular event subscriptions
- `vim` removal left dangling keybinding context checks that caused silent failures (no crash, but broken keyboard input in certain modes)
