# Debug Session 2026-05-20

## Code review — bug fixes

### Fix 1: `connect_unix` ignored MPD protocol version for capabilities

**Root cause:** `MpdAdapter::connect_unix()` hardcoded `capabilities: Default::default()` (readpicture=false, albumart=true) while `connect_tcp()` and `try_unix_socket_connect()` correctly called `MpdCapabilities::from_version(&version)`. Users with explicit Unix socket paths in config always got default capabilities regardless of MPD version.

**Fix:** `src/mpd/mod.rs:333-338` — Parse MPD version from greeting and compute capabilities from it, matching the TCP path.

### Fix 2: Cover Proc `running` AtomicBool leaked on panic

**Root cause:** `cover_proc::spawn()` used `compare_exchange` to guard against duplicate workers, but only reset `running` via `running.store(false)` on clean exit. A panic in the JPEG decode or cache write would leave the flag set to `true` forever, blocking future Cover Proc workers for the lifetime of the MPD connection.

**Fix:** `src/coverart/cover_proc.rs` — Added `RunningGuard` RAII struct that resets the flag on `Drop`. The guard is moved into the thread closure, so it runs on both clean exit and panic unwind.

### Fix 3: Transient idle failure counter created tight retry loop

**Root cause:** After 10 consecutive transient idle errors, `transient_failures` was reset to 0 but `use_idle` remained `true`. The poll-fallback guard `if !use_idle || transient_failures > 0` evaluated to `false`, so the loop re-entered `idle()` immediately — spamming MPD with idle commands if the transient condition persisted.

**Fix:** `src/mpd/state_machine.rs:683` — Set `use_idle = false` when the transient window passes, so the poll fallback engages. The polling path sleeps 100ms between attempts via `recv_timeout`.

### Fix 4: `search_albums()` two-pass parsing of response lines

**Root cause:** The search results were iterated twice — once to build the `album_artist` map, then again to emit deduplicated results in file order. Functionally correct but O(2n) over potentially large search responses.

**Fix:** `src/mpd/mod.rs:854` — Combined into a single pass. Uses `album_order: Vec<String>` to track insertion order alongside the existing `album_artist` HashMap, then emits results by iterating over `album_order` rather than re-parsing lines.

### Verification
- All 119 tests pass (35 unit + 62 bin + 22 integration)
- All 4 search-related tests pass with new single-pass implementation

## Observations (code review)

### Large UI file (`src/ui/mod.rs`, ~3000+ lines)
The main UI module mixes grid layout, queue management, now-playing display, settings dialogs, search results, folder tree, toast notifications, and event dispatch. This is the highest-risk file for regressions from future changes. Consider factoring out settings, search, and queue into separate modules in a future cleanup pass.

### `MpdCapabilities` derived from greeting banner, not `commands` output
The capabilities matrix (`readpicture`, `albumart`) is inferred from the MPD protocol version in the greeting. A more robust approach would be to parse MPD's `commands` output, which lists every supported command explicitly. The version-based heuristic works for standard MPD but could be wrong for forks or builds with features disabled at compile time. Low priority — no known MPD forks that break this assumption.

### `cover_proc.rs:134-136` — Lock ordering note
`write_cache()` writes to disk, then acquires `CoverProvider` read lock to update in-memory entry, then acquires it again for `evict_lru()`. The newly written entry is registered before eviction, so the file won't be evicted immediately. The repeated read-lock acquisition is slightly wasteful but correct.

## Fixes — Post code review (second round)

### Fix 5: PCM track info display showed duplicate format text

**Root cause:** `AudioFormat::display_text()` at `src/mpd/state_machine.rs:145` produced `"24/44.1 · "` (trailing `" · "` separator) for PCM because `parse_single_audio_source` sets `codec: String::new()` for PCM. In `PlaybackDisplay::from_update()`, the `contains()` check `"24/44.1".contains("24/44.1 · ")` returned false, so it concatenated: `"24/44.1 ·  · 24/44.1"` — producing the double dot and duplicate rate/bits the user saw.

**Fix:** `src/mpd/state_machine.rs:145` — Don't append `" · {codec}"` when codec is empty. Now returns `"24/44.1"` instead of `"24/44.1 · "`, which matches the `format_badge()` output and passes the `contains()` check.

### Fix 6: Album play caused metadata connection closed error

**Root cause:** The metadata thread's MPD connection can go stale while sitting idle (the thread blocks on `cmd_rx.recv()`, not reading from MPD). When a `ListAlbumTracks` command finally arrives, `send_command` detects the dead connection (read returns 0 bytes) and returns `Error::Protocol("Connection closed")`. The thread set `adapter = None` and dropped the command — no retry.

**Fix:** `src/mpd/state_machine.rs:1663-1765` — Refactored metadata thread's command dispatch to retry once after reconnect. Changed `match cmd` to `match &cmd` (borrow instead of move) to allow retrying with the same command. On first error: reconnect, set retry flag, continue inner loop. On second error: log final error and give up.

### Fix 7: Grid columns not adjusting on window resize

**Root cause:** The resize handler used `connect_notify_local(Some("width"), ...)` on the ScrolledWindow. In GTK4, `GtkWidget:width` is a deprecated property and the `notify` signal may not fire reliably for allocated-width changes. The grid columns were computed once at initial load and never updated when the user resized the window.

**Fix:** Added width-change detection in the frame-clock tick callback (`window.add_tick_callback`). Uses a `Cell<f64>` to track the last reposition width; when the ScrolledWindow's allocated width differs by >1px from the last reposition, calls `reposition()` to recalculate columns.

**Follow-up — re-entrancy panic on scroll:** The tick callback called `reposition()` while holding a `RefCell` borrow of `album_cells`. `layout.move_()` inside `reposition()` triggered GTK scrollbar adjustment signals, which re-entered and tried to borrow the same `RefCell` → panic. Fixed by cloning the cells Vec before calling `reposition()`, dropping the borrow first. Applied to all 5 call sites (tick callback, resize handler, 3 event handlers).

**Follow-up — scroll index OOB panic:** The scroll timeout estimated visible cells via `scroll_top / CELL_SLOT_H * cols` to compute `start_idx`. But the actual layout height includes group captions (`CAPTION_H` per group), so the fixed row-height math overestimates near the bottom, producing `start_idx > cells.len()`. Fixed by clamping `start_idx` to `cells_binding.len()` and adding an empty-range guard.

### Fix 8: Right rail grew proportionally on wide windows

**Root cause:** The tick callback computed `rail_w = (win_width * (1.0 - split_ratio)).clamp(rw_min, rw_max)`, which made the right rail grow above 320px on windows >~1067px. User wanted a fixed 320px rail.

**Fix:** Removed the proportional formula at 3 sites (initial `right_pane_clamp.set_maximum_size`, initial `wide_right.set_size_request`, and tick callback). Now always uses `rail_width_min` directly.

### Fix 9: Group captions overlapped previous group's covers

**Root cause:** In `reposition()`, when a new group started but the previous group ended mid-row (`x > 0`), the caption was placed at the current `y` — on top of the previous group's covers. The next group's covers then started below the caption.

**Fix:** When a new group starts and `x > 0`, advance `y` by `CELL_SLOT_H` to the next row before placing the caption. This ensures each group starts on a fresh row.

### Fix 10: Album grid rows had zero vertical gap

**Root cause:** `CELL_SLOT_H = 250` matched the cell height exactly (`set_size_request(200, 250)`). Horizontal gap was 16px (`CELL_SLOT_W = 216` − cell width 200) but vertical gap was 0.

**Fix:** Increased `CELL_SLOT_H` from 250 to 258 (8px vertical gap).

### Fix 11: Group caption font too small

**Fix:** `src/ui/style.css` — `.group-caption` font-size changed from `0.85em` to `1.5em`.

### Verification
- All 119 tests pass (35 unit + 62 bin + 22 integration)
