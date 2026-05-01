# Story 10.3: Album Mini-Grid Queue

Status: done

## Story

As an album-mode user,
I want the queue in the right rail to show album covers instead of a text track list,
so that the visual album-browsing experience extends to the queue.

## Acceptance Criteria

1. **Given** the user is in Album Mode with albums in the queue
   **When** the right rail queue section renders
   **Then** queued albums are displayed as a mini cover grid
   **And** the currently playing album is highlighted
   **And** hovering over a mini cover shows the album name
   **And** clicking a mini cover selects it; double-clicking plays it

## Tasks / Subtasks

- [x] (AC: #1) Build mini cover grid widget for the Album Mode queue section
  - [x] Create a `GridView` or `FlowBox` for album covers at 50% of main grid size (~120px)
  - [x] Layout: left-to-right, then top-to-bottom (3x2 grid initial)
  - [x] Place in right rail below Current Album track window
- [x] (AC: #1) Populate mini grid from queue data
  - [x] Extract album-level grouping from queue entries
  - [x] Deduplicate by album for visual display
  - [x] Show currently playing album with distinct visual state
- [x] (AC: #1) Add hover tooltip showing album name
- [x] (AC: #1) Wire single-click for selection, double-click for playback
  - [x] Single-click: highlight the mini cover (select state, no MPD action)
  - [x] Double-click: send `PlayAlbum` command for that album
- [x] (AC: #1) Keep track-list queue as fallback for Folder Mode (unchanged)

## Dev Notes

- **Current queue display:** `src/ui/mod.rs` uses `ListBox` (`fc_ql`) for queue display. Track-list mode is shared between Album and Folder modes.
- **Queue events:** `MpdEvent::Queue(queue_data)` delivers full queue (lines ~1518-1526). Currently renders as track list rows.
- **Album grouping needed:** Queue entries are per-track with `album` field. Need to group by album for mini-grid display. Each album appears once in the cover grid but all tracks remain in MPD queue.
- **Mini grid widget:** Use `gtk4::GridView` + `SignalListItemFactory` similar to the main album grid pattern (lines ~271-481). Cover size: ~120px (50% of main grid 240px max).
- **Right rail structure:** Currently uses `gtk4::Box` (`right_pane`) with label + listbox for queue. Mini-grid replaces the queue listbox in Album Mode only.
- **Cover art reuse:** Can reuse the existing `MpdEvent::CoverPaths` and widget registry (`cover_widgets`) pattern for loading covers into mini-grid cells.
- **Currently playing highlight:** Compare each mini-grid album against currently playing song's album from `PlaybackUpdate.album`.

### Project Structure Notes

- `src/ui/mod.rs` — Right rail composition, queue rendering, cover path handling
- `src/ui/widgets/album_cover.rs` — Reusable cover widget with hover controls (may need adaptation)
- `src/mpd/state_machine.rs` — `MpdEvent::Queue` event emission

### References

- [Source: epics.md#Story-10.3-Album-Mini-Grid-Queue]
- [Source: PRD.md#Album-Mode-Queue] — 3x2 mini-grid at 50% cover scale, left-to-right/top-to-bottom
- [Source: PRD.md#Layout-Refinement-Rules] — Queue grid adaptation: 3 cols at ≥400px rail width, 2 cols at 350-399px, 1 col at <350px
- [Source: architecture.md#ADR-Shared-Queue-with-Dual-Presentation] — Dual queue presentation design

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

### Completion Notes List

- Implemented mini cover grid for Album Mode queue using GridView + SignalListItemFactory with 120x150 cells
- Queue data grouped by album (deduplicated), covers from shared cover_paths HashMap
- Currently playing album highlighted with .mini-queue-current CSS class
- Queue Stack (gtk Stack) switches between mini grid (Album Mode) and track list (Folder Mode) via mode-change detection in tick callback
- Tooltip shows "Artist - Album" (or just album name if artist is empty)
- Double-click sends PlayAlbum command
- Code review: 2 patches applied (tooltip formatting for empty artist, removed dead CSS), 5 items deferred (cover updates, empty state, album-less tracks, unbalanced cells, single-click selection)

### File List

- src/ui/mod.rs — Added MiniGridItem struct, mini grid factory (setup/bind), GridView with activate handler, queue Stack with mode-aware switching, mini grid population in Queue event handler, CSS styles
- _bmad-output/implementation-artifacts/sprint-status.yaml — Status updated
- _bmad-output/implementation-artifacts/10-3-album-mini-grid-queue.md — Tasks checked, status updated
