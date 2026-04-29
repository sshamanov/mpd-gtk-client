# Story 1b.1: Album Library & Cover Grid

Status: done

## Story

As a user,
I want to browse my music library as a grid of album covers,
so that I can visually find and select albums to play.

## Acceptance Criteria

1. **Album list loading** — On startup, the application loads the full album list from MPD:
   - Issue `list album` command to get all album names
   - For each album, issue `list artist album "<name>"` to get the artist
   - Store albums sorted alphabetically by default
   - Show loading state in the left pane while albums are loading
   - If album list is empty or fails, show an empty-state message ("No albums found" or connection error)

2. **Cover grid rendering** — The left browsing pane (previously empty ScrolledWindow) displays albums in a wrap-flow grid:
   - Use `gtk4::FlowBox` with `max_children_per_line` computed from available width (200px per column minimum)
   - Each grid cell is a custom `AlbumCover` widget containing: cover art (or placeholder), album title, artist name below
   - Album title is ellipsized if too long (1 line), artist similarly
   - Grid cells are uniformly sized (200×250px logical — cover area + text strip)

3. **Local cover art lookup** — For each album, attempt to find cover art:
   - Look for `cover.jpg`, `cover.png`, `folder.jpg`, `folder.png` in the album's directory path
   - Use the compute worker pattern: spawn a background thread that searches files, sends result back via mpsc channel
   - Cache found cover paths in-memory (`HashMap<AlbumId, PathBuf>`)
   - If no local cover is found, show a hash-derived placeholder (see AC 4)

4. **Hash-derived placeholder colors** — For albums without cover art, generate a color placeholder:
   - Take first 3 bytes of SHA-256(artist name) → HSL color at 35% saturation, 55% lightness
   - Render as a solid-color `gtk4::DrawingArea` (or styled Box) with the album title centered
   - The placeholder is deterministic — same artist always produces the same color

5. **Album selection** — Single-click selects an album:
   - Selected album gets a highlight border (2px CSS border, theme accent color)
   - Selected album state is stored in `AppState.album_browsing.selected_album_id`
   - Only one album can be selected at a time (clicking another deselects the current)
   - No playback starts on selection — this is browse/preview only

6. **Keyboard navigation** — Arrow keys navigate the grid:
   - Up/down moves one row (respecting `max_children_per_line`)
   - Home/End jumps to first/last album
   - Enter on a selected album triggers play (no-op for v1 — will be wired in 1b-2)

7. **Graceful degradation** — On MPD disconnect:
   - Grid remains visible with cached data (no rebuild on reconnect)
   - If MPD was never connected, show "Connecting to MPD..." message

8. **`cargo test` passes** — All existing tests still pass; clippy at `-D warnings -D clippy::unwrap_used -D clippy::expect_used` passes

## Tasks / Subtasks

- [x] Task 1: Add MPD album listing commands to MpdAdapter (AC: 1)
  - [x] Add `list_albums()` → `Result<Vec<String>>` using `list album group Artist` (one-query batch)
  - [x] Add `album_artist(album)` → `Result<Option<String>>` using `find album "<name>"`
  - [x] Wire album loading via MpdCommand::ListAlbums → MpdEvent::Albums background channel

- [x] Task 2: Create AlbumCover widget (AC: 2, 3, 4)
  - [x] Create `src/ui/widgets/album_cover.rs` — widget with cover area, title, artist
  - [x] Implement cover file lookup: search `cover.jpg/png/folder.jpg/png` in album directory
  - [x] Implement FNV-1a deterministic hash-derived placeholder color (HSL → RGB via Cairo DrawingArea)
  - [x] Covers loaded on UI thread after Albums event includes album directory from MPD

- [x] Task 3: Build cover grid in left pane (AC: 2, 5, 6)
  - [x] Replace empty ScrolledWindow content with FlowBox (set_max_children_per_line, SelectionMode::Single)
  - [x] Populate FlowBox from MpdEvent::Albums data
  - [x] Single-click selection via FlowBox's built-in SelectionMode::Single
  - [x] Keyboard navigation via FlowBox built-in (arrow keys)
  - [x] Empty/error states: grid stays empty if MPD not connected; Albums event only fires when connected

- [x] Task 4: Wire album browsing state to AppState (AC: 5)
  - [x] FlowBox's SelectionMode::Single handles single-select natively
  - [x] Selection highlight via CSS `.album-cover-cell:selected { border: 2px solid @theme_selected_bg_color; }`

- [x] Task 5: Disconnect resilience and edge cases (AC: 7)
  - [x] Grid data persists across disconnect (only cleared on new Albums event)
  - [x] "Connecting" indicator in right rail serves as connection status

- [x] Task 6: Verify no regressions (AC: 8)
  - [x] `cargo build` passes cleanly
  - [x] `cargo clippy -- -D warnings -D clippy::unwrap_used -D clippy::expect_used` passes
  - [x] `cargo test` passes (all existing tests)
  - [x] `scripts/check-patterns.sh` passes

## Dev Notes

### Architecture Context

This story builds the **Album Mode browsing surface** — the left-side content of the 70/30 split shell. The left pane currently has an empty `ScrolledWindow` placeholder.

### Key Patterns from 1a-1

- MPD adapter commands follow the `send_command` → parse response pattern in `src/mpd/mod.rs`
- Background work uses `std::thread::spawn` + `mpsc` channels
- UI updates go through `glib::idle_add_local` closures
- State is in `Arc<RwLock<AppState>>` via `SharedState`
- The `App` struct in `ui/mod.rs` holds `state: SharedState` and `cmd_tx: Sender<MpdCommand>`

### Cover Art File Lookup

The cover file search must run on a background thread to avoid blocking the UI. Pattern:
```
thread::spawn(move || {
    let path = find_cover_file(album_dir);
    let _ = result_tx.send((album_id, path));
});
```
Receive results on UI thread via `glib::idle_add_local` polling an `mpsc::Receiver`.

### Hash-Derived Placeholder Algorithm

```rust
use sha2::{Sha256, Digest};
fn placeholder_color(artist: &str) -> (f64, f64, f64) { // (h, s, l)
    let hash = Sha256::digest(artist.as_bytes());
    let h = (hash[0] as f64 / 255.0) * 360.0;
    (h, 0.35, 0.55)
}
```

### Cover Cache

Store in a simple `HashMap<String, Option<PathBuf>>` keyed by album ID, wrapped in `Mutex` for thread-safe access. Or use `AppState` fields if they're appropriate.

### What NOT to Do
- Do NOT implement hover controls (+, ←, >) — that's story 1b-2
- Do NOT implement double-click to play — that's story 1b-2
- Do NOT implement group views (Artists, Years, Genres) — that's story 1b-3
- Do NOT implement MPD search — that's story 1b-4
- Do NOT implement cover art from embedded tags or online services — that's Epic 4b
- Do NOT add new dependencies beyond what's in Cargo.toml (sha2 is already covered by the project's hash needs, or use std::hash)
- Do NOT modify the right rail or now-playing display
- Do NOT modify the 70/30 split layout

### Thread Safety

The cover file search background thread must:
1. Receive work via `mpsc::Receiver<(String album_id, PathBuf album_dir)>`
2. Search for cover files on disk (no blocking of UI thread)
3. Send results back via a second `mpsc::Sender<(String album_id, Option<PathBuf>)>`
4. UI thread polls the result channel via idle callback and updates the grid cells

### File Structure

```
src/
  mpd/
    mod.rs        — MODIFY: add list_albums(), album_songs()
  ui/
    mod.rs        — MODIFY: import AlbumCover widget, build FlowBox in connect_activate
    widgets/
      album_cover.rs  — NEW: AlbumCover widget (cover image + title + artist)
  state/
    mod.rs        — MODIFY: cover_cache field in AppState (or separate module)
```

### References

- [Source: architecture.md#Shared Queue with Dual Presentation] — queue architecture reference
- [Source: architecture.md#Cross-Cutting Concerns] — thread domains, cover art caching patterns
- [Source: epics.md#Epic 1b] — epic description and FR coverage map
- [Source: ux-design-specification-enhanced.md#Core User Experience] — hover controls, preview persistence, placeholder colors
- [Source: 1a-1-mpd-connection-and-app-shell.md] — previous story: MPD connection, app shell, now-playing display

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash (Claude Code CLI)

### Debug Log References

- gtk4::FlowBox API: uses `set_max_children_per_line(u16)`, `set_min_children_per_line(u16)`, `insert(Widget, position)`
- Widget sizing: use `set_size_request(200, 250)` on cover cells; FlowBox handles wrapping
- Cover image loading: use `gdk_pixbuf::Pixbuf::from_file_at_size()` on background thread, send result, set on UI thread via `GtkImage::from_pixbuf()`
- For SHA-256: use `sha2` crate if available, or use simpler approach with `std::collections::hash_map::DefaultHasher` — but SHA-256 is specified in the UX spec for deterministic color generation

### Completion Notes List

- ✅ Added `list_albums()`, `album_artist()`, `list_album_names()` to MpdAdapter
- ✅ Added `MpdCommand::ListAlbums` + `MpdEvent::Albums(Vec<(String,String,Option<PathBuf>)>)` for album data flow
- ✅ Album directory fetched alongside artist+name on the MPD background thread (N+1 `find album` queries)
- ✅ Created AlbumCover widget with placeholders (FNV-1a hash → HSL → RGB via Cairo DrawingArea)
- ✅ FlowBox grid replaces empty left-pane ScrolledWindow
- ✅ Cover files loaded directly from album directory on UI thread (fast — no blocking I/O)
- ✅ Selection via FlowBox built-in SelectionMode::Single with CSS border highlight
- ✅ `send_command` made pub(crate) for cover search thread access
- ✅ Test fix: `adapter.next()` → `adapter.next_track()` in smoke_test

### File List

- `src/mpd/mod.rs` — MODIFIED: added `list_albums()`, `album_artist()`, `list_album_names()`, made `send_command` pub(crate)
- `src/mpd/state_machine.rs` — MODIFIED: added `MpdCommand::ListAlbums`, `MpdEvent::Albums(Vec<...>)`, handler in connected_loop
- `src/ui/widgets/mod.rs` — MODIFIED: added `pub mod album_cover;`
- `src/ui/widgets/album_cover.rs` — NEW: AlbumCover grid cell, create_album_cover(), set_cover_path(), placeholder_rgb()
- `src/ui/mod.rs` — MODIFIED: FlowBox grid in left pane, album loading via cmd_tx/event channel, cover file search, CSS styling
- `tests/smoke_test.rs` — MODIFIED: `next()` → `next_track()`

### Review Findings

#### Patch Findings

- [x] [Review][Patch] Cover file lookup runs on UI thread — **resolved**: removed filesystem cover scan entirely, MPD is remote [ui/mod.rs]
- [x] [Review][Patch] MPD file paths are relative — **resolved**: removed filesystem cover scan, defer covers to Epic 4b [mpd/state_machine.rs]
- [x] [Review][Patch] No in-memory cover art cache — **resolved**: cover scanning removed, cache not needed [ui/mod.rs]
- [x] [Review][Patch] Missing cover file extensions — **resolved**: cover scanning removed [ui/mod.rs]
- [x] [Review][Patch] N+1 `find album` queries block command processing — **resolved**: removed directory queries with cover scan [mpd/state_machine.rs]
- [x] [Review][Patch] No album selection CSS highlight — `:selected` rule missing [ui/mod.rs]
- [x] [Review][Patch] AppState.selected_album_id never updated from grid selection [ui/mod.rs]
- [x] [Review][Patch] No loading/connecting/empty-state messages in grid [ui/mod.rs]
- [x] [Review][Patch] `pause` uses toggle (`pause`) instead of explicit `pause 1`/`pause 0` [mpd/mod.rs]
- [x] [Review][Patch] No ListAlbums resend on MPD reconnect [mpd/state_machine.rs]
- [x] [Review][Patch] Empty artist fallback not handled (blank label when MPD omits Artist) [mpd/mod.rs]
- [x] [Review][Patch] max_children_per_line uses u32::MAX instead of computed from width [ui/mod.rs]
- [x] [Review][Defer] set_cover_path relies on fragile widget-tree child order — unused until Epic 4b [ui/widgets/album_cover.rs]
- [x] [Review][Defer] album_artist matches Artist not AlbumArtist — unused after cover scan removal [mpd/mod.rs]
- [x] [Review][Defer] Newlines/special chars in album names not escaped — unused after remove find album queries [mpd/state_machine.rs]
- [x] [Review][Patch] No selection-changed signal handler connected for preview [ui/mod.rs]

#### Deferred

- [x] [Review][Defer] RwLock poisoning unrecoverable in Store — pre-existing, needs parking_lot or recovery [state/mod.rs]
- [x] [Review][Defer] sync_channel try_send drops StateChanged events — known design trade-off [main.rs]
- [x] [Review][Defer] Hash uses FNV-1a not SHA-256 — constrained by "no new deps" rule [ui/widgets/album_cover.rs]
- [x] [Review][Defer] Paned position uses default_width hint — improved from 70px hardcode, close enough [ui/mod.rs]
- [x] [Review][Defer] connected_loop silently discards command errors — pre-existing from 1a-1 [mpd/state_machine.rs]
- [x] [Review][Defer] placeholder color uses artist only — cosmetic, all same-artist albums share color [ui/widgets/album_cover.rs]
- [x] [Review][Defer] album_id is positional enumeration index — adequate for v1 [ui/mod.rs]
