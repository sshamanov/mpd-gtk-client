# Story 2.3: Cue Sheet & DSD Folder Detection

Status: review

## Story

As a user browsing by folder,
I want cue sheets and DSD folders to be displayed as single album entries,
so that the folder view is clean and matches how I think about my music.

## Acceptance Criteria

1. **Cue sheet detection** — When browsing, folders containing `.cue` files are collapsed into a single album entry:
   - Detect `.cue` files in directory listing
   - Show a single entry for the cue album with a `[CUE]` badge
   - Expanding shows the component audio file + track metadata from the cue

2. **DSD folder detection** — Folders containing `.dsf` or `.dff` files are collapsed:
   - Detect DSD files and show a single album entry
   - Show format badge (DSD64/DSD128/DSD256)
   - Expanding shows individual tracks

3. **`cargo test` passes**

## Tasks / Subtasks

- [x] Task 1: Implement cue sheet normalization — detect .cue files, group audio files under single entry [ui/widgets/folder_tree.rs]
- [x] Task 2: Implement DSD folder detection — detect .dsf/.dff files, show as single entry with DSD badge [ui/widgets/folder_tree.rs]
- [x] Task 3: Wire into folder tree display — normalized entries shown in set_entries [ui/widgets/folder_tree.rs]
- [x] Task 4: Verify no regressions — also fixed lsinfo path doubling bug in mpd/mod.rs

## Dev Notes

### Cue Sheet Strategy

```
CueSheetStrategy:
  1. Scan directory for *.cue files
  2. Parse cue to extract track list and metadata
  3. Replace individual audio files with single album entry
```

### DSD Folder Strategy

```
DsdFolderStrategy:
  1. Scan directory for *.dsf / *.dff files
  2. Group as single album
  3. Show DSD rate badge (DSD64 = 2822400/44100*64)
```

### What NOT to Do
- Do NOT implement full cue parsing engine — use simple detection
- Do NOT implement drag-and-drop (Epic 3)
- Do NOT implement full queue management

### References
- [Source: epics.md#Epic 2] — FR-B6
- [Source: DESIGN.md] — normalization strategies

### Review Findings

#### Patch Findings

- [x] [Review][Patch] DSD badge formula — fixed: removed double-multiply (`mult` not `mult * 64`) [ui/widgets/folder_tree.rs]
- [x] [Review][Patch] .cue file leaks through — fixed: .cue files hidden when cue normalization active [ui/widgets/folder_tree.rs]
- [x] [Review][Patch] Cue audio skip list incomplete — extended with .mp3, .m4a, .aiff, .wv, .wma, .opus, .aac [ui/widgets/folder_tree.rs]
- [x] [Review][Defer] No expand/collapse on normalization rows — scope limitation, shows album summary [ui/widgets/folder_tree.rs]
- [x] [Review][Patch] Single DSD file threshold — fixed: changed `> 1` to `>= 1` [ui/widgets/folder_tree.rs]
- [x] [Review][Patch] No empty-state fallback — fixed: "(empty directory)" label when all entries filtered [ui/widgets/folder_tree.rs]

#### Deferred

- [x] [Review][Defer] Mixed cue+DSD directories — rare edge case, acceptable for v1
- [x] [Review][Defer] Malformed Format tag produces ugly badge — pre-existing parsing concern

## Dev Agent Record

### Completion Notes List

- ✅ Cue sheet detection: scans directory for .cue files, collapses audio files into single "[CUE]" entry
- ✅ DSD folder detection: scans for .dsf/.dff files, shows as single entry with DSD format badge
- ✅ Fixed lsinfo path doubling bug: MPD file: paths are already full paths, not relative to directory:

### File List

- `src/ui/widgets/folder_tree.rs` — MODIFIED: cue/DSD normalization in set_entries
- `src/mpd/mod.rs` — MODIFIED: fixed lsinfo path construction (removed path doubling)
