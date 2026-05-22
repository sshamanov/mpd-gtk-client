# Story 43-3: Split ui/mod.rs into Sub-Modules

## Status: done

## Context

`ui/mod.rs` is ~3141 lines — grid, queue, now-playing, settings, help, bottom panel, theme all inline. Architecture.md §1529 explicitly notes this as deferred. Extract 6 self-contained sub-modules, each with a clear responsibility and public API.

See `.claude/plans/glittery-wiggling-puzzle.md` Steps 5-11.

## Tasks

### Task 1: Extract `src/ui/grid.rs`
Move from `ui/mod.rs`:
- `AlbumCell` struct, `AlbumCells` type alias
- `CELL_SLOT_W`, `CELL_SLOT_H`, `CAPTION_H` constants
- `reposition()`, `group_caption_for_view()`, `format_year_badge()`
- `placeholder_texture()`, `make_placeholder_cover()`, `placeholder_rgb()`

### Task 2: Extract `src/ui/queue.rs`
Move from `ui/mod.rs`:
- `MiniGridItem`, `MiniGridData`, `MiniCell` types
- `SharedIds` type alias
- `rebuild_mini_fixed()` function
- Queue list building helpers from tick callback (queue_list population)

### Task 3: Extract `src/ui/now_playing.rs`
Move from `ui/mod.rs`:
- `NowPlayingWidgets` struct
- `PlaybackDisplay` struct + `from_update()`
- `handle_now_playing()`, `update_now_playing()`
- `find_album_boundary()`

### Task 4: Extract `src/ui/settings.rs`
- Create `pub fn show(parent: &Window, conn_params, cmd_tx, rail_width_min, rail_width_max, split_ratio)`
- Self-contained modal dialog (lines 981-1291)

### Task 5: Extract `src/ui/help.rs`
- Create `pub fn show(parent: &Window)`
- Keyboard shortcuts dialog (lines 1303-1348)

### Task 6: Extract `src/ui/bottom_panel.rs`
- Create `pub struct BottomPanel` with widget fields
- `pub fn build(cmd_tx: &CommandSender) -> BottomPanel`

### Task 7: Extract `src/ui/theme.rs`
- `HC_CSS` constant
- CSS provider loading, high-contrast application
- Memory monitor timer

## Acceptance Criteria
1. All 7 new modules compile independently
2. `ui/mod.rs` delegates to sub-modules — thin scaffolding
3. All existing functionality preserved (grid, queue, now-playing, settings, help, bottom panel, theme)
4. `cargo build` passes after each extraction
5. `cargo test` passes
