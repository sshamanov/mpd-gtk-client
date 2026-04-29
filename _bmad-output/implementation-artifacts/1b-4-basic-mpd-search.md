# Story 1b.4: Basic MPD Search

Status: review

## Story

As a user,
I want to search my album library by typing text,
so that I can quickly find albums by title, artist, or other metadata.

## Acceptance Criteria

1. **Search bar** — A `gtk4::SearchEntry` in the left pane, below the group bar and above the grid:
   - Placeholder text: "Search albums..."
   - Emits `search-changed` on each keystroke
   - Has a clear button (built into SearchEntry)

2. **MPD search integration** — Typing in the search bar queries MPD's `search` command:
   - Send `MpdCommand::Search(String)` to the MPD background thread
   - MPD thread executes `search any "<query>"` and sends back matching albums as `MpdEvent::SearchResults(Vec<(String, String)>)` — (artist, album_name) pairs
   - Results replace the album grid content
   - Only album-level results (deduplicate by album name)
   - When search is cleared (empty query), restore the current group view (Albums/Artists/Years/Genres)

3. **Debounced input** — Keystrokes are debounced at 150ms:
   - Use `glib::timeout_add` or `glium::idle_add` pattern
   - Each keystroke cancels the previous pending search timer
   - Only send the MPD command after 150ms of no typing

4. **Keyboard shortcuts** — Standard search UX:
   - `Ctrl+F` focuses the search entry
   - `Escape` clears the search and restores group view

5. **`cargo test` passes** — All existing tests pass; clippy clean

## Tasks / Subtasks

- [x] Task 1: Add Search command to MPD adapter and state machine (AC: 2)
  - [x] Added `MpdAdapter::search_albums(query)` using `search any "<query>"` with album dedup
  - [x] Added `MpdCommand::Search(String)` + `MpdEvent::SearchResults(Vec<(String, String)>)`

- [x] Task 2: Build search bar with debounce (AC: 1, 3)
  - [x] `gtk4::SearchEntry` between group bar and grid, "Search albums..." placeholder
  - [x] 150ms debounce via `glib::timeout_add_local_once` (cancels previous on each keystroke)

- [x] Task 3: Wire search results to grid (AC: 2, 4)
  - [x] SearchResults populates grid via `populate_album_grid`
  - [x] Empty search / Escape restores Albums group via `connect_stop_search`
  - [x] Ctrl+F focuses search via GAction + accelerator

- [x] Task 4: Verify no regressions (AC: 5)
  - [x] `cargo build`, `cargo clippy`, `cargo test` pass

## Dev Notes

### MPD Search Command

MPD's `search any "<query>"` searches all tags. Response format:
```
file: path/to/file.flac
Artist: Name
Title: Track
Album: AlbumName
...
```
Deduplicate by album name to get a flat album list.

### Debounce Pattern

```rust
let search_timer: std::cell::Cell<Option<glib::SourceId>> = std::cell::Cell::new(None);
search_entry.connect_search_changed(move |entry| {
    if let Some(id) = search_timer.take() {
        id.remove(); // cancel previous
    }
    let query = entry.text().to_string();
    if query.is_empty() { return; }
    let id = glib::timeout_add_local(Duration::from_millis(150), move || {
        let _ = cmd_tx.send(MpdCommand::Search(query));
        glib::ControlFlow::Break
    });
    search_timer.set(Some(id));
});
```

### What NOT to Do
- Do NOT implement local search index — that's Epic 4a
- Do NOT implement relevance scoring — that's Epic 4a
- Do NOT implement mode-scoped search strategies — that's Epic 4a
- Do NOT add new dependencies

### References
- [Source: epics.md#Epic 1b] — FR-B10, FR-S1 (basic)
- [Source: epics.md#Epic 4a] — full indexed search (future)

### Review Findings

#### Patch Findings

- [x] [Review][Patch] Artist-album pairing wrong in `search_albums` — fixed: two-pass approach (first map artist→album, then build results) [mpd/mod.rs]
- [x] [Review][Patch] `stop_search` doesn't cancel debounce timer — fixed: `se_timer.take()` in both stop_search and search_changed [ui/mod.rs]
- [x] [Review][Patch] Backspace-to-empty keeps stale search results — fixed: empty query restores Albums group [ui/mod.rs]
- [x] [Review][Patch] Search clear always restores "Albums", not the previously active group — fixed: `Rc<RefCell<Option<String>>>` tracks prior group, restored on clear [ui/mod.rs]
- [x] [Review][Defer] No sequence ID on search requests — deferred: added Cell<u64> gen counter but not wired into results filtering yet [ui/mod.rs]
- [x] [Review][Patch] search_albums dedup drops same-name albums — fixed: two-pass preserves first artist per album [mpd/mod.rs]

#### Deferred

- [x] [Review][Defer] Newline injection in MPD commands — pre-existing across multiple methods, needs systematic fix
- [x] [Review][Defer] Large result set causes UI freeze — acceptable for v1, FlowBox is non-virtualizing
- [x] [Review][Defer] No test coverage for search — mock server needs enhancement
- [x] [Review][Defer] SourceId in Cell leaks on closure drop — acceptable for app-lifetime SearchEntry

## Dev Agent Record

### Completion Notes List

- ✅ Added `MpdAdapter::search_albums(query)` — `search any` with album-name dedup
- ✅ Added `MpdCommand::Search(String)` + `MpdEvent::SearchResults(Vec<(String, String)>)`
- ✅ SearchEntry with 150ms debounce via `glib::timeout_add_local_once`
- ✅ Ctrl+F focuses search via GAction, Escape clears via `connect_stop_search` → restores Albums view
- ✅ Search results populate grid, empty results show "No results found"

### File List

- `src/mpd/mod.rs` — MODIFIED: added `search_albums()` method
- `src/mpd/state_machine.rs` — MODIFIED: added `Search` command + `SearchResults` event + handler
- `src/ui/mod.rs` — MODIFIED: SearchEntry, debounce timer, Ctrl+F action, Escape handler, grid population
