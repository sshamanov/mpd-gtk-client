# Story 13.5: Online Cover Lookup (Opt-In)

Status: review

## Story

As a user,
I want the app to optionally fetch covers from online sources when local/MPD art is unavailable,
So that even albums without embedded or folder art can have covers.

## Acceptance Criteria

1. **Feature flag gating** — Given the `online-cover-art` feature is disabled (default), when an album has no locally available cover, then no HTTP request is made, and the album continues showing the hash-derived color placeholder.

2. **Online lookup chain** — Given `AlbumArtProvider` and `ReadPictureProvider` both returned no cover, when the `online-cover-art` feature is enabled, then the album is enqueued for online lookup querying MusicBrainZ (lookup by artist+album) -> Cover Art Archive (download image).

3. **Cache integration** — Given an online cover is successfully downloaded, when the image bytes are received, then they are written to the disk cache using the same `write_cache()` / `emit_cover_path()` / `emit_cover_refreshed()` flow as MPD-fetched covers, and the `CoverProvider` index is updated.

4. **Rate limiting** — Given online lookups are in progress, when requests are made, then they are rate-limited to 1 request per second (configurable via the CoverOnlineProvider's internal rate limiter).

5. **Exponential backoff** — Given an online lookup fails (HTTP error or network error), when retrying, then the retry follows exponential backoff (1s, 2s, 4s, 8s, 16s max) before the album is abandoned.

6. **Privacy by default** — Given the `online-cover-art` feature is disabled, when `cargo build` runs, then no networking code for cover lookup is compiled (`#[cfg(not(feature = "online-cover-art"))]` ensures zero HTTP dependencies beyond what ureq/rustls already provide as optional deps).

## Tasks / Subtasks

- [x] 1. Make `ureq` and `rustls` optional dependencies behind `online-cover-art` feature (AC: #1, #6)
  - [x] 1.1 Add `[features]` section to `Cargo.toml`: `online-cover-art = ["ureq", "rustls"]`
  - [x] 1.2 Change `ureq` dependency to `optional = true`
  - [x] 1.3 Change `rustls` dependency to `optional = true`
  - [x] 1.4 Verify `cargo build` and `cargo build --features online-cover-art` both succeed

- [x] 2. Create `src/coverart/online.rs` — Online cover lookup provider (AC: #2)
  - [x] 2.1 New module with `CoverOnlineProvider` struct, gated by `#[cfg(feature = "online-cover-art")]`
  - [x] 2.2 Constructor accepts `rate_limit_secs: f64` (default 1.0)
  - [x] 2.3 `lookup(artist: &str, album: &str) -> Option<Vec<u8>>` method
  - [x] 2.4 Rate limiter: track `last_request: Instant`, sleep/delay if < 1s since last request
  - [x] 2.5 Query MusicBrainz API: `GET https://musicbrainz.org/ws/2/release/?query=artist:{artist} AND release:{album}&fmt=json` with `User-Agent: mpd-client/0.1.0` header (required by MusicBrainz terms)
  - [x] 2.6 Parse JSON response, extract first release's MBID
  - [x] 2.7 Download cover: `GET https://coverartarchive.org/release/{mbid}/front-250` (250px thumbnail)
  - [x] 2.8 Return raw image bytes on success, `None` on any failure
  - [x] 2.9 Log at debug level for each API call, error level on HTTP failure
  - [x] 2.10 Exponential backoff retry logic for transient failures (HTTP 429, 5xx, connection errors)

- [x] 3. Wire online lookup into ActualRead's `process_one` (AC: #2, #3)
  - [x] 3.1 In `src/coverart/actual_read.rs`, add `CoverOnlineProvider` field to `ActualRead` struct behind `#[cfg(feature = "online-cover-art")]`
  - [x] 3.2 After `readpicture` fallback in `process_one()`, when both primary and fallback returned nothing, call the online provider
  - [x] 3.3 On success: write cache via `write_cache()`, emit events via `emit_cover_path()` + `emit_cover_refreshed()`, update provider index
  - [x] 3.4 On failure: log at debug level, continue (no emission, placeholder remains)

- [x] 4. Update `src/coverart/mod.rs` exports (AC: #2)
  - [x] 4.1 Add `#[cfg(feature = "online-cover-art")] pub mod online;`
  - [x] 4.2 Add `#[cfg(feature = "online-cover-art")] pub use online::CoverOnlineProvider;`

- [x] 5. Build and test (AC: #1-#6)
  - [x] 5.1 `cargo build` — clean build with 0 warnings (feature disabled)
  - [x] 5.2 `cargo build --features online-cover-art` — clean build with 0 warnings (feature enabled)
  - [x] 5.3 `cargo test` — all 57 existing tests pass (no regressions)
  - [x] 5.4 `cargo test --features online-cover-art` — all 75 tests pass (no regressions)
  - [x] 5.5 Manual verification: run with `RUST_LOG=debug cargo run --features online-cover-art`, verify online lookup logs appear for albums without local covers

## Dev Notes

### Architecture Context

The two-layer cover pipeline (CoverProvider + ActualRead) is fully implemented in Stories 13.1-13.4. This story adds the third and final fetch source: online HTTP lookup when both MPD `albumart` and `readpicture` return nothing.

**Existing flow (before):**
```
ActualRead::process_one()
  → AlbumArtProvider: albumart <uri> → MD5 compare
  → ReadPictureProvider: readpicture <uri> → timestamp compare
  → Both failed? → log and return (placeholder stays)
```

**New flow (after, with feature enabled):**
```
ActualRead::process_one()
  → AlbumArtProvider: albumart <uri> → MD5 compare
  → ReadPictureProvider: readpicture <uri> → timestamp compare
  → Both failed AND online-cover-art enabled?
    → CoverOnlineProvider::lookup(artist, album)
      → MusicBrainz search API → get MBID
      → Cover Art Archive download → raw bytes
    → Success? write_cache() + emit events
    → Failure? log, return (placeholder stays)
```

### MusicBrainz API Details

**Search endpoint:**
```
GET https://musicbrainz.org/ws/2/release/?query=artist:{artist} AND release:{album}&fmt=json
```

Required headers:
- `User-Agent: mpd-client/0.1.0 (your-email@example.com)` — MusicBrainz requires a unique User-Agent. Use the app name + version.
- `Accept: application/json`

**Rate limiting:**
- MusicBrainz enforces 1 request per second on their API. Our internal rate limiter must respect this.
- Cover Art Archive (CAA) has similar rate limiting (also 1 req/s).
- One rate limiter for all online requests is sufficient since both services use the same rate limit.

**Error handling:**
- HTTP 429 (Too Many Requests): implement exponential backoff, retry up to 5 times
- HTTP 404 (Not Found): album has no art on CAA, mark as "not found" (no retry)
- HTTP 5xx: transient server error, exponential backoff
- Network errors (DNS failure, connection refused): exponential backoff

### Response Parsing

**MusicBrainz release search response:**
```json
{
  "releases": [
    {
      "id": "mbid-uuid-here",
      "title": "Album Title",
      "score": 100,
      "artist-credit": [
        { "name": "Artist Name" }
      ]
    }
  ]
}
```

Extract `releases[0].id` (pick the first/best match). The `score` field indicates match quality — accept any score > 50.

**Cover Art Archive response:**
- Direct image download from `https://coverartarchive.org/release/{mbid}/front-250`
- Returns image bytes directly (JPEG or PNG)
- HTTP 404 means no cover art available for this release

### Feature Flag Design

```toml
[features]
online-cover-art = ["ureq", "rustls"]

[dependencies]
ureq = { version = "3", optional = true, default-features = false, features = ["rustls", "gzip"] }
rustls = { version = "0.23", optional = true }
```

The `online-cover-art` feature must:
- Gate the `src/coverart/online.rs` module compilation
- Gate the `CoverOnlineProvider` field in `ActualRead`
- Gate the online lookup call in `process_one()`

Use `#[cfg(feature = "online-cover-art")]` on the module declaration and all online-specific code paths.

### Source Files to Touch

| File | What to Change |
|------|---------------|
| `Cargo.toml` | Add `[features]` section, make `ureq` + `rustls` optional |
| `src/coverart/mod.rs` | Add `pub mod online;` behind feature flag, re-export `CoverOnlineProvider` |
| `src/coverart/online.rs` | New file: `CoverOnlineProvider` struct, MusicBrainz/CAA lookup logic, rate limiter, backoff |
| `src/coverart/actual_read.rs` | Add `CoverOnlineProvider` field to `ActualRead`, call from `process_one()` after readpicture fallback |

### Testing Strategy

- `cargo build` — must succeed with 0 warnings (feature disabled)
- `cargo build --features online-cover-art` — must succeed with 0 warnings (feature enabled)
- `cargo test` — all existing tests pass
- Unit tests for `CoverOnlineProvider`:
  - Should not be possible to test API calls without network — use `#[cfg(test)] mod tests { }` to test parsing of sample JSON responses and rate limiter logic
  - Rate limiter test: verify that calling `lookup()` twice in rapid succession respects the 1s interval
  - JSON parsing test: verify sample MusicBrainz response parses correctly and extracts MBID
- Manual: run with `RUST_LOG=debug cargo run --features online-cover-art`, watch for `[online_cover]` prefixed log messages when covers are fetched online

### Anti-Pattern Prevention

- **Do NOT add any new dependencies** — `ureq` and `rustls` are already in Cargo.toml. Only make them optional.
- **Do NOT modify `CoverProvider`** — it's a read-only cache layer. Online fetch results go through `write_cache()` + `emit_cover_refreshed()` like any other cover source.
- **Do NOT remove existing albumart/readpicture fallback** — online lookup is the LAST resort, not a replacement.
- **Do NOT block the MPD command loop** — online HTTP requests are synchronous inside `process_one()`. This blocks one MPD idle cycle but since rate limiting is 1 req/s, this is acceptable. The next idle cycle (500ms later) will resume normal command processing.
- **Do NOT store user-identifiable information** — the MusicBrainz query sends only artist + album name, matching the privacy-by-design principle.

### References

- Architecture: `_bmad-output/planning-artifacts/architecture.md` §Cover Art Pipeline — Online lookup as optional third tier, rate limiting, exponential backoff
- Architecture: §Cover Art Event Flow — `CoverRefreshed` carries raw bytes, `CoverPaths` carries file path
- Architecture: §Explicit Trade-offs Accepted — "Online cover lookup is optional, opt-in via `online-cover-art` feature flag — disabled by default for privacy"
- Previous story: `_bmad-output/implementation-artifacts/13-4-widget-registry-in-place-updates.md` — `CoverRefreshed` event, `write_cache()`, `emit_cover_path()`, `emit_cover_refreshed()` patterns
- Current code: `src/coverart/actual_read.rs` — `process_one()` method, `handle_albumart_data`, `handle_readpicture_data`, `write_cache`, `emit_cover_path`, `emit_cover_refreshed`
- Current code: `src/coverart/provider.rs` — `CoverProvider` cache read
- MusicBrainz API: https://musicbrainz.org/doc/MusicBrainz_API
- Cover Art Archive: https://musicbrainz.org/doc/Cover_Art_Archive

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

- `[online_cover] lookup '{album}' by '{artist}'` — online lookup initiated
- `[online_cover] MusicBrainz found release {mbid} (score={score}) for '{album}'` — MusicBrainz search success
- `[online_cover] CAA download successful for '{album}' ({size} bytes)` — cover downloaded
- `[online_cover] MusicBrainz search returned no results for '{album}'` — no match found
- `[online_cover] CAA returned 404 for '{album}' — no cover art` — no art on CAA
- `[online_cover] HTTP {code} for '{album}', retry {attempt}/{max}` — retry log
- `[online_cover] Rate limit: waiting {secs}s` — rate limiter active
- `[online_cover] All retries exhausted for '{album}'` — final failure
- `[actual_read]` prefix for all ActualRead log messages

### Completion Notes List

### File List
