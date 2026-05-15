# Story 35.1: Incremental Queue Sync via plchanges

Status: done

## Story

As a user,
I want queue updates to preserve my scroll position and selection,
so that browsing the queue is not disrupted by full playlist re-fetches on every change.

## Acceptance Criteria

1. **Given** the state machine processes a queue change event (playlist signal from idle)
   **When** the state machine calls the queue update handler
   **Then** it uses `adapter.plchanges(last_version)` instead of `adapter.list_queue()`
   **And** deletions are reconciled by cross-referencing local positions against reported playlist length
   **And** the merged result is emitted as `MpdEvent::Queue(Vec<QueueEntry>)`

2. **Given** 50 incremental updates have occurred since the last full sync
   **When** the 51st queue change is detected
   **Then** a full `playlistinfo` sync is performed to reconcile drift

3. **Given** the version counter wraps around (new_version < old_version with delta > 1M)
   **When** a queue change is detected
   **Then** a full `playlistinfo` sync is performed

4. **Given** `plchanges` returns an error
   **When** the error is caught
   **Then** fall back to full `playlistinfo` re-fetch

## Tasks / Subtasks

- [ ] Add `playlist_version: u64` field to the `Connected` state in state machine (AC: #1)
- [ ] In the queue update handler (idle response or ListQueue), replace `adapter.list_queue()` with: store current version, `adapter.plchanges(version)`, reconcile deletions (AC: #1)
- [ ] Track `plchanges_call_count` since last full sync; force full sync at 50 (AC: #2)
- [ ] Add version wrap detection (new < old with delta > 1M) (AC: #3)
- [ ] Add fallback to full sync on plchanges error (AC: #4)
- [ ] Test: verify scroll position is preserved during incremental queue updates

## Dev Notes

- `MpdAdapter::plchanges(version)` already exists at `src/mpd/mod.rs:770` but is never called by the state machine
- The `status` response already carries `playlist:` version — this is parsed but not stored for plchanges
- Deletions: `plchanges` returns only added/changed songs. Entries at positions >= new playlist length were deleted. Cross-reference local positions.
- Version wrap: 32-bit counter, extremely unlikely but handled via the threshold check (delta > 1M)
- Current behavior in state machine: `MpdCommand::ListQueue` → `adapter.list_queue()` → full fetch. Change to store version and use plchanges.
- The merged queue (plchanges result + retained entries) is emitted as the same `MpdEvent::Queue` type — UI does not need changes.

### References

- [Source: architecture.md#706] — Queue Update Strategy ADR (plchanges)
- [Source: src/mpd/mod.rs:770] — existing `plchanges()` method on adapter
- [Source: src/mpd/state_machine.rs:902] — current `ListQueue` handler doing full fetch

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Completion Notes List

### File List
- src/mpd/mod.rs
- src/mpd/state_machine.rs
