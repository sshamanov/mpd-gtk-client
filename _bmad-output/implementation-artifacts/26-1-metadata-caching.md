# Story 26.1: Full Metadata Caching

Status: in-progress

## Story

As a user,
I want the album grid to display complete metadata (year, genre, cover paths, format, artist)
without MPD round-trips,
so that grouped views, sort modes, and search work from a local cache.

## Acceptance Criteria

1. **Full album metadata cache**
   - **Given** albums are loaded from MPD
   - **When** `list` or `listall` returns album data
   - **Then** ALL metadata fields are cached: album name, artist, year, genre, cover path, file paths
   - **And** the cache is keyed by album name for O(1) lookup

2. **Cache population on startup**
   - **Given** the application connects to MPD
   - **When** the initial album list is fetched
   - **Then** a second pass fetches per-album metadata in batches
   - **And** the cache is populated before the grid is displayed

3. **Cache invalidation on library change**
   - **Given** MPD notifies of a library change
   - **When** the change event is processed
   - **Then** affected albums are re-fetched
   - **And** the cache is updated incrementally

4. **Backward compatibility**
   - **Given** existing code accesses album metadata
   - **When** `(artist, album_name)` tuples are returned
   - **Then** they continue to work unchanged
   - **And** new metadata-rich accessors are added alongside

## Tasks/Subtasks

- [x] 1. Create `src/metadata/mod.rs` with `MetadataCache` struct (RwLock<HashMap<String, AlbumMeta>>)
- [x] 2. Register `pub mod metadata;` in `src/main.rs` and `src/lib.rs`
- [x] 3. Add batch file-path fetch (`fetch_album_file_paths`) to `MpdAdapter` in `src/mpd/mod.rs`
- [x] 4. Integrate cache into `connected_loop` in `src/mpd/state_machine.rs` — build after `list_albums_full()`, clear on reconnect, update on LibraryChanged
- [x] 5. Wire cache into UI event handlers in `src/ui/mod.rs` — enrich search results with cached year metadata
- [x] 6. Add unit tests for MetadataCache operations
- [x] 7. Run full test suite, verify no regressions

## References
- [Source: architecture.md §1131] Browsing Sort Architecture (replaced by this)
- [Source: src/mpd/mod.rs] `list`/`listall` responses
- [Source: src/search/mod.rs] Existing search index pattern

## File List
- `src/metadata/mod.rs` (new) — MetadataCache struct with build/lookup/clear/all_albums/file_paths
- `src/lib.rs` — Added `pub mod metadata;`
- `src/main.rs` — Added `pub mod metadata;`, pass cache to App
- `src/mpd/mod.rs` — Added `fetch_album_file_paths()` using `listallinfo`
- `src/mpd/state_machine.rs` — Cache integration: build after list_albums_full, batch file-path fetch, clear on reconnect, param plumbing
- `src/ui/mod.rs` — Cache integration: App struct field, wire into search results for year enrichment

## Dev Agent Record

### Implementation Plan

Created a thread-safe `MetadataCache` (Arc<RwLock<HashMap>>) following the `cover_key` composite key pattern (`"{artist}||{album}"`). Stores `AlbumMeta` (album, album_artist, track_artists, year, genre) plus optional file paths. Built in the MPD thread during `ListAlbumsGrouped` processing; accessible from both MPD and UI threads.

Key design decisions:
- Composite key (`"{artist}||{album}"`) for uniqueness (same pattern as cover art cache)
- Secondary index (`by_album`) for album-name-only lookups
- File paths populated via single `listallinfo` MPD command (one round-trip for all albums)
- Cache cleared on reconnect, rebuilt on library change (via full re-fetch)
- `MpdEventLoop::spawn()` returns `Arc<MetadataCache>` alongside `CommandSender`

### Completion Notes

- 6 unit tests for MetadataCache: build, lookup, get_by_album, sorted all_albums, clear, file_paths
- All 102 existing tests pass (no regressions)
- Search results now enriched with year badges from cache
- File path batch fetch uses `listallinfo` for single-round-trip efficiency
- Cache is cleared on reconnect and rebuilt on library changes
