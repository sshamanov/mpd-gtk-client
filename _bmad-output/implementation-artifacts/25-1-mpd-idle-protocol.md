# Story 25.1: MPD Idle Protocol Integration

Status: ready-for-dev

## Epic Context

Epic 25 implements true MPD `idle`/`noidle` protocol, replacing the current 500ms polling. This is a foundational change: every following epic (cover art pipeline, metadata caching, multi-thread workers) depends on event-driven state updates.

**Cross-story context:**
- 25-1 (this): Core idle protocol + subsystem-specific refresh
- 26-1: Metadata caching (depends on idle→database subsystem)
- 28-1/2/3/4: Multi-thread workers (depend on idle→player for covers)
- 27-1: Responsive right rail (benefits from sub-500ms state updates)

## Story

As a user,
I want the application to respond to MPD state changes immediately,
so that playback state, queue, and metadata updates are reflected in the UI without 500ms polling delay.

## Acceptance Criteria

1. **Idle loop replaces 500ms status polling**
   - **Given** the MPD connection is established and no commands are pending
   - **When** the background thread enters idle mode
   - **Then** `idle` command is sent to MPD, blocking until a subsystem changes
   - **And** the thread responds immediately to `noidle` for command dispatch
   - **And** on idle return, only the changed subsystems are refreshed (not all status)

2. **Subsystem-specific refresh**
   - **Given** `idle` returns with `changed: player`
   - **When** the event is processed
   - **Then** only `currentsong` + `status` are fetched (not queue, not database)
   - **Given** `idle` returns with `changed: playlist`
   - **When** processed
   - **Then** only the queue is refreshed via `playlistinfo`
   - **Given** `idle` returns with `changed: database`
   - **When** processed
   - **Then** `MpdEvent::LibraryChanged` is emitted (UI triggers album list refresh)
   - **Given** `idle` returns with `changed: mixer`
   - **When** processed
   - **Then** volume is refreshed via `status` (only the `volume` field needed)

3. **Thread-safe idle break via try_clone**
   - **Given** the worker thread is blocked on `idle`
   - **When** a command arrives via `cmd_rx`
   - **Then** `noidle` is written to a socket clone on the main thread
   - **And** the worker's `idle` returns within one RTT (sub-1ms locally)
   - **And** after commands are processed, `idle` is re-entered

4. **Fallback when idle unavailable**
   - **Given** MPD < 0.19 or idle returns "unknown command"
   - **When** idle is attempted
   - **Then** fall back to 500ms polling permanently
   - **Given** `idle` returns a transient error (connection reset, timeout)
   - **When** the error is detected
   - **Then** fall back to 100ms `status` polling for 10 cycles (1 second), then retry `idle`

## Tasks / Subtasks

- [ ] 1. Add `try_clone()` to `MpdStream` enum (AC: #3)
  - [ ] 1.1 Implement `MpdStream::try_clone() -> io::Result<MpdStream>` delegating to inner stream's `try_clone()`
  - [ ] 1.2 Add `MpdAdapter::stream_clone() -> io::Result<MpdStream>` method returning a write-only clone

- [ ] 2. Add `idle()` and `noidle()` methods to `MpdAdapter` (AC: #1)
  - [ ] 2.1 `idle()`: send `idle` command via existing `send_command` pattern
  - [ ] 2.2 `noidle()`: write `noidle\n` to stream clone (bypasses BufReader, writes directly)
  - [ ] 2.3 Parse idle response into a set of changed subsystems (`Vec<&str>` or `IdleEvent` enum)
  - [ ] 2.4 Handle `idle` returning "unknown command" → store fallback flag

- [ ] 3. Refactor `connected_loop` to idle-driven event loop (AC: #1, #2, #3)
  - [ ] 3.1 Create the socket clone at the start of `connected_loop`
  - [ ] 3.2 Move idle event parsing into the main loop:
    - Send `idle\n` to MPD
    - Block reading idle response
    - Parse subsystems
    - Perform subsystem-specific refresh
    - Process any pending commands from `cmd_rx` (via `try_recv`)
    - Re-enter idle
  - [ ] 3.3 On `MpdCommand` arrival (from main thread):
    - Main thread writes `noidle\n` to socket clone
    - Then sends command via mpsc channel
    - Worker processes command after idle returns
  - [ ] 3.4 Subsystem-specific refresh functions:
    - `handle_idle_player()` → `fetch_full_update()` (status + currentsong)
    - `handle_idle_playlist()` → `list_queue()` + emit `MpdEvent::Queue`
    - `handle_idle_database()` → emit `MpdEvent::LibraryChanged`
    - `handle_idle_mixer()` → `adapter.status()` extract volume only
    - `handle_idle_options()` → fetch fresh status
    - `handle_idle_stored_playlist()` → fetch fresh status
    - `handle_idle_update()` → emit `MpdEvent::LibraryChanged`
    - Combined case: handle multiple subsystems in a single idle return

- [ ] 4. Fallback logic for non-idle-capable MPD (AC: #4)
  - [ ] 4.1 If `idle` returns "unknown command" → set `use_idle = false`, revert to 500ms polling
  - [ ] 4.2 If `idle` returns transient error → 100ms poll for 10 cycles, then retry `idle`
  - [ ] 4.3 On 3 consecutive `fetch_full_update` failures → exit `connected_loop` (existing dead connection detection, unchanged)

- [ ] 5. Update `MpdEventLoop::spawn` to support idle break (AC: #3)
  - [ ] 5.1 Move `noidle` write logic into the command-sending path (main thread side)
  - [ ] 5.2 The stream clone must be accessible from the main thread — options:
    - Pass it back in the spawn return tuple
    - Wrap in `Arc<Mutex<Option<MpdStream>>>`
    - Store on `MpdEventLoop` struct
  - [ ] 5.3 Verify the clone is usable via `ConnectionTarget::Unix` (Unix socket path)

- [ ] 6. Update mock MPD server to handle `idle` command (AC: #1, #3, #4)
  - [ ] 6.1 Accept `idle` command, hold the connection open until `noidle` is received or timeout
  - [ ] 6.2 On `noidle`, return `changed: player` (test default)
  - [ ] 6.3 Support configurable subsystem responses per test (list of subsystems to return)
  - [ ] 6.4 Test: idle → noidle → idle cycle completes correctly
  - [ ] 6.5 Test: "unknown command" response to idle triggers polling fallback
  - [ ] 6.6 Test: transient error during idle triggers 100ms poll retry

- [ ] 7. Integration tests (AC: #1, #2, #3, #4)
  - [ ] 7.1 Test that idle blocks until noidle is sent
  - [ ] 7.2 Test that subsystem-specific refresh emits correct MpdEvent
  - [ ] 7.3 Test fallback to 500ms polling when idle returns "unknown command"
  - [ ] 7.4 Test transient error fallback (connection reset during idle)
  - [ ] 7.5 Test multiple subsystems changed in a single idle response

## Dev Notes

### Current State (lines reference `src/mpd/state_machine.rs`)

- `connected_loop` (line 278) uses `cmd_rx.recv_timeout(Duration::from_millis(100))` — 100ms command poll
- Independent `fetch_full_update()` (line 756) every 500ms regardless of command activity
- Dead connection detection: 3 consecutive `fetch_full_update` failures → return
- `MpdAdapter` owns `BufReader<MpdStream>` for reading and `MpdStream` for writing
- Both `TcpStream` and `UnixStream` support `try_clone()`

### Target Architecture (from architecture.md §239-249)

```
Worker thread:
  loop {
    adapter.idle()           // blocks until MPD responds
    subsystems = parse(response)
    refresh(subsystems)      // subsystem-specific fetches
    while let Ok(cmd) = cmd_rx.try_recv() { process(cmd) }
  }

Main thread (in cmd_tx.send wrapper):
  stream_clone.write_all(b"noidle\n")
  cmd_tx.send(cmd)          // wake worker from idle
```

### Subsystem Response Format

MPD's `idle` returns one `changed:` line per changed subsystem:

```
changed: player
changed: playlist
OK
```

Possible subsystems: `database`, `update`, `stored_playlist`, `playlist`, `player`, `mixer`, `output`, `options`, `partition`, `sticker`, `subscription`, `message`, `neighbor`, `mount`

### IdleEvent enum

```rust
/// Subsystems that changed during an MPD idle session.
/// Returned as a Vec since multiple subsystems can change simultaneously.
enum IdleSubsystem {
    Database,      // Music database changed
    Update,        // A database update finished
    StoredPlaylist, // A stored playlist was modified
    Playlist,      // The current queue changed
    Player,        // Player state changed (play/pause/stop/song)
    Mixer,         // Volume changed
    Output,        // Audio output changed
    Options,       // Options changed (random, repeat, crossfade, etc.)
    Unknown(String), // Future/unknown subsystem
}
```

Parse from `changed: ` lines. Unknown subsystems should be logged at debug level and a full status refresh triggered.

### Key Implementation Details

1. **Socket clone ownership:** The `MpdAdapter` needs `try_clone()` so the main thread can write `noidle`. Two approaches:
   - **Approach A (recommended):** Store the stream clone in an `Arc<Mutex<Option<MpdStream>>>` shared between `MpdEventLoop` and the worker thread. The main thread accesses this through `MpdEventLoop::signal_noidle()`.
   - **Approach B:** Pass the clone back through a separate channel or return value from `spawn()`.

2. **Atomic flag for idle state:** Add `idle_active: AtomicBool` to track whether the worker is currently in `idle`. The main thread's `send_command` wrapper checks this flag before writing `noidle`. If not idle, no need to write `noidle`.

3. **Race condition:** It is possible for the idle to naturally return just as the main thread writes `noidle`. This is safe: MPD processes `noidle` gracefully even outside of idle mode (returns "OK" without subsystems). Log at debug level if `idle` was not active when `noidle` was sent.

4. **Cover art handling during idle:** The ActualRead processing (line 744 in state_machine.rs) currently runs on the `recv_timeout` timeout path. With idle, this needs to run outside idle or after idle returns. Solution: after idle returns and before re-entering idle, check `cmd_rx.try_recv()` AND `actual_read.has_pending()`. If covers are pending but no commands, process one cover fetch, then re-enter idle.

5. **Full status vs subsystem refresh:** `fetch_full_update()` is ~5ms round-trip (status + currentsong). Subsystem-specific fetches are lighter:
   - `player` only: status + currentsong (~2-3ms)
   - `playlist` only: playlistinfo (~1ms per 100 tracks)
   - `mixer` only: status (~1ms)
   - Always emit at minimum: the changed subsystem should produce at minimum one event so UI reflects the change.

6. **Noidle during shutdown:** The `Close` command must also trigger `noidle`. Currently `Close` sends `close` command then stops. With idle: write `noidle`, send `Close` via channel, wait for idle to return, then proceed with close sequence.

### Source Files Modified

| File | Change |
|------|--------|
| `src/mpd/mod.rs` | Add `try_clone()` to `MpdStream`, `stream_clone()` to `MpdAdapter`, `idle()`/`noidle()` methods, `IdleSubsystem` enum, idle response parser |
| `src/mpd/state_machine.rs` | Refactor `connected_loop` to idle-driven loop, subsystem-specific refresh handlers, fallback logic, cover processing integration |
| `src/mpd/mock.rs` | Add `idle` command handler, `noidle` handler, configurable subsystem response |
| `src/main.rs` | Pass stream clone or noidle sender to `MpdEventLoop` if needed |
| `tests/smoke_test.rs` | Add idle-specific integration tests |

### Testing Strategy

- **Unit tests (src/mpd/mod.rs):** Test `IdleSubsystem` parsing from raw response text. Test `noidle()` write works. Test fallback flag set/get.
- **Integration tests (tests/):** Use `MockMpdServer` enhanced with `idle` support. Test the full idle cycle. Test fallback to polling.
- **State machine tests (src/mpd/state_machine.rs):** Add tests for subsystem refresh handlers with mock adapter.

### References

- [Source: architecture.md §239-249] MPD Idle Protocol ADR — design decision, try_clone pattern, fallback behavior
- [Source: architecture.md §172-176] Threading model decision — dedicated thread with state machine
- [Source: architecture.md §227-235] Idle timeout and connection resilience
- [Source: architecture.md §1350-1387] MPD version support matrix (idle requires >= 0.19)
- [Source: prd.md §30-36] MPD Idle Protocol — before/after description
- [Source: src/mpd/state_machine.rs] Current 500ms polling implementation (lines 278-795)
- [Source: src/mpd/mod.rs] MpdAdapter, MpdStream, send_command patterns

### Anti-Regression Notes

- **Must NOT break:** MPD version < 0.19 must still work (fallback to 500ms polling)
- **Must NOT break:** All existing MpdCommand variants must dispatch correctly after idle returns
- **Must NOT break:** Dead connection detection (3 consecutive failures) must still trigger reconnect
- **Must NOT break:** Cover art ActualRead processing must still run (after idle returns, before re-entering)
- **Must NOT break:** `Close` command must still work with idle pattern
- **Must NOT break:** Unix socket connections must support `try_clone()` same as TCP

## Dev Agent Record

### Agent Model Used



### Debug Log References



### Completion Notes List

- When implementing subsystem-specific refresh, ensure the `fetch_full_update()` call for `player` subsystem also emits `MpdEvent::StateChanged` — this is the same event the 500ms polling produced
- The `options` subsystem (random/repeat/consume/single/volume) refresh maps to `MpdEvent::StateChanged` since it carries the full status — no new event type needed
- `database` and `update` subsystems both map to `MpdEvent::LibraryChanged` — the UI already handles this by refreshing album list
- The `tstamp` value in the idle response is informational — it indicates when the idle session started, not when each subsystem changed. Do NOT use it for change ordering.

### File List

- `src/mpd/mod.rs`
- `src/mpd/state_machine.rs`
- `src/mpd/mock.rs`
- `src/main.rs`
- `tests/smoke_test.rs`
