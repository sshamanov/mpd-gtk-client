# Story 11.2: Reorder Queue Items via Drag

Status: done

## Story

As a user,
I want to reorder items in the queue by dragging them to new positions,
so that I can arrange the playback order intuitively.

## Acceptance Criteria

1. **Given** the queue has multiple items
   **When** the user drags a queue item and drops it between two other items
   **Then** a `MoveId` command is sent to move the item to the drop position
   **And** a visual drop indicator shows the insertion point during drag
   **And** the queue updates to reflect the new order

## Tasks / Subtasks

- [x] (AC: #1) Implement `GtkDragSource` on queue list rows
  - [x] Set MIME type `application/x-mpd-queue-item` carrying queue position + id
  - [x] Render drag ghost of the queue row
  - [x] Support both Album Mode and Folder Mode queues
- [x] (AC: #1) Implement `GtkDropTarget` on the queue container
  - [x] Detect insertion position from drop y-coordinate relative to rows
  - [x] Show visual drop indicator line between rows during drag-motion
  - [x] On drop: calculate target position, send `MoveId(item_id, target_pos)`
- [x] (AC: #1) Handle edge cases: drop at start, end, or invalid position
- [x] (AC: #1) Verify queue order updates correctly after each reorder

## Dev Notes

- **Current queue rendering:** `src/ui/mod.rs` lines ~1460-1526 render each queue entry as a `ListBoxRow` inside `fc_ql` (ListBox). Queue entries have `position` and `id` fields from `QueueEntry`.
- **GTK4 drag-drop API:** `GtkDragSource` on each row, `GtkDropTarget` on the ListBox container.
- **MIME type:** `application/x-mpd-queue-item` with JSON or structured content carrying `{id: i32, position: i32}`. Use `glib::Bytes` for the drag data.
- **Drop position calculation:** Use drop y-coordinate relative to the ListBox to determine insertion point. Compare against each row's allocation to find "before row N" position.
- **Position mapping:** Queue positions in MPD are 0-based. When inserting before position P, if no track is currently playing, target position is P. If a track is playing and we're inserting after it, adjust accordingly.
- **Existing command:** `MpdCommand::MoveId(id, target_position)` already exists (state_machine.rs line 31) and is used by Shift+Up/Down keyboard reorder.
- **Drop indicator:** Draw a colored horizontal line (e.g., 2px accent-color) between rows. Use `GtkDropTarget::on_motion` to update indicator position. GTK4 does not have a built-in drop indicator — use a thin separator widget or dynamically add a CSS-bordered widget.
- **Currently supported:** Shift+Up/Down reorder works via keyboard. This story adds drag-based reorder.

### Project Structure Notes

- `src/ui/mod.rs` — Queue rendering (row factory), event loop, `MpdCommand::MoveId` usage
- `src/mpd/state_machine.rs` — `MpdCommand::MoveId` handler

### References

- [Source: epics.md#Story-11.2-Reorder-Queue-Items-via-Drag]
- [Source: PRD.md#Interaction-Rules] — Drag reorder queue items
- [Source: PRD.md#Queue-Management-FR-Q3] — Reorder queue items via drag-and-drop
- [Source: architecture.md#ADR-Drag--Drop-Cross-Cutting-Architecture] — Drop position indicator

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

### Completion Notes List

- GtkDragSource on each queue row (ListBoxRow) in Queue handler loop, carries reorder data as "id:0" format string
- GtkDropTarget on queue_list (ListBox) with MOVE action: motion finds target row from y-coordinate using accumulated heights, drop calculates position with same midpoint logic
- Drop indicator: .drop-indicator-row CSS class (3px top border in selected color) added to target row
- queue_stack DropTarget modified to skip colon-formatted strings (reorder drops go to queue_list DropTarget)
- queue_list DropTarget also handles album drops (non-colon strings) by forwarding Add command, preventing regression of story 11-1
- Beyond-last-row: no indicator shown but drop still works (appends to end)

### File List

- src/ui/mod.rs — Added DragSource per queue row, reorder DropTarget on queue_list with indicator, modified queue_stack DropTarget guard, CSS for drop indicator
- _bmad-output/implementation-artifacts/sprint-status.yaml — Status updated to done
- _bmad-output/implementation-artifacts/11-2-reorder-queue-items-via-drag.md — Tasks checked, notes added
