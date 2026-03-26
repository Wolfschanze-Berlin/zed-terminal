---
name: build-check
description: Run the zed-terminal build verification pipeline correctly. Use this skill when you need to check if the project compiles, run clippy, run tests, verify a change doesn't break anything, or before committing code. Also use when you see unexpected build failures, clippy errors, or need to understand why `cargo clippy` produces different results than the CI. This project has specific build commands that differ from standard Rust projects.
---

# zed-terminal Build Verification

This project has specific build commands that differ from vanilla `cargo` usage. Using the wrong command leads to different results than CI, wasting time on false positives or missing real issues.

## Quick Reference

| Task | Command | Notes |
|------|---------|-------|
| **Type check** | `rtk cargo check` | Fastest feedback loop. Checks default member (zed binary). |
| **Check specific crate** | `rtk cargo check -p <crate_name>` | Useful when working on a single crate. |
| **Clippy** | `rtk ./script/clippy` | NEVER use `cargo clippy` directly. The script adds `--release --all-targets --all-features -- --deny warnings`. |
| **Clippy one crate** | `rtk ./script/clippy -p <crate_name>` | Passes `-p` through to cargo clippy. |
| **Build** | `rtk cargo build` | Debug build of the zed binary. |
| **Release build** | `rtk cargo build --release` | What CI and clippy use. |
| **Test** | `rtk cargo test -p <crate_name>` | Always scope tests to a crate — running workspace-wide tests takes very long. |
| **Format** | `cargo fmt` | Standard rustfmt with workspace config from `rustfmt.toml`. |

## Why `./script/clippy` instead of `cargo clippy`

The script (`script/clippy`) does three things `cargo clippy` alone doesn't:

1. Adds `--workspace` if no `-p` flag is given (checks everything, not just the default member)
2. Adds `--release --all-targets --all-features` (matches CI exactly)
3. Adds `-- --deny warnings` (turns warnings into errors, matching CI)

If you just run `cargo clippy`, you'll check only the default member in debug mode without all features. You'll miss issues that CI catches.

Locally, the script also runs `cargo-machete` (unused deps) and `typos` (spell check) if installed.

## Build order for verification

When verifying a change is safe to commit:

1. `rtk cargo check` — fast type-check, catches most issues
2. `rtk ./script/clippy -p <crate_you_changed>` — catches lint issues in your crate
3. `rtk cargo test -p <crate_you_changed>` — if the crate has tests
4. `rtk cargo build` — full build to catch linker issues (rare but happens with new crates)

For major changes (new crate, crate removal, dependency changes):
1. `rtk cargo check`
2. `rtk ./script/clippy` — full workspace clippy
3. `rtk cargo build` — full debug build

## Common build issues

**"unused import" warnings becoming errors**: Clippy runs with `--deny warnings`. An import that's only used in test builds will fail in non-test clippy. Use `#[cfg(test)]` on test-only imports.

**"feature X not found"**: The project uses `--all-features`, so any feature flags you define must actually exist in your Cargo.toml.

**Linker errors with new crates**: If you add a new crate but forget to add it to `[workspace.dependencies]` in root `Cargo.toml`, the workspace resolver won't find it. You'll get a confusing "no matching package" error rather than a linker error.

**Windows-specific**: The primary development platform is Windows. Some crates have `#[cfg(target_os)]` guards. If you add platform-specific code, make sure it compiles on the current platform.
