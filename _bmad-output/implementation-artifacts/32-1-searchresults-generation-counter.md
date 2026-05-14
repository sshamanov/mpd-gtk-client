# Story 32.1: Generation Counter for SearchResults

Status: done

## Story

As a developer,
I want `SearchResults` events to carry a generation counter,
so that the UI can discard stale results from slow queries that arrive after newer queries have completed.

## Acceptance Criteria

1. **Generation counter added to SearchCommand and SearchResults**
   - Given the user types a search query rapidly (e.g., "a" then "ab" within 200ms)
   - When the first query's results arrive after the second query's results
   - Then the UI discards the first (stale) result set
   - And only the second (latest) result set is displayed
   - And no flicker or race between old and new results

2. **SearchCommand::Search carries generation number**
   - Given the user types a new search query
   - When `SearchCommand::Search(query)` is constructed
   - Then it includes the current generation counter from the UI
   - And the generation is monotonically incrementing (use `wrapping_add`)

3. **MpdEvent::SearchResults carries generation number**
   - Given the search worker processes a query
   - When it emits `MpdEvent::SearchResults`
   - Then the event includes the generation from the SearchCommand
   - And the UI compares `event.generation == local_generation` before applying results

## Technical Requirements

- The UI already has a generation counter for debounced input (`search_gen` at `ui/mod.rs` line 728).
- Currently this counter is used only for the debounce timeout guard — it is never sent to the search worker.
- Change `SearchCommand::Search(String)` to `SearchCommand::Search(String, u64)` — add the generation field.
- Change `MpdEvent::SearchResults(Vec<(String, String)>)` to `MpdEvent::SearchResults { results: Vec<(String, String)>, generation: u64 }`.
- In the UI's search-changed handler (line 783): `scmd.send(SearchCommand::Search(qc.clone(), this_gen))`.
- In the UI's event processing (match on SearchResults): check `gen_c.get() == event.generation` before displaying.
- Check all match arms on `MpdEvent::SearchResults` in `ui/mod.rs` — there may be multiple.
- The `MpdEvent::FileSearchResults` variant (folder mode search) does NOT need a generation counter — folder mode search is MPD-based and less latency-sensitive.

## References
- [Source: epics.md] Epic 32: Search Worker Data Integrity — Story 32.1
- [Source: deferred-work.md] Code review 28-3-search-worker-thread — No generation counter on SearchResults
- [Source: src/search/worker.rs:19-26] SearchCommand enum — needs generation field
- [Source: src/search/worker.rs:78-89] Search handler — needs to echo generation
- [Source: src/mpd/state_machine.rs] MpdEvent::SearchResults variant definition
- [Source: src/ui/mod.rs:728] Existing `search_gen` generation counter
- [Source: src/ui/mod.rs:770-786] Search changed handler — sends SearchCommand
