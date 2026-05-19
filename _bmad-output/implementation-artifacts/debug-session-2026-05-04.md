# Debug Session 2026-05-04

## Actions

### Cover pipeline re-enabled (src/mpd/state_machine.rs)
- Un-prefixed `_cover_provider` / `_actual_read` → restored `cover_provider` / `actual_read` usage
- Restored `FetchCovers` handler: `actual_read.enqueue(albums)` + immediate `process_batch(16)`
- Restored `process_one` call in idle timeout path (was skipped)
- Restored timeouts: recv 100ms (was 1000ms), status poll 500ms (was 5000ms), reconnect sleep 50ms (was 500ms)
- Added `process_batch()` to `ActualRead` — processes up to N covers in a burst

### Cover fetch perf improvements (src/coverart/actual_read.rs, src/mpd/mod.rs)
- Added `albumart_by_uri()` to MpdAdapter — skips redundant `find_album_uris` when URI already known
- `process_one` now calls `find_album_uris` once, passes URI to `albumart_by_uri()` — saves 1 round-trip per cover
- `FetchCovers` handler processes 16 covers immediately (not 1-per-idle-cycle) for initial load speedup

### Cover push re-enabled (src/ui/mod.rs)
- `COVER_PUSH_ENABLED: AtomicBool` set to `true` (was `false`)

### Drag/drop re-enabled (src/ui/mod.rs)
- Uncommented `left_scroll.add_controller(grid_target)` for grid drop target
- Uncommented `row.add_controller(reorder_ds)` for queue drag reorder

### CSS applied from user's example.css (src/ui/mod.rs)
- Cell: padding 4px, margin 2px, border-radius 6px, background-color
- Cell:hover: selected bg highlight
- #cover-image: border-radius 4px, box-shadow 2px 4px 3px
- Labels: font-size 1em, padding 0 4px
- Hover buttons: 28x28px, border-radius 4px, bg alpha, font-weight bold, font-size 1.1em
- Grid spacing: padding 4px, child padding 2px
- Group headers: padding 6px 8px, color @theme_fg_color

### Button symbols changed (src/ui/widgets/album_cover_cell.rs)
- Play next: `<-` → `↩` (leftwards arrow with hook)
- Play: `>` → `▶` (black right-pointing triangle)

### CSS offloaded (src/ui/style.css)
- Created `src/ui/style.css` with all CSS rules
- Updated mod.rs to use `css.load_from_data(include_str!("style.css"))` — embedded at compile time

### Group headers → overlay badges (Option B)
- Removed `AlbumGridItem::Header` variant entirely
- Added `group_label: Option<String>` to `AlbumGridItem::Album`
- First album of each group gets `group_label = Some(header_name)`, rest get `None`
- AlbumCoverCell: replaced `header_label` with `group_label` overlay badge (floating label on cover)
- CSS: `.album-group-badge` styled as small floating badge (background, border-radius, margin)
- Updated pinned header logic to walk backward to nearest `group_label`
- Single-group views (Albums) skip group labels entirely
- Removed `set_header()` method, updated `set_album()` signature

### Top bar restructured
- Single row: Album/Folder icons + refresh + group switcher
- Album/Folder: CD icon (`media-optical`) and folder icon instead of text labels
- Refresh button: `view-refresh` icon instead of `↻` text
- Group switcher: "Album Artists" renamed to "Artists"
- Sort dropdown removed — fixed to artist sort (default)
- Removed `group_bar` from `album_content`
- `top_bar` placed above `mode_stack` in `mode_content`

### Menu bar hidden
- `.show_menubar(false)` on ApplicationWindow
- Removed menubar creation code (File/View/Help menus)
- Keyboard shortcuts (Ctrl+1, Ctrl+2, Ctrl+F, Ctrl+Q) preserved via app actions

## Results
- Cover pipeline re-enabled and faster (bulk fetch + dedup find_album_uris)
- Drag/drop re-enabled
- CSS/button polish applied
- Group headers replaced with overlay badges on first cover of each group
- Top bar consolidated to single row with icons
- Menubar hidden
- Builds clean (only pre-existing warnings)

### Row-break filler padding (2026-05-04)
- Added `AlbumGridItem::Filler` variant — invisible placeholder forces GridView row break
- Added `pad_groups()` function — inserts Fillers after each group so next group starts at column 0
- Columns computed from grid width at populate time; falls back to max_columns when grid not yet allocated
- `set_size_request(200, 250)` added to AlbumCoverCell — ensures Fillers maintain grid geometry
- `AlbumCoverCell::set_filler()` method — hides album_section + group_label for invisible cells
- Factory bind updated to match on Filler → calls set_filler()
- Cover-fetch extractions changed from `.map(|i| let AlbumGridItem::Album {..}=i)` to `.filter_map()` to avoid panic on Filler
- pad_groups called in AlbumsGrouped handler; also called in Albums handler (no-op for ungrouped views)
- Genre mode: overlay badge still on first album only (single genre tag per album, no duplicates)

### Fixes (2026-05-05)
- Group switcher: `set_icon_name(None)` on each ViewStack page — removes themed icons from Albums/Artists/Years/Genres buttons
- Filler column calculation: cell_slot 204px (200 + 4px `gridview > child` CSS padding) — fixes misalignment between pad_groups and actual GridView columns
- AlbumCoverCell measure() override: returns (200,200) horizontal / (250,250) vertical natural size — ensures Fillers report correct size even when album_section is hidden
- FolderBrowser: `set_vexpand(true)` on container + ScrolledWindow — fixes folder list limited to breadcrumb height (~66px)
- Fixed panic in pad_groups: `group_start` was tracked in result space (with fillers) but compared against `i` in input space — split into `group_start_i` and `group_start_res`
