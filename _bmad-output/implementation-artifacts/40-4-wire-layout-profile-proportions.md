# Story 40.4: Wire LayoutProfile Mode Proportions into Layout Code

Status: out-of-scope

## Story

As a user importing a layout profile with custom mode proportions,
I want the imported album mode and folder mode proportions to take effect immediately,
So that the right-rail internal sections resize to match my saved preferences.

## Acceptance Criteria

1. **Given** the user imports a layout profile JSON with custom `album_mode_proportions` and `folder_mode_proportions`
   **When** the import succeeds
   **Then** the album mode right-rail sections (now playing, current album, queue) resize to match the imported proportions
   **And** the folder mode right-rail sections (now playing, queue) resize to match the imported proportions
   **And** no restart is required

2. **Given** the layout profile has `album_mode_proportions: [0.5, 0.2, 0.3]` and `folder_mode_proportions: [0.6, 0.4]`
   **When** the UI sets up the right-rail paned widgets on mode switch
   **Then** the first paned in album mode allocates 50% to now-playing, 20% to current album, 30% to queue
   **And** the first paned in folder mode allocates 60% to now-playing, 40% to queue

3. **Given** `layout_profile` is `None` in the loaded config (no profile imported)
   **When** the UI layout is set up
   **Then** the default hardcoded proportions are used (album `[0.4, 0.2, 0.4]`, folder `[0.55, 0.45]`)
   **And** no layout values crash or produce invalid geometry

## Tasks / Subtasks

- [ ] Audit hardcoded right-rail internal proportions in `src/ui/mod.rs` (album: now-playing/current-album/queue paned; folder: now-playing/queue paned) (AC: 2)
- [ ] Read `config.layout_profile.layout.album_mode_proportions` at startup during panel initialization, fallback to defaults when profile is None (AC: 3)
- [ ] Read `config.layout_profile.layout.folder_mode_proportions` at startup during panel initialization, fallback to defaults when profile is None (AC: 3)
- [ ] Apply proportions when mode switches between Album/Folder (AC: 2)
- [ ] Wire import handler in `ui/mod.rs` to trigger proportion recomputation after `config.import_layout_profile()` (AC: 1)
- [ ] Ensure proportion updates persist (no regression on settings save) (AC: 1)
- [ ] Test: import profile with album_mode_proportions=[0.5, 0.2, 0.3], verify paned positions change

## Dev Notes

- `src/config/mod.rs` defines `LayoutProfile { layout: LayoutSettings { album_mode_proportions, folder_mode_proportions, ... } }` with defaults `[0.4, 0.2, 0.4]` and `[0.55, 0.45]`.
- These fields are currently exported/imported in JSON but NEVER read by `src/ui/mod.rs` layout code.
- The right-rail internal proportions are currently hardcoded in the startup layout and mode-switch code paths (`src/ui/mod.rs`).
- Unlike `split_ratio` and `rail_width_*` (consumed in story 40.3), proportions have NO live-update path during mode switch or resize.
- No new config values needed — the existing `LayoutSettings` struct already contains all required fields.
- The import handler in `src/ui/mod.rs` (~line 1138) calls `config.import_layout_profile()` and already updates split_ratio/rail_width via `irwmin.set()` / `irwmax.set()` / `isratio.set()`. The same pattern must be followed for proportions — either adding new Rc<Cell> trackers or triggering a full layout recomputation.
- Existing startup code (~line 400-411 in `src/ui/mod.rs`) reads rail_width_min/max and split_ratio from `cfg.layout_profile.as_ref()` — extend this to also read and apply proportions.
- ADR reference: `architecture.md` §419 (Layout & Responsive Architecture)

### References

- Source: `_bmad-output/planning-artifacts/epics.md` Epic 40, Story 40.4
- Source: `src/config/mod.rs` — LayoutProfile/LayoutSettings struct and Default impl
- Source: `src/ui/mod.rs` — Layout initialization (Paned split, rail width, proportions), import handler
- ADR: `_bmad-output/planning-artifacts/architecture.md` §419 (Layout & Responsive Architecture)
