# Story 40.3: Wire LayoutProfile Values into Layout Code

Status: done

## Story

As a user importing a layout profile,
I want the imported layout values (split ratio, rail width, proportions) to take effect immediately,
So that importing a profile changes the actual UI layout, not just the saved config.

## Acceptance Criteria

1. **Given** the user imports a layout profile JSON with custom split_ratio, rail_width, and proportions
   **When** the import succeeds
   **Then** the layout of the UI updates immediately to reflect the imported values
   **And** the split ratio, rail width, and internal proportions in both album mode and folder mode use the imported values
   **And** no restart is required

2. **Given** the layout profile values are stored in the Config struct
   **When** the UI layout is initially configured or the window is resized
   **Then** the UI reads `config.layout_profile.split_ratio` for the left/right pane split
   **And** reads `config.layout_profile.rail_width_min`/`rail_width_max` for the right rail clamp
   **And** reads `config.layout_profile.album_mode_proportions` for the album mode right-rail sections
   **And** reads `config.layout_profile.folder_mode_proportions` for the folder mode right-rail sections

3. **Given** `layout_profile` is `None` in the loaded config
   **When** the UI layout is set up
   **Then** the default hardcoded values are used (split 0.7, rail 320-420px, album 40/20/40, folder 55/45)
   **And** no layout values crash or produce invalid geometry

4. **Given** the user resizes the window or changes settings
   **When** layout recomputation occurs
   **Then** the layout code reads from the same config-backed values (not a separate hardcoded copy)

## Tasks / Subtasks

- [ ] Audit all hardcoded layout values in `src/ui/mod.rs` (split ratio, rail width min/max, album/folder proportions) (AC: 1, 2)
- [ ] Create a `layout_values()` helper that reads from `config.layout_profile` with `unwrap_or_default()` fallback (AC: 3)
- [ ] Replace hardcoded `Paned::set_position()` split ratio with config-driven value (AC: 2)
- [ ] Replace hardcoded right-rail width clamping with config-driven `rail_width_min`/`rail_width_max` (AC: 2)
- [ ] Replace hardcoded right-rail internal proportions with config-driven album/folder mode values (AC: 2)
- [ ] Wire import handler in `ui/mod.rs` to trigger layout recomputation after `config.import_layout_profile()` (AC: 1)
- [ ] Ensure layout updates persist (no regression on settings save) (AC: 1)
- [ ] Test: import profile with split_ratio=0.5, verify split moves to 50/50 (AC: 1)

## Dev Notes

- Currently `LayoutProfile` fields (`split_ratio`, `rail_width_min`, `rail_width_max`, `album_mode_proportions`, `folder_mode_proportions`) are defined in `src/config/mod.rs` and persisted via export/import, but are never read by `src/ui/mod.rs`.
- The fix requires identifying all places in `src/ui/mod.rs` where layout geometry is hardcoded and replacing them with reads from `config.layout_profile`.
- Layout update on import: the import handler in `ui/mod.rs` (line ~1121) already calls `config.import_layout_profile()`. After the import, trigger layout recomputation.
- Key layout values to wire: (a) `Paned::set_position()` for the split ratio, (b) `set_size_request()` or CSS for rail width clamping, (c) fractional allocations for right-rail internal paned widgets.
- No new config values needed — the existing `LayoutProfile` struct already contains all required fields.
- This is a purely structural change; no new UI or settings dialog modifications needed.

### References

- Source: `_bmad-output/planning-artifacts/epics.md` Epic 40, Story 40.3
- Source: `src/config/mod.rs` — LayoutProfile struct and Default impl
- Source: `src/ui/mod.rs` — Layout initialization (Paned split, rail width, proportions)
- ADR: `_bmad-output/planning-artifacts/architecture.md` §443 (Grid Layout — coordinate-based album grid), §419 (Layout & Responsive Architecture)
