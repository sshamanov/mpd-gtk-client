# Story 3.3: Context Menu & Remove from Queue

Status: done

## Story

As a user,
I want to right-click queue items for actions and press Delete to remove them,
so that I can manage my queue without reaching for the mouse every time.

## Acceptance Criteria

1. **Right-click context menu** — Right-clicking a queue row shows a popover with:
   - "Play Now" — plays the track
   - "Remove" — removes the track from queue
   - Popover appears at cursor position, dismisses on click outside

2. **Delete key removes selected queue item**

3. **`cargo test` passes**

## Tasks / Subtasks

- [x] Task 1: Right-click popover with Play Now + Remove buttons on each queue row [ui/mod.rs]
- [x] Task 2: Delete key removes selected queue item via EventControllerKey [ui/mod.rs]
- [x] Task 3: Verify no regressions — cargo build, test, clippy pass

## Dev Notes

### GTK4 Popover Pattern

```rust
let popover = gtk4::Popover::new();
popover.set_parent(Some(&row));
let box = Box::new(Orientation::Vertical, 0);
// add buttons...
popover.set_child(Some(&box));
popover.popup();
```

### Delete Key

```rust
key_ctrl.connect_key_pressed(move |_, key, _, _| {
    if key == gdk::Key::Delete {
        // send MpdCommand::DeleteId(id)
    }
});
```

### What NOT to Do
- Do NOT implement drag-and-drop reorder (3-4)

## Dev Agent Record

### Completion Notes List

- ✅ Right-click Popover with Play Now/Remove buttons on each queue row
- ✅ Delete key removes selected queue item (item_ids HashMap for position→ID mapping)
- ✅ GestureClick button=3 for right-click, button=1 for double-click

### File List

- `src/ui/mod.rs` — MODIFIED: popover context menu, Delete key handler, EventControllerKey, HashMap item_ids
