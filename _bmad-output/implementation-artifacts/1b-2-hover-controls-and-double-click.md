# Story 1b.2: Hover Controls & Double-Click

Status: review

## Story

As a user,
I want to hover over an album cover to see action buttons and double-click to play,
so that I can quickly add albums to the queue or start playback.

## Acceptance Criteria

1. **Hover controls on album covers** — Each grid cell has three buttons revealed on hover:
   - `+` — add album to end of queue
   - `←` — insert album after the currently playing track
   - `>` — clear queue and play the album immediately
   - Buttons are positioned at the bottom-right of the cover area, 24×24px minimum
   - Buttons use `GtkOverlay` positioned over the cover image/placeholder area
   - CSS controls visibility: buttons are `opacity: 0` by default, `opacity: 1` on cell hover with 150ms transition

2. **Double-click to play** — Double-clicking an album cover:
   - Clears the current queue
   - Adds all tracks from the album to the queue
   - Starts playback of the first track
   - This is equivalent to clicking the `>` button

3. **MPD command wiring** — The `MpdCommand::Add` and `MpdCommand::Clear` stubs are implemented:
   - `Add(album_name)` → `find album "<name>"` to get track URIs, `add` each to queue
   - `Clear` → send `clear` command to MPD
   - Results are sent back via `MpdEvent::QueueChanged` (add new variant or use Status polling)
   - Commands are sent from UI thread via `cmd_tx: mpsc::Sender<MpdCommand>`

4. **Single-click preserves selection** — Single-click on an unselected cell selects it (existing behavior from 1b-1). Single-click on the already-selected cell does nothing (no playback, no queue action). The hover buttons are the primary action mechanism per UX-DR1.

5. **Buttons use MPD protocol** — All queue operations go through the MPD background thread:
   - `+`: `find album "<name>"` → for each track: `add "<uri>"`
   - `←`: `find album "<name>"` → `status` to get current song position → `add "<uri>"` for each → `move` to position after current
   - `>`: `clear` → `find album "<name>"` → `add` each track → `play 0`
   - Error handling: if any MPD command fails, log the error and continue

6. **`cargo test` passes** — All existing tests pass; clippy clean

## Tasks / Subtasks

- [x] Task 1: Implement Add, InsertNext, PlayAlbum, and Clear MPD commands (AC: 3, 5)
  - [x] Added `MpdCommand::Add(String)`, `MpdCommand::InsertNext(String)`, `MpdCommand::PlayAlbum(String)`
  - [x] `Add`: `find album` → `add` each URI, send StateChanged
  - [x] `InsertNext`: `find album` → `addid` each → `moveid` to after current song
  - [x] `PlayAlbum`: `clear` → `add` album → `play 0`
  - [x] `Clear`: send `clear`, send StateChanged
  - [x] Added `addid()` and `find_album_uris()` helpers to MpdAdapter

- [x] Task 2: Add GtkOverlay with hover buttons to AlbumCover (AC: 1)
  - [x] Wrapped cover area in GtkOverlay with 3 GtkButtons (+, ←, >) bottom-right
  - [x] CSS: buttons opacity 0→1 on :hover with 150ms transition, 24×24px
  - [x] Tooltips on each button

- [x] Task 3: Wire button signals to MpdCommand (AC: 3, 4, 5)
  - [x] Each button clones `cmd_tx` and sends the appropriate command
  - [x] `AlbumCover::create_album_cover` now takes `cmd_tx` and `album_name` params

- [x] Task 4: Wire double-click to play (AC: 2)
  - [x] `FlowBox::connect_child_activated` → `MpdCommand::PlayAlbum(album_name)`
  - [x] Shared `Arc<Mutex<Vec<String>>>` maps child index to album name

- [x] Task 5: Verify no regressions (AC: 6)
  - [x] `cargo build` passes cleanly
  - [x] `cargo clippy -- -D warnings -D clippy::unwrap_used -D clippy::expect_used` passes
  - [x] `cargo test` passes (all existing tests)

## Dev Notes

### Architecture Context

This story adds interaction to the album grid built in 1b-1. The grid cells already exist as Box widgets in a FlowBox. This story wraps the cover area in an overlay and adds buttons.

### Key Pattern

Commands are sent to the MPD background thread via `cmd_tx: mpsc::Sender<MpdCommand>`. The `App` struct stores `cmd_tx` and it's available in the connect_activate closure. Each hover button gets a clone of `cmd_tx` and sends commands on click.

For InsertNext, a new `MpdCommand::InsertNext(String)` variant is needed that: gets current song position, adds album tracks, moves them to position+1.

### GTK4 Overlay Pattern

```rust
let overlay = gtk4::Overlay::new();
overlay.set_child(Some(&cover_area));
let btn_add = gtk4::Button::new();
btn_add.set_css_classes(&["album-cover-hover-btn"]);
overlay.add_overlay(&btn_add);
// Position via CSS
```

### CSS for Hover

```css
.album-cover-hover-btn {
    opacity: 0;
    transition: opacity 150ms ease-in-out;
}
.album-cover-cell:hover .album-cover-hover-btn {
    opacity: 1;
}
```

Position the buttons at bottom-right with CSS:
```css
.album-cover-hover-btn { ... margin: 2px; }
/* Or use halign/valign = End on the overlay children */
```

Actually, GtkOverlay positions overlay children via `halign` and `valign`:
- Set `halign: End, valign: End` for the buttons to place them bottom-right
- Use a horizontal Box for the three buttons to stack them side-by-side

### What NOT to Do
- Do NOT implement cover art (Epic 4b)
- Do NOT implement group views (1b-3)
- Do NOT implement search (1b-4)
- Do NOT implement drag-and-drop (Epic 3)
- Do NOT modify the right rail or now-playing display
- Do NOT add new dependencies

### References

- [Source: ux-design-specification-enhanced.md#UX-DR1] — hover controls pattern
- [Source: ux-design-specification-enhanced.md#UX-DR2] — double-click interaction
- [Source: epics.md#Epic 1b] — FR-B8, FR-B9
- [Source: 1b-1-album-library-and-cover-grid.md] — previous story: album grid, FlowBox, App wiring

### Review Findings

#### Patch Findings

- [x] [Review][Patch] CSS connection indicator uses `set_widget_name` with space — fixed to `set_css_classes` [ui/mod.rs]
- [x] [Review][Patch] Invisible hover buttons remain clickable — fixed with `EventControllerMotion` + `set_sensitive` [ui/widgets/album_cover.rs]
- [x] [Review][Patch] `PlayAlbum` clears queue before confirming album exists — fixed: check `find_album_uris` first [mpd/state_machine.rs]
- [x] [Review][Patch] `InsertNext` position drifts when `addid` fails — fixed: per-track error logging [mpd/state_machine.rs]
- [x] [Review][Patch] MPD `add` commands skip URI escaping — fixed: use `addid` which escapes [mpd/state_machine.rs]
- [x] [Review][Patch] `pause 1` is unconditional — fixed: reverted to toggle `pause` [mpd/mod.rs]
- [x] [Review][Patch] All MPD command errors silently discarded — fixed: `log::error!` on failures [mpd/state_machine.rs]
- [x] [Review][Patch] `InsertNext` sends no error event when `find_album_uris` fails — fixed: `log::error!` [mpd/state_machine.rs]

#### Deferred

- [x] [Review][Defer] SIGINT/SIGTERM handlers return Break without quitting — pre-existing from 1a-1, GTK handles default quit
- [x] [Review][Defer] FlowBox column count never recalculated on resize — static initial value is acceptable for v1
- [x] [Review][Defer] sync_channel try_send drops events — known design trade-off, pre-existing
- [x] [Review][Defer] Grid rebuilt from scratch on reconnect (flicker) — acceptable for v1
- [x] [Review][Defer] `populate_album_grid` tears down and rebuilds all children — acceptable for v1
- [x] [Review][Defer] `child-activated` with `set_activate_on_single_click(false)` — behavior varies by GTK version, verified working

## Dev Agent Record

### Completion Notes List

- ✅ Added `MpdCommand::Add(String)`, `InsertNext(String)`, `PlayAlbum(String)` — all work through `connected_loop`
- ✅ Added `MpdAdapter::addid()` and `MpdAdapter::find_album_uris()` for batch album queue ops
- ✅ AlbumCover now wraps cover area in `GtkOverlay` with three hover buttons
- ✅ CSS opacity transition for hover reveal (0→1, 150ms)
- ✅ Double-click handler via `FlowBox::connect_child_activated`
- ✅ Shared `Arc<Mutex<Vec<String>>>` maps FlowBox child index → album name
- ✅ `populate_album_grid` now takes `cmd_tx` for wiring buttons
- ✅ Clean build, tests, clippy

### File List

- `src/mpd/mod.rs` — MODIFIED: added `addid()`, `find_album_uris()` helpers
- `src/mpd/state_machine.rs` — MODIFIED: `Add(String)`, `InsertNext(String)`, `PlayAlbum(String)` command variants and handlers
- `src/ui/widgets/album_cover.rs` — MODIFIED: `Overlay` with hover buttons, `create_album_cover` now takes `cmd_tx` and `album_name`
- `src/ui/mod.rs` — MODIFIED: double-click wiring, shared album name store, hover button CSS, `populate_album_grid` passes `cmd_tx`
