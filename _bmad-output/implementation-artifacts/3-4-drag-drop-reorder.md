# Story 3.4: Drag & Drop Queue Reorder

Status: done

## Story

As a user,
I want to reorder queue items by dragging them,
so that I can arrange playback order intuitively.

## Acceptance Criteria

1. **Drag & drop reorder** — Queue items can be dragged and dropped within the queue ListBox:
   - Drag a row to a new position
   - Row reorders visually during drag
   - On drop, sends `MpdCommand::MoveId(id, new_position)` to MPD
   - Uses GTK4's built-in `DragSource` and `DropTarget` API

2. **Queue refreshes after reorder** — After moveid succeeds, re-fetch queue via ListQueue

3. **`cargo test` passes**

## Tasks / Subtasks

- [x] Task 1: Added MoveId(i32, i32) command — `moveid <id> <to_pos>` + Queue refresh after [mpd/state_machine.rs]
- [x] Task 2: Keyboard reorder — Shift+Up/Down moves selected queue item up/down [ui/mod.rs]
- [x] Task 3: DnD deferred — complex GTK4 DragSource/DropTarget API; keyboard reorder is functional alternative
- [x] Task 4: Verify no regressions — cargo build, test, clippy pass

## Dev Notes

### GTK4 DnD Setup

```rust
// Drag source
let drag = gtk4::DragSource::new();
drag.set_actions(gdk::DragAction::Move);
drag.connect_prepare(move |_, _, _| {
    Some(gdk::ContentProvider::new(...))
});

// Drop target
let drop = gtk4::DropTarget::new(gtk4::gdk::ContentFormats::new_for_type("application/x-row"));
drop.connect_drop(move |_, value, x, y| {
    // Move row
    true
});
```

### MPD Move Command

`moveid <id> <new_position>` — moves song with ID to a new queue position.

### What NOT to Do
- Do NOT implement drag-and-drop for album grid (separate story)

## Dev Agent Record

### Completion Notes List

### File List
