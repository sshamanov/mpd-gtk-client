# Story 2.2: Breadcrumb Navigation & File Playback

Status: review

## Story

As a user,
I want to see where I am in the folder tree with clickable breadcrumbs and play files by clicking them,
so that I can navigate efficiently and start playback from Folder Mode.

## Acceptance Criteria

1. **Clickable breadcrumb bar** — Top of the folder tree shows the current path as clickable segments:
   - Each path component is a clickable label
   - Clicking any segment navigates directly to that directory
   - Separator characters ("/") between segments are not clickable
   - Current directory (last segment) is visually distinct (bold/underlined)

2. **File click plays** — Clicking a file row in the folder tree plays the file:
   - Sends `MpdCommand::PlayFile(path)` to the MPD background thread
   - MPD thread executes `add "<path>"` then `play <position>`
   - Shows feedback via the now-playing display (existing infrastructure)

3. **`cargo test` passes** — All existing tests pass; clippy clean

## Tasks / Subtasks

- [x] Task 1: Add PlayFile command to MPD adapter and state machine (AC: 2)
  - [x] Added `MpdCommand::PlayFile(String)` — clear, add, play in connected_loop
  - [x] Implemented in connected_loop with URI escaping

- [x] Task 2: Build breadcrumb bar (AC: 1)
  - [x] Clickable path segments with GestureClick, last segment bold (breadcrumb-current CSS)
  - [x] Separators between segments, click navigates to segment path

- [x] Task 3: Wire file row activation to PlayFile command (AC: 2)
  - [x] File rows have widget_name "file:{path}" — detected in row_activated, sends PlayFile

- [x] Task 4: Verify no regressions (AC: 3)
  - [x] `cargo build`, `cargo clippy`, `cargo test` pass

## Dev Notes

### Breadcrumb Implementation

Use a horizontal Box of clickable Labels. When path changes, rebuild:
```
[ ~ ] / [ music ] / [ artist ] / [ album ]    ← last is bold
```

### PlayFile Command

`MpdCommand::PlayFile(path)` in connected_loop:
```
clear
add "<path>"
play 0
```

### What NOT to Do
- Do NOT implement full queue management (Epic 3)
- Do NOT implement drag-and-drop (Epic 3)
- Do NOT implement cue sheet normalization — deferred story

### References
- [Source: ux-design-specification-enhanced.md#UX-DR13] — breadcrumb navigation
- [Source: epics.md#Epic 2] — FR-B7

### Review Findings

#### Patch Findings

- [x] [Review][Patch] Row activation — moved widget_name/css_classes from icon label to hbox, now correctly detected [ui/widgets/folder_tree.rs]
- [x] [Review][Patch] DirectoryListing event — added path field (`DirectoryListing(String, Vec<DirEntry>)`), UI uses event path directly [mpd/state_machine.rs, ui/mod.rs]
- [x] [Review][Patch] Missing CSS — added rules for breadcrumb-current (bold), breadcrumb-segment (link color), breadcrumb-root, breadcrumb-sep [ui/mod.rs]
- [x] [Review][Patch] Root "~" breadcrumb — added GestureClick to send ListDirectory("") on click [ui/widgets/folder_tree.rs]

#### Deferred

- [x] [Review][Defer] No lsinfo test coverage — pre-existing from 2-1
- [x] [Review][Defer] File paths with special chars escaping order — pre-existing concern, works for real-world cases
- [x] [Review][Defer] Breadcrumb overflow on deep paths — acceptable for v1

## Dev Agent Record

### Completion Notes List

- ✅ Added `MpdCommand::PlayFile(String)` — clear, add, play in connected_loop
- ✅ Clickable breadcrumb bar with path segment navigation
- ✅ File row activation sends PlayFile (widget_name "file:{path}" convention)
- ✅ `GestureClick` on breadcrumb segments for click handling

### File List

- `src/mpd/state_machine.rs` — MODIFIED: added `PlayFile` command variant + handler
- `src/ui/widgets/folder_tree.rs` — MODIFIED: breadcrumb bar (clickable segments), file row activation, PlayFile wiring
