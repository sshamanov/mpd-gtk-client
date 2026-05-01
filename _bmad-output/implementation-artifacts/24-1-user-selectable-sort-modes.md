# Story 24.1: User-Selectable Sort Modes

Status: done

## Story

As a user browsing by album grid,
I want to choose between sort modes (By Artist, By Year, By Album Name),
so that I can organize the library in the way that best suits my current task.

## Acceptance Criteria

1. **Sort mode selector in Album Mode**
   - **Given** the user is in Album Mode
   - **When** the user clicks a sort dropdown/button
   - **Then** options are shown: "Artist" (default), "Year (newest)", "Year (oldest)", "Album Name"
   - **And** the selected sort mode is applied immediately to the album grid

2. **Sort implementation**
   - **Given** a sort mode is selected
   - **When** the grid repopulates
   - **Then** albums are sorted client-side from the cached album list (no MPD round-trip)
   - **And** the sort is stable (preserves MPD order for ties)

3. **Persistence**
   - **Given** the user selects a sort mode
   - **When** the mode is changed
   - **Then** the preference is saved for the session
   - **And** the sort mode is not persisted across restarts (follows existing ephemeral browsing state pattern)

4. **Coexistence with grouped views**
   - **Given** a grouped view is active (Artists/Years/Genres)
   - **When** a sort mode is selected
   - **Then** the sort applies within each group
   - **And** the group headers remain the primary organizational structure

## References
- [Source: architecture.md §1131] Browsing Sort Architecture ADR
- [Source: src/ui/mod.rs] Album grid and grouped views setup
- [Source: src/search/mod.rs] Existing album list cache pattern

## File List
- `src/ui/mod.rs` — Sort mode selector UI, sort logic
