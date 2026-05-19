# Debug Session 2026-05-06

## Scroll freeze diagnosis — debug logging added

### Bind callback (`album_factory.connect_bind`)
- `log::debug!("[bind] cache HIT key=... t=...us")` — texture cache hit
- `log::debug!("[bind] cache MISS key=... path=... t=...us")` — cache miss, reading from disk
- `log::debug!("[bind] decoded WxH from disk in ...us key=...")` — disk decode timing
- `log::debug!("[bind] NO COVER key=... t=...us")` — no cover path, using placeholder

### batch_populate
- `log::debug!("[batch_populate] clearing N cover widgets, writing M items")` — tracks widget clearing

## Resize not reconstructing group rows — root cause + fix

### Root cause
`pad_groups()` is called with `grid.width()` before the grid is allocated (it's hidden in a Stack at call time). GridView width returns 0 → fallback to `max_columns()=6`. After `set_visible_child()` shows the grid, the paned width doesn't change → no resize signal fires. The periodic 2s fallback should catch it eventually but `try_borrow()` on `album_grid_data` can fail if bind callbacks hold the borrow during scroll.

### Fix applied
1. Refactored resize handler into a `do_repad` helper closure (shared across 4 signal sources)
2. Added `left_scroll.connect_notify_local("width")` as a 3rd signal source (catches Stack→grid transitions)
3. Added `glib::timeout_add_local_once(200ms)` deferred repad at every `batch_populate` + `set_visible_child` callsite (4 locations: Albums event, AlbumsGrouped event, SearchResults event, local search_changed handler)
4. The deferred repad carries its own generation counter check to debounce against signal-triggered repads

### Files changed
- `src/ui/mod.rs`: resize handler refactored, 4 deferred repad blocks added, debug logging added to bind callback and batch_populate

---

## ICC sRGB conversion not stripped — fix

### User report
`glycin::icc: Converting to sRGB via ICC profile` still appears in logs. sRGB conversion should be dropped for covers.

### Root cause
Two code paths deliver cover data to the UI:
1. **CoverPaths (cache-based)**: `write_cache` strips ICC → writes to disk → UI reads from file via `Pixbuf::from_file_at_size`. This path correctly strips ICC.
2. **CoverRefreshed (raw bytes)**: `emit_cover_refreshed` sent RAW (unstripped) data via `MpdEvent::CoverRefreshed` → UI decodes via `Pixbuf::from_read(cursor)` → glycin processes the ICC profile. This path NEVER stripped ICC.

### Fix applied
- Added `strip_jpeg_icc()` free function in `src/coverart/actual_read.rs` — scans JPEG for APP2 markers (0xFF 0xE2) and removes them with their length-prefixed data
- Strip ICC in `emit_cover_refreshed()` before sending `MpdEvent::CoverRefreshed`
- Strip ICC in `write_cache()` before writing to disk (was already there from earlier uncommitted change)
- Updated test `test_enqueue_replaces_previous` → `test_enqueue_dedup` to match new `enqueue` append-with-dedup behavior

### Files changed
- `src/coverart/actual_read.rs`: added `strip_jpeg_icc()`, ICC strip in `emit_cover_refreshed`, test fix

---

## Grid stuck on resize + freeze — second fix attempt

### User report
- "changing windows size (rescale) does not trigger grid rebuild with correct empty cells positions"
- "application still freeze silently on scroll and window resize"
- "add additional logging to investigate freezes and grid stuck layout on window resize"

### Root cause
Previous fix (commit/change earlier today) replaced `idle_add_local` with `timeout_add_local_once(150ms)` for resize repad. During continuous resize, `paned::width` fires repeatedly, each call resets the 150ms timer, so the timer never fires. Grid never updates during resize. Also removed `paned::position` handler entirely.

### Fix applied (v2)
1. **Reverted to `idle_add_local`** with a **100ms cooldown** (`resize_last_time: Rc<Cell<Instant>>`):
   - Signal fires → check cooldown → if < 100ms since last rebuild, skip with log
   - Otherwise, schedule `idle_add_local` with generation counter
   - Generation counter cancels stale idles, cooldown limits rebuild frequency
2. **Restored `paned::position` handler** — pane splitter drag changes grid allocation without changing paned width
3. **Replaced all `timeout_add_local_once` calls** (`trigger_repad`, search handler repad) back to `idle_add_local`
4. **Added extensive debug logging**:
   - `[resize]` — signal handler: width, cols, cooldown checks, skipped reasons
   - `[resize:idle]` — idle callback: gen check, pad timing, populate timing, total time
   - `[resize:trigger]` — initial-load deferred repad: same format
   - `[resize:search-repad]` — search deferred repad: same format
   - `[batch_populate]` — prep time, splice time, total time, item counts

### Files changed
- `src/ui/mod.rs`: cooldown logic, signal handlers, logging throughout resize/repad pipeline

---

## glycin pool cleanup (informational)

### User question
What are these logs?
```
glycin::pool: Cleaning up loaders
glycin::pool: Loader ".../glycin-image-rs": drop true users 0
glycin::pool: Dropping loader ".../glycin-image-rs" 1
glycin::dbus: Winding down process
glycin::dbus: Killing process due to cancellation
```

### Answer
Normal glycin idle cleanup. glycin (GTK's image loading library) spawns sandboxed subprocesses (`bwrap`) for image decoding. After 30s of inactivity, idle loaders are dropped. This is expected behavior — not related to the freeze issue. The process exit code 9 (SIGKILL) is from glycin forcefully terminating the sandbox after the D-Bus cancellation.

---

## Architectural analysis: offloading image processing from GTK to pure Rust

### Trigger

User asked: "why are we using GTK for image processing? it should be something perfectly clean and simple: MPD -> image in rust thread processing to raw image -> GTK only for representing on UI"

### Current pipeline — everything through glycin

```
MPD ──[TCP]──► raw JPEG bytes
                  │
                  ├──► write_cache() ──► disk (*.jpg, ICC-stripped)
                  │
                  └──► CoverRefreshed event (raw bytes)
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
     Pixbuf::from_file_at_size    Pixbuf::from_read
              │                         │
              ▼                         ▼
    ┌─────────────────────────────────────────┐
    │           glycin sandbox                 │
    │  • bwrap --unshare-all --seccomp        │
    │  • D-Bus IPC to subprocess              │
    │  • ICC profile detection + sRGB conv    │
    │  • JPEG decode to pixels                │
    │  • 30s idle timeout → SIGKILL           │
    └─────────────────────────────────────────┘
              │                         │
              ▼                         ▼
         scale_simple              scale_simple
              │                         │
              ▼                         ▼
         Texture::for_pixbuf       Texture::for_pixbuf
```

**glycin cost per image load:**
| Step | Est. time | Notes |
|------|-----------|-------|
| bwrap sandbox spawn | 1–5ms | One-time, reused while active |
| D-Bus IPC | ~1ms | Per call |
| ICC profile parsing | 2–10ms | Only if ICC present |
| ICC matrix transform + LUT | 5–40ms | Depends on profile complexity |
| JPEG decode to RGBA | 2–8ms | Standard decode |
| scale_simple | ~1ms | Bilinear downscale to 200×200 |
| **Total (cold)** | **~11–65ms** | Worst case: first load with ICC |
| **Total (warm, no ICC)** | **~3–10ms** | Sandbox already running, no ICC |
| Idle cleanup (30s) | ~5ms | SIGKILL sandbox, D-Bus teardown |

**All 4 gdk-pixbuf call sites in the codebase (src/ui/mod.rs):**

1. **Line 70–75** — `placeholder_texture()`: creates a solid-color 200×200 placeholder via `Pixbuf::new` + `fill` + `Texture::for_pixbuf`
2. **Line 539–540** — Bind callback: `Pixbuf::from_file_at_size(p, 200, 200)` — reads from disk cache on cache miss
3. **Line 2599–2600** — Now-playing cover update: same pattern, reads from disk
4. **Line 2640–2644** — CoverRefreshed handler: `Pixbuf::from_read(cursor)` → `scale_simple` → `Texture::for_pixbuf` — decodes raw JPEG bytes from event

### Proposed pipeline — pure Rust, GTK only for display

```
MPD ──[TCP]──► raw JPEG bytes
                  │
                  ├──► write_cache() ──► disk (still JPEG for persistence)
                  │
                  └──► CoverRefreshed event (raw bytes)
                  │         │
                  │         ▼
                  │    image::load_from_memory(bytes)   ← pure Rust JPEG decoder
                  │         │                           ← no sandbox, no D-Bus, no ICC
                  │         ▼
                  │    DynamicImage (RGBA pixels)
                  │         │
                  │         ├──► image::resize_exact(200, 200, Lanczos3)
                  │         │                            ← high-quality downscale in Rust
                  │         ▼
                  │    raw RGBA bytes (Vec<u8>)
                  │         │
                  │         ▼
                  │    gdk4::MemoryTexture::new(w, h, R8g8b8a8, &bytes, stride)
                  │         │                            ← zero-copy GPU upload
                  │         ▼                            ← no glycin, no sandbox, no ICC
                  │    gtk4::Picture.set_paintable()
                  │
                  └──► (cached file path)
                           │
                           ▼
                      image::open(path)                 ← pure Rust file decode
                           │
                           ▼
                      (same MemoryTexture path)
```

### What changes

| Current | Proposed | Savings |
|---------|----------|---------|
| `gdk-pixbuf` crate (0.22) | **removed** | Entire dependency gone |
| `Pixbuf::from_file_at_size` | `image::open` + `MemoryTexture::new` | No glycin |
| `Pixbuf::from_read` | `image::load_from_memory` + `MemoryTexture::new` | No glycin |
| `pixbuf.scale_simple` | `image::resize_exact` | Rust-side, Lanczos3 > bilinear |
| `Texture::for_pixbuf` | `MemoryTexture::new` | Zero-copy, no intermediate pixbuf |
| `Pixbuf::new` + `fill` (placeholder) | `image::RgbImage::from_pixel` + `MemoryTexture` | No glycin even for placeholder |
| `strip_jpeg_icc()` | **removed** | `image` crate decodes directly to sRGB |
| ICC sRGB conversion | **removed** | Not needed — no ICC in decode path |
| glycin sandbox (bwrap) | **removed** | No sandbox subprocess at all |
| glycin D-Bus IPC | **removed** | No D-Bus for image loading |
| glycin pool cleanup logs | **removed** | Cleaner debug output |

### What stays in GTK

Only two things, both pure GPU-side:
- **`gdk4::MemoryTexture::new(w, h, format, bytes, stride)`** — wraps raw RGBA bytes, uploads to GPU as a texture. No I/O, no sandbox, no color management. The stride is `width * 4` for RGBA.
- **`gtk4::Picture.set_paintable(Some(&texture))`** — tells GTK to display the texture. Pure display compositing.

### Prerequisites already in place

- **`image` crate v0.25** — already in `Cargo.toml` with `jpeg`, `png`, `webp` features enabled. Currently compiled but completely unused.
- **`gdk4::MemoryTexture`** — available in gdk4 v0.11.2. Constructor takes `(width, height, MemoryFormat, &[u8], stride)`.
- **`gdk4::MemoryFormat::R8g8b8a8`** — matches `image` crate's RGBA8 output format exactly.

### Trade-off: CMYK JPEGs

`image` crate's JPEG decoder does not support CMYK JPEGs (returns `UnsupportedColorSpace` error). Mitigations:
- Album covers are universally RGB — CMYK is a print format, not used in digital music metadata
- If encountered: `image` returns `Err`, we fall back to the existing placeholder path which already handles decode failures gracefully
- Probability estimate: <0.01% of real-world album covers

### Estimated impact

| Metric | Current (glycin) | Proposed (image crate) |
|--------|-----------------|----------------------|
| Cold decode (first image) | 11–65ms | 3–10ms |
| Warm decode (subsequent) | 3–10ms | 2–6ms |
| Memory per decode | Sandbox ~20MB + pixbuf | Inline ~0.4MB (200×200×4) |
| Binary size | +gdk-pixbuf crate | Already includes `image` crate |
| ICC handling | Automatic (glycin) | None (decoded as sRGB) |
| Sandbox overhead | bwrap + D-Bus | None |
| Log noise | glycin pool/icc/dbus lines | None |

### Implementation plan

**Phase 1 — CoverRefreshed path (raw bytes, most critical)**
- Replace `Pixbuf::from_read(cursor)` + `scale_simple` (lines 2640–2644) with `image::load_from_memory` + `resize_exact` + `MemoryTexture`
- This eliminates ICC conversion on the hot path

**Phase 2 — File path (bind callback + now-playing)**
- Replace `Pixbuf::from_file_at_size` (lines 539, 2599) with `image::open` + `resize_exact` + `MemoryTexture`

**Phase 3 — Placeholder**
- Replace `Pixbuf::new` + `fill` (lines 70–75) with `image::RgbImage::from_pixel` + `MemoryTexture`

**Phase 4 — Cleanup**
- Remove `strip_jpeg_icc()` and its call sites
- Remove `gdk-pixbuf` from `Cargo.toml`
- Remove ICC-stripping comments throughout actual_read.rs

### Files affected
- `src/ui/mod.rs`: 4 call sites (~50 lines changed)
- `src/coverart/actual_read.rs`: remove strip_jpeg_icc (~25 lines removed)
- `Cargo.toml`: remove `gdk-pixbuf` dependency (1 line)

---

## Widget tree analysis: why resize rebuild fails

### User question

"What is paned/stack? What is it for? Explain it to me."

### Current widget tree

```
Window (top-level)
 └── GtkBox (vertical, top_bar + content)
      ├── top_bar (album/folder switch, group buttons, search bar)
      └── GtkPaned (split container, draggable separator)
           ├── left child: GtkStack
           │    ├── Label "Connecting to MPD..."       ← startup, shown until connected
           │    ├── Label "No albums found"            ← shown when library is empty
           │    └── GtkScrolledWindow                  ← shown when albums are loaded
           │         └── GtkGridView (one model: 500+ items including fillers)
           └── right child: GtkStack (queue mini-grid / queue list / now-playing)
```

**GtkPaned**: A two-pane split container with a draggable separator bar. The left pane holds the album browser, the right pane holds the queue/now-playing. The user can drag the separator to resize the left/right proportions. This is why the grid's available width changes during both window resize AND splitter drag.

**GtkStack**: Shows exactly one child at a time, like a tab widget without tabs. Used for state-dependent views — "Connecting..." during startup, "No albums found" when empty, the actual grid when data is available. `set_visible_child()` switches between them.

### Why resize rebuild fails: allocation propagation order

During window resize, GTK allocates widget sizes top-down through the tree:

```
Step 1: Window gets new size from compositor
Step 2: Box gets new allocation
Step 3: GtkPaned gets new allocation         ← paned::width signal fires HERE
Step 4: GtkStack gets new allocation from paned
Step 5: GtkScrolledWindow gets new allocation
Step 6: GtkGridView gets new allocation      ← grid::width signal fires HERE
                                            ← grid.width() is NOW correct
```

Our resize handler connects to `paned::width` at **Step 3**. At that point, the grid hasn't been allocated yet — `grid.width()` returns the **old** width from the previous layout cycle. The column check uses stale data:

```
Window: 1000px → 1200px (user drags right edge wider)
  Step 3: paned::width fires
          grid.width() = 800px (stale, from previous allocation)
          cols = 800 / 216 = 4
          last_cols was 4 → cols == last_cols → return early
          → NO REBUILD SCHEDULED
  Step 6: grid::width fires (grid now at 1000px)
          cols should be 5 (1000 / 216)
          → nobody is listening
```

The grid is 3 widget levels deep inside the paned (Stack → ScrolledWindow → GridView). The deeper the nesting, the more stale the `grid.width()` read at paned signal time.

The fix of listening to `grid::width` (Step 6) was discussed but not yet implemented — it correctly catches the post-allocation width but doesn't address the deeper architectural issue with fillers-in-the-model.

---

## Architectural proposals: eliminate the filler system entirely

### User question

"Why can't we have several grids in the scrolled window, each for each group? It removes the problem with resize entirely. And we can insert captions between grids too."

And: "What about a scrolled window with all album widgets and captions placed pure mathematically? Can we do it? Logic is simple — just count proper coordinates."

### The root problem being solved

The current architecture mixes layout with data: `AlbumGridItem::Filler` items are layout artifacts stored in the data model. When the window resizes, column count changes, filler positions change everywhere, and the entire model must be rebuilt via `model.splice()` — an expensive operation that destroys and recreates all visible GTK widgets.

Both proposals eliminate fillers from the data model entirely.

### Proposal A: One GridView per group

```
GtkScrolledWindow
 └── GtkBox (vertical)
      ├── GtkLabel "Artist A"              ← real widget, not an item
      ├── GtkGridView (10 albums, no fillers) ← GTK reflows natively
      ├── GtkLabel "Artist B"
      ├── GtkGridView (7 albums, no fillers)
      ├── GtkLabel "Artist C"
      └── GtkGridView (3 albums, no fillers)
      ...
```

**How it works:** Each group gets its own small GridView containing only album items. GTK's built-in GridView column flow handles reflow on width change natively — no padding, no fillers, no signal handlers, no `pad_groups()`, no `batch_populate` rebuild on resize.

**What is deleted:**
- `pad_groups()` — entirely removed
- `AlbumGridItem::Filler` — variant deleted from enum
- `do_repad` / `trigger_repad` — no resize rebuild logic
- `resize_last_cols` / `resize_generation` / `resize_last_time` — no debouncing
- All resize signal handlers — GridViews handle their own reflow
- Group padding logic — captions are real GtkLabels between grids

**Widget count:** 500 albums across 50 groups → 500 GridView child widgets (all created eagerly, no recycling). 500 widgets × ~2KB = ~1MB memory. GTK only paints visible widgets — GPU load bounded by viewport.

**Trade-off:** GridView recycling is lost — all widgets live in memory. For 5000+ albums the startup time increases linearly. For typical libraries (hundreds to low thousands) this is well within budget.

**Resize behavior:** Window gets wider → each GridView immediately reflows its items using one fewer row. Zero application code involved — GTK handles it.

### Proposal B: Coordinate math (GtkLayout)

```
GtkScrolledWindow
 └── GtkLayout (one flat container, infinite canvas)
      ├── GtkLabel "Artist A"              ← move(widget, 0.0, y)
      ├── AlbumCoverCell                   ← move(widget, x * 216.0, y)
      ├── AlbumCoverCell                   ← move(widget, x * 216.0, y)
      ├── GtkLabel "Artist B"
      ├── AlbumCoverCell
      └── AlbumCoverCell
      ...
```

**How it works:** Place every widget in a single GtkLayout (a fixed-position container that supports scrolling). On resize, iterate through all items once, compute new (x, y) coordinates, call `layout.move_(widget, x, y)`. No GridView, no ListModel, no factories, no bind/unbind callbacks, no splice. Pure Rust math → GTK display.

**Resize algorithm (~30 lines):**
```rust
fn reposition(layout: &gtk4::Layout, width: f64, items: &[AlbumWidget]) {
    let cols = (width / 216.0).floor().max(1.0);
    let mut x = 0.0;
    let mut y = 0.0;
    let mut current_group = None;

    for item in items {
        if item.group_label != current_group {
            // Caption row
            layout.move_(&item.caption, 0.0, y);
            x = 0.0;
            y += CAPTION_HEIGHT;
            current_group = item.group_label.clone();
        }
        layout.move_(&item.cover_cell, x * CELL_WIDTH, y + y_offset);
        x += 1.0;
        if x >= cols { x = 0.0; y += CELL_HEIGHT; }
    }
    layout.set_size(width, y + CELL_HEIGHT);
}
```

**Performance:** 500 items → 500 `layout.move_()` calls → ~1ms. The function just sets x/y fields on GTK widgets. No widget creation, no texture reload, no model manipulation. GTK queues a redraw only for changed areas.

**What is deleted (in addition to A):**
- All ListModel/Selection/Factory machinery
- All bind/unbind callbacks
- `StringObject` index hack (`StringObject::new(&i.to_string())`)
- `AlbumGridData` RefCell — update items in-place via direct widget access
- Widget registry `cover_widgets` HashMap — widgets are always alive, just call `set_paintable()` directly

**What stays:** Just `GtkLayout` + `GtkScrolledWindow` + `AlbumCoverCell` widgets + captions. Three widget types handling display only — all layout logic is in Rust.

### Comparison

| | Current | Proposal A (multi-GridView) | Proposal B (coord math) |
|---|---|---|---|
| Resize logic | signal → idle → cooldown → model rebuild (~100 lines) | **none** (GTK built-in) | reposition loop (~30 lines) |
| Fillers | `AlbumGridItem::Filler` + `pad_groups()` | deleted | deleted |
| Group captions | fake items in model | real GtkLabel widgets | real GtkLabel widgets |
| Widget lifecycle | recycled by GridView (~30-50 alive) | all alive (~500) | all alive (~500) |
| Widget memory | ~60-100KB | ~1MB | ~1MB |
| ListModel required | yes (ListStore + Selection + Factory) | yes (per group) | **no** |
| bind/unbind callbacks | yes (cache check per visible item) | yes (per group) | **no** |
| Cover texture update | widget registry HashMap lookup | widget registry lookup | direct `set_paintable()` on always-alive widget |
| Resize speed | 5–50ms (model rebuild + bind) | instant (GTK native) | ~1ms (iterate + move) |
| Startup | creates ~30 widgets | creates ~500 widgets (~50ms) | creates ~500 widgets (~50ms) |
| Lines of code to delete | baseline | ~150 | ~250 |

### Recommendation

**Proposal B (coordinate math)** is the cleanest match for the project's stated principle: "MPD → Rust processing → GTK only for display." It removes the most code, entirely eliminates the category of resize bugs, drops the factory/bind/unbind lifecycle management, and replaces it with a straightforward reposition loop that any programmer can reason about. The 1MB widget memory cost is negligible against the 200MB budget.

Cover updates become simpler too: instead of the widget registry pattern (`HashMap<String, Picture>` + `borrow()` + `get()` + conditional `set_paintable()`), each `AlbumCoverCell` is always alive. When a `CoverRefreshed` or `CoverPaths` event arrives, we find the cell by index (O(1) Vec lookup) and call `cell.set_cover_texture(&texture)` directly — no HashMap, no RefCell, no borrow contention.
