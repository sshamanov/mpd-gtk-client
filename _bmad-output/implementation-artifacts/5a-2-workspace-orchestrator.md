# Story 5a.2: Workspace Mode Orchestrator

Status: ready-for-dev

## Story

As a user,
I want mode switching to preserve my browsing context,
so that when I switch between Album and Folder modes, my scroll position and selection are kept.

## Acceptance Criteria

1. **Preserve scroll position** — Scroll positions persist per mode:
   - Album grid scroll position saved in `AppState.album_browsing.scroll_position`
   - Folder tree scroll position saved in `AppState.folder_browsing.scroll_position`
   - Restored on mode switch via `ScrolledWindow::set_vadjustment_value()`

2. **Preserve selection** — Selected album/folder entry restored on mode switch

3. **Hide, don't destroy** — Mode content is hidden via visibility, not destroyed/recreated

4. **`cargo test` passes**

## Tasks / Subtasks

- [ ] Task 1: Save/restore scroll positions on mode switch (AC: 1)
- [ ] Task 2: Save/restore selections (AC: 2)
- [ ] Task 3: Verify no regressions (AC: 4)

## Dev Notes

### Scroll Position Pattern

```rust
// On mode switch OUT:
let adj = scrolled_window.vadjustment();
let pos = adj.value();
app_state.album_browsing.scroll_position.1 = pos;

// On mode switch IN:
adj.set_value(saved_pos);
```

### Current Architecture

Modes already switch via visibility toggling (album_content vs folder_content in left_pane_box). This story adds state persistence on top.

## Dev Agent Record

### Completion Notes List

### File List
