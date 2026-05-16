# Story 41.1: Configurable Mode-Aware Search Track Caps

Status: done

## Story

As a user searching in Album Mode vs Folder Mode,
I want the track result cap to differ by mode (100 for albums, 500 for folders),
So that Album Mode stays focused on album-level results while Folder Mode returns all matching files.

## Acceptance Criteria

1. **Given** the user searches for a common term that matches >100 tracks in Album Mode
   **When** the search results are displayed
   **Then** track results are capped at 100
   **And** an inline note reads "Showing 100 of N track results" when cap is active

2. **Given** the user searches for the same term in Folder Mode
   **When** the search results are displayed
   **Then** track results are capped at 500 (not 100)
   **And** an inline note reads "Showing 500 of N track results" when cap is active

3. **Given** the user sets `search_track_cap_album = 200` and `search_track_cap_folder = 1000` in config.toml
   **When** a search is performed
   **Then** Album Mode uses 200 as the track result cap
   **And** Folder Mode uses 1000 as the track result cap

4. **Given** the user has no config overrides for track caps
   **When** a search is performed
   **Then** the default caps are used (album: 100, folder: 500)

## Tasks / Subtasks

- [ ] Add `search_track_cap_album: u32 = 100` and `search_track_cap_folder: u32 = 500` default config values to `src/config/mod.rs` (AC: 3, 4)
- [ ] Pass mode context (Album vs Folder) in the search query/request struct so the search worker knows which cap to apply (AC: 1, 2)
- [ ] Update search worker in `src/search/` to accept the cap value and enforce it instead of the current single hardcoded limit (AC: 1, 2)
- [ ] Add inline note display in search results UI: "Showing X of Y track results" when cap is active (AC: 1)
- [ ] Wire config override into search query flow so caps can be user-configured (AC: 3)
- [ ] Test: search with 200+ matches in each mode, verify caps apply correctly

## Dev Notes

- Search worker currently uses a single hardcoded track cap. This must be split into two configurable values.
- The search query/request struct needs to carry the cap value (or at minimum the current mode) so the search worker applies the correct limit.
- Add config values to `config.toml` under a `[search]` section or attach to existing search config parameters.
- The inline "Showing X of Y" note is a new UI element below search results — use the existing status label pattern.
- Architecture ref: architecture.md §2613-2618 (Elicitation-Driven Refinements — Mode-Aware Search Track Cap)

### References

- Source: `_bmad-output/planning-artifacts/epics.md` Epic 41, Story 41.1
- Source: `_bmad-output/planning-artifacts/architecture.md` §2613-2618 (Elicitation-Driven Refinements)
- Source: `src/config/mod.rs` — Config struct for new fields
- Source: `src/search/` — Search worker for cap enforcement
- Source: `src/ui/mod.rs` — Search results display for inline note
