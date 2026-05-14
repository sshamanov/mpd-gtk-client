# Story 31.4: JPEG Decode Size Guard

Status: done

## Story

As a developer,
I want the JPEG decode path in the Cover Proc worker to reject images larger than a configured maximum dimension,
so that the intermediate RGBA buffer does not cause an OOM crash.

## Acceptance Criteria

1. **Dimension check before decode**
   - Given the Cover Proc worker receives a very large cover image (e.g., 16384x16384 pixels, the MPD protocol maximum)
   - When `decode_and_resize()` processes the raw bytes
   - Then the function checks the image dimensions via `image::image_dimensions()` before decoding
   - And if width or height exceeds MAX_DIM (default: 4096 pixels), the image is rejected with a warning log
   - And no CoverRefreshed event is emitted (OOM risk avoided)
   - And the existing CoverPaths event (for disk-cached fallback) is still emitted

2. **Normal images unaffected**
   - Given a normal cover image (e.g., 500x500 pixels)
   - When `decode_and_resize()` processes the data
   - Then behavior is unchanged — decode, resize to 200x200, emit RGBA

3. **Constant defined and documented**
   - Given the codebase
   - When a developer reads the cover proc module
   - Then the MAX_DIM constant (4096) is documented with the memory budget rationale
   - And the constant can be adjusted without changing logic

## Technical Requirements

- `image::load_from_memory` decodes the full image into memory before any resize. A 16384x16384 RGBA buffer = 1GB (16384 * 16384 * 4 bytes). The current 200x200 target zoom has no impact on intermediate memory allocation.
- Use `image::image_dimensions(bytes)` which parses only the image header (fast, no full decode) to get dimensions.
- MAX_DIM = 4096 → max RGBA buffer = 4096*4096*4 = 64MB, which is large but manageable and unlikely to be exceeded by real album art.
- The JPEG file can still be served from disk cache via GDK's built-in decoder (which has its own size guards) — so the CoverPaths event provides a safe fallback path.
- The `image::image_dimensions()` function is already available from the `image` crate and works on raw bytes without full decode.

## References
- [Source: epics.md] Epic 31: Cover Pipeline Reliability — Story 31.4
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — No size guard on JPEG decode
- [Source: src/coverart/cover_proc.rs:156-161] `decode_and_resize()` — current implementation with no size check
