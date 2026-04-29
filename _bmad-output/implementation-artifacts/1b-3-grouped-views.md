# Story 1b.3: Grouped Views

Status: done

## Story

As a user,
I want to browse my album library grouped by artist, year, or genre,
so that I can navigate by different organizational structures.

## Acceptance Criteria

1. **Group selector bar** — A horizontal bar above the album grid with group mode buttons:
   - Options: "Albums" (default, flat list), "Artists", "Years", "Genres"
   - Use `gtk4::ToggleButton` in a `Box` with a single active state
   - Active button is visually distinct (CSS `:checked` state)
   - Bar is between the left pane top and the album grid

2. **Grouped album queries** — `MpdAdapter::list_albums_grouped(group: &str)` returns data with headers:
   - `"Albums"` — existing flat list  (`list album group Artist` → flatten to (artist, album))
   - `"Artists"` — `list album group Artist` (returns Artist→[Album, ...])
   - `"Years"` — `list album group Date` (returns Year→[Album, ...]), albums without Date tagged as "Unknown Year"
   - `"Genres"` — `list album group Genre` (returns Genre→[Album, ...]), albums without Genre tagged as "Unknown Genre"
   - Each mode returns `Vec<(String, Vec<(String, String)>)>` — (group_header, [(artist, album_name), ...])

3. **Grouped grid display** — When a group mode is active:
   - Each group has a header label (bold, left-aligned, with group name + count)
   - Albums within each group are displayed as grid cells (same AlbumCover widget)
   - Group headers are not pinned/sticky — that's deferred

4. **Grid re-population** — Switching group modes re-populates the grid:
   - Send `MpdCommand::ListAlbumsGrouped(group_type)` to the MPD background thread
   - Response returns as `MpdEvent::AlbumsGrouped(Vec<(String, Vec<(String, String)>)>)`
   - UI clears and rebuilds the grid with headers

5. **`cargo test` passes** — All existing tests pass; clippy clean

## Tasks / Subtasks

- [x] Task 1: Add grouped album query support to MpdAdapter (AC: 2)
  - [x] Added `list_albums_grouped(group)` that dispatches to MPD `list album group {type}` commands
  - [x] Supports "Artist", "Date", "Genre", "Albums" (flat fallback)

- [x] Task 2: Add `ListAlbumsGrouped` command/event to state machine (AC: 2, 4)
  - [x] Added `MpdCommand::ListAlbumsGrouped(String)`, `MpdEvent::AlbumsGrouped(AlbumGroup)`
  - [x] Added `AlbumGroup` type alias (`Vec<(String, Vec<(String, String)>)>`)
  - [x] Handler in connected_loop

- [x] Task 3: Build group selector bar (AC: 1)
  - [x] 4 ToggleButtons in a horizontal Box above the grid
  - [x] Only one active at a time, sends `ListAlbumsGrouped` on toggle

- [x] Task 4: Render grouped grid with headers (AC: 3)
  - [x] `populate_grouped_grid()` inserts header labels + album cells per group
  - [x] Headers show group name + count
  - [x] "Albums" mode uses flat list via `list_albums()` internally

- [x] Task 5: Verify no regressions (AC: 5)
  - [x] `cargo build`, `cargo clippy`, `cargo test` pass

## Dev Notes

### MPD Commands

```
list album group Artist    → Artist: name \n Album: name
list album group Date      → Date: 2024 \n Album: name
list album group Genre     → Genre: name \n Album: name
```

### Group Selector Layout

```
[Albums] [Artists] [Years] [Genres]  ← group bar
┌─────────────────────────────────┐
│ Header: Artist Name (12)        │
│ [cover] [cover] [cover]         │
│ Header: Another Artist (8)      │
│ [cover] [cover]                 │
└─────────────────────────────────┘
```

### What NOT to Do
- Do NOT implement sticky/pinned group headers — deferred
- Do NOT implement hover controls (already in 1b-2)
- Do NOT modify right rail or now-playing
- Do NOT add new dependencies

### References
- [Source: epics.md#Epic 1b] — FR-B3, FR-B4
- [Source: ux-design-specification-enhanced.md#UX-DR13] — breadcrumb navigation (group context)

### Review Findings

#### Patch Findings

- [x] [Review][Patch] MPD tag names wrong — fixed: mapping from display labels to "Artist"/"Date"/"Genre" [ui/mod.rs]
- [x] [Review][Patch] FlowBox homogeneous — fixed: non-homogeneous mode for grouped, headers span via size_request [ui/mod.rs]
- [x] [Review][Patch] album_names not updated in grouped mode — fixed: update store from grouped data [ui/mod.rs]
- [x] [Review][Patch] ToggleButtons lack radio-group — fixed: set_group on buttons [ui/mod.rs]
- [x] [Review][Patch] Duplicate ListAlbumsGrouped command — fixed: removed explicit startup send [ui/mod.rs]
- [x] [Review][Patch] "Unknown Year"/"Unknown Genre" fallback — fixed: query flat list, diff against grouped, add untagged [mpd/mod.rs]

#### Deferred

- [x] [Review][Defer] No child virtualization for large groups — acceptable for v1, group by moderate tags
- [x] [Review][Defer] Mock server doesn't test grouped commands — acceptable for v1
- [x] [Review][Defer] Newline injection in album names — pre-existing from 1b-1, needs systematic fix

## Dev Agent Record

### Completion Notes List

- ✅ Added `MpdAdapter::list_albums_grouped(group)` for Artist/Date/Genre/Albums views
- ✅ Added `MpdCommand::ListAlbumsGrouped(String)` + `MpdEvent::AlbumsGrouped(AlbumGroup)`
- ✅ Added `AlbumGroup` type alias for `Vec<(String, Vec<(String, String)>)>`
- ✅ Group selector bar with 4 ToggleButtons above grid (Albums, Artists, Years, Genres)
- ✅ `populate_grouped_grid()` renders header labels + album cells per group
- ✅ CSS for group headers (bold) and selected group button

### File List

- `src/mpd/mod.rs` — MODIFIED: added `AlbumGroup` type alias, `list_albums_grouped()`
- `src/mpd/state_machine.rs` — MODIFIED: added `ListAlbumsGrouped` command + `AlbumsGrouped` event + handler
- `src/ui/mod.rs` — MODIFIED: group selector bar, `populate_grouped_grid()`, `AlbumsGrouped` event handler, CSS
