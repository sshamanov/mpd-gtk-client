# Story 38.1: Spawn Dedicated MPD Metadata Connection Thread

Status: done

## Story

As a developer,
I want a third MPD TCP connection dedicated to metadata queries,
so that the command/status connection is never blocked by large list or lsinfo responses.

## Acceptance Criteria

1. **Given** the application starts and connects to MPD
   **When** the MPD IO thread establishes the command/status connection
   **Then** a third MPD connection is established for metadata queries (alongside the existing command/status and cover connections)
   **And** the metadata connection uses its own `TcpStream` to the same MPD host and port

2. **Given** the metadata thread is running
   **When** a metadata command (e.g., `ListAlbumsGrouped`, `ListAlbumTracks`, `ListDirectory`) arrives
   **Then** it is dispatched on the metadata connection, not the command/status connection
   **And** the command/status connection remains free to handle status, queue, and playback commands concurrently

3. **Given** the MPD connection drops
   **When** reconnection occurs
   **Then** the metadata connection is also re-established
   **And** any in-flight metadata query is re-issued on the new connection

## Tasks / Subtasks

- [ ] Define metadata command channel type: `mpsc::sync_channel<(MpdCommand, mpsc::SyncSender<MpdEvent>)>` (request-response pattern) (AC: #2)
- [ ] Spawn metadata thread in `connected_loop()` after initial status fetch (AC: #1)
- [ ] Metadata thread connects to MPD with its own `MpdAdapter` (AC: #1)
- [ ] Metadata thread loop: recv command, execute on adapter, send result via event_tx (AC: #2)
- [ ] On MPD error, metadata thread reconnects to same target (AC: #3)
- [ ] Thread shuts down when `stop` flag is set (AC: #3)
- [ ] Test: mock MPD server handles metadata commands on separate connection

## Dev Notes

- Follow pattern from MPD Cover thread (`src/mpd/cover.rs`): separate TCP connection, spawned in connected_loop
- Metadata thread is persistent (unlike Cover thread which has 30s idle timeout) — once spawned, stays alive
- Uses request-response pattern: caller sends `(MpdCommand, SyncSender<MpdEvent>)`, thread executes and sends result
- The `event_tx` is NOT used directly by the metadata thread — results go back via the response SyncSender, and the caller forwards to event_tx. This keeps the metadata thread decoupled from the event system.
- Commands to handle: `ListAlbums`, `ListAlbumsGrouped`, `ListDirectory`, `ListAlbumTracks`, `Search`, `SearchFiles`
- `MpdAdapter` methods already exist for all metadata commands — just call the right method based on the MpdCommand variant
- Connection target: same `ConnectionTarget` passed to `connected_loop`

### Thread Model

```
main (GTK)                          mpd-event-loop                     mpd-metadata
    │                                     │                                │
    │── ListAlbums ──────────────────────►│                                │
    │                                     │── (cmd, reply_tx) ───────────►│
    │                                     │                                │── MpdAdapter::list_albums()
    │                                     │◄── MpdEvent::Albums ──────────│
    │◄── MpdEvent::Albums ───────────────│                                │
```

## References

- [Source: architecture.md#1087] — Concurrency & Threading ADR
- [Source: architecture.md#60-71] — Cross-cutting concerns, thread domains
- [Source: src/mpd/cover.rs:86-103] — Cover thread spawn pattern (reference)
- [Source: src/mpd/state_machine.rs:490] — connected_loop function
- [Source: src/mpd/state_machine.rs:360] — run() where search worker is spawned

## Dev Agent Record

### Agent Model Used

deepseek-v4-pro

### Completion Notes List

### File List
- src/mpd/state_machine.rs
- src/mpd/mock.rs
