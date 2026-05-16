# Story 42.1: Visual Indicator for Normalized Directory Entries

Status: done

## Story

As a user browsing the folder tree,
I want to see a visual indicator on normalized entries (cue sheets, DSD folders),
So that I can distinguish a collapsed multi-file entry from a regular single-track file.

## Acceptance Criteria

1. **Given** the folder tree contains a CUE sheet directory that has been normalized into a single `CueSummary` entry
   **When** the entry is rendered in the folder tree
   **Then** it shows a subtle visual indicator (e.g., dimmed text label "CUE" or a distinct icon/glyph)
   **And** the indicator is discoverable but non-intrusive

2. **Given** the folder tree contains a DSD folder that has been normalized into a single album entry
   **When** the entry is rendered in the folder tree
   **Then** it shows a visual indicator (e.g., dimmed text label "DSD" or distinct icon/glyph)

3. **Given** a regular file entry (not normalized)
   **When** rendered in the folder tree
   **Then** it has NO normalized indicator

4. **Given** the user hovers or selects a normalized entry
   **When** the entry has focus
   **Then** a tooltip or expanded label explains the normalization (e.g., "Cue sheet — 7 tracks" or "DSD folder — 1 album")

## Tasks / Subtasks

- [x] Add a CSS class `.normalized-badge` for styling the indicator text (small, dimmed, right-aligned) (AC: 1, 2)
- [x] In the folder tree row renderer, detect CUE sheet dir and append a "CUE" label with `.normalized-badge` class (AC: 1)
- [x] In the folder tree row renderer, detect DSD files and append a "DSD" label with `.normalized-badge` class (AC: 2)
- [x] Ensure regular file entries have no badge (AC: 3)
- [x] Add tooltip for normalized entries showing expanded description (AC: 4)
- [x] Test: build and cargo test pass (22 tests)

## Dev Agent Record

### Implementation Plan
- Added `.normalized-badge` CSS class in `src/ui/style.css` (dimmed, 0.8em, alpha 0.45)
- Enhanced `cue_summary_row()` to use subtler "CUE" badge with `.normalized-badge` class and tooltip showing track count
- Added DSD detection in `add_children()` and `render_dir()` 
- Created `dsd_summary_row()` with "DSD" badge, tooltip showing file count
- Added DSD track URI storage (`dsd_tracks` map) for grouped playback
- Added `dsd:` prefix handling in row activation and context menu
- Passed `dsd_tracks` through `rebuild_list()`, `render_dir()`, and `rebuild()` call chains

### Completion Notes
- CUE summary rows show "Cue Sheet Album" with dimmed "CUE" badge, tooltip: "Cue sheet — N tracks"
- DSD summary rows show "DSD Album" with dimmed "DSD" badge, tooltip: "DSD folder — N files"
- Regular file entries unchanged — no badge
- All 22 existing tests pass, build clean

## File List
- `src/ui/style.css` — added `.normalized-badge` CSS class
- `src/ui/widgets/folder_tree.rs` — enhanced CUE/DSD summary rows, dsd_tracks map, dsd: prefix support

## Change Log
- feat: add visual indicators (CUE/DSD badges) for normalized folder tree entries with tooltips

## Dev Notes

- `NormalizedEntry` enum in `src/presenters/folder_norm.rs` already has `CueSummary` and `DsdSummary` variants with track/album counts — the indicator rendering is a UI-only change.
- The badge should be a `GtkLabel` appended to the row's right side, not replacing existing metadata or format labels.
- A new CSS class `.normalized-badge` should be added to `src/ui/style.css`.
- Tooltips use GTK4's `set_tooltip_text()` on the row widget.
- Architecture ref: architecture.md §2624-2630 (Elicitation-Driven Refinements — Normalized Entry Visual Indicator)

### References

- Source: `_bmad-output/planning-artifacts/epics.md` Epic 42, Story 42.1
- Source: `_bmad-output/planning-artifacts/architecture.md` §2624-2630 (Elicitation-Driven Refinements)
- Source: `src/presenters/folder_norm.rs` — NormalizedEntry enum (CueSummary, DsdSummary, File)
- Source: `src/ui/mod.rs` — Folder tree row rendering
- Source: `src/ui/style.css` — New .normalized-badge CSS class
