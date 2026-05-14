# Story 31.5: Atomic index.json with Corruption Recovery and Failed Write Protection

Status: done

## Story

As a developer,
I want the cover cache index.json to be updated atomically and to handle corruption without silently losing all entries,
so that the cache remains consistent across crashes and transient errors.

## Acceptance Criteria

1. **Corrupt index.json backed up instead of silently reset**
   - Given index.json is corrupt (invalid JSON, truncated)
   - When `CoverProvider::load_index()` is called at startup
   - Then the corrupt file is backed up to `index.json.bad`
   - And a warning log includes the backup path
   - And the provider starts with an empty index (functionality preserved, backup recoverable)

2. **index.json not updated when JPEG write fails**
   - Given a JPEG write to cache via `fs::write()` fails (disk full, permissions)
   - When `write_cache` completes processing
   - Then `update_index_json` is NOT called (the index should not reference a non-existent file)
   - And a warning is logged about the skipped index update

3. **Atomic update via temp file + rename preserved**
   - Given index.json is updated after a successful cache write
   - When the update proceeds
   - Then `fs::write` writes to `index.json.tmp` first (already implemented)
   - And `fs::rename` atomically replaces `index.json` with `index.json.tmp` (already implemented)
   - And if either step fails, the original `index.json` is preserved

## Technical Requirements

- The temp-file-plus-rename pattern already exists in `update_index_json()` at lines 194-223 of `cover_proc.rs` — this is correct and should be preserved.
- The bug: `update_index_json` is called unconditionally after `write_cache` (line 182), even when `fs::write` for the JPEG fails. Fix: move `update_index_json` inside the success branch of `fs::write`.
- For corrupt index.json at startup (`provider.rs` lines 199-200): replace `serde_json::from_str(&s).unwrap_or_default()` with: (a) attempt parse, (b) if parse fails, rename `index.json` → `index.json.bad`, (c) log warning with backup path, (d) return empty HashMap. Use `fs::rename` for the backup.

## References
- [Source: epics.md] Epic 31: Cover Pipeline Reliability — Story 31.5
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — Corrupt index.json silently resets entire cache; update_index_json called on failed fs::write
- [Source: src/coverart/cover_proc.rs:164-223] `write_cache()` and `update_index_json()`
- [Source: src/coverart/provider.rs:192-201] `load_index()` — unwrap_or_default silently loses corrupt data
