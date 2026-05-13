# Story 28.2: Cover Proc Worker

Status: done

## Story

As a user,
I want JPEG decoding and cache operations offloaded from both MPD IO and GTK threads,
so that cover processing never blocks the MPD command socket or the UI.

## Acceptance Criteria

1. **Cover Proc thread receives raw JPEG bytes from MPD Cover thread**
   - Given the MPD Cover thread fetches binary cover data
   - When raw JPEG bytes are available
   - Then they are sent via channel directly to the Cover Proc worker
   - And the MPD IO thread is never involved in draining cover results

2. **JPEG decode + resize offloaded to image crate**
   - Given raw JPEG bytes arrive at the Cover Proc worker
   - When the worker processes them
   - Then `image::load_from_memory()` decodes to DynamicImage
   - And `image::resize_exact(200, 200, Lanczos3)` produces RGBA bytes
   - And no gdk-pixbuf, glycin, or GTK code runs on the Cover Proc thread

3. **MD5 hash, cache comparison, cache write on Cover Proc thread**
   - Given decoded RGBA bytes and original JPEG bytes
   - When the Cover Proc worker processes a cover
   - Then MD5 hash is computed from JPEG bytes (cache key)
   - And hash is compared against CoverProvider cache
   - And if new/different: JPEG is written to disk cache, index.json is updated
   - And `CoverRefreshed { album_id, data: RGBA_bytes }` is emitted

4. **UI receives RGBA bytes for zero-copy GPU upload**
   - Given `MpdEvent::CoverRefreshed` arrives with RGBA bytes
   - When the UI handler processes it
   - Then `gdk4::MemoryTexture::new(w, h, R8g8b8a8, &rgba, stride)` creates the texture
   - And no gdk-pixbuf decode or scale_simple is called
   - And existing widget update logic (grid, mini-cover, now-playing) is unchanged

5. **Cleanup: remove gdk-pixbuf from CoverRefreshed path**
   - Given the Cover Proc worker is operational
   - When the migration is complete
   - Then `gdk_pixbuf::Pixbuf::from_read()` is removed from the CoverRefreshed handler
   - `gdk_pixbuf::Pixbuf::scale_simple()` is removed from the CoverRefreshed handler
   - `gdk4::Texture::for_pixbuf()` is replaced with `gdk4::MemoryTexture::new()`

6. **Backward compatibility**
   - Given existing cover cache files on disk
   - When the Cover Proc worker processes a cover
   - Then the cache file format (JPEG with md5 hash filename) is unchanged
   - And index.json format is unchanged
   - And CoverProvider reads from cache identically

## Technical Requirements

### Thread Architecture

Per `architecture.md` §2233-2238, the Cover Proc worker:

```
MPD Cover ──channel──→ Cover Proc ──AppEvent──→ GTK thread
                ↑
          raw JPEG bytes         RGBA bytes (200x200)
```

- **Thread type**: Persistent (1 thread), spawned once
- **Location**: `src/coverart/cover_proc.rs` (NEW)
- **Input channel**: `mpsc::SyncSender<CoverProcJob>` from MPD Cover result channel
- **Output**: `MpdEvent::CoverRefreshed { album_id, data: Vec<u8> }` via `event_tx`
- **data field changes**: from raw JPEG bytes → raw RGBA bytes (200×200×4 = 160KB)

### Cover Proc Job

```rust
pub struct CoverProcJob {
    pub key: String,         // "artist||album"
    pub data: Vec<u8>,       // raw JPEG bytes
    pub mtime: Option<u64>,  // from readpicture, None for albumart
}
```

### Cover Proc Processing Pipeline

For each job:
1. Compute MD5 hash of raw JPEG bytes
2. Compare against CoverProvider cache (read-only via `Arc<RwLock<CoverProvider>>`)
3. If hash matches and (mtime is None or timestamp not newer): skip, no emission
4. If new data:
   a. Write JPEG to disk cache (`{md5}.jpg`), update `index.json`
   b. Decode JPEG via `image::load_from_memory(&jpeg_bytes)` → `DynamicImage`
   c. Resize: `image::imageops::resize_exact(200, 200, FilterType::Lanczos3)`
   d. Convert to RGBA bytes: `.to_rgba8().into_raw()` — 200×200×4 = 160KB
   e. Emit `MpdEvent::CoverRefreshed { album_id: key, data: rgba_bytes }`
   f. Update CoverProvider cache entry

### UI Changes

In `src/ui/mod.rs` CoverRefreshed handler:
- Remove: `Cursor::new(data)`, `Pixbuf::from_read(cursor)`, `pixbuf.scale_simple(200, 200, Bilinear)`, `Texture::for_pixbuf(&scaled)`
- Replace with: `MemoryTexture::new(200, 200, R8g8b8a8, &rgba, 200*4)`
- Widget update logic (fc_ev_cover_widgets, fc_mini_cw, fc_np_cover, fc_current_album) stays identical

### Cleanup

After Cover Proc is operational:
- `strip_jpeg_icc()` function and all call sites can be removed (image crate decodes directly to sRGB)
- The `gdk-pixbuf` dependency remains for other paths (placeholder, cached file reads) — not removed yet

### Integration Points

| File | Change |
|------|--------|
| `src/coverart/cover_proc.rs` | **NEW** — `CoverProc::spawn()` with job channel, MD5/cache/decode/resize/emit |
| `src/coverart/mod.rs` | Add `pub mod cover_proc;` |
| `src/mpd/cover.rs` | Change result channel direction: MPD Cover → Cover Proc instead of MPD Cover → MPD IO |
| `src/mpd/state_machine.rs` | Remove cover result draining from idle/poll loops (moved to Cover Proc) |
| `src/ui/mod.rs` | Replace gdk-pixbuf decode with MemoryTexture in CoverRefreshed handler |
| `src/coverart/actual_read.rs` | Remove `process_cover_result()`, `handle_albumart_data()`, `handle_readpicture_data()`, `emit_cover_refreshed()`, `strip_jpeg_icc()` — moved to Cover Proc |

### Key Rules

- Cover Proc has **no** MPD protocol knowledge — no `MpdAdapter` import
- Cover Proc has **no** GTK imports — `gtk4`, `gdk4`, `gdk_pixbuf` forbidden
- Cover Proc's only I/O: read CoverProvider cache index (RwLock), write JPEG to disk, emit event
- The `image` crate is already a dependency (v0.25, features: jpeg, png, webp)
- CMYK JPEGs: if `image::load_from_memory` returns `UnsupportedColorSpace`, emit nothing (placeholder fallback)

## Tasks/Subtasks

- [ ] 1. Create `src/coverart/cover_proc.rs` — `CoverProc` with `spawn()`, MD5 hash, cache compare from CoverProvider, JPEG write, index update
- [ ] 2. Implement `image` crate decode + Lanczos3 resize to 200×200 RGBA
- [ ] 3. Change MPD Cover result channel: send CoverProcJob directly to Cover Proc (remove cover_result_rx from MPD IO)
- [ ] 4. Update `src/mpd/state_machine.rs`: remove cover result draining from idle/poll loops, spawn Cover Proc
- [ ] 5. Update `src/ui/mod.rs`: replace gdk-pixbuf decode in CoverRefreshed with MemoryTexture
- [ ] 6. Remove `process_cover_result()`, `handle_albumart_data()`, `handle_readpicture_data()`, `emit_cover_refreshed()`, `strip_jpeg_icc()` from ActualRead
- [ ] 7. Add `pub mod cover_proc;` to `src/coverart/mod.rs`
- [ ] 8. Build and run full test suite — verify zero regressions, zero warnings

### Review Findings

- [x] [Review][Patch] JPEG decode fallback emitted raw JPEG as CoverRefreshed [src/coverart/cover_proc.rs:143] — FIXED: now skips CoverRefreshed on decode failure, letting CoverPaths disk-based fallback handle it
- [x] [Review][Patch] Unnecessary data.to_vec() heap copy in UI CoverRefreshed handler [src/ui/mod.rs:2804] — FIXED: data moved directly into glib::Bytes::from_owned
- [x] [Review][Defer] Multiple Cover Proc workers during reconnection window — pre-existing, index.json read-modify-write not atomic across threads
- [x] [Review][Defer] MPD Cover thread blocking send can stall permanently if Cover Proc panics — pre-existing in cover.rs design
- [x] [Review][Defer] Non-JPEG embedded cover art may enter delete-recycle loop via is_valid_jpeg — pre-existing
- [x] [Review][Defer] Orphaned {md5}.jpg files accumulate when cover art changes — no GC pass
- [x] [Review][Defer] update_index_json called on failed fs::write — pre-existing pattern (partial update tracking)
- [x] [Review][Defer] No size guard on JPEG decode allows large intermediate allocations — pre-existing
- [x] [Review][Defer] CoverProvider RwLock poison silently disables cache I/O — pre-existing pattern
- [x] [Review][Defer] try_send event drops invisible to caller — pre-existing pattern
- [x] [Review][Defer] Corrupt index.json silently resets entire cache — pre-existing, unwrap_or_default() replaces with empty map

## References
- [Source: architecture.md §345-408] Cover Art Image Pipeline ADR — image crate, MemoryTexture, Lanczos3 resize
- [Source: architecture.md §2233-2238] Cover Proc worker spec — 1 persistent thread, no MPD/GTK knowledge
- [Source: architecture.md §2253-2263] Three-connection MPD architecture
- [Source: src/coverart/actual_read.rs] Current MD5/cache/emit pipeline — moving to Cover Proc
- [Source: src/ui/mod.rs:2801-2830] Current CoverRefreshed handler — gdk-pixbuf decode path
- [Source: Cargo.toml:22] image crate v0.25 with jpeg, png, webp features

## File List
- `src/coverart/cover_proc.rs` (NEW) — Cover Proc worker: MD5, cache, decode, resize, emit
- `src/coverart/mod.rs` — Add `pub mod cover_proc;`
- `src/coverart/actual_read.rs` — Remove cover processing methods moved to Cover Proc
- `src/mpd/cover.rs` — Change result channel: Cover Proc receives results directly
- `src/mpd/state_machine.rs` — Remove cover result draining, spawn Cover Proc
- `src/ui/mod.rs` — Replace gdk-pixbuf with MemoryTexture in CoverRefreshed handler
