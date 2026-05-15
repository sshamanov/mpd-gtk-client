# Story 32.3: Remove Unconditional MPD Search Fallback

Status: done

## Story

As a developer,
I want local search queries to not also trigger a full MPD `search` command,
so that MPD traffic is not doubled on every keystroke and result races between local and MPD results are eliminated.

## Acceptance Criteria

1. **MPD Search command no longer sent on keystroke**
   - Given the user types a search query in Album Mode
   - When the debounced search fires
   - Then only `SearchCommand::Search(query)` is sent to the local search worker
   - And the unconditional `MpdCommand::Search(query)` on line 785 of `ui/mod.rs` is NOT sent

2. **Local search results unaffected**
   - Given the local search worker returns results
   - When `MpdEvent::SearchResults` arrives at the UI
   - Then results are displayed as currently implemented
   - And there is no functional change to the search display

3. **MpdCommand::Search variant preserved**
   - Given the codebase
   - When the MPD search command is removed from the search-changed handler
   - Then the `MpdCommand::Search` variant and its handler in `state_machine.rs` are kept for other uses
   - And they remain available for CLI-based search or future explicit search features

## Technical Requirements

- Line 784-785 in `src/ui/mod.rs`:
  ```rust
  scmd.send(SearchCommand::Search(qc.clone()));
  let _ = tx.send(MpdCommand::Search(qc));  // ← REMOVE THIS LINE
  ```
- The MPD search fallback was added before the local search worker existed. Now that the local search worker (story 28-3) handles all queries with an in-memory index, the MPD fallback is redundant.
- The MPD round-trip adds 50-200ms latency per keystroke and creates a race condition where MPD results arrive after the debounce timeout and overwrite local results.
- Remove just the `let _ = tx.send(MpdCommand::Search(qc));` line. Do not remove the `MpdCommand::Search` variant itself — it may be useful for other features.
- Verify there are no other call sites that send `MpdCommand::Search` in response to UI search input.
- Folder mode search (`MpdCommand::SearchFiles`) is separate and should NOT be removed — it serves a different purpose (full-text file path search that the local index does not support).

## References
- [Source: epics.md] Epic 32: Search Worker Data Integrity — Story 32.3
- [Source: deferred-work.md] Code review 28-3-search-worker-thread — Unconditional MPD fallback alongside local search worker
- [Source: src/ui/mod.rs:784-785] The line to remove
- [Source: src/ui/mod.rs:796-810] Folder search handler — keep SearchFiles unchanged
