# Story 26.1: Full Metadata Caching

Status: backlog

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

## References
- [Source: architecture.md §1131] Browsing Sort Architecture (replaced by this)
- [Source: src/mpd/mod.rs] `list`/`listall` responses
- [Source: src/search/mod.rs] Existing search index pattern

## File List
- `src/metadata/` (new module) — Metadata cache, population, lookup
- `src/mpd/mod.rs` — Add batch metadata fetch
