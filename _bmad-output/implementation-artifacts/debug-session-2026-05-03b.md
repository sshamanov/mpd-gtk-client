# Debug Session — 2026-05-03b

## Grid Cell Height Jump After Startup

### Symptom
Cover grid in album mode looks correct on start (~250px cell height), then 0.2-0.5s later cells jump to ~1200px. Suspect cover art async updates are the trigger.

### Investigation

#### Cover update flow (3 paths to grid Picture widgets)

1. **`bind` handler** (line 563-576): Sets placeholder (200x200) or cover from `cover_paths` via `Pixbuf::from_file_at_size(p, 200, 200)` → `Texture::for_pixbuf()` → `pic.set_paintable()`. Correctly scaled.

2. **`CoverPaths` handler** (line 2323-2357): `Pixbuf::from_file_at_size(p, 200, 200)` → texture → `pic.set_paintable()`. Correctly scaled.

3. **`CoverRefreshed` handler** (line 2359-2390): `Pixbuf::from_read()` at native resolution → `scale_simple(200, 200, Bilinear)` → texture → `pic.set_paintable()`. Commit `676c998` fixed the `unwrap_or(pixbuf)` native-resolution fallback; now skips if scale_simple returns None.

All 3 paths produce 200x200 textures. So the textures themselves shouldn't cause 1200px cells.

#### Potential root causes

**A. `set_filename` paths (mini-queue + now-playing, NOT grid)**

Line 1423: `cover.set_filename(Some(path))` — mini-queue cover, loads at native resolution.
Line 2517: `w.cover.set_filename(Some(p))` — now-playing cover, loads at native resolution.
Line 52 (album_cover.rs): `cover_image.set_filename(Some(path))` — widget helper, loads at native resolution.

These don't affect the grid directly, but confirm native-resolution loading happens elsewhere.

**B. Grid row height sharing**

GtkGridView rows share height — all cells in a row have the same height negotiated as the max of all cells' natural heights. If ONE cell expands, the entire row expands.

**C. `scale_simple` failure edge case**

If `scale_simple` returns None, the code now skips (post-676c998). But what if it returns Some but with wrong dimensions? Unlikely but possible with corrupted JPEG data.

**D. CSS `max-height: 250px` not enforced on dynamic updates**

GTK4 CSS sizing via style providers may not be re-evaluated on `set_paintable()` changes. The initial layout respects CSS, but dynamic texture changes might bypass CSS constraint re-evaluation.

**E. `set_paintable` triggers remeasure cascade**

When `set_paintable()` is called with a new texture (even 200x200), the Picture widget queues a resize. This propagates up through Overlay → album_section Box → cell container → GridView row. During this cascade, intermediate widgets might report inflated natural sizes.

### Diagnostic Approach

Added diagnostic logging in `CoverPaths` and `CoverRefreshed` handlers:
- Log texture dimensions after scaling
- Log widget allocation before/after update

Added cover update kill switch: `static COVER_PUSH_ENABLED: AtomicBool` — set to `false` to test whether disabling all cover updates prevents the height jump. If cells stay at 250px with covers disabled, the cover update path is confirmed as the trigger.

### Changes Made

1. **Cover update kill switch**: Added `static COVER_PUSH_ENABLED: AtomicBool` in `ui/mod.rs:20`. Both `CoverPaths` and `CoverRefreshed` handlers skip all widget updates when set to `false`. To test: start the app, then observe whether cells still jump. If they don't, covers ARE the trigger.

2. **Dimension logging**: Both handlers now log native pixbuf dimensions, scaled texture dimensions, and pre-update widget allocation:
   - CoverPaths: `[UI] cover push: '{album}' tex=WxH w=W h=H`
   - CoverRefreshed: `[UI] cover refresh: '{album}' native=WxH, bytes=N` + `scaled=WxH tex=WxH`
   - Both: `...[UI] cover push: updating grid widget for '{album}', alloc before=WxH`

3. **`can-shrink(true)` on grid Picture**: The Picture widget in factory setup now has `set_can_shrink(true)`. Default is false, meaning GTK refuses to allocate less than the texture's natural size. With can-shrink=true, the widget accepts the parent's 200x200 allocation even if the texture reports a larger natural size. This should prevent cell expansion regardless of texture dimensions.

### Ruled Out

- **Cover updates**: Disabled completely — cover adapter no-ops on FetchCovers and process_one. COVER_PUSH_ENABLED=false. Cells still grow.
- **Timer frequency**: All timers stretched 10x (100→1000ms recv, 500→5000ms status, 300→3000ms scroll debounce, etc.). Cells still grow.
- **Drag and drop**: All DnD disabled — grid DragSource, grid DropTarget, queue DropTargets, queue DragSources. Pending test.

### Current State

Cover adapter totally dead, timers 10x slowed, cover pushes blocked, DnD disabled. Symptom persists: cells gradually step-grow, suggesting animation/transition rather than data-driven update.

### Complete Grid Flow (enabled state)

1. **App::run** → builds widget tree: Paned(split=0.7/0.3), left=ScrolledWindow(GridView), right=queue+now-playing
2. **connect_setup** (called once per recycled widget shell): Creates container Box(200x250) with CSS `album-cover-cell`. Inside: header_label (hidden) + album_section Box (hidden). album_section: Overlay[size=200x200, valign=Start] with child Picture[200x200, ContentFit=ScaleDown, can_shrink=true] + title Label + artist Label. DragSource on container (now disabled).
3. **MPD connects** → `Connected` event → sends `ListAlbumsGrouped("Albums")` + `ListQueue`
4. **AlbumsGrouped arrives** → builds `Vec<AlbumGridItem>` (Header+Album variants), calls `batch_populate()`
5. **batch_populate**: `model.remove_all()`, `cover_widgets.clear()`, stores items in `AlbumGridData` Rc<RefCell<Vec>>. Then idle_add_local: appends 16 StringObject indices per idle cycle (~8 batches for 122 items). `ControlFlow::Continue` chains batches; `Break` on completion.
6. **Each model.append()** triggers `connect_bind`: reads index from StringObject, looks up item in AlbumGridData, shows header or album section. For albums: hides header, shows album_section, updates labels, loads cover from `cover_paths` RefCell or generates `placeholder_texture()` (200x200 Pixbuf→Texture). Registers Picture in `cover_widgets` HashMap (key=artist||album).
7. **Initial cover fetch**: idle_add_local after batch_populate completes → sends `FetchCovers(all_albums)` (all albums, not just visible)
8. **FetchCovers** → `actual_read.enqueue(albums)`. One `process_one()` per recv_timeout cycle (100ms) → MPD `albumart`/`readpicture` → MD5 hash → write_cache → emit CoverPaths(path) + CoverRefreshed(raw_bytes)
9. **CoverPaths handler** (frame clock tick, vsync): loads cached JPEG at 200x200 via `Pixbuf::from_file_at_size()`, creates Texture, `pic.set_paintable()` on grid Picture widget from `cover_widgets` HashMap. Also updates mini-queue and now-playing.
10. **CoverRefreshed handler**: decodes raw JPEG bytes at native resolution via `Pixbuf::from_read()`, `scale_simple(200,200)`, `pic.set_paintable()` on same grid Picture. Redundant when CoverPaths already updated the widget.
11. **Scroll cover fetch**: 300ms debounce (now 3000ms) → `calculate_visible_albums()` → `FetchCovers(new_albums)` where `new_albums` filtered to exclude already-covered items (checks `cover_paths` RefCell)
12. **Status polling**: every 500ms (now 5000ms) in connected_loop → `fetch_full_update()` → `StateChanged` event → `update_now_playing()` (updates np_cover via `set_filename()` at native resolution)
13. **Frame clock tick** (every vsync ~16ms): mode check, process up to 64 MPD events from rx, status check. Returns `ControlFlow::Continue`.
14. **Queue polling**: every 30s (now 300s) → `ListQueue` → `QueueChanged` event → rebuilds queue listbox rows

### GridView Column Count

GV calculates columns from: `grid_width / max(1, cell_min_width)`. Cell min width = 200px. For 1200px grid width = 6 columns. For 800px = 4 columns. Column count settles only when ScrolledWindow gets its final allocation. If allocation arrives in steps (window decorations, Paned split), column count changes → relayout → all cells remeasured.

### Split vs Clone Decision

**Split screen (50/50 two grids):** Modifies Paned layout — put two ScrolledWindow+GridView stacks in left pane. ~30-line change. Both grids share same model/data/factory. If only one has the height issue → widget tree problem. If both → model/bind logic.

**Clone mode:** Add a third mode alongside Album/Folder that creates a minimal grid from scratch. More disruptive to existing mode-switching logic. Better for incremental rebuild.

**Recommendation: Split screen** — cheaper, less invasive, direct comparison. Just wrap existing grid + new empty grid in a vertical Box inside the left pane.

### To Test

1. Test with DnD disabled (current build)
2. If still jumping: add a second identical grid below the first — compare behavior
3. If second grid doesn't jump: the issue is in the original grid's widget tree or factory setup
4. If both jump: the issue is in batch_populate, bind logic, or GTK GridView internals

---

## Round 3 — Fix Implementation (2026-05-03)

### Critical Finding: `max-height` Not a Valid GTK4 CSS Property

Checked official GTK4 CSS properties list (https://docs.gtk.org/gtk4/css-properties.html). GTK4 CSS only supports `min-height` and `min-width` for sizing — **`max-height` and `max-width` are NOT valid CSS properties**. The `.album-cover-cell { max-height: 250px; }` rule was a no-op — silently ignored by GTK4's CSS parser. This means cells had a minimum height (via `set_size_request(200, 250)` + CSS `min-height: 250px`) but **no maximum height constraint whatsoever**.

### Fix 1: Atomic Model Replacement via `splice()`

**File:** `src/ui/mod.rs:91-106`

Replaced the incremental `idle_add_local` batch-populate pattern (16 items per idle cycle, ~8 batches for 122 items) with a single `model.splice()` call:

```rust
fn batch_populate(model: &ListStore, backing: &AlbumGridData, items: Vec<AlbumGridItem>,
                  cover_widgets: &std::rc::Rc<std::cell::RefCell<HashMap<String, gtk4::Picture>>>
                  ) {
    cover_widgets.borrow_mut().clear();
    *backing.borrow_mut() = items;
    let total = backing.borrow().len();

    let new_items: Vec<StringObject> = (0..total)
        .map(|i| StringObject::new(&i.to_string()))
        .collect();

    model.splice(0, model.n_items(), &new_items);
}
```

**What changed:**
- `model.remove_all()` + `idle_add_local` with 16-item batches → single `model.splice(0, n_items, &all_new_items)`
- Removed generation counter (`bp-gen`) for canceling stale populates — no longer needed since there's no async chain to cancel
- Removed `use std::cell::Cell` and `use std::rc::Rc` local imports
- Kept `cover_widgets.clear()` — still necessary to prevent stale Picture widget mappings across re-populations (recycled GTK widgets get new album assignments)

**Why it should fix the height jump:** The old incremental pattern caused 8+ sequential GridView layout passes. Each pass measured cells against an incomplete model, and the GridView's column count / row composition calculation produced different results as more items arrived. The gradual stepped height growth (250→1200px) exactly matches the 8-batch cadence. With `splice()`, all items arrive in one atomic operation → one `items-changed` signal → one GridView layout pass.

### Fix 2: Removed Non-Functional CSS `max-height`

**File:** `src/ui/mod.rs:2445`

```diff
- .album-cover-cell { min-height: 250px; min-width: 200px; max-height: 250px; }
+ .album-cover-cell { min-height: 250px; min-width: 200px; padding: 0; margin: 0; }
```

- Removed `max-height: 250px;` — dead code, not a valid GTK4 CSS property
- Added `padding: 0; margin: 0;` — explicit zeroing to eliminate theme CSS padding as a variable in size calculations

### Remaining Concern: No Max-Height Mechanism

Without CSS `max-height`, the cell container's height is constrained only by:
- `set_size_request(200, 250)` — sets **minimum**, not maximum
- `set_valign(Align::Start)` — aligns to top of allocation but doesn't cap
- Children's natural heights (Overlay 200 + labels ~36 = ~236px)

If `splice()` alone doesn't prevent height growth, the next step is to add a widget-level height constraint programmatically (e.g., `set_vexpand(false)` or overriding `measure()`).

### Code Changes Summary

| File | Change | Lines |
|------|--------|-------|
| `src/ui/mod.rs` | Replaced `batch_populate` with splice-based version | 91-106 |
| `src/ui/mod.rs` | CSS: removed max-height, added padding:0; margin:0 | 2445 |
