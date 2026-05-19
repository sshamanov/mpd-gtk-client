# Debug Session 2026-05-16

## Fix: idle connection timeout causing broken pipe flood

### User request
> app starts but very slow, errors in log — investigate and fix

### Symptoms observed
- Flood of `[MPD] idle transient error: MPD connection failed: Broken pipe (os error 32)` in log
- First error preceded by: `Resource temporarily unavailable (os error 11)` (EAGAIN from read timeout)
- ~10-12 seconds after startup, the main MPD connection dies
- UI becomes unresponsive because the main command connection is dead but `connected_loop` never returns to trigger reconnect

### Root cause
`MpdAdapter::connect_tcp()` sets a 10-second read timeout on the TCP socket. The `idle()` method blocks on `read_line()` waiting for MPD to report subsystem changes. If no MPD event occurs within 10 seconds (common during idle periods), the read times out with EAGAIN.

The error handling in `connected_loop` treated ALL idle errors as transient, incrementing a failure counter and falling into a 100ms polling loop. On each loop iteration it re-tried `idle()` on the same (now confused) connection, sending another "idle\n" while MPD was still in the previous idle session. This protocol confusion caused MPD to close the connection → "Broken pipe" on subsequent writes. The loop never exited, so reconnection never happened.

### Fix (2 changes)

**1. `src/mpd/mod.rs:476` — Remove read timeout during idle**

MPD's `idle` protocol is designed to block indefinitely until a subsystem changes. Set `read_timeout` to `None` on the reader's stream before the blocking `read_line` loop, restore to 10s after idle completes. This prevents the 10s timeout from killing a healthy idle connection.

Every exit path (OK response, ACK error, read EOF, read I/O error) restores the timeout before returning.

**2. `src/mpd/state_machine.rs:664` — Detect fatal idle errors and reconnect**

Match on `Error::Connection` with `BrokenPipe`/`ConnectionReset`/`ConnectionAborted` kinds, and `Error::Protocol` with "Connection closed" — return from `connected_loop` so the outer state machine reconnects. Other errors remain treated as transient with polling fallback.

### Verification
- App ran 20 seconds with no broken pipe errors (previously flooded within 12 seconds)
- All 119 tests pass (35 unit + 62 bin + 22 integration)
- Idle stays alive across quiet periods

## Fix: Cover fetch pipeline stalling after 32 covers + GTK thread blocking

### User request
> UI freezes. cover fetch in logs too slow. visually nothing changed.
> work until: all covers fetched in less then 5 seconds; mpd-client process consume less then 10% CPU
> check architecture. covers should work over separate dedicated mpd connection. check how plattenalbum do it, they got covers in a second
> check if GTK thread blocked and is carrying heavy tasks

### Symptoms observed
- Only 32 of 120 covers processed per run, then pipeline stalled
- CPU at 0% after first burst — app completely idle
- Cover events all at same second, then no more log output for remaining timeout

### Root cause analysis

**Primary: Cover job channel capacity 32 with silent drops**
`src/mpd/cover.rs:104`: `sync_channel::<CoverJob>(32)` — channel holds 32 jobs max.
`src/mpd/cover.rs:79-83`: `enqueue()` uses `let _ = self.tx.try_send(job.clone())` — silently discards Full errors. When the UI sends 120 albums for cover fetch, only the first 32 fit; 88 are silently dropped. The MPD cover thread processes those 32 then idles for 30s before terminating.

**Secondary: `image::open()` on GTK thread blocks UI**
`src/ui/mod.rs:2301-2312` (3 places): During album grid creation, if a cached cover path exists, the code calls `image::open()` + Lanczos3 resize synchronously on the GTK main thread. Each JPEG decode takes 10-50ms; for 120 covers that would freeze the UI for seconds. This path is rarely hit during initial load (cover_paths cache is empty until CoverPaths events arrive) but would hit on reconnect or mode switch.

**Comparison with plattenalbum:**
- plattenalbum fetches covers lazily (only for visible albums), not all 120 at startup
- Uses `Gdk.Texture.new_from_filename()` (async GDK load) and `Gdk.Texture.new_from_bytes()` (GPU-side decode) — never decodes JPEG on the main thread
- Single MPD connection with sequential `albumart` commands per album

### Fix (3 changes)

**1. `src/mpd/cover.rs:104` — Increase job channel capacity 32→512**
Prevents silent drops. 512 is large enough for any realistic library. `enqueue()` now counts and logs dropped jobs via `log::warn!` so backpressure issues are visible.

**2. `src/mpd/cover.rs:79-83` — Make enqueue log drops**
Added counting of Full and Disconnected errors. Reports dropped count as warning so operators can see when the pipeline is overloaded.

**3. `src/ui/mod.rs:2296-2312` (3 places) — Replace `image::open()` with `set_filename()`**
Removed synchronous JPEG decode + Lanczos3 resize from GTK main thread. Now uses `Picture::set_filename()` which delegates loading and scaling to GDK's async compositor. The texture cache (`tc`) is still populated by CoverRefreshed events (pre-decoded RGBA from cover_proc).

### Verification
- 120/120 covers processed in ~2 seconds (target: <5s) ✅
- CPU usage ~0% throughout (target: <10%) ✅
- Zero "Channel full" warnings ✅
- Zero errors in log ✅
- All 119 tests pass ✅
- App runs with no broken pipe errors (idle timeout fix still holds) ✅

## Fix: Skip MPD fetch for cached covers (zero round-trips)

### User request
> cache should cover MPD speed limit, why it still only 2s?

### Root cause
The MD5 cache was checked AFTER fetching binary data from MPD. `process_success()` in cover_proc computed the MD5 of fetched data, then checked if it matched the cache. So even for 100% cached albums, ALL 120 `albumart` commands were sent to MPD, each triggering MPD file I/O + binary transfer over TCP. 120 x ~15ms = ~2 seconds.

The cache prevented JPEG **decode** (saving CPU), but NOT the MPD **fetch** (still paying network/file I/O).

### Fix: `src/mpd/state_machine.rs` — FetchCovers cache fast-path

**Single-pass check in `FetchCovers` handler** — before any MPD communication:
1. Acquire `cover_provider.read()` ONCE (not per-album)
2. For each album: HashMap lookup → build cache path → `Path::exists()`
3. Cache hit → insert into `CoverPaths` HashMap, emit directly to event channel
4. Cache miss → enqueue for full MPD pipeline (resolve URI → albumart → MD5 → cache write)

**Single pass optimization:** Combined the partition + emit loop that previously did 240 `cover_provider.read()` calls and 240 `Path::exists()` calls into one pass with 120 of each.

### Verification
- **Zero MPD albumart round-trips** (0 `albumart: got` log lines) ✅
- Cache check sub-second (enqueue and emit in same log second) ✅
- All 120 covers emitted from cache ✅
- All 119 tests pass ✅
- On cache miss: still falls through to full pipeline (resolve URI → albumart → MD5 → cache)
