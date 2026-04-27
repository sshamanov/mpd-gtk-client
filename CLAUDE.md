# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

A GTK4-based MPD (Music Player Daemon) client in Rust with two primary workflows:
1. **Album Mode** — visual cover grid with hover controls, grouped views, local search
2. **Folder Mode** — directory browser with breadcrumbs, cue/DSD normalization

## Architecture

Single Rust crate (edition 2024, MSRV 1.85). No workspace. `src/main.rs` entry point.

### Threading
- **UI thread** — GTK4 main loop, polls event channel every 30ms (64-event batch limit)
- **MPD background thread** — `src/mpd/state_machine.rs`, `connected_loop` with independent 500ms status polling
- **Channel** — `std::sync::mpsc::sync_channel<MpdEvent>(1024)` for MPD→UI events, `mpsc::channel<MpdCommand>` for UI→MPD commands
- No async runtime — `std::thread` over tokio (4.2MB vs 15MB binary)

### Module Map
| Module | Purpose |
|--------|---------|
| `src/main.rs` | Entry point, env_logger init, signal handlers, GTK app launch |
| `src/mpd/mod.rs` | MPD TCP protocol adapter (`MpdAdapter`) |
| `src/mpd/state_machine.rs` | Background thread: connection lifecycle, command dispatch, 500ms status poll |
| `src/mpd/mock.rs` | Mock MPD server for integration tests |
| `src/state/mod.rs` | Application state (`SharedState = Arc<RwLock<AppState>>`) |
| `src/ui/mod.rs` | GTK4 UI: window, menus, album grid, queue, now-playing, settings |
| `src/ui/widgets/` | Reusable widgets: album_cover, folder_tree, toast |
| `src/config/mod.rs` | TOML config persistence (`~/.config/mpd-client/config.toml`) |
| `src/search/mod.rs` | Local keyword→album search index (hash-based, no MPD round-trip) |
| `src/coverart/mod.rs` | Cover art fetching (stub — returns None pending implementation) |
| `src/constants.rs` | Layout constants |
| `src/errors.rs` | Error types |

### Key Architecture Decisions
- **MPD is remote** — no local filesystem assumptions, all data via MPD protocol
- **Status polling** — 500ms independent poll in `connected_loop` (not tied to `recv_timeout`); `fetch_full_update()` combines `status` + `currentsong` for complete metadata
- **External change detection** — `last_song_pos` tracking in periodic poll; triggers `Queue` event on external song changes
- **Channel backpressure** — `try_send` with drop; channel-full warnings at warn level; overflow tolerance via position re-detection on next poll
- **`Rc<RefCell<>>`** — shared state pattern for UI-thread-only data; `Arc<Mutex<>>` for cross-thread state

## Development Status

- **Phase**: v1 implementation complete — all 7 epics done
- **Tests**: 11 integration tests with mock MPD server (`cargo test`)
- **Pending**: integration tests for real MPD, keyboard shortcut docs in-app
- **BMad**: installed for project management (`_bmad/` directory)

## Build & Run

```bash
cargo build
cargo test                    # 11 integration tests
RUST_LOG=info cargo run       # normal operation
RUST_LOG=debug cargo run      # verbose MPD protocol logging
```

## Key Files

- **Specification**: `DESIGN.md` — complete product design and interaction rules
- **Architecture decisions**: `_bmad-output/planning-artifacts/architecture.md`
- **Sprint status**: `_bmad-output/implementation-artifacts/sprint-status.yaml`
- **Deferred work**: `_bmad-output/implementation-artifacts/deferred-work.md`
- **Config**: `~/.config/mpd-client/config.toml` (host/port, auto-created on first save)

## Code Conventions

- No async — all blocking I/O on background thread, all UI on GTK main thread
- Log via `log` crate + `env_logger`; errors at error level, channel-full at warn, timing at debug
- No unwrap/expect in library code; `if let Ok(...)` patterns for fallible operations
- MPD adapter commands return `Result<_, Error>`; callers log errors and continue
- GTK4 0.11 with v4_14 feature; glib 0.20, gdk4 0.11
