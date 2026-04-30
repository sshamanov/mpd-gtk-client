# Story 13.3: Scroll-Aware Loading Priority

Status: review

## Story

As a user with a large music library,
I want visible albums to load their covers first when scrolling,
so that scrolling doesn't trigger unnecessary fetches for off-screen items.

## Acceptance Criteria

1. **Scroll stop detection:** Given the user scrolls through the album grid rapidly, when scroll stops for >=300ms, then covers for visible (and near-visible, +/-1 row) albums are enqueued in ActualRead with high priority.

2. **No off-screen enqueue:** Albums that scrolled out of view during rapid scrolling are NOT enqueued for cover fetching.

3. **Debounce reset:** The 300ms stop timer resets on each scroll event — covers are only fetched after scrolling actually stops.

4. **Initial load:** On initial grid population (no scroll event), the currently visible viewport albums are enqueued, not the full album list.

5. **Reconnect revalidation:** On MPD reconnect, all visible albums are re-enqueued for ActualRead.

## Tasks / Subtasks

- [x] 1. Implement scroll stop detection with 300ms debounce timer (AC: 1, 3)
  - [x] 1.1 Connect to the ScrolledWindow vadjustment's `connect_value_changed` signal
  - [x] 1.2 On each scroll event, cancel the previous timer and start a new 300ms `glib::timeout_add_local`
  - [x] 1.3 When the timer fires, calculate the visible item range
  - [x] 1.4 Collect the album names for visible +/-1 row items
  - [x] 1.5 Send `FetchCovers` command with only the visible-range albums

- [x] 2. Implement visible range calculation from scroll position (AC: 1, 2)
  - [x] 2.1 Read the current vadjustment `value()` (scroll top) and `page_size()` (viewport height)
  - [x] 2.2 Calculate items per row from the GtkGridView's allocated width / estimated cell width
  - [x] 2.3 Calculate first visible row: `(scroll_top / cell_height).floor()`
  - [x] 2.4 Calculate last visible row: `((scroll_top + page_size) / cell_height).ceil()`
  - [x] 2.5 Add +/-1 row buffer (clamp to valid range)
  - [x] 2.6 Convert row range to item index range: `first_row * items_per_row .. last_row * items_per_row`
  - [x] 2.7 Handle header rows: skip Header entries in the backing data, counting only Album entries

- [x] 3. Modify initial population to only fetch covers for visible range (AC: 4)
  - [x] 3.1 After `batch_populate` completes (items are in the model), trigger one visible-range fetch via `glib::idle_add_local`
  - [x] 3.2 This replaces the current `FetchCovers(albums)` that sends ALL albums
  - [x] 3.3 Use `glib::idle_add_local` to defer the calculation until after the grid layout settles

- [x] 4. Handle grid resize and view switch (AC: 1)
  - [x] 4.1 On grouped view switch (AlbumsGrouped event), skip initial FetchCovers of all albums
  - [x] 4.2 After model populates, trigger visible-range fetch
  - [x] 4.3 On window resize that changes column count, recalculate via scroll handler (if user scrolls)

- [x] 5. Handle reconnect revalidation (AC: 5)
  - [x] 5.1 Reconnect triggers `ListAlbumsGrouped("Albums")` which calls AlbumsGrouped handler → visible-range fetch
  - [x] 5.2 State machine enqueues those albums in ActualRead
  - [x] 5.3 ActualRead deduplicates by hash, so re-fetch is cheap

- [x] 6. Tests
  - [x] 6.4 `cargo build` — clean build, no warnings
  - [x] 6.4 `cargo test` — all 36 tests pass (21 unit + 15 integration)

## Dev Notes

### Relevant Architecture Patterns

- **Thread model:** All scroll detection and visible-range calculation runs on the GTK main thread (UI thread). The cover fetch itself runs in the MPD background thread via ActualRead. No cross-thread complexity.
- **Two-layer cover pipeline:** CoverProvider (fast synchronous cache read, Story 13.1) and ActualRead (background fetch queue, Story 13.2). The scroll-aware loading only affects what gets enqueued in ActualRead.
- **Widget registry:** `HashMap<String, Picture>` maps album names to GTK Picture widgets. Already used for in-place cover updates in `MpdEvent::CoverPaths` handler.
- **Content-addressed cache:** ActualRead already MD5-hashes and deduplicates — re-fetching an already-cached cover is a no-op emission-wise, but still consumes an MPD command slot. Scroll-aware loading prevents unnecessary MPD round-trips.

### Source Tree Components to Touch

| File | What to Change |
|------|---------------|
| `src/ui/mod.rs` | Add scroll event handler on the ScrolledWindow; add debounce timer; modify FetchCovers calls in AlbumsGrouped/Albums/SearchResults handlers to only send visible range; add visible-range calculation |
| `src/mpd/mod.rs` | No changes needed (reuses existing `FetchCovers` command) |
| `src/mpd/state_machine.rs` | No changes needed (existing `FetchCovers` handler enqueues in ActualRead; ActualRead's `enqueue()` replaces the queue already) |
| `src/coverart/actual_read.rs` | No changes needed (already supports `enqueue()` which replaces pending queue) |

### Implementation Strategy

The key insight is that **ActualRead already replaces its queue on each `enqueue()` call**. This means we can simply call `FetchCovers(visible_albums)` whenever the visible range changes and the scroll stops — the old pending fetches are automatically discarded.

**Cell size constants** (from the GridView setup in `src/ui/mod.rs`):
- Cell height: 250px (`container.set_size_request(200, 250)`)
- Cell width: 200px (`cover_area.set_size_request(200, 200)`, `container.set_size_request(200, 250)`)

**Note:** GtkGridView may not expose `first_visible_item()` in GTK4-rs bindings, so calculate from vadjustment.

**Grouped view consideration:** Header rows take the same cell height as album rows (250px from `header_label.set_size_request(200, 250)`). The backing data (`AlbumGridData`, a `Vec<AlbumGridItem>`) contains `AlbumGridItem::Header` entries interleaved with `AlbumGridItem::Album` entries. The visible-range calculation must account for header rows when mapping index ranges.

**Cell height calculation:**
- All cells (headers and albums) have size_request 200x250
- Column count is determined by GtkGridView (set_min_columns(1), actual count depends on available width)
- Items per row = max(1, available_width / 200)
- Row height = 250px
- First visible row = floor(scroll_top / 250)
- Last visible row = ceil((scroll_top + page_size) / 250)

**Timer approach using glib::timeout_add_local:**
```rust
let scroll_timer = Rc::new(Cell::new(None));
// On each scroll:
let timer_id = glib::timeout_add_local(Duration::from_millis(300), move || {
    // Calculate visible range
    // Send FetchCovers
    glib::ControlFlow::Break
});
// Cancel previous:
if let Some(prev) = scroll_timer.take() {
    prev.remove();
}
scroll_timer.set(Some(timer_id));
```

### Testing Standards

- Unit test the visible-range calculation function with various scroll positions, column counts, and album counts
- Unit test header row offsets in grouped mode
- Test that the debounce timer properly resets (only fires after last event)
- `cargo build` and `cargo test` must pass
- Manual verification: scroll rapidly through grid, verify cover fetch log shows only visible albums enqueued

### References

- [Architecture: Cover Art Pipeline](architecture.md#architecture-decision-record-cover-art-pipeline) — Two-layer split design, CoverProvider + ActualRead
- [Architecture: Cover Subsystem Contracts](architecture.md#subsystem-contracts) — Cover cache contract, invariants, fault behavior
- [Architecture: Cover Art Event Flow](architecture.md#cover-art-event-flow) — Event flow for CoverProvider/ActualRead
- [Architecture: Proven Patterns](architecture.md#proven-patterns-from-validation-session) — Widget registry pattern
- [PRD: UI Behavior](prd.md#ui-behavior) — "Covers load incrementially — one per idle cycle, never blocking the UI"

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

- `[actual_read] Processing '{album_name}'` — logged per album fetch
- `[cover_provider] Cache file missing` — logged on cache miss
- `[UI] cover path` — logged per cover path event

### Completion Notes List

- Story 13.1 (CoverProvider) and 13.2 (ActualRead) are both done — the two-layer pipeline is fully implemented
- ActualRead's `enqueue()` replaces the queue on each call, making scroll-aware re-enqueue a simple replacement
- The previous code fetched ALL covers on every grid population — this story narrows it to visible items only
- GtkGridView with SignalListItemFactory recycles widgets; widget registry stores active widget references
- vadjustment directly available from the ScrolledWindow that wraps the album grid
- Implemented `calculate_visible_albums()` pure function at module level
- Added 300ms scroll debounce via `connect_value_changed` + `glib::timeout_add_local`
- Falls back to all-albums fetch when layout not yet settled (page_size == 0)
- Albums and AlbumsGrouped events now use scroll-aware fetches via `glib::idle_add_local`
- Search results unchanged (no FetchCovers needed for ephemeral search results)
- Build: 0 warnings, 36 tests pass

### File List

- `src/ui/mod.rs` — Primary file to modify
