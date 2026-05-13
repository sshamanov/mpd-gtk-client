# Story 28.3: Search Worker Thread

Status: review

## Story

As a user,
I want search index builds and queries offloaded from the GTK thread,
so that large library searches never block the UI.

## Acceptance Criteria

1. **Search worker thread owns the search index**
   - Given the MPD IO thread emits Albums/AlbumsGrouped events
   - When new album data arrives
   - Then the GTK thread sends an index build command to the search worker
   - And the search index is never locked or accessed from the GTK thread

2. **Search queries run on the worker thread**
   - Given the user types in the search omnibox
   - When the debounced query fires
   - Then the GTK thread sends a search command via channel to the search worker
   - And the search (tokenization, intersection, scoring, sort, truncate) runs on the worker thread
   - And the GTK thread never calls `SearchIndex::search()` directly

3. **Search results emitted as events**
   - Given the search worker completes a query
   - When results are ready
   - Then `MpdEvent::SearchResults(Vec<(String, String, u32)>` is emitted via `event_tx`
   - And the existing UI handler in `src/ui/mod.rs` displays results unchanged

4. **Index rebuild on library change**
   - Given `MpdEvent::LibraryChanged` fires
   - When the GTK thread processes it
   - Then a reset-index command is sent to the search worker
   - And the worker clears its index

5. **Worker thread isolation**
   - Given the search worker thread is running
   - When it processes commands
   - Then it has no `MpdAdapter` import (no MPD protocol knowledge)
   - And it has no `gtk4`/`gdk4` imports (no GTK access)
   - And it uses `Arc<AtomicBool>` for graceful shutdown

6. **Backward compatibility**
   - Given existing search functionality
   - When the search worker is operational
   - Then MPD-based search (`MpdCommand::Search`) on the MPD IO thread is unchanged
   - And the search omnibox widget behavior is identical
   - And scoring/relevance logic is identical (moved, not rewritten)

## Technical Requirements

### Thread Architecture

Per `architecture.md` §2240-2244, the Search worker:

```
GTK thread ──SearchCommand──→ Search Worker ──AppEvent──→ GTK thread
                  ↑                    │
          BuildIndex/Query     SearchResults (scored)
```

- **Thread type**: Persistent (1 thread), spawned once per connection
- **Location**: `src/search/worker.rs` (NEW)
- **Input channel**: `mpsc::SyncSender<SearchCommand>` from GTK thread
- **Output**: `MpdEvent::SearchResults(Vec<(String, String, u32)>)` via `event_tx`

### Search Command

```rust
pub enum SearchCommand {
    BuildIndex(Vec<(String, String)>),  // (artist, album)
    Search(String),                      // query string
    Reset,                               // clear index
    Shutdown,
}
```

### Thread Lifecycle

- Spawned at MPD connection time (in `state_machine.rs` `connected_loop`)
- Runs until `stop` flag is set or command channel disconnects
- Uses `mpsc::sync_channel` with bound 64 for commands
- Uses `std::thread::Builder::new().name("search-worker".into()).spawn(...)`

### Integration Points

| File | Change |
|------|--------|
| `src/search/worker.rs` | **NEW** — `spawn()`, command loop, index ownership |
| `src/search/mod.rs` | Add `pub mod worker;`, `SearchCommand` enum, export `spawn` |
| `src/mpd/state_machine.rs` | Spawn search worker at connection time, pass `event_tx` and `stop` |
| `src/ui/mod.rs` | Replace direct `fc_si.write()/read()` calls with channel sends to worker |

### Key Rules

- Search worker has **no** MPD protocol knowledge — no `MpdAdapter` import
- Search worker has **no** GTK imports — `gtk4`, `gdk4` forbidden
- Search worker's only I/O: receive commands, compute, emit events
- Existing `SearchIndex` struct stays in `src/search/mod.rs` — the worker is a wrapper that owns it
- Existing scoring/tokenization logic in `src/search/mod.rs` stays unchanged
- The search worker thread is spawned **once** per MPD connection, alongside Cover Proc

## Tasks/Subtasks

- [x] 1. Create `src/search/worker.rs` — `spawn()` with command loop, owns `SearchIndex`, emits `SearchResults`
- [x] 2. Add `pub mod worker;` and `SearchCommand` enum to `src/search/mod.rs`
- [x] 3. Update `src/mpd/state_machine.rs`: spawn search worker at connection time
- [x] 4. Update `src/ui/mod.rs`: replace direct `SearchIndex` calls with channel sends to worker
- [x] 5. Remove `search_index: Arc<RwLock<SearchIndex>>` from UI state (no longer needed)
- [x] 6. Build and run full test suite — verify zero regressions, zero warnings

## Dev Agent Record

### Implementation Plan
- Created search worker thread in `src/search/worker.rs` following the pattern from `src/coverart/cover_proc.rs`
- Worker receives `SearchCommand` (BuildIndex, Search, Reset) via `mpsc::sync_channel(64)`
- Worker emits `MpdEvent::SearchResults` via cloned `event_tx`
- `SearchCommandSender` newtype provides a clean API for the UI thread
- Worker spawned once at app startup in `MpdEventLoop::spawn()` alongside the MPD IO thread
- UI search debounce handler changed from synchronous `idx.read().search()` to fire-and-forget channel send
- Local search results and MPD fallback results both arrive via `MpdEvent::SearchResults` handler

### Completion Notes
- All 84 tests pass, zero warnings
- Search worker has no MPD protocol knowledge (no MpdAdapter import)
- Search worker has no GTK imports
- Existing SearchIndex, scoring, and tokenization logic unchanged — only ownership moved
- MPD-based search (MpdCommand::Search) on MPD IO thread unchanged

### Change Log
- NEW: `src/search/worker.rs` — Search worker thread with command loop
- MOD: `src/search/mod.rs` — Added `pub mod worker;`, re-exported `SearchCommand`, `SearchCommandSender`
- MOD: `src/mpd/state_machine.rs` — Spawn search worker in `spawn()`, return `SearchCommandSender`
- MOD: `src/main.rs` — Accept and pass `SearchCommandSender` to `App::new()`
- MOD: `src/ui/mod.rs` — Replaced `Arc<RwLock<SearchIndex>>` with `SearchCommandSender`; all index operations now channel sends

### Review Findings (2026-05-13)

3 parallel reviewers (Blind Hunter, Edge Case Hunter, Acceptance Auditor). Acceptance: all 6 ACs PASS.

**Patched (2):**
- `try_send` silently dropped commands when channel full — added `log::warn!` on failure (`src/search/worker.rs:40`)
- Worker panic kills search permanently — wrapped loop body in `catch_unwind` with error log + index reset (`src/search/worker.rs:69-117`)

**Deferred (5):**
- Unconditional MPD fallback fires alongside local search — doubled MPD traffic, result race (src/ui/mod.rs debounce)
- Race window between Reset and BuildIndex on reconnect — user search between events sees empty index
- 500ms recv_timeout latency — follows Cover Proc pattern, debounce masks it
- Event channel saturation — SearchResults try_send drops when channel full (pre-existing pattern)
- No generation counter on SearchResults — stale results can overwrite fresh ones

**Dismissed (8):** blocking send (false — uses try_send), missing Clone (false — derived), unused `_`-prefixed clones (refactor artifacts), Send not verified (compile-time), vestigial RwLock (none in SearchIndex), BuildIndex/Search race (worker is single-threaded sequential), fragile FIFO timing (channel(64) sufficient), spec deviations (intentional/functionally equivalent).

## References
- [Source: architecture.md §2240-2244] Search worker spec — 1 persistent thread, no MPD/GTK knowledge
- [Source: architecture.md §2185-2215] 6-thread model topology
- [Source: src/search/mod.rs] Current SearchIndex, scoring, tokenization — moving to worker ownership
- [Source: src/ui/mod.rs:976] Current Arc<RwLock<SearchIndex>> in UI state — to be removed
- [Source: src/ui/mod.rs:2346] Current index build on Albums event — to become channel send
- [Source: src/ui/mod.rs:2428] Current index build on AlbumsGrouped event — to become channel send
- [Source: src/coverart/cover_proc.rs] Reference implementation pattern for worker thread spawn

## File List
- `src/search/worker.rs` (NEW) — Search worker: command loop, index ownership, query execution
- `src/search/mod.rs` — Added `pub mod worker;`, `SearchCommand` enum, `SearchCommandSender` newtype
- `src/mpd/state_machine.rs` — Spawn search worker in `spawn()`, return `SearchCommandSender`
- `src/main.rs` — Accept and pass `SearchCommandSender` to `App::new()`
- `src/ui/mod.rs` — Replaced `Arc<RwLock<SearchIndex>>` with `SearchCommandSender`; all index operations via channel sends
