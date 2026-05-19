# Debug Session — 2026-05-03

Full codebase bug audit. All 26 source files (~6000 lines) reviewed.

## Bug 1 (MEDIUM) — `albumart` wastes first response round-trip

**File:** `src/mpd/mod.rs:557-581`

The first `albumart "..." 0` request (line 559) returns both the size header AND the first binary chunk. Only `parse_albumart_size()` is called on the response — the binary data is discarded. The loop then re-requests offset 0 for the chunk. This is an extra MPD round-trip per cover fetch.

`readpicture` (line 590-619) correctly extracts the chunk from the initial response at line 607; `albumart` should do the same.

**Fix:** Extract the first chunk from the initial response before entering the fetch loop, matching the readpicture pattern.

## Bug 2 (MEDIUM) — `InsertNext` silently breaks with no feedback

**File:** `src/mpd/state_machine.rs:473`

```rust
if current_pos < 0 { break; }
```

When there's no current track (MPD returns `-1` for song position), `InsertNext` silently does nothing. No log, no toast, no error event.

**Fix:** Log a warning and/or emit an `MpdEvent::Error` before breaking.

## Bug 3 (MEDIUM) — `cover_provider.read().unwrap()` panics on poisoned lock

**File:** `src/mpd/state_machine.rs:549`

```rust
actual_read.process_one(&mut adapter, &caps, &cover_provider.read().unwrap(), event_tx);
```

If the CoverProvider RwLock is poisoned, this `unwrap()` crashes the MPD event loop thread. Should degrade gracefully.

**Fix:** Replace `.unwrap()` with `if let Ok()` or log-and-skip.

## Bug 4 (LOW) — `SHUTDOWN_REQUESTED` is dead code in both crate roots

**Files:** `src/main.rs:32`, `src/lib.rs:14`

Two separate `AtomicBool` statics exist — one in the binary crate root, one in the library crate root. The comment on `lib.rs:12-13` says "the frame clock event-processing callback in the UI layer checks this flag" but no such check exists. Signal handler registration was removed (noted at `main.rs:248-251`). Ctrl+Q calls `app.quit()` directly.

**Fix:** Remove both dead statics and the associated comment.

## Bug 5 (LOW) — `readpicture` version check too strict

**File:** `src/mpd/mod.rs:240-242`

```rust
fn supports_readpicture(&self) -> bool {
    self.major >= 1 || (self.major == 0 && self.minor >= 24)
}
```

MPD's `readpicture` was added in 0.22, not 0.24. MPD 0.22 and 0.23 will have readpicture incorrectly disabled. Since MPD never released 0.24 (jumped from 0.23.x), readpicture is effectively never enabled via version check.

**Fix:** Change `24` to `22`.

## Bug 6 (LOW) — No read timeout on Unix sockets

**File:** `src/mpd/mod.rs:51-58`

```rust
MpdStream::Unix(_) => Ok(()), // Unix sockets don't need timeouts
```

Unix domain sockets DO need read timeouts — if MPD dies mid-connection without closing the socket, reads block indefinitely. The `read_albumart_response` method temporarily sets a 5-second timeout, but it's a no-op on Unix.

**Fix:** Call `stream.set_read_timeout(timeout)` on the Unix socket, or set SO_RCVTIMEO via `libc::setsockopt`.

## Bug 7 (TRIVIAL) — Multiple action flags silently overwrite

**File:** `src/main.rs:105-116`

`--next --start-playing --prev` → last wins (`Previous`). Test at line 436-443 confirms this is accepted behavior, but there's no warning to the user.

**Fix:** Emit a warning to stderr when an action flag overwrites a previous one.

## Summary

| # | Severity | Area | Issue |
|---|----------|------|-------|
| 1 | MEDIUM | Cover art | Extra MPD round-trip per albumart fetch |
| 2 | MEDIUM | Queue | InsertNext silently ignores when no song playing |
| 3 | MEDIUM | Cover art | RwLock poison panics MPD event loop thread |
| 4 | LOW | Shutdown | Dead AtomicBool statics in both crate roots |
| 5 | LOW | MPD protocol | readpicture version check blocks MPD 0.22-0.23 |
| 6 | LOW | MPD transport | Unix sockets lack read timeouts |
| 7 | TRIVIAL | CLI parsing | Conflicting action flags overwrite silently |

No data corruption, crash-on-startup, or security issues found. Codebase is in good shape for v1.

---

## Round 4 — Re-apply AlbumCoverCell (2026-05-03 ~18:00)

### User feedback
- "gigantic cell was before the custom widget was done" — the 1140px cell issue pre-dated AlbumCoverCell
- "custom widget is not a problem, most likely"
- "try this custom widget from plattenalbum one more time"
- User rejected the revert to plain Box approach — it goes back to the broken 1140px state

### Actions taken
1. **Added debug session rules to CLAUDE.md**: never revert without approval, annotate all debug changes with timestamped comments, log every action/edit/result
2. **Re-applied AlbumCoverCell** in connect_setup (replaces ~130 lines of nested Box tree with ~5 lines)
3. **Re-applied simplified connect_bind** (AlbumCoverCell methods instead of widget tree traversal)
4. **Kept model.splice()** for atomic batch_populate
5. **Removed widget_set_str/widget_get_str/WIDGET_ALBUM_KEY** again (AlbumCoverCell stores album_key in RefCell)
6. **Removed pr/pg/pb from AlbumGridItem::Album** (unused, computed via placeholder_rgb at runtime)
7. **Removed Button/Overlay from imports** (AlbumCoverCell encapsulates these internally)
8. **COVER_PUSH_ENABLED** still false for debugging

### Build result
Compiles cleanly (only pre-existing warnings). Ready for visual evaluation.

### What AlbumCoverCell provides
- Single GtkBox subclass widget instead of nested Box > Box > Overlay > Picture
- request_mode() = HeightForWidth (like plattenalbum's AlbumCover)
- set_header()/set_album() for mode switching (no set_visible() on external children)
- Deferred button wiring (buttons created in constructed(), handlers wired after cmd_tx set in new())
- set_cover_texture() for cover updates from CoverPaths/CoverRefreshed

### Notes
- DnD DragSource removed (was on container in old setup). AlbumCoverCell stores album_key in RefCell — DnD can be re-added by accessing cell.album_key() from the DragSource prepare handler
