# Story 36.2: Use Batch for PlayAlbum and AddAlbum

Status: done

## Story

As a user,
I want adding or playing a multi-track album to be faster,
so that queue operations complete in a single round-trip regardless of album track count.

## Acceptance Criteria

1. **Given** the user plays an album via `PlayAlbum("AlbumName")`
   **When** the state machine processes the command
   **Then** a `Batch` is constructed containing: `Clear` + `Add(track_uris...)` + `Play(0)`
   **And** the batch is sent as a single MPD command_list transaction

2. **Given** the user adds an album to the queue via `AddAlbum("AlbumName")`
   **When** the state machine processes the command
   **Then** a `Batch` is constructed containing: `Add(track_uris...)`
   **And** the batch is sent as a single MPD command_list transaction

3. **Given** `PlayUris` or `AddUris` are used
   **When** they construct their command list
   **Then** they are refactored to use the new `Batch` variant for consistency

## Tasks / Subtasks

- [ ] In `handle_command(PlayAlbum)`, after `ListAlbumTracks` returns URIs, construct a `Batch(Clear, Add(uri1), Add(uri2), ..., Play(0))` and dispatch it instead of iterating individual adds (AC: #1)
- [ ] In related album-play flow, construct batch with `Clear` + individual `Add` calls + `Play(0)` (AC: #1)
- [ ] In `handle_command(AddAlbum)` or equivalent, construct a `Batch(Add(uri1), Add(uri2), ...)` (AC: #2)
- [ ] Refactor `PlayUris` and `AddUris` to use `Batch` internally (AC: #3)
- [ ] Test: verify a 20-track album play sends 1 Batch message, not 22 individual messages

## Dev Notes

- Currently `PlayUris` and `AddUris` manually construct `command_list_begin/end` sequences in the state machine handler — refactor to emit a single `Batch` command instead
- The `ListAlbumTracks` call must still happen first to get URIs; only the Add/Play portion is batched
- `MpdCommand::Play` takes no arguments — it plays from queue position 0. In batch: `Clear + Add(uri1) + Add(uri2) + ...` followed by `Play 0` as the last command
- The mock MPD server needs to support command_list_begin/end sequences in test mode for integration tests

### References

- [Source: architecture.md#689] — Command List Batching ADR
- [Source: src/mpd/state_machine.rs] — PlayUris, AddUris, PlayAlbum handlers

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Completion Notes List

### File List
- src/mpd/state_machine.rs
- src/mpd/mod.rs
- src/mpd/mock.rs
