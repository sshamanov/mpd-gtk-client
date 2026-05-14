# Story 29.4: Presenter Layer

Status: ready-for-dev

## Story

As a developer,
I want a dedicated `src/presenters/` module containing stateless pure projection functions that transform application state into display-ready view models,
so that presentation logic is separated from GTK widget code, testable without a display server, and the UI layer never performs data projection.

## Acceptance Criteria

1. **`src/presenters/` module exists with no GTK imports**
   - Given the presenter module is compiled
   - When `grep -r "gtk4\|gdk4\|gdk_pixbuf" src/presenters/` is run
   - Then no results are returned
   - And `cargo check -p mpd-client` compiles without errors

2. **`presenters::types` module with shared non-GTK types**
   - Given a presenter needs to output display-ready values
   - When `presenters::types` is imported
   - Then it provides: format label strings, CSS class name strings, sort key enums, and small ViewModel structs
   - And no type depends on GTK types

3. **Format badge extraction from `state_machine.rs`**
   - Given `AudioFormat::display_text()` and `format_badge_text()` exist in `src/mpd/state_machine.rs`
   - When the presenter layer is created
   - Then these functions are moved to `src/presenters/format.rs` as pure functions taking string/struct inputs
   - And `state_machine.rs` re-exports or calls the presenter function (not duplicating logic)
   - And `PlaybackDisplay::from_update()` calls the presenter format function instead of inline format building

4. **Grid coordinate mapper extracted from `ui/mod.rs`**
   - Given `reposition()` and grid math constants exist in `src/ui/mod.rs`
   - When the presenter layer is created
   - Then grid column/row calculation, group header placement, and coordinate mapping are extracted into `src/presenters/browse/album_grid.rs`
   - And `reposition()` in the UI calls the presenter's pure coordinate function, then applies GTK `.move_()` calls
   - And `group_caption_for_view()` moves to the presenter
   - And `format_year_badge()` moves to the presenter

5. **Folder normalization strategies extracted**
   - Given CUE/DSD detection logic exists in `src/ui/widgets/folder_tree.rs`
   - When the presenter layer is created
   - Then CUE sheet detection (`has_cue` checks, `ends_with_ci(".cue")`) is extracted into a `FolderNormalizer` trait in `src/presenters/folder_norm.rs`
   - And DSD format detection (`to_lowercase() == "dsd"`) is extracted as a strategy
   - And the folder tree widget imports the normalizer, passing raw `DirEntry` lists and receiving normalized/annotated lists

6. **Backward compatibility**
   - Given all existing tests pass
   - When the presenter extraction is applied
   - Then `cargo test` shows zero regressions
   - And `cargo build` shows zero warnings (excluding pre-existing)
   - And all UI behavior is visually identical

## Technical Requirements

### Module Structure

```
src/
└── presenters/
    ├── mod.rs              # Re-exports public API only
    ├── types.rs            # Shared non-GTK presentation types
    ├── format.rs           # AudioFormat display, format badge, year badge
    ├── browse/
    │   ├── mod.rs          # Re-exports album_grid
    │   └── album_grid.rs   # Grid coordinate mapping, group caption, column math
    └── folder_norm.rs      # FolderNormalizer trait + CUE/DSD strategies
```

### Module Rules (from architecture.md §2453-2459, §2710-2715)

- `src/presenters/` must contain **zero** `gtk4`, `gdk4`, or `gdk_pixbuf` imports
- Presenters are stateless pure functions — no structs with mutable state
- `mod.rs` re-exports only public API, not internal helpers
- `presenters::types` scope: string type aliases, flat enums, small structs (≤3 fields), no GTK types
- Presenters compute *what* to display; widgets decide *how* (widget type, CSS class, layout)

### What Gets Extracted

| Source | Function/Logic | Destination |
|--------|---------------|-------------|
| `src/mpd/state_machine.rs` | `AudioFormat::display_text()` | `src/presenters/format.rs` |
| `src/mpd/state_machine.rs` | `format_badge_text()` | `src/presenters/format.rs` |
| `src/ui/mod.rs` | `format_year_badge()` | `src/presenters/format.rs` |
| `src/ui/mod.rs` | `group_caption_for_view()` | `src/presenters/browse/album_grid.rs` |
| `src/ui/mod.rs` | Column count calc (`width / CELL_SLOT_W`), row/column positioning math | `src/presenters/browse/album_grid.rs` |
| `src/ui/mod.rs` | `CELL_SLOT_W`, `CELL_SLOT_H`, `CAPTION_H` constants | `src/presenters/browse/album_grid.rs` |
| `src/ui/widgets/folder_tree.rs` | CUE detection (`ends_with_ci(".cue")`, `has_cue`) | `src/presenters/folder_norm.rs` |
| `src/ui/widgets/folder_tree.rs` | DSD format detection | `src/presenters/folder_norm.rs` |

### What Stays in Place

- `reposition()` in `src/ui/mod.rs` — GTK `.move_()` calls stay; the coordinate *calculation* is extracted
- `PlaybackDisplay::from_update()` in `src/ui/mod.rs` — delegates format building to presenter
- `cue_summary_row()` in folder_tree — GTK widget construction stays
- `parse_mpd_audio_format()` in `state_machine.rs` — MPD protocol parsing stays at the adapter boundary
- `AudioFormat` struct definition — stays in `state_machine.rs` (MPD data model, not presentation)

### Integration Points

| File | Change |
|------|--------|
| `src/presenters/mod.rs` | NEW — re-exports: `pub mod types; pub mod format; pub mod browse; pub mod folder_norm;` |
| `src/presenters/types.rs` | NEW — `FormatBadge(String)`, `YearBadge(Option<String>)`, `GroupCaption(Vec<String>)`, `GridCell { row: usize, col: usize, x: f64, y: f64 }` |
| `src/presenters/format.rs` | NEW — `display_text(audio: &AudioFormat) -> String`, `format_badge(song: &HashMap<String,String>) -> Option<String>`, `year_badge(year: Option<&str>) -> Option<String>` |
| `src/presenters/browse/mod.rs` | NEW — re-exports `album_grid` |
| `src/presenters/browse/album_grid.rs` | NEW — `grid_layout(cells: usize, width: f64, captions: &[Option<GroupCaption>]) -> Vec<GridCell>`, `group_caption(view_mode, header, meta) -> Option<GroupCaption>` |
| `src/presenters/folder_norm.rs` | NEW — `FolderNormalizer` trait, `CueSheet`, `DsdFolder`, `NormalizedEntry` |
| `src/mpd/state_machine.rs` | Move `display_text()` to `presenters::format`; call from there in `PlaybackUpdate` usage |
| `src/ui/mod.rs` | Import presenters; delegate format/coordinate logic; keep GTK widget operations |
| `src/ui/widgets/folder_tree.rs` | Import `FolderNormalizer`; delegate CUE/DSD detection |
| `src/main.rs` | Add `pub mod presenters;` if lib.rs structure requires |
| `Cargo.toml` | Verify no gtk4/gdk4 leakage into presenters |

### Key Rules

- **Do NOT change behavior** — extraction only, no functional changes
- **Do NOT introduce new abstractions** — extract existing logic as-is, don't redesign
- **Do NOT break tests** — all 84 tests must pass after extraction
- **Presenter functions must compile without GTK** — even if they're never called from non-GTK context
- **Import direction:** `ui/` → `presenters/` → `mpd/` (types only). Never reverse.

## Tasks / Subtasks

- [ ] 1. Create `src/presenters/` module skeleton
  - [ ] 1.1 Create `src/presenters/mod.rs` with module declarations and re-exports
  - [ ] 1.2 Create `src/presenters/types.rs` with `FormatBadge`, `YearBadge`, `GroupCaption`, `GridCell`
  - [ ] 1.3 Register `pub mod presenters;` in `src/main.rs` or `src/lib.rs`
  - [ ] 1.4 Verify `cargo check` compiles cleanly
- [ ] 2. Extract format presentation to `src/presenters/format.rs`
  - [ ] 2.1 Move `AudioFormat::display_text()` implementation to `presenters::format::display_text()`
  - [ ] 2.2 Move `format_badge_text()` to `presenters::format::format_badge(song)`
  - [ ] 2.3 Move `format_year_badge()` to `presenters::format::year_badge()`
  - [ ] 2.4 Update call sites in `state_machine.rs` and `ui/mod.rs` to use presenter functions
  - [ ] 2.5 Verify `cargo build` and `cargo test` pass
- [ ] 3. Extract grid coordinate mapping to `src/presenters/browse/album_grid.rs`
  - [ ] 3.1 Create `src/presenters/browse/mod.rs`
  - [ ] 3.2 Move `CELL_SLOT_W`, `CELL_SLOT_H`, `CAPTION_H` constants
  - [ ] 3.3 Extract `compute_grid_layout(cells, width, captions) -> Vec<GridCell>` from `reposition()`
  - [ ] 3.4 Move `group_caption_for_view()` to presenter, return `GroupCaption` type
  - [ ] 3.5 Refactor `reposition()` to call presenter for coordinates, keep GTK `.move_()` calls
  - [ ] 3.6 Verify grid display is identical (visual check or test)
- [ ] 4. Extract folder normalizer to `src/presenters/folder_norm.rs`
  - [ ] 4.1 Define `FolderNormalizer` trait: `fn normalize(&self, entries: &[DirEntry]) -> Vec<NormalizedEntry>`
  - [ ] 4.2 Implement `CueSheet` normalizer (detect `.cue` files, group associated tracks)
  - [ ] 4.3 Implement `DsdFolder` normalizer (detect DSD format from file extensions)
  - [ ] 4.4 Wire folder_tree widget to use normalizer, keeping widget construction in GTK code
  - [ ] 4.5 Verify folder tree behavior is identical
- [ ] 5. Run full test suite — verify zero regressions, zero new warnings

## Dev Agent Record

### Implementation Plan
(To be filled by dev agent)

### Completion Notes
(To be filled by dev agent)

### Change Log
(To be filled by dev agent)

## References
- [Source: architecture.md §2040-2068] Presenter Location: Flat `presenters/` module structure and file layout
- [Source: architecture.md §2453-2459] Presenter Purity Rule — no GTK imports, `presenters::types` scope limits
- [Source: architecture.md §2710-2715] Presenter/UI boundary — `src/presenters/` pure functions, `src/ui/` GTK widgets
- [Source: architecture.md §1506-1517] Presenter directory structure, cyclic dependency prevention
- [Source: architecture.md §2125-2156] Export enforcement rules and architecture invariants
- [Source: architecture.md §2704-2708] Module import rules — `mpd/` never imports `ui/`/`presenters/`, `presenters/` never imports `mpd/`
- [Source: src/ui/mod.rs:219-256] `reposition()` — grid layout with GTK `.move_()` calls, coordinate math to extract
- [Source: src/ui/mod.rs:262-279] `group_caption_for_view()` — group caption logic to extract
- [Source: src/ui/mod.rs:282-288] `format_year_badge()` — year badge formatting to extract
- [Source: src/mpd/state_machine.rs:102-130] `AudioFormat::display_text()` — format display to extract
- [Source: src/mpd/state_machine.rs:1277-1310] `format_badge_text()` — format badge to extract
- [Source: src/ui/widgets/folder_tree.rs:307-502] CUE/DSD detection — normalization to extract
