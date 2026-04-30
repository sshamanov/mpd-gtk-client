# Story 9.7: Fix search_albums Stale Artist Edge Cases

Status: review

## Story

As a user searching the library,
I want search results to always show the correct artist name,
so that I don't see the previous track's artist carried over to a tagless track.

## Acceptance Criteria

1. **Given** an album has tracks with mixed Artist tags (some present, some missing)
   **When** `search_albums` processes the MPD response
   **Then** each album's artist is reset at each `file:` boundary
   **And** albums with entirely missing Artist tags show "Unknown Artist"
   **And** the `AlbumArtist` fallback from Story 9.5 still applies when Artist is missing

2. **Given** the first track of an album is missing Artist tags but a later track has them
   **When** `search_albums` processes the MPD response
   **Then** the album gets the non-empty artist from the later track, not an empty string

3. **Given** all tracks in an album have no Artist tag
   **When** `search_albums` processes the MPD response
   **Then** the album's artist is "Unknown Artist" in the search results

4. **Given** the AlbumArtist tag is present when Artist is missing
   **When** `search_albums` processes the MPD response
   **Then** AlbumArtist is used as the artist (Story 9.5 behavior preserved)

5. **Given** `cargo test` passes
   **When** the implementation is complete
   **Then** all existing tests pass and new edge case tests are added

## Tasks / Subtasks

- [x] (AC: #1, #2, #3) Fix `search_albums` in `src/mpd/mod.rs` to handle empty artist across tracks
  - [x] Change the `Album:` handler in the first pass: use `and_modify` to update empty entries when a non-empty artist is found, instead of `or_insert_with` which preserves the first (potentially empty) insertion
  - [x] After the first pass, replace any remaining empty artist values with "Unknown Artist"
  - [x] Verify `current_artist.clear()` on `file:` boundary is preserved (correct behavior)
  - [x] Verify `AlbumArtist:` override behavior is preserved (from Story 9.5)

- [x] (AC: #5) Add test coverage for the edge cases
  - [x] Add `make_search_missing_artist_response()` to `src/mpd/mock.rs` — search response where some tracks have no Artist tag
  - [x] Add `test_search_albums_missing_artist()` to `tests/smoke_test.rs` — verify album with mixed Artist tags gets the correct artist
  - [x] Add `test_search_albums_all_missing_artist()` — verify album with no Artist tags at all shows "Unknown Artist"
  - [x] Add `test_search_albums_albumartist()` — verify AlbumArtist fallback still works when Artist is missing

- [x] (AC: #5) Verify `cargo build` and `cargo test` pass

## Dev Notes

### Root Cause

In `search_albums` (src/mpd/mod.rs lines 352-385), the first pass maps album names to artists:

```rust
let mut current_artist = String::new();
for line in &lines {
    if line.starts_with("file: ") {
        current_artist.clear();
    } else if let Some(artist) = line.strip_prefix("Artist: ") {
        current_artist = artist.to_string();
    } else if let Some(artist) = line.strip_prefix("AlbumArtist: ") {
        current_artist = artist.to_string();
    } else if let Some(album) = line.strip_prefix("Album: ") {
        album_artist.entry(album.to_string())
            .or_insert_with(|| current_artist.clone());
    }
}
```

The bug: When `current_artist` is empty (because `file:` cleared it and no `Artist:`/`AlbumArtist:` followed for this track), `or_insert_with(|| current_artist.clone())` inserts `""` into `album_artist`. Since `or_insert_with` never overwrites an existing entry, if the first track of an album has no artist but a later one does, the entry remains `""`.

Additionally, albums with entirely missing artist tags get `""` instead of `"Unknown Artist"`.

### Fix Strategy

The fix has two parts, both in the first-pass loop:

**Part 1: Fix the Album: handler** — Instead of `or_insert_with` which never overwrites, use a pattern that:
- If the album is not in the map yet and `current_artist` is non-empty, insert it
- If the album IS in the map with an empty value and `current_artist` is non-empty, update it
- If the album is not in the map and `current_artist` is empty, still track it (so we know to set "Unknown Artist" later)

Recommended approach using `entry()` + `and_modify()` + `or_insert_with()`:

```rust
} else if let Some(album) = line.strip_prefix("Album: ") {
    if !current_artist.is_empty() {
        album_artist.entry(album.to_string())
            .and_modify(|e| { if e.is_empty() { *e = current_artist.clone(); } })
            .or_insert_with(|| current_artist.clone());
    } else {
        album_artist.entry(album.to_string())
            .or_insert_with(String::new);
    }
}
```

**Part 2: Replace empty artists** — After the first pass loop, iterate and fix empty values:

```rust
for (_, artist) in album_artist.iter_mut() {
    if artist.is_empty() {
        *artist = "Unknown Artist".to_string();
    }
}
```

### Edge Cases Covered

| Scenario | Expected | Mechanism |
|----------|----------|-----------|
| Track has `Artist:` before `Album:` | Artist used | First `or_insert_with` captures it |
| Track has no `Artist:`, same album already has artist | Existing artist preserved | `or_insert_with` doesn't overwrite |
| First track of album has no artist, later track does | Later artist fills in | `and_modify` updates empty entry |
| No track of album has any artist | "Unknown Artist" | Post-loop replacement of empty strings |
| `AlbumArtist:` present, `Artist:` missing | AlbumArtist used | AlbumArtist setter runs before Album handler |
| `file:` boundary between different albums | Artist reset per track | `current_artist.clear()` on `file:` preserved |

### The existing test

The current `test_search_albums` test in `tests/smoke_test.rs` uses the mock response where both tracks have Artist tags. It passes with the current code. The fix must not break this test.

### Files to touch

- `src/mpd/mod.rs` — Fix `search_albums` first-pass logic (lines 369-371) and add post-loop empty-artist cleanup
- `src/mpd/mock.rs` — Add new mock search response variants for edge cases
- `tests/smoke_test.rs` — Add 2-3 new test functions for the edge cases

### Testing

- `cargo test` must pass
- New tests should verify:
  1. Mixed artist tags: album gets artist from the track that has it
  2. All missing artist tags: album shows "Unknown Artist"
  3. AlbumArtist fallback: AlbumArtist is used when Artist is missing

### What NOT to do

- Do NOT change the `Album:` line second pass (lines 374-383) — it correctly builds results in order
- Do NOT remove `current_artist.clear()` on `file:` boundary — that's the intended per-track reset
- Do NOT change the mock search response for existing tests — they must still pass

### References

- [Source: `src/mpd/mod.rs`] — lines 352-385, `search_albums` method
- [Source: `src/mpd/mock.rs`] — lines 195-203, existing `make_search_response()`
- [Source: `tests/smoke_test.rs`] — lines 58-66, existing `test_search_albums`
- [Source: `_bmad-output/planning-artifacts/epics.md`] — Story 9.7 acceptance criteria and technical notes
- [Source: `_bmad-output/implementation-artifacts/deferred-work.md`] — "search_albums stale artist across tracks when Artist tag missing"
- [Source: `_bmad-output/implementation-artifacts/1b-4-basic-mpd-search.md`] — Original story, review findings, deferred items

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

### Completion Notes List

- Fixed `search_albums` stale artist edge case in `src/mpd/mod.rs`: Changed `Album:` handler to use `and_modify` + `or_insert_with` so that a non-empty artist from a later track can fill in when the first track has no artist. Added post-loop empty-artist cleanup to replace remaining empty strings with "Unknown Artist".
- Added 3 new mock search responses in `src/mpd/mock.rs`: `make_search_missing_artist_response` (mixed artist tags), `make_search_no_artist_response` (no artist tags), `make_search_albumartist_response` (AlbumArtist fallback).
- Added 3 new tests in `tests/smoke_test.rs`: `test_search_albums_missing_artist`, `test_search_albums_all_missing_artist`, `test_search_albums_albumartist`.
- All 15 tests pass (13 existing + 3 new).

### File List
- src/mpd/mod.rs
- src/mpd/mock.rs
- tests/smoke_test.rs
