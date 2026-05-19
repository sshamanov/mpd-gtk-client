# Debug Session — 2026-05-02

## Committed Fixes

### Grid Scroll
- **Timer race**: Replaced `SourceId::remove()` + `catch_unwind` band-aid with `AtomicUsize` generation counter. Each scroll event bumps generation; stale timers become no-ops. No more panic, no more cancelled timers.
- **Scroll jitter**: Replaced vertical `Box` cover_area with `Overlay` — `cover_image` as `set_child` (always visible), placeholder `DrawingArea` as `add_overlay` (toggles). Cover loading no longer changes cell allocation → no 10-20px layout shifts during scroll.
- **`ContentFit::ScaleDown`** on grid Picture widgets — images scale within fixed 200x200 allocation.
- **CSS `min-height/min-width`** on `.album-cover-cell` — prevents cell sizing from content changes.

### Cover Art Display
- **Placeholder not hiding on async load**: `CoverPaths`/`CoverRefreshed` events updated the Picture but never hid the placeholder overlay on top. Added `hide_cover_placeholder()` helper called from both handlers.
- **Initial cover fetch only covered first 16 albums**: `calculate_visible_albums()` fired when `batch_populate` still populating (batches of 16 per idle). Changed initial fetch to collect ALL album entries from items list, not just visible ones. Group-switch fetch also fixed.
- **Placeholder wrong size**: Added `set_halign(Fill)` + `set_valign(Fill)` to placeholder DrawingArea so it matches the Picture it overlays.
- `process_one` returned early if `find_album_uris` failed — but `albumart` only needs album name. Now proceeds with `albumart` regardless, skips `readpicture` if no URI.

### Cover Art — Album Queue (Mini-Grid)
- **No widget registry for mini-queue**: Mini-grid cells were populated on bind only. Covers arriving later had no path to update mini-queue cells. Added `mini_cover_widgets` HashMap (symmetrical to grid's `cover_widgets`), updated from both `CoverPaths` and `CoverRefreshed` handlers.

### AlbumArtist
- Changed default grouping tag from `Artist` to `AlbumArtist` in group switcher, `list_albums()`, and `list_albums_grouped()`.
- Artist fallback when AlbumArtist absent.
- Sort dropdown labels updated.

### Library Update Button
- Added `MpdCommand::Update`, `MpdAdapter::update_library()`, and ↻ button in group bar.
- Triggers `LibraryChanged` event for immediate grid refresh after MPD rescan.

---

## Reverted — Wrong Direction

### Second MPD Connection (in same thread)
Three commits reverted (`4c21a93`, `969a4ad`, `bd48fbe`):
- Added second `MpdAdapter` in `connected_loop` for cover fetching, drained all covers on `FetchCovers`.
- Added cache fast-path in `process_one` (skip MPD fetch if cached file exists on disk).
- Added ADR documenting this approach.

**Why reverted**: Architecture specifies a separate **MPD Cover thread** with its own connection + **Cover Proc worker thread** for decode/hash/cache. The inline second-connection approach puts binary I/O and JPEG decode on the MPD thread — wrong thread model.

**What should be done instead**: Implement the architecture as specified:
1. MPD Cover thread — own MPD connection, receives `FetchCovers`, sends `albumart`/`readpicture`, ships raw bytes to Cover Proc
2. Cover Proc worker — decodes JPEG via gdk-pixbuf, MD5 hash, writes disk cache, emits `CoverRefreshed`/`CoverPaths`
3. Cache fast-path IS correct but should live in the Cover Proc thread, not inline in ActualRead

---

## Architecture Gaps — In Scope, Not Tracked

These are specified in `architecture.md` but have NO story in `sprint-status.yaml`:

### Missing Threads
| Thread | Spec Location | What It Does |
|--------|--------------|--------------|
| **MPD Cover thread** | architecture.md §1914-1918 | Separate MPD connection for binary cover I/O. Receives URIs, sends albumart/readpicture, ships raw bytes to Cover Proc. Created on demand, 30s idle timeout. |
| **Cover Proc worker** | architecture.md §1920-1924 | Decodes JPEG bytes via gdk-pixbuf, MD5 hash, compares cache, writes disk, emits CoverRefreshed. No MPD or GTK knowledge. |
| **Search worker** | architecture.md §1926-1930 | Owns search index. Receives queries, runs full-text match, scores, caps results. Emits SearchResults. Currently runs on GTK main thread via idle callbacks. |
| **NotificationRouter** | architecture.md §1932-1938 | Receives Toast events, checks notification mode setting, fires desktop notification via D-Bus if enabled. |

### Missing Protocol Feature
| Feature | Spec Location | Status |
|---------|--------------|--------|
| **MPD idle protocol** | architecture.md §241-250, epic 25 | In backlog as 25-1. Eliminates 500ms status polling, uses push-based updates. |

### Thread Model Mismatch
Architecture specifies a **compute worker** (architecture.md §805) combining cover art + search on a shared priority queue. Current implementation has neither. The thread model described in the concurrency ADR and the refined thread model in §1900-1908 are the TARGET, not implemented.

---

## Documentation Issues

### "V2" Labeling
Architecture.md, prd.md, epics.md, deferred-work.md all use "v2" to label refinements. User directive: there is no v2. Everything is "in scope" or "out of scope". All items currently labeled "v2" are in-scope and must be:
- Marked done in sprint-status if implemented
- Added to backlog if not yet implemented
- Marked "out of scope" only if explicitly deferred

### deferred-work.md is Stale
- "Cover Art (v2 pipeline)" says unimplemented — but the two-layer CoverProvider + ActualRead IS implemented. The missing part is the separate thread architecture (MPD Cover thread + Cover Proc).
- Needs update to reflect current state.

### sprint-status.yaml has Gaps
- Epics 25 (idle), 26 (metadata caching), 27 (responsive right-rail) in backlog — correct.
- MPD Cover thread, Cover Proc worker, Search worker, NotificationRouter — NOT in any epic.
- These need backlog stories.

---

## User Directives from This Session

1. **Fix the grid, don't rebuild** — architecture is correct (GtkGridView + factory pattern), fix the glue code.
2. **Consolidate now-playing handler** — single function: write SharedState → update UI → forward MPRIS. ADRs written and merged into architecture.md. Implementation deferred to next story.
3. **Cover is independent from playback metadata** — CoverPaths/CoverRefreshed handler updates np_cover directly, not through handle_now_playing.
4. **No "v2" labeling** — everything is in-scope or out-of-scope. Rolling releases, no version gates.
5. **Second MPD connection must follow architecture** — separate MPD Cover thread + Cover Proc worker, not inline in same thread.
6. **Debug session notes** — this file, capturing all findings for later processing.

---

## Items for Backlog

These need stories created in sprint-status.yaml:

1. **MPD Cover thread** — separate thread + MPD connection for binary cover I/O
2. **Cover Proc worker** — decode/hash/cache thread, offloads from MPD IO
3. **Search worker thread** — move search index operations off GTK main thread
4. **NotificationRouter thread** — desktop notification routing
5. **Implement now-playing consolidation ADRs** (from architecture.md §504-545)
6. **Remove all "v2" labeling** from authoritative documents
7. **Update deferred-work.md** — reflect current implementation state

## User Request — Full MPD Metadata Caching

"i want introduce caching for the whole mpd library. it should be FAST in capital letters, very fast"

### Current State
- `connected_loop` has `cached_flat_albums: Vec<(String, String)>` — in-memory only, lost on restart
- Group switching for Albums/Years/Genres calls MPD `list album group {tag}` every time
- No album year, genre, or format in cache — only (artist, album_name) tuples
- Cover paths in separate CoverProvider cache (disk-backed, but keyed by MD5 hash)

### Story 26-1
- File: `_bmad-output/implementation-artifacts/26-1-metadata-caching.md`, status: backlog
- Specifies in-memory cache populated at startup, O(1) lookup by album name
- AC: cache year, genre, cover path, file paths; populate before grid display; invalidate on library change
- No persistence described — in-memory only, rebuilt every startup

### What Architecture Says
- §349: "Local grouping cache: Flat album list cached on background thread. Switching between Albums/Artist/Years/Genres regroups locally — zero MPD round-trips"
- §861: "Persistent cache (write-through, LRU): Cover art disk cache, metadata cache, search index"
- §1186: "Sorting and grouping operate on cached metadata without MPD round-trips. See epic 26"
- §2208: "Group view switch: 0ms (local cache) — zero MPD round-trips"

### What "FAST" Means
1. **Startup**: Load cache from disk → show grid instantly (no MPD wait). Background verify with MPD.
2. **Group switching**: Artist/Years/Genres regrouped locally from HashMap — 0ms, no MPD call.
3. **Search**: Query local index built from cache, not MPD `search` command.
4. **Disk format**: Binary (bincode/rmp-serde). 10K albums load in ~5ms vs ~5s for JSON/serde.
5. **Invalidation**: MPD `status` returns `updating_db` job ID. If changed, rebuild cache.

### Proposed MetadataCache Module (`src/metadata/`)
```rust
struct MetadataCache {
    albums: HashMap<String, CachedAlbum>,  // keyed by album name
    db_update: Option<String>,             // MPD's updating_db job ID
}

struct CachedAlbum {
    artist: String,
    album: String,
    year: Option<u32>,
    genre: Option<String>,
    cover_path: Option<PathBuf>,
    file_paths: Vec<String>,
}

impl MetadataCache {
    fn load_from_disk() -> Result<Self>;           // bincode deserialize
    fn save_to_disk(&self) -> Result<()>;           // bincode serialize
    fn populate_from_mpd(adapter: &mut MpdAdapter) -> Result<Self>;  // full fetch
    fn albums_grouped(&self, tag: &str) -> AlbumGroup;  // local regroup
    fn invalidate_if_stale(&self, adapter: &mut MpdAdapter) -> bool;
}
```

### Startup Flow (Target)
1. `MetadataCache::load_from_disk()` → <5ms for 10K albums
2. Emit `AlbumsGrouped` from cache → grid visible immediately
3. Background: `adapter.status()` → check `updating_db`
4. If changed: `populate_from_mpd()` → `save_to_disk()` → emit updated `AlbumsGrouped`
5. If unchanged: done, cache is current

### Dependencies
- `bincode` or `rmp-serde` crate (binary serialization, no new system deps)
- No SQLite — binary file is simpler, faster, and avoids schema migrations

## Final State — Cover Pipeline (2026-05-02)

After all fixes, the cover delivery pipeline is working correctly:
- 122 unique albums in grid, 65 have embedded cover art (CoverPaths events)
- 57 albums without embedded art show generated colored placeholder textures
- Composite keys (artist||album) prevent collisions
- Now-playing and album queue covers update correctly
- 79 CoverPaths events, 79 cover updates, 0 misses

### Remaining Issues
**Albums Without Embedded Art**: 57 albums lack embedded cover art. Options:
- Folder-based cover art: check album dir for cover.jpg/folder.jpg (not implemented)
- Online cover lookup: feature-gated behind `online-cover-art` (code exists, not enabled)

**Case-Sensitive Duplicates**: "Never For Ever" vs "Never for Ever" are distinct MPD albums (different masterings). Case-sensitive composite key is correct. Future: album-mode deduplication by normalized (artist, album) pair.

**Row Spacing**: Resolved. Caused by Pixbuf::from_mut_slice use-after-free (UB) producing garbage cell dimensions. Fixed with Pixbuf::new() + fill(). Cover images now pre-scaled to 200x200 everywhere.
