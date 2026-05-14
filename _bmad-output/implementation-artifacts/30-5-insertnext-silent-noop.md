# Story 30.5: InsertNext Silent Noop

Status: done

## Story

As a developer,
I want `InsertNext` queue operations to produce visible errors when they cannot complete,
so that silent failures don't confuse users.

## Acceptance Criteria

1. **Empty URI list produces error**
   - Given `InsertNext` is called for an album with no files
   - When `find_album_uris()` returns an empty list
   - Then the handler returns early with `false`
   - And no silent no-op occurs

2. **No current track produces error**
   - Given `InsertNext` is called when nothing is playing
   - When `current_pos < 0`
   - Then a warning is logged
   - And an `MpdEvent::Error` is sent to the UI with a user-visible message

3. **Channel full doesn't stall thread**
   - Given the event channel is full
   - When `InsertNext` sends an error event
   - Then `try_send` is used (not blocking `send`)
   - And the MPD connection thread continues without stalling

4. **Backward compatibility**
   - Given all existing tests pass
   - When the fix is verified
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Background

The `MpdCommand::InsertNext` handler at `src/mpd/state_machine.rs:948-982` previously had issues:
- `send` (blocking) could stall the MPD connection thread if the event channel was full
- Empty URI lists were silently ignored

### Fix (Already Implemented)

Applied in commit `71ceda65` (2026-05-12). The handler now:
1. Returns `false` early when `uris.is_empty()` (line 951)
2. Uses `try_send` for error events to avoid thread stall (line 959)
3. Logs a warning and emits `MpdEvent::Error` when no current track (lines 957-962)
4. Logs errors for batch failures (line 971)

### Key Files

| File | Change |
|------|--------|
| `src/mpd/state_machine.rs:948-982` | InsertNext handler with error handling |

## Dev Agent Record

### Completion Notes

- Fix applied in commit `71ceda65` (2026-05-12)
- `send` → `try_send` for error path avoids thread stall
- Empty URI check + no-current-track check prevent silent no-ops
- No code changes needed
- All 94 tests pass

## References
- [Source: src/mpd/state_machine.rs:948-982] InsertNext handler implementation
- [Source: git 71ceda65] Commit that applied the fix
