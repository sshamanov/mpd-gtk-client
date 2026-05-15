# Story 38.2: Route Metadata Commands Through the Dedicated Connection

Status: done

## Story

As a developer,
I want the state machine to route metadata-heavy commands to the metadata connection,
so that command/status throughput is not degraded by bulk queries.

## Acceptance Criteria

1. **Given** the UI sends a `ListAlbumsGrouped("Date")` command
   **When** the state machine dispatches it
   **Then** the command is forwarded to the metadata connection's channel (not the command/status connection)
   **And** the result arrives as an `MpdEvent::AlbumsGrouped` on the main event channel (same as today)

2. **Given** the metadata connection is not yet established (during initial connection setup)
   **When** a metadata command is received
   **Then** it is dispatched on the command/status connection (fallback behavior)
   **And** a debug log notes the fallback

3. **Given** the metadata connection is established
   **When** a low-latency command (Status, CurrentSong, Play, Pause, Next, etc.) is received
   **Then** it is dispatched on the command/status connection as before (no change)
   **And** no metadata connection overhead is incurred for fast commands

## Tasks / Subtasks

- [ ] Change `_metadata_tx` to `metadata_tx` and store as `Option<mpsc::SyncSender<MpdCommand>>` (AC: #1, #2)
- [ ] Define which commands route to metadata: `ListAlbums`, `ListAlbumsGrouped`, `ListDirectory`, `ListAlbumTracks`, `Search`, `SearchFiles` (AC: #1, #3)
- [ ] In `process_command()`, before the main match: if metadata_tx is Some and command is metadata-type, forward and return early (AC: #1, #3)
- [ ] Fallback: if metadata_tx is None, process on command/status connection with debug log (AC: #2)
- [ ] Remove now-unreachable metadata command arms from the main process_command match (or keep as fallback) (AC: #2)
- [ ] Verify: `cargo test` passes — existing tests still work after routing change

## Dev Notes

- `_metadata_tx` is currently created at `state_machine.rs:532` but discarded — change to `let metadata_tx = ...` and pass to `process_command`
- `process_command` signature needs `metadata_tx: &Option<mpsc::SyncSender<MpdCommand>>` parameter
- Routing decision: check `cmd` variant against metadata command list, if match → `metadata_tx.try_send(cmd)` and return early
- Commands to route: `ListAlbums`, `ListAlbumsGrouped(_)`, `ListDirectory(_)`, `ListAlbumTracks(_)`, `Search(_)`, `SearchFiles(_)`
- Keep on main: `Status`, `CurrentSong`, `Play`, `Pause`, `Next`, `Previous`, `Stop`, `Seek`, `Add`, `AddAt`, `DeleteId`, `MoveId`, `Clear`, `ListQueue`, `PlayPosition`, `Reconnect`, `Close`, `Update`, `FetchCovers`, playback/batch commands
- The `ListAlbumsGrouped` handler in metadata thread doesn't use `metadata_cache` or `cached_flat_albums` — this is OK for now; the cached version can stay as the main-thread fallback
- When metadata_tx is None or try_send fails: fall through to main connection handler with `log::debug!("[MPD] metadata command on main connection (fallback)")`

### References

- [Source: src/mpd/state_machine.rs:532] — metadata_tx creation
- [Source: src/mpd/state_machine.rs:840-942] — process_command metadata handlers
- [Source: src/mpd/state_machine.rs:1540-1670] — metadata_thread function

## Dev Agent Record

### Agent Model Used

deepseek-v4-pro

### Completion Notes List

### File List
- src/mpd/state_machine.rs
