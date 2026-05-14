# Story 32.2: Fix Search Race on Reconnect

Status: done

## Story

As a user reconnecting to MPD,
I want the search index to be ready before search is available,
so that searches during reconnection do not silently return empty results.

## Acceptance Criteria

1. **No empty results during reconnection window**
   - Given the MPD connection is re-established
   - When `MpdEvent::Connected` fires
   - Then the search index is reset (`SearchCommand::Reset`)
   - And `ListAlbumsGrouped` is sent to fetch fresh album data
   - When `MpdEvent::Albums` arrives
   - Then `SearchCommand::BuildIndex` is sent with the fresh album data
   - And any `SearchCommand::Search` received before `BuildIndex` returns empty results (preferred over stale)

2. **UI shows indexing state, not "no matches"**
   - Given the user searches during the Connected→Albums window
   - When the search worker receives a Search command before BuildIndex
   - Then it returns `MpdEvent::SearchIndexing` (a new event variant) instead of `SearchResults`
   - And the UI handles `SearchIndexing` by showing a "Indexing..." placeholder or suppressing results
   - And a debug log indicates the search was attempted before index was built

3. **New MpdEvent variant added**
   - Given the MpdEvent enum
   - When `SearchIndexing` is added
   - Then it is handled in all match arms (reduce, event processing, etc.)
   - And the variant has no payload (it is purely a signal)

## Technical Requirements

- The race: `Connected` fires → `SearchCommand::Reset` is sent → user searches immediately (UI is interactive) → `Search` arrives before `BuildIndex` → empty index returns no results.
- Fix: In the search worker, track whether an index has been built by checking `index.album_count() > 0`. If a `Search` arrives when `album_count() == 0`, send `MpdEvent::SearchIndexing` instead of `SearchResults`.
- Add `MpdEvent::SearchIndexing` variant to the enum in `state_machine.rs`. It takes no data — it's a signal.
- Handle in `state/mod.rs` `reduce()` — no state change, just a no-op match arm.
- Handle in `ui/mod.rs` event processing — show "Indexing..." text in the search results area or suppress the results display.
- No changes needed to the debounce, search entry, or display logic beyond the new event handling.

## References
- [Source: epics.md] Epic 32: Search Worker Data Integrity — Story 32.2
- [Source: deferred-work.md] Code review 28-3-search-worker-thread — Race window between Reset and BuildIndex on reconnect
- [Source: src/search/worker.rs:64-118] Search worker loop — index lifecycle
- [Source: src/ui/mod.rs:770-786] Search changed handler
- [Source: src/state/mod.rs:192-240] reduce() match on MpdEvent
- [Source: src/mpd/state_machine.rs] MpdEvent enum definition
