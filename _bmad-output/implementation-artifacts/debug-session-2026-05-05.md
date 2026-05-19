# Debug Session 2026-05-05

## Actions

### Group switcher icons removed (src/ui/mod.rs)
- Replaced libadwaita ViewSwitcher with linked ToggleButtons (`.linked` CSS class)
- ViewSwitcher kept showing themed icons despite `set_icon_name(None)` + `set_property("icon-name", None)`
- New approach: `Box` of `ToggleButton`s with `linked` CSS class — no icons possible
- Group button map stored in `Rc<RefCell<HashMap<String, ToggleButton>>>` for programmatic activation during search restore
- Active group tracked in `active_group: Rc<RefCell<String>>` (display name)
- Search stop_search / search_changed handlers updated to use button map instead of `ViewStack::set_visible_child_name`
- Stale CSS rule `viewswitcher button > box > image` removed

### Artists/Years/Genres grouped views fixed (src/ui/mod.rs)
- Root cause: `pad_groups()` treated `None` group_label (non-first items) as group boundaries
- Fix: changed boundary detection from `this_label != current_label` to `this_label.is_some() && this_label != current_label`
- Only items that explicitly carry a `group_label: Some(...)` can start a new group. `None` items are always continuations.
- Before: [A1, F, F, A2, A3, F, B1, F, F, B2, B3, B4] — scrambled
- After: [A1, A2, A3, B1, B2, B3, B4, F, F] — correct group layout

### Folder view rewritten as expandable tree (src/ui/widgets/folder_tree.rs, src/ui/mod.rs)
- Flat ListBox with breadcrumb navigation → expandable tree with inline directory expansion
- Directories expand/collapse in-place (click to toggle)
- Indentation based on depth (20px per level)
- Directory listings cached in `HashMap<String, Vec<DirEntry>>`
- Expanded state tracked in `HashSet<String>`
- CUE/DSD normalization preserved per-directory
- Breadcrumb still present for context and up-navigation
- MPD navigation requests set `shared_path` before sending command; tree expansions don't change `shared_path`
- `set_entries` distinguishes navigation vs expansion by comparing `path == shared_path`
- Navigation rebuilds breadcrumb; expansion just adds to tree
- `shared_path` updated before `ListDirectory` in: mode switch closures, breadcrumb handlers, keyboard back handler
- Folder search result rows now have themed icons matching browser style
- ScrolledWindows have `set_has_frame(true)` for inset border
- Themed GTK icons replace text symbols (folder-symbolic, audio-x-generic-symbolic, etc.)

## Results
- Group switcher: no more icons (linked ToggleButtons)
- Grouped views: correct grid layout (pad_groups boundary detection fixed)
- Folder view: expandable tree with in-place expansion, normalized CUE/DSD
- 20/21 tests pass (test_enqueue_replaces_previous failing — pre-existing, in coverart module)
- Builds clean (only pre-existing warnings)

### cell_slot column calculation fixed (src/ui/mod.rs)
- Root cause: `cell_slot = 204` only accounted for content (200) + gridview child padding (4). It omitted `.album-cover-cell` padding (4px×2=8) and margin (2px×2=4).
- Fix: changed to `216` = 200 (content) + 8 (padding) + 4 (margin) + 4 (gridview child padding)
- DEBUG RESULT: pending visual evaluation

### DSD/CUE folder playback fixed (src/mpd/state_machine.rs, src/ui/widgets/folder_tree.rs)
- Root cause: click handler sent `MpdCommand::Add(uri)` with file URIs, but the `Add` handler calls `find_album_uris()` which does `find album "uri"` MPD query — broken for file paths
- Fix: added `PlayUris(Vec<String>)` and `AddUris(Vec<String>)` variants that batch direct `add "uri"` commands via `send_batch`, bypassing album search entirely
- Folder tree CUE/DSD click handlers now send a single `PlayUris(uris)` instead of individual `Clear` + `Add(uri)` + `PlayPosition(0)`
- This eliminates the connection storm (15 greetings in 8 seconds) caused by individual state machine processing of Clear → Add → Add → ... → PlayPosition
- DEBUG RESULT: pending visual evaluation

### Context menus added to album grid (src/ui/mod.rs)
- Added right-click GestureClick (button 3) in album factory `connect_setup`
- Reads `cell.album_key()` at click time (populated during bind)
- Popover with: Play Now (PlayAlbum), Play Next (InsertNext), Add to Queue (Add)
- DEBUG RESULT: pending visual evaluation

### Context menus added to folder tree (src/ui/widgets/folder_tree.rs)
- Added right-click GestureClick (button 3) on ListBox, uses `row_at_y()` for hit testing
- File rows: Play Now (PlayFile), Add to Queue (AddUris)
- CUE rows: Play Now (PlayUris), Add to Queue (AddUris)
- DSD rows: Play Now (PlayUris), Add to Queue (AddUris)
- DEBUG RESULT: pending visual evaluation

### DSD/CUE freeze with "Tried to remove non-child" fixed (src/ui/widgets/folder_tree.rs)
- Root cause: `rebuild_list()` used `while let Some(child) = list.first_child() { list.remove(&child); }` — if a child's parent wasn't the ListBox, `remove()` failed (GTK warning) and the child stayed in place, causing `first_child()` to return it again → infinite loop → UI freeze
- Fix: replaced with `list.remove_all()` which handles internal bookkeeping correctly and only removes actual rows
- DEBUG RESULT: pending visual evaluation

### Queue context menu fixed (src/ui/mod.rs)
- Root cause: `GestureClick` was created and connected but never added to the row with `row.add_controller(rclick)` — the gesture was orphaned
- Fix: added `row.add_controller(rclick)` before appending row to ListBox
- DEBUG RESULT: pending visual evaluation

### Album grid context menu redesigned (src/ui/mod.rs)
- Previous approach: per-cell GestureClick in factory connect_setup — events consumed by cell child widgets (Picture, Labels, Buttons)
- New approach: single GestureClick on the GridView itself, uses `pick(x, y)` to find the AlbumCoverCell under the cursor via parent traversal
- Context menu: Play Now (PlayAlbum), Play Next (InsertNext), Add to Queue (Add)
- DEBUG RESULT: pending visual evaluation

### Folder tree context menu parent fix (src/ui/widgets/folder_tree.rs)
- Popover parent was previously set to the ListBox widget; changed to the clicked row's child widget for correct positioning
- Removed unused `gest` parameter to suppress warning
- DEBUG RESULT: pending visual evaluation

### Context menu still broken — per-cell approach (src/ui/widgets/album_cover_cell.rs)
- Root cause theory: GridView-level GestureClick has gesture arbitration conflicts with GTK internal gestures
- New approach: moved context menu GestureClick directly onto AlbumCoverCell widget (in `wire_context_menu()`)
- Uses set_button(0) + current_button() filter + Capture phase + set_pointing_to() + popover storage
- GridView-level gesture removed
- Same fix pattern applied to folder tree and queue menus
- Added eprintln! debug logging to all three to verify gesture fires
- DEBUG RESULT: pending visual evaluation

### Group padding on window resize (src/ui/mod.rs)
- Connected to GridView's width property via `connect_notify_local(Some("width"), ...)`
- Tracks last column count in `Cell<usize>` to avoid redundant re-padding
- On column count change: extracts Album items, re-pads, re-populates model
- Used `connect_notify_local` (not `connect_notify`) to avoid Send+Sync bounds on Rc types
- DEBUG RESULT: pending visual evaluation

### Album group badges removed (src/ui/widgets/album_cover_cell.rs, src/ui/mod.rs, src/ui/style.css)
- Removed visual group_label badge overlay from AlbumCoverCell (constructed, set_album, set_filler)
- Removed `.album-group-badge` CSS rule
- `group_label` field kept in AlbumGridItem for pad_groups() boundary detection
- Removed group_label parameter from AlbumCoverCell::set_album() signature
- DEBUG RESULT: pending visual evaluation

### Album metadata cache with year and genre (src/mpd/mod.rs, src/mpd/state_machine.rs, src/ui/mod.rs, tests)
- Added `AlbumMeta` struct: album, album_artist, year, genre
- Changed AlbumGroup type from `Vec<(String, Vec<(String, String)>)>` to `Vec<(String, Vec<AlbumMeta>)>`
- Added `list_albums_full()` — single MPD query: `list album group Album group AlbumArtist group Date group Genre`
- Changed `list_albums_grouped()` to pure local grouping from cached data (no MPD round-trip)
- State machine caches `Vec<AlbumMeta>` on first fetch, all subsequent views use cache
- Updated `MpdEvent::Albums` and `MpdEvent::AlbumsGrouped` to carry AlbumMeta
- Year now populated in AlbumGridItem (was always None before)
- Year label already existed in AlbumCoverCell with `.album-cell-year` CSS — now receives real data
- Removed unused `group_albums_by_artist()` helper
- Updated mock server response and smoke tests
- All 17 integration tests pass, 20/21 unit tests pass (1 pre-existing failure)
- DEBUG RESULT: pending visual evaluation

### MPD "Conflicting group" error fixed (src/mpd/mod.rs, src/mpd/mock.rs)
- Root cause: `list album group Album group AlbumArtist group Date group Genre` — MPD only supports a single `group` keyword. Multiple `group` keywords produce ACK `{list} Conflicting group`.
- Fix: split into 3 valid MPD queries:
  - `list album group AlbumArtist` → album→albumartist mapping
  - `list album group Date` → album→date mapping
  - `list album group Genre` → album→genre mapping
- Results merged locally by album name via HashMap. Albums that appear in date/genre but not artist are included with empty album_artist.
- Mock server updated: 4 response functions (list album, list album group AlbumArtist, list album group Date, list album group Genre)
- DEBUG RESULT: pending visual evaluation

### Context menu popover parent fix (3 files)
- Root cause: Popover.set_parent() widget didn't match the coordinate space of the GestureClick coordinates. Folder tree had parent = row.child() but coordinates relative to ListBox. Queue had parent = gest.widget() (possibly wrong widget).
- Fix folder tree: set_parent to rctx_list (ListBox, matching coordinate space), popup() instead of present()
- Fix queue: set_parent to row.clone() (captured directly, matching coordinate space), popup() instead of present()
- Fix album cover cell: popup() instead of present() (parent already correct)
- DEBUG RESULT: pending visual evaluation

### Date normalized to Year only (src/mpd/mod.rs)
- Added `normalize_year()` helper: splits on '-' to extract just the year from full dates like "2024-03-15"
- Applied in `list_albums_full()` when parsing Date tags from MPD `list album group Date` response
- Also applied in `parse_song_update()` for now-playing Date tag
- DEBUG RESULT: pending visual evaluation

### Year added to now-playing display (src/mpd/state_machine.rs, src/ui/mod.rs, src/ui/style.css)
- Added `year: Option<String>` field to `PlaybackUpdate` struct
- `parse_song_update()` extracts and normalizes Date from currentsong
- `fetch_full_update()` merges year from song into status update
- Created `track_year` Label in now-playing section (dimmed, small, CSS `.track-year`)
- Added to `NowPlayingWidgets` struct, display in `update_now_playing()`
- DEBUG RESULT: pending visual evaluation

### Year placement redesigned in album cover cell (src/ui/widgets/album_cover_cell.rs)
- Removed separate year label (3rd line in cell)
- Year combined inline with artist: "Artist · 2024" with middle-dot separator
- Artist stays bold (`.album-cell-artist`), year follows naturally inline
- Saves vertical space, reads like common music apps
- DEBUG RESULT: pending visual evaluation

### Label width increased to fill cell (src/ui/widgets/album_cover_cell.rs)
- Removed `set_max_width_chars(18)` from title and artist labels
- Added `set_hexpand(true)` so labels fill available cell width
- Labels still ellipsize with `EllipsizeMode::End` for long text
- DEBUG RESULT: pending visual evaluation

### Group caption recovered on first cover (src/ui/widgets/album_cover_cell.rs, src/ui/mod.rs, src/ui/style.css)
- Added `group_caption` Label overlay on SquareCover (top-left, semi-transparent background)
- CSS `.group-caption`: bold, 0.85em, 75% opaque background, rounded corners
- `set_group_caption(text)` method shows/hides the caption
- Factory bind reads `group_label` from `AlbumGridItem::Album` and calls `set_group_caption`
- Only first item in each group carries a `group_label: Some(...)`, so caption appears on first cover
- DEBUG RESULT: pending visual evaluation

### Folder tree: "Play Next" added to file/CUE context menus (src/ui/widgets/folder_tree.rs, src/mpd/state_machine.rs)
- Added `InsertNextUris(Vec<String>)` MpdCommand variant for URI-based insert-after-current
- Handler: gets current song position, batch-adds URIs via `addid "uri" position`
- Falls back to plain `add` if no current track is playing
- File rows: + "Play Next" button between "Play Now" and "Add to Queue"
- CUE rows: + "Play Next" button for all CUE-associated tracks
- DEBUG RESULT: pending visual evaluation

### Folder tree: directory context menu added (src/ui/widgets/folder_tree.rs, src/mpd/state_machine.rs)
- Added 3 new MpdCommand variants: `PlayDirectory`, `AddDirectory`, `InsertNextDirectory`
- Each handler: calls `adapter.lsinfo(dir)`, extracts file URIs, batch commands
- PlayDirectory: clear + add + play
- AddDirectory: add all files to queue
- InsertNextDirectory: addid each file after current track position
- Directory rows ("dir:" prefix): context menu with Play Now / Play Next / Add to Queue
- DEBUG RESULT: pending visual evaluation

### GestureClick set_button(0) breaks left-clicks (3 files)
- Root cause: `set_button(0)` at Capture phase recognized ALL mouse buttons, consuming events before ListBox row activation handlers could see them
- Fix: changed `set_button(0)` → `set_button(3)` everywhere, removed `current_button() != 3` guard
- Files: folder_tree.rs, ui/mod.rs (queue), album_cover_cell.rs
- DEBUG RESULT: pending visual evaluation

### Group captions: every album, Years/Genres/Artists views (src/ui/mod.rs, src/mpd/mod.rs, src/mpd/mock.rs)
- Added `track_artists: Vec<String>` to AlbumMeta — populated via 4th MPD query `list artist group album`
- New helper `group_caption_for_view()`:
  - Albums (flat): None (no caption)
  - Years/Genres: Some(header) on EVERY album in group
  - Artists: comma-separated track artists that differ from AlbumArtist (e.g., compilation albums showing actual performers)
- All items in a group now get group_label (removed first-only `i == 0` guard)
- Mock server: added `make_list_artist_group_album_response()` with Featured Artist for Test Album
- DEBUG RESULT: pending visual evaluation

### Fix "Tried to remove non-child" freeze (2026-05-06)
- **Action**: Replaced `container.remove(&child)` with `child.unparent()` at 4 locations:
  - `src/ui/mod.rs:1901` (fc_track_win StateChanged handler)
  - `src/ui/mod.rs:2133` (fc_fs_list FileSearchResults handler)
  - `src/ui/mod.rs:2168` (fc_track_win AlbumTracks handler)
  - `src/ui/widgets/folder_tree.rs:250` (breadcrumb rebuild)
- **Why**: GTK4 ListBox.remove() can hit internal linked-list inconsistencies during batch event processing. unparent() operates at the GtkWidget level and bypasses the ListBox-specific remove path.
- **Result**: Pending visual verification.

### Redesign group captions as stacked labels (2026-05-06)
- **Action**: Changed caption from `Option<String>` to `Option<Vec<String>>` (max 5), replaced single group-caption Label with vertical Box of stacked Labels in AlbumCoverCell.
  - `AlbumGridItem::Album.caption`: `Option<String>` → `Option<Vec<String>>`
  - `group_caption_for_view()`: returns `Option<Vec<String>>`, Artists case uses `.take(5)` directly
  - `AlbumCoverCell`: `group_caption` field now `RefCell<Option<Box>>`, `set_group_captions(&[String])` method creates up to 5 stacked labels
- **Result**: Pending visual verification.

### Prevent selection/activation on filler cells (2026-05-06)
- **Action**: In bind callback, for Filler items set `list_item.set_activatable(false)`, `cell.set_can_focus(false)`, `cell.set_can_target(false)`. For Album items reset all to true (cells get recycled).
- **Plattenalbum investigation**: plattenalbum has no filler items at all — its model contains only real Album objects, so the last row naturally has fewer cells. No selection-on-empty-cell problem exists there. Our filler approach is different because we use fillers for group boundary padding.
- **Result**: Pending visual verification.

### Segfault fix: reverted unparent() back to remove() (2026-05-06)
- **Action**: Reverted 4 `unparent()` calls back to `container.remove(&child)` — unparent() corrupts ListBox internal sibling linked-list, causing `gtk_widget_insert_after` assertion failures and segfault on next append.
- **Result**: Compiles. Pending visual verification.

### Year badge caption (2026-05-06)
- **Action**: Added `year_badge: Option<String>` field to AlbumGridItem::Album (last 2 digits of year). Added top-right corner Label in AlbumCoverCell overlay. Shown in all views except Years. CSS class `.year-badge`. Stacked artists captions capped at 6 (was 5).
- **Result**: Compiles. Pending visual verification.

### Grid resize recalculation fix (2026-05-06)
- **Action**: Changed resize handler from `album_grid.connect_notify_local("width")` to `left_scroll.connect_notify_local("width")` because GridView width inside ScrolledWindow tracks natural content width, not viewport width. Added `glib::idle_add_local` deferral with pending flag to avoid RefCell borrow conflicts and to ensure re-pad runs outside the notify signal.
- **Result**: Compiles. Pending visual verification.

### Year badge styling: backtick for pre-2000 (2026-05-06)
- **Action**: Changed year badge format — `'84` for years < 2000, full 4 digits for >= 2000. Added `format_year_badge()` helper, applied at all 3 construction sites.
- **Result**: Pending visual verification.

### Root-cause "Tried to remove non-child" — comprehensive fix (2026-05-06)
- **Root cause**: The `while let Some(child) = container.first_child() { container.remove(&child); }` pattern used in 6 locations. During `remove()`, GTK emits signals that can modify the child list, corrupting the iteration. Next `first_child()` returns a widget whose parent is no longer the container.
- **Fix**: All 6 locations fixed:
  - GtkBox (breadcrumb, toast): collect-then-remove via `std::iter::from_fn`
  - ListBox (track_win ×2, fs_list, queue_list): `remove_all()`
  - Also fixed M1 from code review: unsafe loop in `set_group_captions`
- **Additional fix**: ".." row handler now updates `shared_path` before sending command (was only done in keyboard handler)
- **Architecture analysis**: Written to `_bmad-output/implementation-artifacts/ui-architecture-analysis.md`
- **Code review**: 0 CRITICAL, 2 HIGH, 5 MEDIUM, 4 LOW findings. HIGH: HC CSS scope, fragile folder scroll traversal. MEDIUM: snapshot-pattern gap (fixed), dead toast.rs, stale comments (fixed), timer-after-close, implicit key format coupling. LOW: debug eprintlns (fixed), disabled drag-drop, redundant CSS, i32 truncation.
- **Advanced Elicitation**: 4 key questions — stale cover_widgets on GridView recycle, synchronous JPEG decode on main thread, folder expansion state loss on mode switch, dir_cache never invalidated on LibraryChanged.
- **Result**: Pending visual verification.
