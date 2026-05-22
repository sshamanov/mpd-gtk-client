# Story 43-6: Create Centralized strings.rs Module

## Status: done

## Context

Architecture.md i18n ADR (lines 1325-1328) specifies all user-facing strings centralized in `strings.rs` as constants or simple functions. Currently strings are inline across 8+ source files.

## Tasks

### Task 1: Create `src/strings.rs`
Organize by source category:
- **Constants:** mode tooltips, search placeholders, window title, status labels, connection messages, button tooltips (play/add/queue), context menu labels ("Play Now", "Play Next", "Add to Queue"), badge text ("CUE", "DSD", "Cue Sheet Album", "DSD Album"), notification messages, toast messages, action descriptions for keybindings
- **Functions:** `cap_note(showing, total) -> String`, `memory_warning(rss_mb) -> String`, `mpd_error(msg) -> String`, `desktop_entry(exe_path) -> String`, `usage_text() -> &'static str`, `shortcut_list() -> &'static [(&'static str, &'static str)]`

### Task 2: Add `pub mod strings;` to `src/lib.rs`

### Task 3: Replace inline strings across codebase
Update imports and replace strings in:
- `src/main.rs` — usage text, error messages
- `src/config/mod.rs` — desktop entry template
- `src/ui/mod.rs` — labels, placeholders, titles, cap notes, memory warnings
- `src/ui/widgets/album_cover_cell.rs` — button tooltips, context menu
- `src/ui/widgets/folder_tree.rs` — badge text, labels
- `src/keybindings.rs` — action_description strings
- `src/notifications/router.rs` — notification titles
- `src/mpd/state_machine.rs` — toast messages

### Task 4: Verify no bare user-facing strings remain
```bash
grep -rn '"MPD Client\|"Search\|"Play Now\|"Add to queue\|"Connecting\|"No albums' src/ --include='*.rs'
```

## Acceptance Criteria
1. `strings.rs` contains all user-facing strings as constants/functions
2. All 8 files import from `strings.rs` instead of inline literals
3. `cargo build` passes
4. `cargo test` passes
5. Manual test: no regressions in UI text display
