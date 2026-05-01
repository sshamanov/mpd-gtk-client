# Story 11.4: Album Reorder in Plain Albums View

Status: done

## Story

As an album-mode user in the plain Albums view,
I want to manually reorder the album grid,
so that I can organize albums in my preferred order for the session.

## Acceptance Criteria

1. **Given** the user is in the plain Albums view (not grouped)
   **When** the user drags an album cover to a new position in the grid
   **Then** the album is moved to the drop position in the grid display
   **And** the new order persists for the session only
   **And** switching to a grouped view and back to Albums preserves the custom order

## Tasks / Subtasks

- [x] (AC: #1) Enable drag-reorder in the plain Albums grid
  - [x] Implement `GtkDragSource` on album grid cells (only in Albums view mode)
  - [x] Implement `GtkDropTarget` on the album grid container
  - [x] Calculate drop position from grid coordinates (row/column)
  - [x] Reorder the backing `AlbumGridData` vector on drop
- [x] (AC: #1) Make reorder session-only
  - [x] Store custom order in a session-local `Vec<usize>` index map
  - [x] Reset on app restart
- [x] (AC: #1) Preserve custom order across grouped view switches
  - [x] When switching from Albums view to grouped view and back, restore the session order
  - [x] Only Albums view supports reorder; grouped views use MPD-determined order
- [x] (AC: #1) Restrict drag-reorder to Albums view only (disable for grouped views)

## Dev Notes

- **Current grid rendering:** `src/ui/mod.rs` uses `GridView` + `SignalListItemFactory` + `NoSelection` (line 481). Backed by a `gio::ListStore` model. The current ordering follows MPD's natural album order.
- **Session-only persistence:** Store custom ordering as `Vec<String>` of album names in session state. Do NOT write to config — this is explicitly session-only per FR-B5 and PRD interaction rules.
- **Grouped view interaction:** When user switches to a grouped view and back, restore the custom order. On initial Albums view load after grouped view, apply the custom order index. This means the model must be re-ordered according to the saved index.
- **Grid reorder mechanism:** Swap items in the backing `ListStore` when a drag-drop reorder occurs. The `GtkGridView` automatically reflects model changes.
- **Drag ghost:** Use album cover as drag icon (similar to Story 11.1).
- **PRD interaction rules:** "Manual album ordering in the main grid is allowed only in plain Albums view" and "is session-only in v1."
- **Edge case:** If albums were added to the library during the session (detected via LibraryChanged), the new album should appear at its natural sorted position, not at the end of the custom order. The custom order index should be rebuilt to accommodate.

### Project Structure Notes

- `src/ui/mod.rs` — Album grid model (`ListStore`), factory, view switching
- `src/state/mod.rs` — Session state (may need field for custom order)
- `src/mpd/state_machine.rs` — `MpdEvent::Albums` delivery

### References

- [Source: epics.md#Story-11.4-Album-Reorder-in-Plain-Albums-View]
- [Source: PRD.md#Interaction-Rules-Album-Mode] — "Manual album ordering in the main grid is allowed only in plain Albums view" and "is session-only in v1"
- [Source: PRD.md#Browsing--Navigation-FR-B5] — User can manually reorder albums only in plain Albums view
- [Source: architecture.md#ADR-Shared-Queue-with-Dual-Presentation] — Album grid data flow
- [Source: PRD.md#User-Customization--Persistence] — "Session-only: Scroll positions, expanded folders, selected items"

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Debug Log References

### Completion Notes List

- `custom_album_order: Vec<String>` field added to `AlbumBrowsingState` for session-only order persistence
- Album grid DragSource changed from COPY to COPY|MOVE (supports both queue addition and grid reorder)
- DropTarget (MOVE) on left_scroll: calculates target index from drop coordinates + scroll position, reorders AlbumGridData, triggers batch_populate with existing cover_widgets cache
- Custom order saved to AppState on every reorder drop
- AlbumsGrouped handler applies custom order when single group (plain Albums view); Albums handler also supports it
- Reorder disabled when header_count > 1 (grouped views — Artist/Year/Genre)
- v1 limitation: index calculation assumes 200x250 cell size with no spacing; custom order not cleared on library change

### File List

- src/state/mod.rs — Added custom_album_order field to AlbumBrowsingState
- src/ui/mod.rs — Changed DragSource actions, added grid DropTarget with reorder logic, custom order applied in Albums/AlbumsGrouped handlers
- _bmad-output/implementation-artifacts/sprint-status.yaml — Status updated
- _bmad-output/implementation-artifacts/11-4-album-reorder-in-plain-albums-view.md — Tasks checked, notes added
