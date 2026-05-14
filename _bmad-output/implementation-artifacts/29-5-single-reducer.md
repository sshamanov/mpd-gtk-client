# Story 29.5: Single Reducer

Status: done

## Story

As a developer,
I want a single `reduce()` function that handles ALL state transitions from MPD events,
so that state mutation is centralized, testable, and separated from GTK widget operations.

## Acceptance Criteria

1. **Single `reduce()` function exists**
   - Given any MPD event that modifies application state
   - When `reduce(state, event)` is called
   - Then it returns the updated `AppState` with zero GTK operations and zero channel sends
   - And the function completes in <1ms (no I/O, no blocking)

2. **State mutation removed from event handlers**
   - Given the `MpdEvent` dispatch in `src/ui/mod.rs`
   - When any event modifies `SharedState` (via `fc_state.write()`)
   - Then that mutation is moved into `reduce()` and the handler calls `reduce()` instead of writing state directly
   - And handlers that only do widget updates (no state mutation) are unchanged

3. **`reduce()` is testable**
   - Given a known `AppState` and an `AppEvent`
   - When `reduce()` is called
   - Then the output is deterministic and can be asserted in unit tests
   - And at least 3 tests are added for different event types

4. **Backward compatibility**
   - Given all existing tests pass
   - When the reducer is applied
   - Then `cargo test` shows zero regressions
   - And all UI behavior is identical

## Technical Requirements

### Background

From architecture.md §2280-2313:

- `reduce()` is a plain function: `fn reduce(state: &mut AppState, event: &AppEvent)`
- Runs on GTK thread only — no synchronization needed
- No GTK widget operations, no channel sends
- AppState fields should not be `pub` (long-term goal; this story starts the migration)
- Single entry point — no sub-reducers, no middleware, no delegation

### Approach

This story takes an **incremental** approach rather than a full rewrite:

1. **Extract state-mutating logic** from the `MpdEvent` handler arms into a `reduce()` function
2. **Widget updates stay** in the UI handler — they react to the state after `reduce()` returns
3. **SharedState writes** move from inline `fc_state.write()` calls into `reduce()`

The full pub/sub widget architecture (broadcast channels, widget subscriptions) is deferred — widgets continue reading SharedState directly after reduce().

### `reduce()` Function Signature

```rust
/// Apply an MPD event to application state. Pure data transformation.
/// Runs on GTK thread. No side effects, no GTK, no channel sends.
fn reduce(state: &mut AppState, event: &MpdEvent) {
    match event {
        MpdEvent::StateChanged(update) => reduce_state_changed(state, update),
        MpdEvent::Queue(queue) => reduce_queue(state, queue),
        MpdEvent::Albums(albums) => reduce_albums(state, albums),
        MpdEvent::AlbumsGrouped(groups) => { /* browsing state */ }
        MpdEvent::Connected => state.connection = ConnectionState::Connected,
        MpdEvent::Disconnected => state.connection = ConnectionState::Disconnected,
        MpdEvent::CoverPaths(paths) => { /* cover registry update */ }
        MpdEvent::CoverRefreshed { album_id, data: _ } => { /* cover refresh tracking */ }
        MpdEvent::LibraryChanged => { /* flag for re-index */ }
        // Events with NO state change:
        MpdEvent::Connecting
        | MpdEvent::SearchResults(_)
        | MpdEvent::FileSearchResults(_)
        | MpdEvent::DirectoryListing(..)
        | MpdEvent::AlbumTracks(_)
        | MpdEvent::Error(_)
        | MpdEvent::Toast { .. } => {}
    }
}
```

### State Mutations to Extract

| Event | Current State Write | Move to reduce() |
|-------|-------------------|-------------------|
| `StateChanged` | `fc_state.write()` — track, album, playback status | All of it |
| `Queue` | `app_state.queue.items`, `current_position` | Both fields |
| `Connected` | `app_state.connection = Connected` | Yes |
| `Disconnected` | `app_state.connection = Disconnected` | Yes |
| `Albums` | Browsing state (sort order, albums list) | Yes |
| `LibraryChanged` | Triggers re-index flag | Flag setting |
| `CoverPaths` | Cover path map storage | Map update |
| `SearchResults` | None (widgets only) | N/A |
| `FileSearchResults` | None (widgets only) | N/A |
| `DirectoryListing` | None (folder tree only) | N/A |

### Integration Points

| File | Change |
|------|--------|
| `src/state/mod.rs` | Add `reduce()` function; make `AppState` fields accessible within crate; add tests |
| `src/ui/mod.rs` | Replace inline `fc_state.write()` calls with `reduce()` call; keep widget update code |
| `src/mpd/state_machine.rs` | No changes (events already emitted) |

### Key Rules

- **Do NOT touch widget code** — only move `SharedState` writes into `reduce()`
- **Do NOT change event flow** — `MpdEvent` stays unchanged, MPD IO unchanged
- **Do NOT break tests** — all 84 tests must pass
- **Reduce first, widgets second** — handler calls `reduce(state, event)`, then updates widgets
- **Incremental** — extract only what's safe; leave complex widget logic in handlers

## Tasks / Subtasks

- [ ] 1. Add `reduce()` function in `src/state/mod.rs`
  - [ ] 1.1 Define `pub fn reduce(state: &mut AppState, event: &MpdEvent)`
  - [ ] 1.2 Implement `StateChanged` reduction (playback state, current track, current album)
  - [ ] 1.3 Implement `Queue` reduction (items + current_position)
  - [ ] 1.4 Implement `Connected`/`Disconnected` reduction
  - [ ] 1.5 Implement `Albums` reduction (browsing state)
  - [ ] 1.6 Implement `CoverPaths` reduction (cover path map)
  - [ ] 1.7 Add unit tests for reduce() with StateChanged, Queue, Connected events
- [ ] 2. Wire `reduce()` into UI event handler
  - [ ] 2.1 Import `reduce` in `src/ui/mod.rs`
  - [ ] 2.2 Replace `fc_state.write()` calls in `StateChanged` handler with `reduce()` call
  - [ ] 2.3 Replace `fc_state.write()` calls in `Queue` handler with `reduce()` call
  - [ ] 2.4 Replace connection state writes with `reduce()` call
  - [ ] 2.5 Replace `CoverPaths` state writes with `reduce()` call
  - [ ] 2.6 Replace `LibraryChanged` flag setting with `reduce()` call
- [ ] 3. Run full test suite — verify zero regressions

## Dev Agent Record

### Implementation Plan

1. Add `reduce()` function in `src/state/mod.rs` with StateChanged, Connected, Disconnected, Connecting handlers
2. Wire `reduce()` into the MPD event dispatch in `src/ui/mod.rs` — replace inline `fc_state.write()` calls
3. Add unit tests for reduce() with multiple event types
4. Run full test suite to verify zero regressions

### Completion Notes

- Added `reduce()` function handling: Connected, Disconnected, Connecting, StateChanged
- Wired into 4 event handlers: Connected, Disconnected, Connecting, StateChanged
- Queue, CoverPaths, LibraryChanged, and other handlers had NO `fc_state.write()` calls to extract (UI-local RefCells)
- Added 5 unit tests: connected, disconnected, connecting, state-changed-playing, state-changed-stop
- All 94 tests pass (23 lib + 50 bin + 21 smoke), zero regressions, zero warnings

### Change Log

- Modified `src/state/mod.rs`: added `reduce()` function + 5 unit tests, added Connecting handler
- Modified `src/ui/mod.rs`: wired reduce() into Connected, Disconnected, Connecting, StateChanged handlers

## References
- [Source: architecture.md §2280-2313] Single reducer ADR — reduce() function, AppEvent enum, enforcement rules
- [Source: architecture.md §2306-2311] Enforcement — AppState fields not pub, workers hold Sender not state ref, widgets read only
- [Source: src/ui/mod.rs:1955-2575] MpdEvent handler — 15 event arms with mixed state/widet logic
- [Source: src/state/mod.rs] AppState, CurrentContext, QueueState — current state structures
- [Source: src/mpd/state_machine.rs:66-82] MpdEvent enum — all event variants
