# Story 16.3: Window Geometry Persistence

Status: done

## Story

As a user,
I want the application to remember my window size and position between sessions,
so that I don't have to resize and reposition the window every time I launch it.

## Acceptance Criteria

1. **Geometry saved on shutdown**
   - **Given** the user resizes and repositions the window
   - **When** the application exits normally
   - **Then** the window width, height, and (x, y) position are saved to the config file

2. **Geometry restored on startup**
   - **Given** the application is restarted after a normal exit
   - **When** the window is created
   - **Then** the window geometry from the saved config is applied
   - **And** the window appears at the saved position with the saved size

3. **Off-screen detection**
   - **Given** the saved position is off-screen (e.g., monitor disconnected)
   - **When** the window geometry is restored
   - **Then** GTK4's default window positioning is used as fallback
   - **And** the off-screen position is ignored with a debug log message

4. **No save on crash**
   - **Given** the application crashes
   - **When** it did not exit normally
   - **Then** the previous window geometry is preserved (not overwritten)

## Tasks / Subtasks

- [ ] (AC: 1) Add `window_width`, `window_height`, `window_x`, `window_y` fields to `Config`
- [ ] (AC: 1) Save window geometry on graceful shutdown (not on every resize)
- [ ] (AC: 2) Read and apply saved geometry on window creation
- [ ] (AC: 2) Set window default size from config before `window.present()`
- [ ] (AC: 3) Implement off-screen detection using `GdkDisplay::monitors()`
- [ ] (AC: 4) Ensure geometry is only written on explicit shutdown, not on crashes

## Dev Notes

- **GTK4 API:**
  - `Window::default_width()` / `Window::default_height()` — set before `present()`
  - `Window::get_position()` — returns (x, y) on Wayland/X11
  - `Window::get_default_size()` — returns current size
  - `GdkDisplay::monitors()` — iterate to check if position intersects any monitor

- **Wayland note:** Wayland does not allow clients to set absolute window position. Geometry save still works (position is stored) but restore is best-effort — `GtkWindow::move_()` is ignored by Wayland compositors. Default positioning is used.
- **Save timing:** Write geometry in the shutdown sequence, after the GTK main loop exits but before the process terminates. The existing shutdown code in `main.rs` (after `app.run()`) is the correct place.
- **Geometry struct:** Add `WindowGeometry { width: i32, height: i32, x: i32, y: i32 }` to `Config` as an optional field (default `None`).
- **Off-screen check:** After setting position, verify that the window's rectangle intersects at least one monitor's geometry. If not, reset to GTK4 default positioning.

### Source Files to Touch
- `src/config/mod.rs` — Add window geometry fields
- `src/ui/mod.rs` or `src/main.rs` — Read geometry on window creation, write on shutdown

### Testing
- Unit test for geometry serialization/deserialization
- Unit test for off-screen detection logic
- Manual QA: verify position restore on X11
- Edge cases: multi-monitor, monitor disconnected, Wayland

## References

- [Source: architecture.md §810] Session Persistence & State Restoration — Window geometry
- [Source: epics.md §16] Epic 16: Infrastructure & Code Quality

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- Wayland position restore is best-effort (protocol limitation)
- Geometry saved only on graceful shutdown, never during normal operation

### File List

- `src/config/mod.rs`
- `src/main.rs`
- `src/ui/mod.rs`
