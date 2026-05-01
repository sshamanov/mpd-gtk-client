# Story 9.8: Search Relevance Scoring

Status: done

## Story

As a user searching the album library,
I want results to be sorted by relevance (exact title matches first, then artist, then partial),
so that the most likely intended album appears at the top without manual scanning.

## Acceptance Criteria

1. **Album Mode scoring**: search results are scored using the PRD weighting scheme
   - Exact album title match: 100 points
   - Exact artist match: 80 points
   - Partial album title match: 60 points x match percentage (character-level overlap ratio)
   - Partial artist match: 40 points x match percentage
   - Track title match: 30 points per matching track (max 90)
   - Year/genre exact match: 20 points
   - **Results below 20-point threshold are excluded**

2. **Folder Mode scoring**: search results use a separate scoring scheme
   - Exact filename match: 100 points
   - Exact folder name match: 80 points
   - Partial path match: 50 points x match percentage
   - File extension match: 10 points
   - **Results below 20-point threshold are excluded**

3. **Sorting**: results are sorted by score descending, with original list order as tiebreaker (stable sort)
   - When two items have identical scores, their relative order matches the original album list order

4. **Result limit**: at most 200 results are displayed initially
   - A "Show all N results" button appears when the full result set exceeds 200
   - Clicking the button expands to show all matching results (same score ordering)

5. **Performance**: scoring must not block the UI thread
   - Scoring is a pure computation on the existing match set — no additional MPD queries
   - For a 50,000-track library with 500 matching albums, scoring completes in <5ms

## Tasks / Subtasks

- [ ] Add `pub fn score_album(query: &str, artist: &str, album: &str, album_data: &AlbumDetail) -> u32` to `src/search/mod.rs`
  - [ ] Implement PRD Album Mode weighting (exact title 100, exact artist 80, partial title 60x%, partial artist 40x%, track titles 30 each, year/genre 20)
  - [ ] Implement match percentage for partial matching (character-level overlap: matching_chars / max(len(query), len(target)))
- [ ] Add `pub fn score_folder(query: &str, entry: &FolderEntry) -> u32` for Folder Mode scoring
  - [ ] Implement PRD Folder Mode weighting (exact filename 100, exact folder 80, partial path 50x%, extension 10)
- [ ] Modify `SearchIndex::search()` in `src/search/mod.rs` to return scored results
  - [ ] Change return type from `Vec<(String, String)>` to `Vec<(String, String, u32)>` (add score field)
  - [ ] Apply scoring to match set before returning
  - [ ] Sort by score descending (stable, maintaining original order for ties)
  - [ ] Apply 20-point threshold filter
- [ ] Modify folder search in `src/ui/mod.rs` to apply Folder Mode scoring
  - [ ] Collect folder entry metadata (filename, folder path, extension) at search time
  - [ ] Score and sort results before populating the search results list
- [ ] Add "Show all N results" expand button to search results UI (both modes)
- [ ] Verify no regression: existing search behavior (token intersection, filtering) is preserved

## Dev Notes

- The existing `SearchIndex::search()` in `src/search/mod.rs` returns `Vec<(String, String)>` — this needs to become `Vec<(String, String, u32)>` to carry relevance scores. All callers must be updated.
- Scoring is a pure computation layer on top of the existing token-intersection match set. The search index itself (inverted token map) does NOT change — only the result processing does.
- Match percentage for partial matching = `(2 * matching_chars) / (query_len + target_len)` where matching_chars = length of longest common substring or character-level overlap. Simpler approach: count characters from the query that appear in order within the target, divided by max(query_len, target_len).
- Folder Mode scoring needs the folder tree's normalized entry metadata at search time. The folder search currently filters `FolderTreeEntry` data — ensure the scoring function receives the full entry metadata (filename, folder_path, extension).
- The 200-result limit is a soft cap: the full result set is computed and scored, then truncated to 200 for display. "Show all" lifts the cap without recomputing.
- No new dependencies needed. Scoring is pure string manipulation on existing data structures.
- The search index is behind `RwLock` — scoring reads from the index (shared ref) and the albums Vec, both of which are already behind the lock. No additional synchronization needed.

### Project Structure Notes

- Module: `src/search/mod.rs` — add scoring functions here alongside the existing `SearchIndex` struct
- UI: `src/ui/mod.rs` — folder search result processing is at `~line 2035`, album search result display is in the album grid search path
- No new files needed; this is a modification to existing modules

### References

- PRD section "Relevance Scoring & Ranking" for exact weighting formulas
- PRD section "Search Functionality > Thresholds" for 20-point minimum and 200-result limit
- [Source: _bmad-output/planning-artifacts/prd.md#Relevance-Scoring--Ranking]
- [Source: _bmad-output/planning-artifacts/epics.md#Story-9.8-Search-Relevance-Scoring]

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
