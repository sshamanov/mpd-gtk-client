# Migration: Workspace Restructure & gtk4 Upgrade

## Goal

Collapse 5-sub-crate workspace into a single crate with flat module hierarchy. Upgrade gtk4-rs 0.8 → 0.11.

## Step 1: git mv — traceable file moves

Run these commands from the project root to preserve `git log --follow` history:

```bash
# Core module moves
git mv mpd-adapter/src/lib.rs src/mpd/mod.rs
git mv state/src/lib.rs src/state/mod.rs
git mv ui/src/lib.rs src/ui/mod.rs
git mv cover-fetcher/src/lib.rs src/coverart/mod.rs
git mv bin/src/main.rs src/main.rs

# Empty directories to remove after migration
rmdir mpd-adapter/src mpd-adapter
rmdir state/src state
rmdir ui/src ui
rmdir cover-fetcher/src cover-fetcher
rmdir bin/src bin
```

## Step 2: Create module structure

Create directories and mod.rs files:

```bash
mkdir -p src/presenters/browse
mkdir -p src/ui/widgets/{album_grid,folder_tree,queue_album,queue_track,now_playing,cover_display,search_bar,toast_overlay,settings_dialog}
mkdir -p src/coverart/providers
mkdir -p src/search
mkdir -p src/utils
mkdir -p src/config
```

## Step 3: Create new source files

| File | Purpose |
|------|---------|
| `src/app.rs` | Thin wiring — create state, presenters, connect signals |
| `src/errors.rs` | UserFacingError trait, ErrorSinkEvent, ErrorLevel |
| `src/constants.rs` | Layout constants |
| `src/ui/gtk_reexport.rs` | gtk4/glib re-exports for all widget files |

## Step 4: Rewrite root Cargo.toml

Replace workspace `[workspace]` + sub-crate refs with single-crate manifest:
- Remove: tokio, async-trait, futures, reqwest, id3, mp4ameta, lru, anyhow
- Update: gtk4 0.8 → 0.11 (v4_14), gdk4 0.8 → 0.11, gdk-pixbuf 0.20
- Add: ureq, rustls, toml, dirs, env_logger, image, unicode-normalization, serde_json
- Set: edition = "2024", rust-version = "1.85"

## Step 5: Verify

```bash
cargo build
cargo clippy -D warnings
```

## Original-to-Target File Map

| Original path | Target path |
|---------------|-------------|
| `mpd-adapter/src/lib.rs` | `src/mpd/mod.rs` |
| `state/src/lib.rs` | `src/state/mod.rs` |
| `ui/src/lib.rs` | `src/ui/mod.rs` |
| `cover-fetcher/src/lib.rs` | `src/coverart/mod.rs` |
| `bin/src/main.rs` | `src/main.rs` |
