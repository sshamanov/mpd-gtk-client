# Story 30.7: CLI Action Overwrite Warning

Status: done

## Story

As a developer,
I want a visible warning logged when a stale IPC lock file is overwritten,
so that lock file lifecycle is traceable in logs for debugging.

## Acceptance Criteria

1. **Stale lock overwrite is logged**
   - Given a previous instance's lock file exists but the PID is dead
   - When a new instance starts and overwrites the stale lock
   - Then a warning is logged with the stale PID

2. **Normal lock acquisition is not noisy**
   - Given no lock file exists
   - When a new instance acquires the lock normally
   - Then no extra warning is logged

3. **Backward compatibility**
   - Given all existing tests pass
   - When the fix is applied
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Affected Code

| File | Location | Change |
|------|----------|--------|
| `src/ipc.rs:70-80` | Stale lock removal | Add `log::warn!` before removing stale lock |

### Key Rules

- Add only a log statement — no functional changes
- Use `log::warn!` (already available via `log` crate dependency)

## Dev Agent Record

### Implementation Plan

1. Add `log::warn!` when stale lock is detected and overwritten in `try_acquire_lock()`
2. Build and test

### Completion Notes

- Added `log::warn!` with stale PID in `try_acquire_lock()` (`src/ipc.rs:71`)
- Only fires when stale lock detected (PID dead or not mpd-client)
- Normal lock acquisition path unchanged (no noise)
- All 94 tests pass, zero regressions
- Epic-30 complete (7/7 stories done)

### Change Log

- Modified `src/ipc.rs`: added stale lock overwrite warning log

### References
- [Source: src/ipc.rs:67-87] `try_acquire_lock()` stale lock handling
