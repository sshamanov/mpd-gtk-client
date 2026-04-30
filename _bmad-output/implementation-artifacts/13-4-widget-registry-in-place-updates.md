# Story 13.4: Widget Registry Integration for In-Place Updates

Status: review

## Story

As a user,
I want cover art to appear in the grid as soon as it's fetched,
So that placeholders are replaced seamlessly without grid rebuilds.

## Acceptance Criteria

1. **CoverRefreshed event** — Given AlbumArtProvider emits `CoverRefreshed(id, data)` carrying raw JPEG bytes and the album ID, when the UI thread receives the event, then the data is decoded into a `GdkTexture` on the main thread using `gdk_pixbuf`.

2. **Widget registry lookup** — Given the `cover_widgets` HashMap contains a `Picture` widget for the album ID, when `CoverRefreshed` fires, then the widget is looked up by album_id and the texture is applied via `set_paintable()`.

3. **No grid rebuild** — Given a cover is fetched for an album that already has a placeholder in the grid, when `CoverRefreshed` is handled, then the cell is updated in-place with no call to `model.remove_all()` or `batch_populate()`. Only `queue_draw()` is called.

4. **Now-playing update** — Given `CoverRefreshed` fires for the currently playing album, when the handler processes the event, then the now-playing cover widget is also updated with the new cover.

5. **Dead code removal** — Given `CoverFetcher` and `fetch_via_mpd` in `src/coverart/mod.rs` are no longer referenced by any code, when the cleanup is applied, then they are removed along with unused imports.

## Tasks / Subtasks

- [x] 1. Add `CoverRefreshed` event variant to `MpdEvent` in `src/mpd/state_machine.rs` (AC: #1)
  - [x] 1.1 Add `CoverRefreshed { album_id: String, data: Vec<u8> }` variant to the `MpdEvent` enum
  - [x] 1.2 The data field carries raw JPEG bytes fetched by ActualRead

- [x] 2. Modify ActualRead to emit `CoverRefreshed` after cache write (AC: #1)
  - [x] 2.1 In `src/coverart/actual_read.rs`, added `emit_cover_refreshed` method
  - [x] 2.2 In `handle_albumart_data()`, calls `emit_cover_refreshed()` after the cache write
  - [x] 2.3 In `handle_readpicture_data()`, calls `emit_cover_refreshed()` after the cache write
  - [x] 2.4 Keep existing `CoverPaths` emission — `CoverRefreshed` is additive
  - [x] 2.5 Updated comment in `emit_cover_path()` about future stories

- [x] 3. Add UI handler for `CoverRefreshed` event in `src/ui/mod.rs` (AC: #2, #3, #4)
  - [x] 3.1 Added match arm in event processing loop for `MpdEvent::CoverRefreshed { album_id, data }`
  - [x] 3.2 Decode raw JPEG bytes via `gdk_pixbuf::Pixbuf::from_read()` → `gdk4::Texture::for_pixbuf()`
  - [x] 3.3 Logs info-level message on successful refresh
  - [x] 3.4 Logs warn-level message on decode failure
  - [x] 3.5 Only widget-level updates, no grid rebuild

- [x] 4. Update now-playing cover from `CoverRefreshed` (AC: #4)
  - [x] 4.1 Added `fc_current_album` shared variable to track current now-playing album
  - [x] 4.2 `StateChanged` handler stores `update.album` in `fc_current_album`
  - [x] 4.3 `CoverRefreshed` handler checks `fc_current_album` against `album_id`, updates now-playing cover
  - [x] 4.4 `CoverPaths` is also updated for backward compat
  - [x] 4.5 `fc_np_cover` is directly updated with decoded texture

- [x] 5. Remove dead `CoverFetcher` code (AC: #5)
  - [x] 5.1 Removed `CoverFetcher` struct, `fetch_via_mpd` function, all dead code
  - [x] 5.2 Kept module exports for `actual_read` and `provider`
  - [x] 5.3 Removed unused imports (`std::io::Write`, `std::path::PathBuf`, `crate::mpd::MpdAdapter`, `std::collections::HashMap`)

- [x] 6. Test and verify (AC: #1-#5)
  - [x] 6.1 `cargo build` — clean build, no warnings
  - [x] 6.2 `cargo test` — all 36 tests pass (21 unit + 15 integration)
  - [x] 6.3 `CoverFetcher` is fully removed, no dead code references remain

## Dev Notes

### Architecture Context

The two-layer cover pipeline (CoverProvider + ActualRead) is already fully implemented in Stories 13.1-13.3. The widget registry (`HashMap<String, Picture>`) has been used since v1 via the `CoverPaths` event handler. This story formalizes:

1. **`CoverRefreshed` event** — the raw-bytes variant that the architecture document specifies (see architecture.md §Cover Art Event Flow)
2. **Dead code cleanup** — `CoverFetcher` was explicitly left in place during Story 13.2 with a note to remove in Story 13.4
3. **Direct texture decoding** — the UI thread decodes JPEG bytes into `GdkTexture` via `gdk-pixbuf`, setting as paintable on the widget

### Current Flow (Before)

```
ActualRead fetches cover
  → write_cache() → writes JPEG to {cache_dir}/{md5}.jpg
  → emit_cover_path() → MpdEvent::CoverPaths({album → Some(path)})
  → UI handler: cover_paths HashMap + widget registry → set_filename(path)
```

### New Flow (After)

```
ActualRead fetches cover
  → write_cache() → writes JPEG to {cache_dir}/{md5}.jpg
  → emit_cover_path() → MpdEvent::CoverPaths({album → Some(path)})  [kept for backward compat]
  → emit_cover_refreshed() → MpdEvent::CoverRefreshed { album_id, data }
  → UI handler:
    → decode data via gdk_pixbuf::Pixbuf::from_read()
    → create gdk::Texture via Texture::for_pixbuf()
    → widget_registry lookup by album_id
    → pic.set_paintable(Some(&texture)) + queue_draw()
    → if matches now-playing album: also update now-playing cover widget
```

### Decoding in GTK4

The `gdk-pixbuf` crate (v0.22) is already a dependency. The decoding pattern:

```rust
use gdk_pixbuf::Pixbuf;
// data is &[u8] from the event
let cursor = std::io::Cursor::new(data);
if let Ok(pixbuf) = Pixbuf::from_read(cursor) {
    let texture = gdk4::Texture::for_pixbuf(&pixbuf);
    picture.set_paintable(Some(&texture));
}
```

`Pixbuf::from_read()` accepts any `Read + Seek` type. `Cursor<&[u8]>` satisfies both. It uses the system's gdk-pixbuf library for JPEG decoding, which is always available on Linux.

The `gdk4::Texture::for_pixbuf()` method creates a texture that can be passed to `gtk4::Picture::set_paintable()`. This is the idiomatic GTK4 way to display images from raw bytes.

### Why Keep CoverPaths

The `CoverPaths` event is still needed for:
- **Now-playing cover at startup** — `update_now_playing()` reads from `cover_paths` HashMap on every `StateChanged` event. This provides cached-path lookup for the now-playing cover without re-decoding bytes.
- **Background loading** — `set_filename()` is simpler and more efficient for the common case where the cache file exists.
- **Gradual migration** — removing `CoverPaths` would break now-playing's path-based lookup. Kept for backward compat; `CoverRefreshed` provides the new raw-bytes path.

However, `CoverRefreshed` now ALSO directly updates the now-playing cover widget (task 4), so the now-playing cover updates immediately when a cover is refreshed, regardless of whether the `StateChanged` handler fires.

### Finding the Now-Playing Album Name

The current `StateChanged` handler stores the current album name. We need to capture it for use in the `CoverRefreshed` handler. The variable is typically named something like `fc_np_album` in the `src/ui/mod.rs` event closure. Let me find it:

Looking at the `StateChanged` handler:
```rust
MpdEvent::StateChanged(update) => {
    update_now_playing(&update, ...);
    // ...
}
```

The `update` struct has `update.album` which is `Option<String>`. We need to capture this as a shared variable. Looking at how `update_now_playing` is called, the album is used as `w.album.set_text(a)` where `a = update.album.as_deref()`.

We'll need to find or create a shared `Rc<RefCell<Option<String>>>` for the current album name. Let me check if one already exists...

Actually, looking at the `update_now_playing` signature:
```rust
fn update_now_playing(w: &NowPlayingWidgets<'_>, update: &PlaybackUpdate) {
    if let Some(ref a) = update.album {
        w.album.set_text(a);
        // reads cover_paths for this album
    }
}
```

The now-playing widgets are reconstructed from fc_np_* references directly. The album name is just used to look up in `cover_paths`. For `CoverRefreshed`, we need access to the current album name. We can:

1. Check the now-playing album label text at refresh time
2. OR store the current album name in a shared Rc<RefCell<Option<String>>>

Looking at how fc_np_* variables work, they capture closures. The simplest approach: just check the now-playing album label text when CoverRefreshed fires.

Actually, the simplest approach: modify `update_now_playing` to also store the current album in a shared variable, or just check the label widget's text. Let me look at how the now-playing album label is referenced.

Let me look at fc_np references more carefully.

Actually, the simplest clean approach: add a shared `Rc<RefCell<Option<String>>>` for the current now-playing album name, populated by the `StateChanged` handler. But to keep changes minimal, I can just check if the album_id appears in the `cover_paths` map AND the now-playing is visible with that album title.

Hmm, let me just go with: in the `CoverRefreshed` handler, clone the `fc_ev_cover_paths` to the now-playing closure. Actually even simpler: `CoverRefreshed` can just push the path into `cover_paths` so `update_now_playing` picks it up on next `StateChanged`. The grid widget gets the immediate update via `set_paintable`.

That's the cleanest approach. The grid widget updates immediately (from `CoverRefreshed`), and the now-playing widget updates on the next `StateChanged` (which happens every 500ms or on the next user action).

But the AC says: "Given CoverRefreshed fires for the currently playing album, when the handler processes the event, then the now-playing cover widget is also updated with the new cover."

So we do need to update now-playing immediately. The simplest way: pass the now-playing Picture into the CoverRefreshed handler.

Looking at the closures in `build_ui`, the now-playing cover is `fc_np_cover`. Let me check.

Actually, looking at line ~1105:
```
cover: &fc_np_cover,
```

So `fc_np_cover` is the now-playing Picture widget, already captured in the event closure's scope. Good. We also need the album name. Let me check what variable holds the album label.

Line ~1401-1404:
```
struct NowPlayingWidgets<'a> {
    album: &'a Label,
    ...
}
```

And it's called at line ~1105 with `update_now_playing(&NowPlayingWidgets {..., album: &fc_np_album_label, ...}, &update)`.

So `fc_np_album_label` is the Label widget for the album name. We can read its text to get the current album name.

But reading GTK widget text is fine:
```rust
let current_album = fc_np_album_label.text().to_string();
if album_id == current_album {
    // update now-playing cover
}
```

Wait, that would involve borrowing issues. Better to store the album in a shared variable.

OK let me just add a `fc_current_album: Rc<RefCell<Option<String>>>` that gets set from the `Step 1: update now-playing ...` code path. The `StateChanged` handler already runs `update_now_playing`, so I can add it there.

Let me simplify: I'll look at the actual code in `build_ui` to find the existing closure scope and understand what variables are available.

Actually, I realize I should stop analyzing and just write the story file. The dev agent will figure out the exact implementation. Let me make the story file complete and correct, with enough context for the dev to work from.

### Key Source Files to Touch

| File | What to Change |
|------|---------------|
| `src/mpd/state_machine.rs` | Add `CoverRefreshed { album_id: String, data: Vec<u8> }` variant |
| `src/coverart/actual_read.rs` | Add `emit_cover_refreshed()`, call from `handle_albumart_data` and `handle_readpicture_data` |
| `src/coverart/mod.rs` | Remove dead `CoverFetcher`, `fetch_via_mpd`, unused imports |
| `src/ui/mod.rs` | Add `CoverRefreshed` match arm, decode bytes, update grid + now-playing widgets |

### Testing Strategy

- `cargo build` must complete with 0 warnings and no dead_code warnings about `CoverFetcher`
- `cargo test` — all existing 36+ tests pass (no behavioral change to existing tests)
- Manual: run with `RUST_LOG=info cargo run`, check log output shows `[UI] cover refreshed: '{album}' (N bytes)` when covers load
- Manual: verify now-playing cover updates when a new cover is fetched for the current album

### References

- Architecture: `_bmad-output/planning-artifacts/architecture.md` §Cover Art Event Flow — `CoverRefreshed` event definition, raw-bytes handoff
- Architecture: §Proven Patterns from Validation Session — "Widget registry for covers: HashMap<String, Picture> maps album names to GTK Picture widgets"
- Architecture: §Image Pipeline ADR — "Decode: Background thread decodes via gdk-pixbuf or image crate. Result sent as Vec<u8> in CoverRefreshed event"
- Architecture: §Cover Subsystem Contracts — fault behavior for cache write failures
- Previous story: `_bmad-output/implementation-artifacts/13-2-actualread-background-fetch.md` — notes "Do NOT remove CoverFetcher yet — it will be fully removed in Story 13.4"
- Current code: `src/ui/mod.rs` lines ~1317-1335 — existing `CoverPaths` handler
- Current code: `src/coverart/mod.rs` — `CoverFetcher` struct to remove
- Current code: `src/coverart/actual_read.rs` — ActualRead implementation to modify

## Dev Agent Record

### Agent Model

deepseek-v4-flash

### Debug Log References

- `[UI] cover refreshed: '{album_id}' (N bytes)` — logged on successful CoverRefreshed decode and widget update
- `[UI] cover refresh failed: couldn't decode image for '{album_id}'` — logged on gdk-pixbuf decode failure
- `[actual_read]` prefix for all ActualRead log messages
- `[cover_provider]` for CoverProvider log messages

### Completion Notes

Story 13.4 implemented: Widget Registry Integration for In-Place Updates. Key changes:

1. **Added `CoverRefreshed` event** (`src/mpd/state_machine.rs`): New `CoverRefreshed { album_id: String, data: Vec<u8> }` variant carries raw JPEG bytes for direct texture decoding on the UI thread.

2. **ActualRead emits CoverRefreshed** (`src/coverart/actual_read.rs`): Added `emit_cover_refreshed()` method, called from both `handle_albumart_data()` and `handle_readpicture_data()` after cache write. Kept existing `CoverPaths` emission for backward compat — both events fire for each new cover.

3. **UI handler decodes raw bytes** (`src/ui/mod.rs`): Added `CoverRefreshed` match arm in the frame clock event loop. Decodes JPEG bytes via `gdk_pixbuf::Pixbuf::from_read()` → `gdk4::Texture::for_pixbuf()` → `pic.set_paintable()`. Updates both the album grid widget (via widget registry lookup in `cover_widgets`) and the now-playing cover (via `fc_np_cover`).

4. **Now-playing tracking** (`src/ui/mod.rs`): Added `fc_current_album` shared variable populated by the `StateChanged` handler. `CoverRefreshed` handler checks against this to update the now-playing cover immediately.

5. **Dead code removal** (`src/coverart/mod.rs`): Removed entire `CoverFetcher` struct, `impl CoverFetcher`, `fetch_via_mpd` function, and all unused imports. Module now only re-exports `ActualRead`, `CachedCover`, and `CoverProvider`.

6. **Build + tests**: `cargo build` — 0 warnings. `cargo test` — all 36 tests pass (21 unit + 15 integration).

### File List

- `src/mpd/state_machine.rs` — Added `CoverRefreshed { album_id: String, data: Vec<u8> }` variant to `MpdEvent` enum
- `src/coverart/actual_read.rs` — Added `emit_cover_refreshed()` method; wired into `handle_albumart_data()` and `handle_readpicture_data()`; updated comments
- `src/coverart/mod.rs` — Removed dead `CoverFetcher`, `fetch_via_mpd`, and unused imports
- `src/ui/mod.rs` — Added `fc_current_album` tracking; added `CoverRefreshed` handler with gdk-pixbuf decode, grid widget update, and now-playing cover update
