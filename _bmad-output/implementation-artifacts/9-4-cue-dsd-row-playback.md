# Story 9.4: CUE/DSD Row Playback

Status: ready-for-dev

## Story

As a folder-mode user,
I want clicking a CUE or DSD summary row to play the associated tracks,
so that I can listen to cue sheet albums or DSD albums directly from the normalized view.

## Acceptance Criteria

1. **Given** a CUE summary row is displayed in the folder tree
   **When** the user clicks or activates the row
   **Then** all audio tracks associated with the cue sheet in that directory are added to the queue and playback starts

2. **Given** a DSD summary row is displayed
   **When** the user clicks or activates the row
   **Then** all DSD tracks (.dsf/.dff) in that directory are added to the queue and playback starts

3. **Given** the user clicks a CUE or DSD row
   **When** playback is already active
   **Then** the current queue is cleared and replaced with the CUE/DSD tracks (same behavior as `PlayFile`)

## Tasks / Subtasks

- [ ] (AC: #1, #2) Modify CUE and DSD summary row widget names for identification in `row_activated` handler (AC: #1, #2)
  - [ ] Give CUE summary row `widget_name("cue:")` (or similar unique prefix)
  - [ ] Give DSD summary row `widget_name("dsd:")` (or similar unique prefix)
- [ ] (AC: #1, #2) Handle CUE/DSD activation in `row_activated` — clear queue, add all associated tracks, play first (AC: #1, #2)
  - [ ] In `set_entries()`, collect the URIs of cue-associated audio files and DSD files at the time the rows are built
  - [ ] Store the URIs so the `row_activated` handler can access them
  - [ ] On activation: send `Clear` → `Add(uri)` for each track → `PlayPosition(0)`
- [ ] (AC: #3) Verify existing behavior is preserved — regular file rows, directory rows, and `..` row still work
- [ ] (AC: #1, #2, #3) Build and run tests: `cargo build` and `cargo test`

## Dev Notes

### Current state (as of 2026-04-29)

The CUE and DSD summary rows are created in `FolderBrowser::set_entries()` (`src/ui/widgets/folder_tree.rs`, lines 186-211). Both rows have `hbox.set_widget_name("")` and CSS class `dir-entry`. The `row_activated` handler (lines 42-61) treats them as directory entries, navigating to the current path — effectively a no-op. This matches the deferred-work.md entry: "Cue/DSD rows clickable but navigate to same directory (no-op)."

### Required changes

All changes are in `src/ui/widgets/folder_tree.rs`:

1. **Give CUE/DSD rows unique widget names** so the `row_activated` handler can distinguish them:
   - CUE row: `hbox.set_widget_name("cue:")` or `hbox.set_css_classes(&["cue-entry"])`
   - DSD row: `hbox.set_widget_name("dsd:")` or `hbox.set_css_classes(&["dsd-entry"])`

2. **Store the file URIs** for the associated tracks. The CUE row's associated audio files are the audio files hidden by cue normalization (lines 159-168). The DSD row's associated files are the `.dsf`/`.dff` files that were collapsed (line 170-172). Options:
   - Store them in a `HashMap<String, Vec<String>>` field on `FolderBrowser` keyed by `"cue:"` or `"dsd:"`
   - Or capture the `cmd_tx` and send commands directly when building the CUE/DSD rows using `connect_row_activated` on the individual rows

3. **Handle in `row_activated`** by matching `widget_name` starts with `"cue:"` or `"dsd:"`:
   - Send `MpdCommand::Clear`
   - Send `MpdCommand::Add(uri)` for each track
   - Send `MpdCommand::PlayPosition(0)`

### Implementation approaches for file URI storage

**Option A (recommended — minimal change):** Store the CUE/DSD track URIs in a field on `FolderBrowser`:
```rust
// Field
cue_tracks: Rc<RefCell<Vec<String>>>,
dsd_tracks: Rc<RefCell<Vec<String>>>,
```
Populate in `set_entries()` by tracking which files would be hidden by normalization. Reference in `row_activated` when a CUE/DSD row is clicked.

**Option B (alternative):** Connect per-row activation handlers directly when building rows in `set_entries()`. Use `connect_row_activated` on the individual `ListBoxRow` (but this conflicts with the existing `list.connect_row_activated`). Instead, use `connect_activated` on the row itself via GTK's `Activatable` trait.

**Option C (simplest):** Store the current entries list in `FolderBrowser` and filter it at activation time:
```rust
// In FolderBrowser:
entries_cache: Rc<RefCell<Vec<DirEntry>>>,
```
When a CUE/DSD row is activated, re-compute which files belong to it from the cached entries.

### MPD command pattern

The existing `MpdCommand::PlayFile` handler (state_machine.rs line 380-384) clears the queue, adds one file, plays position 0. For CUE/DSD multi-file playback, the sequence is:
```
clear
add "file1. flac"
add "file2. flac"
play 0
```
This can be sent as individual commands from `row_activated` via the existing `cmd_tx`.

### Source files to touch

- `src/ui/widgets/folder_tree.rs` — primary implementation changes
- No MPD adapter changes needed (reuses existing commands)
- No state machine changes needed

### Testing

- `cargo test` must pass
- Manually verify in a folder with .cue + split tracks: clicking [CUE] row starts playback of all split tracks
- Manually verify in a folder with .dsf files: clicking [DSD] row starts playback of all DSD tracks
- Verify existing behavior: clicking file rows still plays individual files, clicking directory rows navigates, `..` goes up

### References

- [Source: `src/ui/widgets/folder_tree.rs`] — lines 42-61 (row_activated), 186-211 (CUE/DSD row creation)
- [Source: `src/mpd/state_machine.rs`] — line 26 (MpdCommand enum), lines 380-384 (PlayFile handler)
- [Source: `_bmad-output/implementation-artifacts/deferred-work.md`] — "Cue/DSD rows clickable but navigate to same directory (no-op)"
- [Source: `_bmad-output/planning-artifacts/epics.md`] — Story 9.4 acceptance criteria
- [Source: `_bmad-output/planning-artifacts/architecture.md`] — §Folder Normalization Contract (lines 1090-1107)

## Dev Agent Record

### Agent Model Used

Claude Code (deepseek-v4-flash)

### Debug Log References

### Completion Notes List

### File List
- src/ui/widgets/folder_tree.rs
