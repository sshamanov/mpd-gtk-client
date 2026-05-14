# Story 31.7: Log Cover Pipeline Silent Failure Paths

Status: done

## Story

As a developer,
I want silent failure paths in the cover pipeline to log diagnostic messages,
so that operational issues are detectable without digging through every `if let Ok` pattern.

## Acceptance Criteria

1. **Poisoned RwLock logging verified and consistent**
   - Given `CoverProvider`'s `RwLock` is poisoned
   - When any method encounters the poison
   - Then a `log::error!` message is emitted with the method name and error details
   - And all five methods (`get`, `invalidate`, `update_entry`, `len`, `search` in `SearchIndex`) are audited for consistent logging

2. **try_send drop events logged at warn level**
   - Given `try_send` fails when emitting `CoverPaths` or `CoverRefreshed` events (channel full)
   - When the send fails silently via `let _ = event_tx.try_send(...)`
   - Then the pattern is replaced with `if let Err(e) = event_tx.try_send(...) { log::warn!("...") }`
   - And the message includes the event type and album ID
   - And logging is rate-limited (log at most once per 10 failures to avoid spam)

3. **image::open decode failures logged**
   - Given `image::load_from_memory()` fails in `decode_and_resize()`
   - When the `if let Ok` pattern catches the failure
   - Then the error is logged at warn level with the album key (already partially implemented — verify line 148)
   - And the data size is included in the log message

## Technical Requirements

- Audit all `if let Ok` and `let _ = ...` patterns in `cover_proc.rs`, `provider.rs`, and `actual_read.rs`.
- Current state: `provider.rs` already logs RwLock poison at error level (lines 103, 150, 162, 173, 180) — verify these cover all methods.
- In `cover_proc.rs:134-152`, the `CoverRefreshed` try_send already has `let _ = event_tx.try_send(...)`. Also the `emit_cover_path()` function (line 239) silently drops. Replace these with logged variants.
- Rate limiting for log messages: use a counter pattern or a cooldown timer to avoid log spam during burst conditions. A simple approach: `if fails_since_last_log % 10 == 0 { log::warn!(...) }`.
- In `search/mod.rs:42` (`if let Ok(mut guard) = self.index.write() { *guard = idx; }`), the write lock failure is silently ignored — add an `else { log::error!(...) }`.

## References
- [Source: epics.md] Epic 31: Cover Pipeline Reliability — Story 31.7
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — CoverProvider RwLock poison silently disables cache I/O; try_send event drops invisible to caller
- [Source: src/coverart/provider.rs] RwLock patterns
- [Source: src/coverart/cover_proc.rs:134-152, 239] try_send silent drops
- [Source: src/search/mod.rs:42] Silent RwLock write failure in SearchIndex::build()
