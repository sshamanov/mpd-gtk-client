# Story 3.1: Queue Display

Status: done

## Story

As a user,
I want to see the current playback queue with track information,
so that I know what's playing next and what has already played.

## Acceptance Criteria

1. **Queue polling** — On a timer (every 30s), fetch the current MPD queue:
   - Add `MpdCommand::ListQueue` → `MpdEvent::Queue(Vec<QueueItem>)`
   - `MpdAdapter::list_queue()` using MPD `playlistinfo` command
   - Queue items include position, track title, artist, duration

2. **Queue display in right rail** — Below the now-playing section, show:
   - Current position indicator (highlighted row)
   - Track title, artist, duration per item
   - Scrollable list (reuse ListBox or ScrolledWindow)
   - Shows in both Album and Folder modes

3. **Initial queue load on connect** — Fetch queue on startup and on reconnect

4. **`cargo test` passes**

## Tasks / Subtasks

- [x] Task 1: Add list_queue to MpdAdapter — playlistinfo parser with QueueEntry struct [mpd/mod.rs]
- [x] Task 2: Add ListQueue command + Queue event to state machine [mpd/state_machine.rs]
- [x] Task 3: Queue display in right rail — ListBox with title + duration, current position highlight [ui/mod.rs]
- [x] Task 4: Queue loads on startup + reconnect + 30s polling timer via glib::timeout_add [ui/mod.rs]
- [x] Task 5: Verify no regressions — cargo build, test, clippy pass

## Dev Notes

### MPD playlistinfo

`playlistinfo` returns all tracks in the queue:
```
file: path
Title: Name
Artist: Name
Album: Album
Duration: 245.0
Pos: 0
Id: 123
```

### QueueItem struct

Already exists in `src/mpd/mod.rs`:
```rust
pub struct QueueItem {
    pub position: usize,
    pub track_id: String,
    pub album_id: String,
}
```

May need to extend with title/artist for display.

### What NOT to Do
- Do NOT implement drag-and-drop reorder (Epic 3, later story)
- Do NOT implement play now/next buttons (3-2)
- Do NOT modify album grid or folder tree

### References
- [Source: epics.md#Epic 3] — queue management

### Review Findings

#### Patch Findings

- [x] [Review][Patch] Current position highlight — fixed: compares against `current_song_pos` (Cell<Option<i32>>) updated from StateChanged events [ui/mod.rs]
- [x] [Review][Patch] Artist not displayed — fixed: added artist label below title in queue row [ui/mod.rs]
- [x] [Review][Patch] `.queue-current` CSS rule added — highlight visible with theme_selected_bg_color [ui/mod.rs]

#### Deferred

- [x] [Review][Defer] Large queue (1000+ items) — acceptable for v1, ListBox handles moderate sizes
- [x] [Review][Defer] No CSS for `.queue-header` — cosmetic, acceptable for v1

## Dev Agent Record

### Completion Notes List

- ✅ QueueEntry struct with position, id, title, artist, album, duration, file
- ✅ list_queue() via MPD playlistinfo command with stateful line parser
- ✅ MpdCommand::ListQueue + MpdEvent::Queue(Vec<QueueEntry>)
- ✅ Queue display ListBox in right rail below now-playing
- ✅ 30s polling timer via glib::timeout_add_local

### File List

- `src/mpd/mod.rs` — MODIFIED: added QueueEntry struct, list_queue() method
- `src/mpd/state_machine.rs` — MODIFIED: added ListQueue command + Queue event + handler
- `src/ui/mod.rs` — MODIFIED: queue ListBox widget in right rail, Queue event handler, startup + 30s timer
