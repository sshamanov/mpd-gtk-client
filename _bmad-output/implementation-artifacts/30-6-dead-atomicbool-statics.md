# Story 30.6: Dead AtomicBool Statics

Status: done

## Story

As a developer,
I want dead `static AtomicBool` declarations removed from the codebase,
so that the code is clean and doesn't suggest unused shutdown mechanisms.

## Acceptance Criteria

1. **No dead static AtomicBool declarations**
   - Given a grep for `static.*AtomicBool` across the source tree
   - When the cleanup is verified
   - Then zero results are returned

2. **All AtomicBools are properly scoped**
   - Given any `AtomicBool` in the codebase
   - When its usage is traced
   - Then it is wrapped in `Arc` and shared via clone
   - And no bare `static` declarations exist

3. **Backward compatibility**
   - Given all existing tests pass
   - When the cleanup is verified
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Background

Previous code iterations had `static` `AtomicBool` declarations for shutdown signaling that were never used or were replaced by `Arc<AtomicBool>` patterns. Commit `71ceda65` (2026-05-12) removed the dead imports and declarations.

### Fix (Already Implemented)

Applied in commit `71ceda65` (2026-05-12):
- Removed unused `AtomicBool` import
- All remaining `AtomicBool` usage is properly `Arc`-wrapped:
  - `src/mpd/cover.rs:75` — `shutting_down: Arc<AtomicBool>`
  - `src/mpd/mock.rs:20` — `stop: Arc<AtomicBool>`
  - `src/main.rs:229` — `ipc_stop: Arc<AtomicBool>`
- Zero `static AtomicBool` declarations exist

### Key Files

| File | Change |
|------|--------|
| Various | Removed dead static declarations in 71ceda65 |

## Dev Agent Record

### Completion Notes

- Fix applied in commit `71ceda65` (2026-05-12)
- Verified: zero `static AtomicBool` declarations via grep
- All AtomicBools are `Arc`-wrapped with proper ownership
- No code changes needed
- All 94 tests pass

## References
- [Source: src/mpd/cover.rs:75] `shutting_down: Arc<AtomicBool>`
- [Source: src/main.rs:229] `ipc_stop: Arc<AtomicBool>`
- [Source: git 71ceda65] Commit that removed dead statics
