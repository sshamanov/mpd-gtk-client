# Story 17.1: Populate SharedState CurrentContext from MPD State Changes

Status: done

## Story

As an MPRIS client (or any SharedState reader),
I want `SharedState.current.track` and `SharedState.current.album` to be populated from MPD state changes,
so that `playerctl metadata` returns the correct title, artist, and album for the current track.

## Acceptance Criteria

1. **SharedState populated on every StateChanged event**
   - **Given** a track is playing with artist, title, and album available from MPD
   - **When** the `StateChanged` handler processes the `PlaybackUpdate`
   - **Then** `Store::update_current_context()` is called with the track metadata from the update
   - **And** `SharedState.current.track` contains `Some(Track { artist, title, file, duration })`
   - **And** `SharedState.current.album` contains `Some(album_name)`

2. **SharedState cleared on stop**
   - **Given** no track is playing (MPD state is `stop`)
   - **When** the `StateChanged` handler processes the update
   - **Then** `Store::update_current_context()` is called with `CurrentContext { track: None, album: None }`

3. **MPRIS metadata returns real values**
   - **Given** a track is playing
   - **When** `playerctl metadata` is invoked
   - **Then** `xesam:title`, `xesam:artist`, and `xesam:album` return the correct values
   - **And** `mpris:length` returns the correct duration in microseconds

4. **Existing UI labels unchanged**
   - **Given** the `StateChanged` handler processes a `PlaybackUpdate`
   - **When** the handler runs
   - **Then** the existing `update_now_playing()` call still updates all GTK labels correctly
   - **And** no GTK widget changes are needed — this is purely a SharedState write

5. **No race conditions on SharedState**
   - **Given** the UI thread processes MPD events
   - **When** `update_current_context()` is called
   - **Then** no panic or deadlock occurs (RwLock is write-locked briefly)
   - **And** the write completes within the same event handler iteration

## Tasks / Subtasks

- [ ] (AC: 1-2) Add `update_current_context()` call in `StateChanged` handler in `src/ui/mod.rs`
  - [ ] Map `PlaybackUpdate` fields to `Track` struct (artist, title, file path, duration)
  - [ ] Call `store.update_current_context(|ctx| { ctx.track = track; ctx.album = album; })`
  - [ ] Handle None cases: when stop state, clear both fields
- [ ] (AC: 3) Verify MPRIS metadata output
  - [ ] Build with `--features mpris` and `mpris.enabled = true` in config
  - [ ] Manual test: `playerctl --player=mpdclient metadata` returns non-empty values
  - [ ] Manual test: `playerctl --player=mpdclient status` returns Playing/Paused correctly
- [ ] (AC: 4) Verify no regression in existing UI
  - [ ] `cargo test` passes (all 82+ tests)
  - [ ] Manual: verify Now Playing labels still update correctly
- [ ] (AC: 5) Verify no SharedState contention
  - [ ] Ensure write happens on GTK main thread (it already is — StateChanged runs on the UI event handler)

## Dev Notes

### Root Cause

The `StateChanged` handler in `src/ui/mod.rs` (around line 1768) receives `MpdEvent::StateChanged(update)` where `update` is a `PlaybackUpdate` struct containing `artist`, `title`, `album`, `elapsed`, `duration`, `state`, `format`, `song`, `volume`. The handler calls `update_now_playing()` which writes to GTK labels (title, artist, album, seekbar, time_display, format_badge, cover) but NEVER writes to `SharedState.current`.

The `Store::update_current_context()` method already exists in `src/state/mod.rs` (line 161-168) but is never called from any handler. The `CurrentContext` struct has `track: Option<Track>` and `album: Option<String>` fields, both initialized to `None` in `create_initial_state()`.

### Fix Location

Single file: `src/ui/mod.rs`, in the `MpdEvent::StateChanged` match arm (around line 1768-1792).

### Data Mapping

The `PlaybackUpdate` struct (from `src/mpd/state_machine.rs:69`):
```rust
pub struct PlaybackUpdate {
    pub state: String,        // "play", "pause", "stop"
    pub song: Option<u32>,    // MPD song position
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub volume: i16,
    pub elapsed: Option<f64>,
    pub duration: Option<f64>,
    pub playlist_version: Option<String>,
    pub format: Option<String>,
}
```

Maps to `CurrentContext`:
- `track: Some(Track { artist, title, file: ???, duration })` — the `file` field is not in `PlaybackUpdate`; it needs to be retrieved from the queue state OR set to the current queue item's file. Alternatively, `file` can be left as `String::new()` since MPRIS only uses it for `mpris:artUrl` which is derived from the cover path, not the file path. See `src/mpris.rs` lines 197-206 for how the track fields are consumed.
- `album: update.album.clone()`

### Existing MPRIS Consumption

In `src/mpris.rs`:
- Line 156: `self.read_state(|s| s.current.track.is_some())` — returns `CanPlay`/`CanPause`/etc.
- Lines 197-206: builds metadata dict from `s.current.track` and `s.current.album` — currently always empty

### Thread Safety

- `StateChanged` handler runs on the GTK main loop thread
- `SharedState` is `Arc<RwLock<AppState>>` — `Send + Sync`
- The write happens on the same thread as other UI updates, so no additional synchronization is needed
- MPRIS reads happen on the zbus IO thread via `state.read().unwrap()` — safe because RwLock

### Testing

- `cargo test` — verify zero regressions
- Manual with MPRIS: `cargo run --features mpris` with `[mpris] enabled = true` in config, then `playerctl --player=mpdclient metadata` should show real data
- No automated D-Bus tests in v1 (requires session bus)

### References

- [Source: epics.md §17] Epic 17: MPRIS & SharedState Integration
- [Source: src/state/mod.rs §24-27] CurrentContext struct definition
- [Source: src/state/mod.rs §161-168] Store::update_current_context() method (exists, never called)
- [Source: src/mpd/state_machine.rs §69-80] PlaybackUpdate struct definition
- [Source: src/ui/mod.rs §1768-1792] StateChanged handler — location where fix is applied
- [Source: src/mpris.rs §156, 197-206] MPRIS metadata getters reading CurrentContext

### Dev Agent Record

#### Agent Model Used

Claude Code (deepseek-v4-flash)

#### Debug Log References

- Deferred from code review of story 14-2 (2026-05-01)
- Review finding: "SharedState playback fields never populated — Pre-existing architectural issue: MPD StateChanged handler in ui/mod.rs extracts metadata to local vars but never writes to SharedState."

#### Completion Notes List

#### File List

- `src/ui/mod.rs` (edit — single change location)
