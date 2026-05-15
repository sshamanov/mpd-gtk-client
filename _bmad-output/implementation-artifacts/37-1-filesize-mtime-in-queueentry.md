# Story 37.1: Add file_size and mtime Fields to QueueEntry

Status: done

## Story

As a developer,
I want `QueueEntry` to carry file_size and mtime metadata,
so that track identity can be verified across reconnections.

## Acceptance Criteria

1. **Given** a `QueueEntry` struct is constructed from MPD's `playlistinfo` or `plchanges` response
   **When** the response includes `file_size:` and `mtime:` fields
   **Then** these fields are parsed into `QueueEntry.file_size: Option<u64>` and `QueueEntry.mtime: Option<u64>`
   **And** if either field is missing from the MPD response, it defaults to `None`

2. **Given** the `QueueEntry` struct
   **When** it is serialized or cloned
   **Then** `file_size` and `mtime` are preserved (derive Clone handles this automatically)

## Tasks / Subtasks

- [ ] Add `file_size: Option<u64>` and `mtime: Option<u64>` fields to `QueueEntry` struct (AC: #1)
- [ ] Update `MpdAdapter::parse_queue_response()` to extract `file_size:` and `mtime:` lines from MPD response (AC: #1)
- [ ] Verify all QueueEntry construction sites populate the new fields (AC: #1)
- [ ] Add test: verify queue response parsing extracts file_size and mtime (AC: #2)

## Dev Notes

- MPD's `playlistinfo` response includes optional `file_size:` and `mtime:` lines per entry
- Current `QueueEntry` struct at `src/mpd/mod.rs:188` has fields: `position, id, title, artist, album, duration, file`
- Add: `pub file_size: Option<u64>, pub mtime: Option<u64>`
- The parser at `src/mpd/mod.rs:775` (`parse_queue_response`) needs to handle these new key-value pairs
- Both fields are optional in MPD responses — default to `None` when absent
- The mtime from MPD is the modification time MPD recorded when it scanned the file, NOT the filesystem mtime

### References

- [Source: architecture.md#858] — Track Identity ADR
- [Source: src/mpd/mod.rs:188] — QueueEntry struct definition
- [Source: src/mpd/mod.rs:775] — parse_queue_response function

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Completion Notes List

### File List
- src/mpd/mod.rs
- src/mpd/state_machine.rs
