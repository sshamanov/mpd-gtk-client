# Story 30.4: Cover RwLock Poison Logging

Status: done

## Story

As a developer,
I want RwLock poison errors in the cover pipeline to be logged,
so that lock poisoning is visible in debug logs rather than silently ignored.

## Acceptance Criteria

1. **Poison errors logged in cover_proc.rs**
   - Given `provider.read()` returns a `PoisonError`
   - When `process_success()` attempts to read the CoverProvider
   - Then the poison error is logged at error level
   - And execution continues gracefully (no panic)

2. **Poison errors logged in provider.rs**
   - Given `self.index.read()` or `self.index.write()` returns a `PoisonError`
   - When `get()`, `invalidate()`, `update_entry()`, or `len()` is called
   - Then the poison error is logged at error level
   - And the method returns a safe default (None, 0, or no-op)

3. **Backward compatibility**
   - Given all existing tests pass
   - When the fix is applied
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Affected Code

| File | Line(s) | Current Pattern | Fix |
|------|---------|----------------|-----|
| `cover_proc.rs` | 98 | `if let Ok(ref prov) = provider.read()` | Add `else { log::error!(...) }` |
| `cover_proc.rs` | 123 | `if let Ok(ref prov) = provider.read()` | Add `else { log::error!(...) }` |
| `provider.rs` | 102 | `self.index.read().ok()?` | Log before `?` |
| `provider.rs` | 142 | `if let Ok(mut index) = self.index.write()` | Add `else { log::error!(...) }` |
| `provider.rs` | 155 | `if let Ok(mut index) = self.index.write()` | Add `else { log::error!(...) }` |
| `provider.rs` | 171 | `self.index.read().map(...).unwrap_or(0)` | Log in `map_err` |

### Key Rules

- No functional changes — only add logging
- No new dependencies or imports beyond existing `log` crate
- All existing tests must pass unchanged

## Dev Agent Record

### Implementation Plan

1. Add `else { log::error!(...) }` clauses to all 6 poison-silently-ignored locations
2. Build and test

### Completion Notes

- Converted 6 `if let Ok(...)` patterns to `match` with `Err(e) => log::error!(...)` across 2 files
- cover_proc.rs: 2 locations (cache read, update_entry)
- provider.rs: 4 locations (get, invalidate, update_entry, len)
- All 94 tests pass, zero regressions
- Code review: clean, zero findings

### Change Log

- Modified `src/coverart/cover_proc.rs`: `if let Ok` → `match` with poison logging (2 locations)
- Modified `src/coverart/provider.rs`: `if let Ok` → `match` with poison logging (4 locations)

### References
- [Source: src/coverart/cover_proc.rs:87-148] `process_success()` with CoverProvider reads
- [Source: src/coverart/provider.rs:95-177] CoverProvider methods with RwLock access
