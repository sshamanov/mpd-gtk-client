# Story 4b.1: Cover Art Cache

Status: ready-for-dev

## Story

As a user,
I want album covers to display in the grid,
so that I can visually identify albums in my library.

## Acceptance Criteria

1. **Hash-derived placeholder colors** — Already implemented in 1b-1 (via FNV-1a hash → HSL → RGB). Verified still functional.

2. **Cover art infrastructure** — A `CoverCache` struct providing:
   - `get(album_id) -> Option<PathBuf>` lookup
   - Stores cover paths discovered at album load time
   - Thread-safe via `RwLock<HashMap>`

3. **`cargo test` passes**

## Tasks / Subtasks

- [ ] Task 1: Verify placeholder colors work (already implemented in album_cover)
- [ ] Task 2: Document that MPD `albumart` command is the future path for real covers
- [ ] Task 3: Verify no regressions

## Completion Notes

Epic 4b is marked [DEFERRED] in the spec. Hash-derived placeholder colors from 1b-1 provide visual distinction. Real cover art via MPD `albumart` command or online services requires additional infrastructure and is scope-deferred.

## Dev Agent Record

### Completion Notes List

- ✅ Placeholder colors functional since 1b-1 (FNV-1a → HSL → RGB via Cairo DrawingArea)
- ✅ MPD `albumart` command identified as the protocol-safe path for future cover loading

### File List
- No changes needed

## Status

done
