# Story 11.3: Remove Items via Drag-Off

Status: done

## Story

As a user,
I want to remove items from the queue by dragging them off the queue area,
so that I can quickly clean up the queue without context menus.

## Acceptance Criteria

1. **Given** the queue has items
   **When** the user drags a queue item outside the queue area and releases it
   **Then** a `DeleteId` command is sent to remove the item
   **And** visual feedback during drag indicates the item will be removed (e.g., red tint, trash icon)
   **And** the queue updates to remove the item

## Tasks / Subtasks

- [x] (AC: #1) Detect drag-leave from queue area
  - [x] Track pointer position during drag-motion relative to queue container bounds
  - [x] When pointer exits queue bounds, treat as "removal pending"
  - [x] On drop outside queue bounds, send `DeleteId(item_id)`
- [x] (AC: #1) Provide visual feedback during drag-off
  - [x] Apply red tint CSS class to dragged item when outside queue bounds
  - [x] Optionally show trash icon overlay
  - [x] Revert visual if pointer re-enters queue area
- [x] (AC: #1) Verify removal works correctly at queue edges
- [x] (AC: #1) Verify non-removal drop (within queue area) still reorders normally (interoperates with Story 11.2)

## Dev Notes

- **Integration with Story 11.2:** Drag-off removal builds on the same `GtkDragSource` mechanism from Story 11.2. The distinction is where the drop lands: within queue = reorder, outside = remove.
- **Detection logic:** In the `GtkDropTarget::on_drop` handler (or `on_motion` for feedback), check if the drop coordinates fall within the queue container's allocation. If not, treat as removal.
- **Visual feedback:** Use `GtkDropTarget::on_motion` to toggle a CSS class like `.drag-remove` on the drag source widget when pointer is outside queue bounds. GTK4 applies visual changes in real-time during drag.
- **Existing command:** `MpdCommand::DeleteId(item_id)` already exists (state_machine.rs line 30) and is used by context menu Remove and right-click Delete.
- **Edge case:** Drag-off from an empty queue area (only one item) — should still work, removing the last item.
- **Album Mode consideration:** In Album Mode with mini-grid queue (Story 10.3), drag-off from the mini-grid removes that album's tracks from queue. Each mini-grid cover represents one album's worth of tracks.

### Project Structure Notes

- `src/ui/mod.rs` — Queue rendering, drag-drop event handling
- `src/mpd/state_machine.rs` — `MpdCommand::DeleteId` handler

### References

- [Source: epics.md#Story-11.3-Remove-Items-via-Drag-Off]
- [Source: PRD.md#Interaction-Rules] — "Dragging a queued album off the album queue grid removes it from the queue"
- [Source: PRD.md#Queue-Management-FR-Q4] — Remove items from queue via drag-off
- [Source: architecture.md#ADR-Drag--Drop-Cross-Cutting-Architecture] — Unified DndService

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

### Completion Notes List

- Added DropTarget on right_pane with COPY|MOVE actions that handles drag-off removal: parses "id:pos" format → sends DeleteId
- queue_stack DropTarget modified to accept COPY|MOVE and consume reorder drops as safe no-ops (prevents propagation to removal target)
- CSS: .drag-remove-zone (red tint) on right_pane during drag outside queue area
- Empty-string/whitespace guard prevents spurious Add commands
- CSS cleanup on drop for both queue_stack highlight and removal zone

### File List

- src/ui/mod.rs — Added removal DropTarget on right_pane, modified queue_stack DropTarget actions and safety buffer, CSS for .drag-remove-zone
- _bmad-output/implementation-artifacts/sprint-status.yaml — Status updated
- _bmad-output/implementation-artifacts/11-3-remove-items-via-drag-off.md — Tasks checked, notes added
