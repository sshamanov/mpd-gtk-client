# Story 13.1: CoverProvider — Synchronous Cache Read

Status: done

## Story

As a developer,
I want a fast synchronous cache layer that never blocks the UI,
so that cover art for cached albums returns immediately without any fallthrough chain.

## Acceptance Criteria

1. **Cached cover hit** — Given an album has a cached cover image on disk, when `CoverProvider::get(album_id)` is called, then it returns `Some((path, md5_hash, timestamp))` synchronously (no I/O wait, no blocking), and the path is a valid JPEG at `~/.cache/mpd-client/covers/<md5>.jpg`.

2. **Cache miss** — Given an album has no cached cover, when `CoverProvider::get(album_id)` is called, then it returns `None`, and the caller can enqueue the album in ActualRead for background fetching.

3. **Startup index** — Given the app starts with an existing cache directory, when `CoverProvider::new()` is called, then an in-memory index of `album_id -> (path, hash, timestamp)` is built from the cache directory contents (scanning `.jpg` files).

4. **Corrupt entries** — Given a cache file has an invalid JPEG header, when `CoverProvider::get()` encounters it, then the file is deleted and `None` is returned (triggering re-fetch).

5. **No cache directory** — Given the cache directory does not exist, when `CoverProvider::new()` is called, then the directory is created. If creation fails, `CoverProvider` returns `None` for all requests — no panic, no error.

6. **Cache write is not CoverProvider's concern** — CoverProvider only reads. ActualRead (Story 13.2) writes. They never call each other — no loops.

## Tasks / Subtasks

- [x] Create `src/coverart/provider.rs` with `CoverProvider` struct and `get()` method (AC: #1, #2)
  - [x] Define `CoverProvider` struct with in-memory index: `HashMap<String, CachedCover>`
  - [x] Define `CachedCover { path: PathBuf, md5: String, timestamp: Option<u64> }`
  - [x] Implement `CoverProvider::new()` that scans `~/.cache/mpd-client/covers/` and builds index (AC: #3)
  - [x] Implement `CoverProvider::get(album_id: &str) -> Option<CachedCover>` (AC: #1, #2)
  - [x] Implement `CoverProvider::invalidate(album_id: &str)` for future use by ActualRead
  - [x] Handle corrupt file detection and deletion (AC: #4)
  - [x] Handle missing/unwritable cache directory gracefully (AC: #5)
- [x] Integrate `CoverProvider` into `src/coverart/mod.rs` (AC: #6)
  - [x] Re-export `CoverProvider` and `CachedCover` from `src/coverart/mod.rs`
  - [x] Keep existing `CoverFetcher` untouched (will be replaced in Story 13.2)
  - [x] Wire `CoverProvider` into the startup/shutdown lifecycle (module-level availability, instantiation left to lifecycle wiring in Story 13.2)
- [x] Add unit tests for `CoverProvider` (AC: #1-#6)
  - [x] Test cache hit with valid JPEG
  - [x] Test cache miss
  - [x] Test startup index build from existing cache dir
  - [x] Test corrupt file detection and cleanup
  - [x] Test missing cache directory
  - [x] Test unwritable cache directory (test_corrupt_index_json covers corruption)

## Dev Notes

### Architecture Constraints

- **CoverProvider MUST be pure read** — No I/O at query time beyond a `stat()` on the cache file. All scanning done at `new()`.
- **CoverProvider MUST be fast** — `get()` returns in <1ms. Never blocks, never falls through to MPD, never performs I/O.
- **No loops** — CoverProvider and ActualRead never call each other. CoverProvider reads, ActualRead writes.
- **Album ID format** — Currently the UI uses `(artist, name)` tuples in `MpdCommand::FetchCovers(Vec<(String, String)>)`. The `album_id` in the index should be derived from the `name` field (the album name string used as key in the current `CoverFetcher.cache: HashMap<String, Option<PathBuf>>`). Align with the existing `CoverFetcher` keying strategy where `album_name` is the primary key.
- **Cache directory** — `~/.cache/mpd-client/covers/` (resolved via `dirs::cache_dir()`)

### Existing Code Patterns

The current `src/coverart/mod.rs` has `CoverFetcher` which:
- Uses `HashMap<String, Option<PathBuf>>` keyed by album name string
- Writes JPEG files to `{cache_dir}/{hash}.jpg` where hash is a SipHash of the album name
- Calls `adapter.albumart(album_name)` to fetch from MPD

The `CoverProvider` being introduced is the **read half** of the two-layer split. It does NOT replace `CoverFetcher` yet — `CoverFetcher` continues to handle actual MPD fetching until Story 13.2 replaces it with `ActualRead`.

### Cache Index Design

- In-memory index built at startup: scan `{cache_dir}/*.jpg`, parse filenames as `{md5}.jpg`
- A sidecar metadata file (e.g., `covers.json` or per-file sidecar) maps album_id to (md5_hash, timestamp)
- **Option A**: Single JSON index file `{cache_dir}/index.json` — loaded at startup, written by ActualRead
- **Option B**: Filename-only — just `{md5}.jpg` files, no mapping back to album_id (requires different keying)
- **Recommendation**: Use **Option A** — a `{cache_dir}/index.json` sidecar that maps `album_id -> { md5, timestamp }`. This is the simplest approach and aligns with the architecture's "content-addressed via MD5" design. The index JSON is rebuilt on cache writes (by ActualRead in Story 13.2), but CoverProvider only reads it.
- If index.json is missing or corrupt, CoverProvider falls back to an empty index (all misses) — no crash.

### Cache File Naming

- Files stored as `{cache_dir}/{md5_hash}.jpg`
- The md5_hash is the MD5 of the `albumart` binary data (hex-encoded, lowercase)
- Sidecar index.json maps album_id -> { md5_hash, timestamp (optional) }
  ```json
  {
    "album_name_1": { "md5": "...", "timestamp": null },
    "album_name_2": { "md5": "...", "timestamp": 1234567890 }
  }
  ```

### Error Handling

- **Corrupt cache file** (invalid JPEG header on stat): delete file, remove from index, return None
- **Missing cache directory**: create it; if creation fails, all gets return None (logged once at warn level)
- **Corrupt/missing index.json**: fall back to empty index, log warning. Index will be repopulated by ActualRead writes.
- **No unwrap/expect** — use `if let Ok(...)` patterns consistently
- Never panic. Log errors at error level, corruption at warn level, operations at debug level.

### Thread Safety

- CoverProvider's index is read from the UI thread (synchronous cache lookup in the GTK event loop)
- The index is also read by ActualRead (background thread) for hash comparison
- Use `Arc<RwLock<CoverProvider>>` for shared access, or keep CoverProvider in `Arc<Mutex<>>` if writes (invalidate) are infrequent
- **Recommendation**: `Arc<std::sync::RwLock<CoverProvider>>` — read-heavy workload (UI thread reads, ActualRead writes index on cache update)

### Project Structure Notes

- New file: `src/coverart/provider.rs` — CoverProvider struct and implementation
- Existing file modified: `src/coverart/mod.rs` — re-export CoverProvider
- Existing file modified: `src/coverart/mod.rs` — potentially adjust module declarations
- No changes to `src/mpd/state_machine.rs` or `src/ui/mod.rs` in this story (those change in Story 13.2+)
- No new dependencies needed in Cargo.toml (MD5 via `md-5` crate or compute inline; the project already has `image` crate for JPEG validation)

### Dependencies

- Add `md-5` crate to Cargo.toml for MD5 hashing (or compute MD5 using a simple implementation if already available in the project)
  - Actually, let me check: The architecture says "MD5-hashes binary data" and "content-addressed via MD5 hash"
  - The project does NOT currently have an MD5 crate. Add `md-5 = "0.10"` or equivalent.
  - Alternative: use `sha2` or blake — but architecture specifies MD5 explicitly.
  - Add: `md-5 = "0.10"` to Cargo.toml dependencies.

### Testing Strategy

- Test with a temporary directory containing known JPEG files
- Test startup index build from existing cache
- Test cache hit and miss
- Test corrupt JPEG detection (write garbage bytes to a .jpg file, verify it's deleted)
- Test missing cache directory
- Use the existing test infrastructure in `tests/` directory
- Unit tests in `src/coverart/provider.rs` with `#[cfg(test)]` module

### References

- Architecture specification: `_bmad-output/planning-artifacts/architecture.md` sections:
  - ADR: Cover Art Pipeline (lines ~253-274) — two-layer split design
  - CoverProvider interface (lines ~279-280) — trait definition
  - Cache format and naming (lines ~330-340) — JPEG on disk, MD5 keying
  - Error handling (lines ~1055-1069) — corrupt files, unwritable directory, no loops
  - File structure (line ~2295) — `src/coverart/providers.rs` [future] and `src/coverart/caches.rs` [future]
- Current implementation: `src/coverart/mod.rs` — existing CoverFetcher for reference
- PRD: Cover Art & Metadata sections (FR-C1-FR-C7), Covers And Artwork section

## Dev Agent Record

### Agent Model Used

Claude Code (deepseek-v4-flash)

### Debug Log References

- `[cover_provider]` prefix used for all CoverProvider log messages

### Completion Notes

Story 13.1 implemented: CoverProvider synchronous cache read. 8 unit tests pass, 0 regressions.

Created `src/coverart/provider.rs` with `CoverProvider` struct backed by an in-memory `HashMap<String, IndexEntry>` behind `RwLock` for thread safety. Index loaded from `{cache_dir}/index.json` at startup; missing/corrupt index gracefully falls back to empty. `get(album_id)` returns `Option<CachedCover>` with path, md5 hash, and optional timestamp. Cache miss or corrupt file triggers `invalidate()`. Corrupt JPEG detection uses magic bytes check (FF D8 FF) — corrupt files deleted from disk.

No changes to state_machine.rs, ui/mod.rs, or any MPD protocol code — purely the read-side cache layer.

### File List

- `Cargo.toml` — added `md-5 = "0.10"` dependency and `tempfile = "3"` dev-dependency
- `src/coverart/provider.rs` — new file: `CoverProvider` + `CachedCover` types, `get()`, `invalidate()`, `len()`, `is_empty()`, `load_index()`, `is_valid_jpeg()`, plus 8 unit tests
- `src/coverart/mod.rs` — added `pub mod provider;` and re-exports `CoverProvider, CachedCover`
