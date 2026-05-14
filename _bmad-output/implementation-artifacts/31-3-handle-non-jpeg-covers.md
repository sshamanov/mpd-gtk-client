# Story 31.3: Handle Non-JPEG Cached Covers Without Delete Loop

Status: done

## Story

As a developer,
I want non-JPEG cover images (PNG, WebP) cached as `.jpg` to be handled gracefully,
so that they don't enter an infinite delete-recycle loop via `CoverProvider::is_valid_jpeg`.

## Acceptance Criteria

1. **Non-JPEG covers normalized to JPEG on cache write**
   - Given MPD `albumart` returns non-JPEG binary data (PNG, WebP, etc.)
   - When `write_cache` is called in the Cover Proc worker
   - Then the data is decoded via `image::load_from_memory()` and re-encoded as JPEG before writing to `{md5}.jpg`
   - And the JPEG quality is adequate (85-90 quality, sufficient for album art)
   - And the MD5 hash is computed from the original binary data (not the re-encoded JPEG)

2. **No delete-recycle loop**
   - Given a non-JPEG file was previously cached as `.jpg` and `is_valid_jpeg` rejects it
   - When `CoverProvider::get()` checks the file
   - Then instead of deleting and invalidating (which triggers re-fetch + re-cache + re-delete), the method converts the non-JPEG to JPEG in place or marks it as valid
   - And the album still gets a cached cover

3. **Backward compatibility**
   - Given existing cache files on disk that are valid JPEGs
   - When the application starts
   - Then behavior is unchanged — `is_valid_jpeg` passes and covers display normally

## Technical Requirements

- Root cause: `albumart` returns whatever format MPD has (often JPEG, but can be PNG/WebP), and `write_cache` always writes to `{md5}.jpg` without format conversion. Then `CoverProvider::is_valid_jpeg` (which checks for `FF D8 FF` magic bytes) rejects non-JPEG files, deletes them, and invalidates the index entry — creating a loop on next fetch.
- Fix: In `cover_proc.rs::write_cache`, detect the actual image format from magic bytes before writing. If not JPEG, use `image::load_from_memory()` to decode and `image::encode_to()` (with `image::codecs::jpeg::JpegEncoder`) to write as JPEG before saving.
- The `image` crate is already a dependency with jpeg, png, webp features.
- Alternative: change the cache file extension based on actual format and extend `is_valid_jpeg` to accept multiple formats. But normalizing to JPEG is simpler and keeps the cache uniform.
- The MD5 hash should be computed from the original MPD response data (for content-addressed dedup), not from the re-encoded JPEG (which would differ due to re-encoding artifacts).

## References
- [Source: epics.md] Epic 31: Cover Pipeline Reliability — Story 31.3
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — Non-JPEG embedded cover art delete-recycle loop
- [Source: src/coverart/provider.rs:205-215] `is_valid_jpeg()` — only checks JPEG magic bytes
- [Source: src/coverart/cover_proc.rs:164-183] `write_cache()` — always writes as `.jpg`
