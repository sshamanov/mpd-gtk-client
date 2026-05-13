# Story 27.1: Responsive Right Rail with MultiLayoutView + BottomSheet

Status: done

## Story

As a user,
I want the right rail to adapt to window width,
so that on narrow windows the queue/now-playing panel becomes a bottom sheet instead of crowding the layout.

## Acceptance Criteria

1. **MultiLayoutView replaces OverlaySplitView for the main split**
   - **Given** the window is wide (≥800px)
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
   - **Then** it shows Now Playing + Queue
   - **Given** the bottom sheet is open in Folder Mode
   - **Then** it shows Now Playing + Queue

## Tasks/Subtasks

- [x] 1. Replace `adw::OverlaySplitView` with `adw::MultiLayoutView`
- [x] 2. Create "wide" layout (side-by-side with LayoutSlot left/right)
- [x] 3. Create "narrow" layout (BottomSheet with right_pane as sheet content)
- [x] 4. Update Ctrl+B toggle to open/close BottomSheet in narrow mode
- [x] 5. Update bottom panel queue button to toggle BottomSheet
- [x] 6. Update frame clock tick callback for layout-name switching
- [x] 7. Build and test — all 21 tests pass, no warnings

## Dev Agent Record

### Implementation

Replaced `adw::OverlaySplitView` with `adw::MultiLayoutView` using two layouts:
- **"wide" layout**: Horizontal Box with `LayoutSlot("left")` (hexpand) + `LayoutSlot("right")` (320px)
- **"narrow" layout**: `BottomSheet` with `LayoutSlot("left")` as content, `LayoutSlot("right")` as sheet

Layout switching on frame clock tick: `<800px` → narrow, `≥800px` → wide. Initial layout set from saved window geometry to avoid one-frame flash. Ctrl+B and bottom panel queue button toggle `BottomSheet` in narrow mode.

### Review Findings

- [x] [Review][Patch] Stabilize temporary LayoutSlot binding — assigned to `narrow_right_slot` variable [src/ui/mod.rs:1949]
- [x] [Review][Patch] Avoid one-frame flash on narrow startup — set initial layout from `cfg.window_geometry` width [src/ui/mod.rs:1967]
- [x] [Review][Patch] Close BottomSheet when switching to wide layout — prevents stale open state on return to narrow [src/ui/mod.rs:2198]
- [x] [Review][Defer] DropTargets unreachable with sheet closed in narrow mode — same behavior as old hidden sidebar, design choice [src/ui/mod.rs:1773]
- [x] [Review][Defer] CSS `.bottom-panel` border-top may create artifact under sliding BottomSheet — requires visual testing [style.css:92]
- [x] [Review][Defer] BottomSheet overscroll gesture may conflict with album grid scroll — requires visual testing [src/ui/mod.rs:1942]
