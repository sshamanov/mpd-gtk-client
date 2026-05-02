# Story 27.1: Responsive Right Rail with MultiLayoutView + BottomSheet

Status: backlog

## Story

As a user,
I want the right rail to adapt to window width,
so that on narrow windows the queue/now-playing panel becomes a bottom sheet instead of crowding the layout.

## Acceptance Criteria

1. **MultiLayoutView replaces GtkPaned for the main split**
   - **Given** the window is wide (≥1000px)
   - **When** the UI is laid out
   - **Then** content uses the current side-by-side split (left pane + right rail)
   - **Given** the window is narrow (<800px)
   - **Then** the right rail content becomes a toggleable bottom sheet

2. **BottomSheet for right rail content**
   - **Given** the window is in narrow mode
   - **When** the user clicks a toggle button
   - **Then** the right rail content slides up as an `Adw.BottomSheet`
   - **And** Now Playing, Current Album Tracks, and Queue are accessible in the sheet

3. **Mode-specific content in sheet**
   - **Given** the bottom sheet is open in Album Mode
   - **Then** it shows Now Playing (40%), Current Album Tracks (20%), Queue (40%)
   - **Given** the bottom sheet is open in Folder Mode
   - **Then** it shows Now Playing (55%), Queue (45%)

## References
- [Source: architecture.md §474] libadwaita Integration ADR
- [Source: prd.md Layout] Responsive breakpoints and right rail adaptation
- [Adw.MultiLayoutView docs] https://docs.rs/libadwaita/latest/libadwaita/struct.MultiLayoutView.html
- [Adw.BottomSheet docs] https://docs.rs/libadwaita/latest/libadwaita/struct.BottomSheet.html

## File List
- `src/ui/mod.rs` — Replace Paned with MultiLayoutView, add BottomSheet
