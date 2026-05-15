# Story 35.2: Persist and Recover playlist_version Across Reconnections

Status: done

## Story

As a developer,
I want the `playlist_version` counter to reset on reconnect,
so that the first queue update after reconnection uses a full playlistinfo sync rather than stale version comparison.

## Acceptance Criteria

1. **Given** the MPD connection drops and reconnects
   **When** the new `connected_loop` starts
   **Then** the stored `playlist_version` is reset to 0
   **And** the first queue update after reconnect uses a full `playlistinfo` sync (not plchanges)

2. **Given** a full sync is performed
   **When** the sync completes
   **Then** the new `playlist_version` from the `status` response is stored for subsequent incremental updates

## Tasks / Subtasks

- [ ] Reset `playlist_version` to 0 on state transition from Disconnected to Connected (AC: #1)
- [ ] After first full `playlistinfo` sync after reconnect, extract `playlist` version from `status` response and store it (AC: #2)
- [ ] Verify that if version is 0, a full sync is done not plchanges (AC: #1)

## Dev Notes

- `playlist_version` is already present in the `status` response but not stored on the state machine's Connected state
- Simple guard: if `stored_version == 0`, do full sync; otherwise use plchanges
- This is a prerequisite for story 35.1 — the plchanges logic needs a way to distinguish "first sync after reconnect" from "subsequent incremental updates"

### References

- [Source: architecture.md#706] — Queue Update Strategy ADR
- [Source: src/mpd/state_machine.rs] — state machine transitions

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Completion Notes List

### File List
- src/mpd/state_machine.rs
