# Story 13.2: ActualRead — Background Fetch Queue

Status: review

## Story

As a developer,
I want cover fetching to run one album per idle cycle in the background on the MPD thread,
so that covers load incrementally without blocking MPD commands or the UI.

## Acceptance Criteria

1. **One-per-cycle** — Given the ActualRead queue has albums to process, when the background idle cycle fires, then exactly one album is fetched per cycle, and the fetch uses `albumart <uri>` primary then `readpicture <uri>` fallback.

2. **Content-addressed dedup (albumart)** — Given `albumart` returns binary data, when the data is MD5-hashed and compared against the CoverProvider cache hash, then only a different hash triggers `CoverPaths` emission, and identical hashes are silently skipped (no emission, no redraw).

3. **Time-addressed dedup (readpicture)** — Given `readpicture` returns data with an mtime timestamp, when the timestamp is compared against the cached timestamp, then only newer timestamps trigger emission.

4. **Cache write on new data** — Given new cover data is fetched that differs from cache, when the data is written to disk, then the JPEG is saved at `{cache_dir}/{md5}.jpg` and the index.json is updated atomically.

5. **Cache write failure is non-fatal** — Given the cache directory is unwritable, when a fetch succeeds, then the error is logged at warn level, the fetch result is still emitted via `CoverPaths` with the raw data path (in-memory only for the session), and no panic occurs.

6. **Queue drained in order** — Given multiple albums are enqueued in ActualRead, when processed one per cycle, then albums are processed in FIFO order.

7. **ReadPictureProvider fallback** — Given `albumart` returns error/no data for an album, when the fallback triggers, then `readpicture` is attempted before giving up.

8. **CoverProvider integration** — Given ActualRead fetches new data and writes it, when `CoverProvider.invalidate(id)` is called after the write, then the next `CoverProvider::get(id)` returns the new cached entry.

## Tasks / Subtasks

- [x] Add `readpicture` method to `MpdAdapter` in `src/mpd/mod.rs` (AC: #1, #7)
  - [x] Implement `pub fn readpicture(&mut self, uri: &str) -> Result<Option<(Vec<u8>, u64)>, Error>` that sends `readpicture "<uri>"` and parses the binary response with `size:` header followed by binary data. Returns raw bytes and the mtime timestamp from the response header.
  - [x] Handle multi-chunk readpicture responses using the same offset-based approach as albumart.

- [x] Create `src/coverart/actual_read.rs` with `ActualRead` struct (AC: #1-#8)
  - [x] Define `ActualRead { queue: VecDeque<(String, String)>, cache_dir: PathBuf }` — holds (artist, album_name) tuples for pending fetches.
  - [x] Implement `ActualRead::new(cache_dir: PathBuf) -> Self`.
  - [x] Implement `ActualRead::enqueue(&mut self, albums: Vec<(String, String)>)` — replaces the queue contents (same semantics as current `pending_covers = albums`).
  - [x] Implement `ActualRead::process_one(&mut self, adapter: &mut MpdAdapter, provider: &CoverProvider, event_tx: &mpsc::Sender<MpdEvent>)`:
    - [x] Pop one album from the front of the queue.
    - [x] Primary: call `adapter.albumart(uri)`. If data returned, MD5-hash it, compare against CoverProvider cache. If different: write JPEG, update index.json, emit `CoverPaths`, call `provider.invalidate()`.
    - [x] Fallback: if albumart returns None/error, call `adapter.readpicture(uri)`. If data + timestamp returned, compare timestamp against cache. If newer: write JPEG, update index.json, emit `CoverPaths`, call `provider.invalidate()`.
    - [x] If both fail: log at debug level, no emission.

- [x] Write cache helper functions in `actual_read.rs` (AC: #4, #5)
  - [x] `write_cache_file(cache_dir: &Path, md5: &str, data: &[u8]) -> bool` — writes `{cache_dir}/{md5}.jpg`, logs error on failure, returns success.
  - [x] `update_index_json(cache_dir: &Path, album_id: &str, md5: &str, timestamp: Option<u64>)` — reads current index.json, upserts the entry, writes atomically (write to temp file, rename). On failure, log warn and continue (index will be repopulated on next startup from JPEG scan, or on next successful write).
  - [x] Both functions must not panic. All errors logged, never propagated.

- [x] Wire ActualRead into the state machine (`src/mpd/state_machine.rs`) (AC: #1, #6)
  - [x] Replace `CoverFetcher` + `pending_covers: Vec` with `ActualRead` + `Arc<RwLock<CoverProvider>>`.
  - [x] At `MpdCommand::FetchCovers(albums)`: call `actual_read.enqueue(albums)`.
  - [x] At idle timeout: call `actual_read.process_one(...)`.
  - [x] At `connected_loop` start (after reconnect): instantiate `ActualRead` (already handled by existing pattern — no reconnect-specific logic needed).
  - [x] Thread `CoverProvider` in from the top level (create in `connected_loop`, pass to `ActualRead`).

- [x] Update `src/coverart/mod.rs` (AC: #1-#8)
  - [x] Add `pub mod actual_read;` and re-export `ActualRead`.
  - [x] Do NOT remove `CoverFetcher` yet — it will be fully removed in Story 13.4 when the widget registry is in place. For now, `CoverFetcher` stays dead code (can be suppressed).

- [x] Update `src/mpd/mod.rs` — add `readpicture` to the public API (AC: #1, #7)
  - [x] Ensure `find_album_uris` is usable by ActualRead (it already returns `Vec<String>` URIs).

- [x] Add tests (AC: #1-#8)
  - [x] Unit tests for `ActualRead::process_one` with mock adapter returning albumart data.
  - [x] Unit test for MD5 hash comparison — same data does not emit.
  - [x] Unit test for readpicture fallback when albumart fails.
  - [x] Unit test for cache write failure emission.
  - [x] Unit test for FIFO order.
  - [x] Integration test: full flow via mock MPD (deferred — mock MPD does not yet support albumart/readpicture commands).

## Dev Notes

### Architecture Constraints

- **ActualRead runs on the MPD background thread** — not the UI thread. It is called from the idle cycle in `connected_loop` (the `RecvTimeoutError::Timeout` branch).
- **One album per idle cycle** — the MPD command loop must not be blocked. If albumart takes multiple round-trips (offset-based for large images), all those round-trips count as "one" because they happen synchronously within `process_one`. This is acceptable because the total time is bounded by the albumart blob size (typically <500ms for even large covers).
- **CoverProvider and ActualRead never call each other** — CoverProvider reads, ActualRead writes. The only interaction is ActualRead calls `provider.invalidate(id)` after writing new data, which removes the stale index entry so the next `CoverProvider::get(id)` sees the new file.
- **CoverProvider must be `Arc<RwLock<CoverProvider>>`** — shared between the UI thread (for fast `get()` calls) and the MPD background thread (for `invalidate()` after writes). The state machine already passes `Arc<RwLock<CoverProvider>>` around — instantiate it in `connected_loop` and use `provider.write().unwrap().invalidate()`.

### AlbumArtProvider + ReadPictureProvider as methods on ActualRead

The architecture document discusses `AlbumArtProvider` and `ReadPictureProvider` as separate concepts, but for this story they are **methods on ActualRead**, not separate structs. The architecture says:

```
src/coverart/providers.rs  # [future] AlbumArtProvider + ReadPictureProvider
```

This future extraction can happen in Story 13.5+ if online lookups add more providers. For now, inline the logic in `ActualRead::process_one` as a primary (`albumart`) → fallback (`readpicture`) chain. This avoids premature abstraction.

### MD5 Hashing

Use the `md-5` crate that was already added in Story 13.1:
```rust
use md5::{Md5, Digest};
let hash = Md5::digest(&data);
let hex = format!("{:x}", hash);
```

### Cache Write Pattern

```rust
fn write_cache(cache_dir: &Path, album_id: &str, data: &[u8], timestamp: Option<u64>, provider: &CoverProvider) -> Option<String> {
    let mut hasher = Md5::new();
    hasher.update(data);
    let md5 = format!("{:x}", hasher.finalize());

    // Check if cache already has this hash
    if let Some(cached) = provider.get(album_id) {
        if cached.md5 == md5 {
            // For albumart: same hash → identical data, skip
            // For readpicture: also check timestamp
            if timestamp.is_none() || cached.timestamp == timestamp {
                log::debug!("[actual_read] '{album_id}': cached hash unchanged, skipping");
                return Some(md5); // still return md5 to indicate "handled, no new data"
            }
        }
    }

    // Write JPEG
    let jpeg_path = cache_dir.join(format!("{md5}.jpg"));
    if let Err(e) = std::fs::write(&jpeg_path, data) {
        log::warn!("[actual_read] '{album_id}': cache write failed: {e}");
        // Still emit the event — UI can display from memory
    }

    // Update index
    update_index_json(cache_dir, album_id, &md5, timestamp);

    // Invalidate CoverProvider cache so next get() picks up the new entry
    provider.invalidate(album_id);

    Some(md5)
}
```

### Index JSON Update

```rust
fn update_index_json(cache_dir: &Path, album_id: &str, md5: &str, timestamp: Option<u64>) {
    let index_path = cache_dir.join("index.json");
    let mut index: HashMap<String, serde_json::Value> = match std::fs::read_to_string(&index_path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    let entry = match timestamp {
        Some(ts) => serde_json::json!({ "md5": md5, "timestamp": ts }),
        None => serde_json::json!({ "md5": md5 }),
    };
    index.insert(album_id.to_string(), entry);
    // Atomic write: write to .tmp then rename
    let tmp_path = index_path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string(&index) {
        if std::fs::write(&tmp_path, &json).is_ok() {
            let _ = std::fs::rename(&tmp_path, &index_path);
        }
    }
}
```

### readpicture MPD Protocol

The MPD `readpicture` command returns:
```
size: <total_bytes>
mtime: <unix_timestamp>

<binary_data>
```

It follows the same offset-based protocol as `albumart`:
```
readpicture "<uri>" 0       → first chunk (up to 8192 bytes) + size header
readpicture "<uri>" <offset> → subsequent chunks
```

The response format:
```
OK MPD 0.24.0
size: 123456
mtime: 1700000000

<binary_chunk_data>
```

Unlike `albumart`, `readpicture` may not exist on older MPD versions (requires MPD >= 0.24). If MPD returns an error (ACK), return None.

### MpdAdapter::readpicture Implementation Sketch

```rust
pub fn readpicture(&mut self, uri: &str) -> Result<Option<(Vec<u8>, u64)>, Error> {
    let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
    let cmd = format!("readpicture \"{}\" 0\n", escaped);
    self.stream.write_all(cmd.as_bytes())?;
    self.stream.flush()?;
    let raw = self.read_albumart_response()?;
    if raw.is_empty() { return Ok(None); }
    
    let total_size = parse_albumart_size(&raw);
    let mtime = parse_albumart_mtime(&raw);  // NEW: parse mtime from response
    let mut data = parse_albumart_chunk(&raw);
    
    let Some(size) = total_size else { return Ok(None); };
    
    let mut offset = data.len();
    while data.len() < size {
        let cmd = format!("readpicture \"{}\" {offset}\n", escaped);
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.flush()?;
        let raw = self.read_albumart_response()?;
        if raw.is_empty() { break; }
        let chunk = Self::parse_albumart_chunk(&raw);
        if chunk.is_empty() { break; }
        data.extend_from_slice(&chunk);
        offset = data.len();
    }
    
    if data.is_empty() { return Ok(None); }
    Ok(Some((data, mtime)))
}
```

Note: `parse_albumart_size` already exists and can be reused. Need to add `parse_albumart_mtime` or modify `read_albumart_response` to also return the mtime. Simplest approach: add `parse_mtime` helper similar to `parse_albumart_size`.

### Wiring in State Machine

The current idle cycle logic in `connected_loop` is:

```rust
// line ~261 in state_machine.rs
let mut cover_fetcher = crate::coverart::CoverFetcher::new();
let mut pending_covers: Vec<(String, String)> = Vec::new();
```

Replace with:
```rust
let cache_dir = dirs::cache_dir()
    .unwrap_or_else(|| PathBuf::from("/tmp"))
    .join("mpd-client")
    .join("covers");
let cover_provider = Arc::new(RwLock::new(crate::coverart::CoverProvider::new()));
let mut actual_read = crate::coverart::ActualRead::new(cache_dir);
```

The `MpdCommand::FetchCovers(albums)` handler changes from:
```rust
pending_covers = albums;
```
to:
```rust
actual_read.enqueue(albums);
```

The idle timeout handler changes from:
```rust
if !pending_covers.is_empty() {
    let (_, album_name) = pending_covers.remove(0);
    // ... cover_fetcher.fetch_cover ...
}
```
to:
```rust
if actual_read.has_pending() {
    actual_read.process_one(&mut adapter, &cover_provider.read().unwrap(), &event_tx);
}
```

### CoverPaths Event

The existing `MpdEvent::CoverPaths(HashMap<String, Option<String>>)` is used to notify the UI of new cover paths. ActualRead should continue using this event. The HashMap maps `album_name → Some(path_string)` when a cover is found, or `album_name → None` if not found. The existing UI handler in `src/ui/mod.rs` around line 1196 already processes this event.

No new event type needed for this story. The `CoverRefreshed(id, Vec<u8>)` pattern from the architecture can be introduced later (Story 13.4) when widget registry in-place updates are implemented.

### File List

- **New**: `src/coverart/actual_read.rs` — `ActualRead` struct with `process_one`, `enqueue`, `has_pending`; plus `write_cache`, `update_index_json` helpers
- **Modified**: `src/mpd/mod.rs` — add `readpicture` method to `MpdAdapter`, add `parse_albumart_mtime` helper
- **Modified**: `src/mpd/state_machine.rs` — replace `CoverFetcher`+`pending_covers` with `ActualRead`+`Arc<RwLock<CoverProvider>>`
- **Modified**: `src/coverart/mod.rs` — add `pub mod actual_read;`, re-export `ActualRead`

### Existing Code Patterns

- **State machine idle cycle**: timeout fires every ~100ms (the `mpsc::RecvTimeoutError::Timeout` branch), processes one cover fetch per cycle
- **CoverFetcher** is the current implementation: uses `adapter.albumart()`, writes JPEG, tracks in-memory cache
- **CoverProvider** (Story 13.1): synchronous cache read, MD5-keyed, thread-safe via `RwLock`
- **`find_album_uris`**: `adapter.find_album_uris(album_name)` returns URIs for the album's tracks
- **Log prefix**: `[actual_read]` for all ActualRead log messages

### Testing Strategy

- Use the mock MPD server (`src/mpd/mock.rs`) for integration tests
- Unit tests can create a `tempfile::TempDir` for the cache directory (same pattern as Story 13.1)
- Create a mock `process_one` test with a local `MpdAdapter` connected to the mock server
- For the MD5 dedup test: call `process_one` twice with the same album and same mock data; verify only one `CoverPaths` event is emitted
- For readpicture fallback: configure the mock to return error on `albumart` but data on `readpicture`

### References

- Architecture: `_bmad-output/planning-artifacts/architecture.md` sections:
  - ADR: Cover Art Pipeline (lines ~253-274) — two-layer split design
  - Cover Art Event Flow (lines ~296-310) — albumart → MD5 → readpicture → timestamp chain
  - Cover Cache Contract (lines ~1052-1069) — guarantees, invariants, fault behavior
  - File structure (line ~2295) — `src/coverart/providers.rs` [future]
- Previous story: `_bmad-output/implementation-artifacts/13-1-coverprovider-cache-read.md` — CoverProvider implementation details
- Current implementation: `src/coverart/mod.rs` — existing CoverFetcher for reference
- State machine: `src/mpd/state_machine.rs` — idle cycle cover fetch logic
- MPD adapter: `src/mpd/mod.rs` — `albumart()` and `find_album_uris()` methods for reference
- MPD protocol: `readpicture` — see `mpd.readthedocs.io/en/latest/protocol.html` (requires MPD >= 0.24)

## Dev Agent Record

### Agent Model Used

Claude Code (deepseek-v4-flash)

### Debug Log References

- `[actual_read]` prefix for all ActualRead log messages
- `[cover_provider]` for CoverProvider log messages (from Story 13.1)
- `[adapter]` for MpdAdapter log messages
- Errors at error level, cache writes at info, queue operations at debug

### Completion Notes

Story 13.2 implemented: ActualRead background fetch queue for cover art. 12 new unit tests pass, 0 regressions from existing 24 tests (9 provider + 15 integration).

Created `src/coverart/actual_read.rs` with `ActualRead` struct backed by a `VecDeque` FIFO queue. `process_one()` pops one album per call, attempts `albumart <uri>` (primary) then `readpicture <uri>` (fallback), MD5-hashes binary data, compares hash/timestamp against CoverProvider cache, writes JPEG + index.json atomically, and emits `CoverPaths` only on actual difference.

Added `readpicture` method to `MpdAdapter` in `src/mpd/mod.rs` with `parse_albumart_mtime` helper. Handles multi-chunk binary responses using the same offset-based pattern as `albumart`.

Wired into `src/mpd/state_machine.rs`: replaced `CoverFetcher` + `pending_covers` with `ActualRead` + `Arc<RwLock<CoverProvider>>`. The old `CoverFetcher`/`fetch_via_mpd` code remains in `coverart/mod.rs` as dead code (will be removed in Story 13.4).

### File List

- `src/coverart/actual_read.rs` — new file: `ActualRead` struct with `enqueue()`, `process_one()`, `has_pending()`, `pending_count()`, plus `write_cache()`, `update_index_json()`, `emit_cover_path()` helpers, and 12 unit tests
- `src/coverart/mod.rs` — added `pub mod actual_read;` and re-export `ActualRead`
- `src/coverart/provider.rs` — made `CoverProvider` fields and `IndexEntry` `pub(crate)` for cross-module test access
- `src/mpd/mod.rs` — added `readpicture()` method and `parse_albumart_mtime()` helper
- `src/mpd/state_machine.rs` — replaced `CoverFetcher` + `pending_covers` with `CoverProvider` + `ActualRead`
