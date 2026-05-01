# Story 18.2: Keyboard Navigation Audit and Completion

Status: done

## Story

As a user who relies on keyboard navigation,
I want to traverse all interactive elements using only the keyboard,
so that I can fully operate the application without a mouse.

## Acceptance Criteria

1. **Given** the application is running
   **When** the user presses Tab/Shift-Tab
   **Then** focus traverses through all major zones in logical order: search bar, album grid/folder tree, queue, transport controls, mode switcher, menu
   **And** no interactive element is unreachable by Tab
   **And** the focus order follows visual left-to-right, top-to-bottom order

2. **Given** focus is in the album grid
   **When** the user presses arrow keys
   **Then** focus moves in 2D (respecting column count and group boundaries)
   **And** Enter/Space activates the focused item (select or play)

3. **Given** focus is in the folder tree
   **When** the user presses Left/Right arrow
   **Then** the focused folder is collapsed/expanded
   **And** Up/Down moves between rows

4. **Given** focus is in the queue
   **When** the user presses Shift+Up/Shift+Down
   **Then** the focused item is moved up/down in the queue

5. **Given** a keyboard shortcut is bound (Ctrl+F, Ctrl+1/2, Space, etc.)
   **When** the shortcut is pressed
   **Then** the corresponding action is triggered regardless of where focus is (global shortcuts)
   **And** no shortcut conflicts with GTK4 built-in widget shortcuts

6. **Given** the audit is complete
   **When** all findings are documented
   **Then** a report lists each missing or broken keyboard interaction
   **And** all identified issues are resolved or documented as known limitations

## Tasks / Subtasks

- [ ] 1. Create keyboard navigation audit checklist
  - [ ] 1.1 List all interactive elements in Album Mode
  - [ ] 1.2 List all interactive elements in Folder Mode
  - [ ] 1.3 List all interactive elements in Settings dialog, context menus
  - [ ] 1.4 Identify elements unreachable by Tab or with broken focus order
- [ ] 2. Fix album grid keyboard navigation (AC: 2)
  - [ ] 2.1 Arrow keys move focus in 2D respecting column count
  - [ ] 2.2 Enter/Space activates selection or play
  - [ ] 2.3 Grouped view headers are reachable but don't break focus flow
- [ ] 3. Fix folder tree keyboard navigation (AC: 3)
  - [ ] 3.1 Left/Right for expand/collapse
  - [ ] 3.2 Up/Down for row selection
  - [ ] 3.3 Enter for play on selected track
- [ ] 4. Fix queue keyboard navigation (AC: 4)
  - [ ] 4.1 Arrow keys for selection
  - [ ] 4.2 Shift+Up/Down for reorder
  - [ ] 4.3 Delete key for removal
- [ ] 5. Verify global shortcuts (AC: 5)
  - [ ] 5.1 No GTK widget shortcut conflicts
  - [ ] 5.2 All shortcuts work regardless of focus location
- [ ] 6. Document and close audit gaps (AC: 6)
  - [ ] 6.1 Known limitations documented
  - [ ] 6.2 All fixable issues resolved

## Dev Notes

- **Focus chain:** GTK4 uses `set_focus_child()` and `set_can_focus()` on containers. Ensure each interactive widget has `set_can_focus(true)` and containers use `set_focus_child()` for tab order.
- **Album grid:** GtkGridView already handles 2D arrow navigation natively. Verify `GridView`'s built-in focus management is active.
- **Folder tree:** Uses custom widget rendering. May need manual `GtkEventControllerKey` for Left/Right expand/collapse handling.
- **Context menus:** `GtkPopoverMenu` handles keyboard navigation internally — verify Tab/Enter work.
- **Hover buttons:** Must be focusable and show visual focus indicator (not just hover). Add `set_can_focus(true)` to hover overlay buttons. Show focus ring via CSS `:focus-visible`.
- **Seekbar:** Arrow keys for fine seek (+/-5s), PageUp/PageDown for coarse (+/-30s). Follow GTK4 `GtkScale` built-in key handling pattern.
- **Accessible names:** Add `set_accessible_label()` to buttons and interactive elements missing labels for screen reader support.

### Project Structure Notes

- `src/ui/mod.rs` — Main UI file with widget setup, focus management
- `src/ui/widgets/album_cover.rs` — Hover buttons over album covers
- `src/ui/widgets/folder_tree.rs` — Folder tree widget keyboard handling
- `src/ui/widgets/toast.rs` — Toast dismiss may need keyboard support
- No new files needed — modifications to existing UI files only

### References

- [Source: _bmad-output/planning-artifacts/prd.md §197] — "keyboard-only navigation, sufficient color contrast (WCAG 2.1 AA)"
- [Source: _bmad-output/planning-artifacts/prd.md §195] — "Comprehensive shortcut support"
- [Source: _bmad-output/planning-artifacts/architecture.md §703-722] — Accessibility Architecture ADR
- [Source: _bmad-output/planning-artifacts/epics.md §Epic 18] — Epic definition

## Dev Agent Record

### Agent Model Used

TBD

### Debug Log References

### Completion Notes List

### File List
- `src/ui/mod.rs` — Focus chain, keyboard handlers
- `src/ui/widgets/album_cover.rs` — Hover button focus
- `src/ui/widgets/folder_tree.rs` — Arrow/Enter key handlers
