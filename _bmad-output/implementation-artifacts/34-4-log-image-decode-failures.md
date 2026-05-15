# Story 34.4: Add Diagnostic Log for image::open Decode Failures

Status: done

## Story

As a developer,
I want `image::open` decode failures in the Cover Proc worker to produce a diagnostic log message,
so that corrupted cover files in the cache are traceable without requiring deep code inspection.

## Acceptance Criteria

1. **Decode failure logged with context**
   - Given a cover JPEG file on disk is corrupted or truncated
   - When the Cover Proc worker calls `image::load_from_memory(data)` in `decode_and_resize`
   - Then if the decode fails, a `log::warn!` message is emitted including the album key, data size, and error details
   - And the existing `Err` return path is preserved

2. **No regression on success**
   - Given a cover file decodes successfully
   - When `decode_and_resize` processes the data
   - Then no additional log messages are emitted (no regression)
   - And the success return is unchanged

## Technical Requirements

- In `cover_proc.rs`, `decode_and_resize()` at line 156-161:
  ```rust
  fn decode_and_resize(data: &[u8]) -> Result<Vec<u8>, String> {
      let img = image::load_from_memory(data).map_err(|e| format!("{e}"))?;
      let rgba = img.to_rgba8();
      let resized = image::imageops::resize(&rgba, 200, 200, FilterType::Lanczos3);
      Ok(resized.into_raw())
  }
  ```
- The caller at line 136 does log: `log::warn!("[cover-proc] '{key}': JPEG decode failed ({e}), skipping CoverRefreshed (placeholder fallback)")`.
- Verify this log message already contains sufficient context (album key). If it does, the issue may be that the error message from `image::load_from_memory` is too generic (e.g., "Invalid image").
- Fix: In `decode_and_resize`, log the data size before attempting decode so administrators can correlate the failure with specific data sizes. Also, the existing caller log should include the data size.
- No behavioral change — logging only.

## References
- [Source: epics.md] Epic 34: General Code Quality — Story 34.4
- [Source: deferred-work.md] Code review 29-1-image-crate-migration — image::open decode failures silently swallowed by if let Ok
- [Source: src/coverart/cover_proc.rs:156-161] decode_and_resize — image::load_from_memory call
- [Source: src/coverart/cover_proc.rs:136-151] Caller that logs the failure
