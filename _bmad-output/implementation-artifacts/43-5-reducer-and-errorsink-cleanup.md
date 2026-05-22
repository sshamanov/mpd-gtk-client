# Story 43-5: Normalize Single Reducer and Remove ErrorSink

## Status: done

## Context

Architecture.md Consolidated Refinements §4 requires a single `reduce(AppState, AppEvent) -> AppState` function as the sole state mutation entry point. Currently:
- `reduce()` in `state/mod.rs` handles only Connected/Connecting/Disconnected/StateChanged
- Direct `state.write()` calls exist in `ui/mod.rs` for mode switches and drag-drop
- ErrorSinkEvent is dead code in `errors.rs`

See `.claude/plans/glittery-wiggling-puzzle.md` Steps 13-14.

## Tasks

### Task 1: Expand `reduce()` to cover all MpdEvent variants
In `src/state/mod.rs`:
- Replace catch-all arm with explicit match arms for every `MpdEvent` variant
- Variants that don't mutate state (Queue, Albums, AlbumsGrouped, CoverPaths, CoverRefreshed, SearchResults, etc.) get explicit no-op arms with doc comments explaining they're UI-display events
- `Toast` → push to new `AppState.toast_queue: VecDeque<(String, ToastLevel)>`
- `Error` → convert to Toast via toast_queue

### Task 2: Add `toast_queue` to AppState
- New field: `pub toast_queue: VecDeque<(String, ToastLevel)>`
- Initialize as `VecDeque::new()` in `create_initial_state()`

### Task 3: Move all direct `state.write()` calls into reduce()
- Mode toggle (`album_browsing.scroll_position` + mode switch) → existing `Store::switch_mode()` pattern
- `custom_album_order` from drag-drop → add `AlbumBrowsingState::custom_album_order` field update in reduce

### Task 4: Verify no direct `state.write()` remains outside `reduce()`
```bash
grep -rn "\.write()" src/ --include='*.rs' | grep -v reduce | grep -v test | grep -v Store
```

### Task 5: Verify ErrorSink removal
Already done in Story 43-1 Task 3. If not done, remove `ErrorSinkEvent`, `UserFacingError`, `ErrorLevel` from `src/errors.rs`.

## Acceptance Criteria
1. `reduce()` has explicit arms for every `MpdEvent` variant — no catch-all
2. `AppState.toast_queue` field exists and is populated by Toast/Error events
3. No `state.write()` calls outside of `reduce()` or `Store` methods
4. `errors.rs` contains only structural content — no ErrorSinkEvent/UserFacingError/ErrorLevel
5. `cargo build` passes
6. `cargo test` passes (17 integration tests + unit tests)
