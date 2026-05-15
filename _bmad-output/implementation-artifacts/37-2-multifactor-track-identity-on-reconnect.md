# Story 37.2: Implement Multi-Factor Track Identity Verification on Reconnect

Status: done

## Story

As a user,
I want the queue to correctly identify tracks across reconnections,
so that modified or deleted files are detected and removed from the queue gracefully.

## Acceptance Criteria

1. **Given** the MPD connection drops and reconnects, and the queue is re-fetched
   **When** the state machine reconciles the re-fetched queue against the local snapshot
   **Then** each track's identity is verified using the (file_size, mtime, file) triple
   **And** tracks where file_size or mtime differ are treated as "modified — no match" and removed from the queue
   **And** tracks where all three match are treated as unchanged and kept in place

2. **Given** a track in the queue has no file_size or mtime (MPD did not provide them)
   **When** identity verification runs
   **Then** the fallback uses (file, artist, album, duration) for matching
   **And** if the fallback match fails, the track is removed with a toast notification

3. **Given** a file referenced in the queue no longer exists in MPD's playlist
   **When** the queue is re-fetched
   **Then** the missing track is removed from the local queue
   **And** a toast notification is shown: "Removed N missing track(s) from queue"

## Tasks / Subtasks

- [ ] Lift `local_queue` out of `connected_loop` into `run()` so it survives reconnect cycles (AC: #1, #3)
- [ ] Pass `&mut local_queue` to `connected_loop` instead of initializing `Vec::new()` inside (AC: #1)
- [ ] Add `reconcile_after_reconnect(old_snap: &[QueueEntry], new_queue: Vec<QueueEntry>) -> (Vec<QueueEntry>, usize)` function (AC: #1, #2)
  - Primary match: `(file_size, mtime, file)` triple — all three must match
  - Fallback match (no file_size/mtime): `(file, artist, album, duration)` — all four must match
  - Unmatched entries in new queue are removed from result
- [ ] In `sync_queue()`, after full `list_queue()` when `local_queue` is non-empty (reconnect detected): call reconcile, emit toast for removed tracks (AC: #1, #2, #3)
- [ ] In `connected_loop`, after initial full sync at line 540: reconcile if queue was preserved (AC: #1)
- [ ] Ensure new entries (no match in old snapshot) are kept (not false-positively removed) (AC: #1)
- [ ] Test: verify reconnect reconciliation removes modified tracks (mock MPD)

## Dev Notes

- `local_queue` currently initialized as `Vec::new()` at state_machine.rs:504 inside `connected_loop` — lost on every reconnect
- Must be lifted to `run()` (the caller at line ~300) so it persists across `connected_loop()` invocations
- `last_playlist_version` is already `None` on reconnect entry — signals that a full sync will happen
- Reconciliation trigger: `local_queue.is_empty()` is false when reconnect preserves old snapshot → full sync path in `sync_queue()` should reconcile
- On initial startup, `local_queue` IS empty — no reconciliation needed (nothing to compare)
- Identity matching per ADR §858: primary `(file_size, mtime)` + path, metadata fallback `(artist, album, duration)` + path
- MPD's `file_size` and `mtime` are MPD's own metadata (from when MPD scanned the files), NOT filesystem stat — correct source

### Identity Match Logic

```
fn identity_match(old: &QueueEntry, new: &QueueEntry) -> bool {
    // Path must always match — different file = different track
    if old.file != new.file { return false; }

    // Primary: exact file_size + mtime match (rsync-style)
    if let (Some(old_sz), Some(new_sz), Some(old_mt), Some(new_mt)) =
        (old.file_size, new.file_size, old.mtime, new.mtime)
    {
        return old_sz == new_sz && old_mt == new_mt;
    }

    // Fallback: metadata match when file metadata unavailable
    old.title == new.title
        && old.artist == new.artist
        && old.album == new.album
        && old.duration == new.duration
}
```

### References

- [Source: architecture.md#858] — Track Identity & Queue Synchronization ADR
- [Source: src/mpd/state_machine.rs:490-510] — connected_loop function, local_queue init
- [Source: src/mpd/state_machine.rs:1274-1328] — sync_queue function
- [Source: src/mpd/state_machine.rs:390-430] — run() reconnection loop
- [Source: src/mpd/mod.rs:188] — QueueEntry struct (now with file_size, mtime)

## Dev Agent Record

### Agent Model Used

deepseek-v4-pro

### Completion Notes List

### File List
- src/mpd/state_machine.rs
