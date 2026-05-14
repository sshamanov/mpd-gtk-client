# Story 30.1: Albumart First Chunk Reuse

Status: done

## Story

As a developer,
I want the `albumart_by_uri()` method to reuse the first binary chunk from the initial `albumart <uri> 0` response,
so that we save one MPD round-trip per cover fetch.

## Acceptance Criteria

1. **First chunk extracted from initial response**
   - Given an `albumart <uri> 0` MPD command is sent
   - When the response arrives (size header + binary chunk + OK)
   - Then `parse_albumart_chunk()` extracts the binary data from the response
   - And the first chunk is used as the starting point for reassembly

2. **No re-request at offset 0**
   - Given the first chunk has been extracted
   - When subsequent chunks are fetched
   - Then the offset starts from `data.len()` (length of first chunk)
   - And no redundant `albumart <uri> 0` command is sent

3. **Backward compatibility**
   - Given all existing tests pass
   - When the fix is verified
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Background

From architecture.md §638-640: The first `albumart <uri> 0` request returns both the size header AND the first binary chunk. Originally only `parse_albumart_size()` was called on the response — the binary data was discarded, requiring a re-request at offset 0.

### Fix (Already Implemented)

The fix was applied in commit `71ceda65` (2026-05-12). The `albumart_by_uri()` method at `src/mpd/mod.rs:637-650` now:

1. Extracts both `total_size` and `first_chunk` from the initial response (line 643-645)
2. Initializes `data = first_chunk` (line 650)
3. Fetches remaining chunks starting from `data.len()` (line 652)

### Key Files

| File | Change |
|------|--------|
| `src/mpd/mod.rs` | `albumart_by_uri()` extracts and reuses first chunk |
| `src/mpd/mod.rs` | `parse_albumart_chunk()` helper extracts binary from raw response |

## Dev Agent Record

### Implementation Plan

Verification-only: confirm the fix is in place via git blame and code review.

### Completion Notes

- Fix already applied in commit `71ceda65` (2026-05-12) as part of review fixes
- `albumart_by_uri()` at line 643-650 correctly extracts and reuses the first binary chunk
- `readpicture()` at line 685-686 also extracts first chunk (was already correct)
- No code changes needed; sprint status just needed updating
- All 94 tests pass

### Change Log

- No code changes — verification only
- Updated sprint-status.yaml: 30-1 → done
- Updated epic-30 → in-progress

## References
- [Source: architecture.md §638-640] First chunk reuse ADR — describes the problem and fix
- [Source: src/mpd/mod.rs:637-660] `albumart_by_uri()` — fix implementation
- [Source: src/mpd/mod.rs:609-619] `parse_albumart_chunk()` — binary chunk parser
- [Source: git 71ceda65] Commit that applied the fix
