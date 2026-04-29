# Story 6.6: Graceful MPD Shutdown

Status: review

## Story

As a developer,
I want MPD to receive a clean `close` command on app shutdown,
so that the server doesn't see an abrupt disconnect and the client exits cleanly.

## Acceptance Criteria

1. **Given** the app is shutting down normally (window closed, Ctrl+Q, SIGINT/SIGTERM)
   **When** the shutdown sequence begins
   **Then** a `close` command is sent to MPD before the TCP socket is dropped
   **And** the MPD background thread exits within 100ms of receiving the shutdown signal
   **And** `process::exit(0)` is replaced with a proper GTK lifecycle shutdown that drains pending events

2. **Given** SIGINT or SIGTERM is received
   **When** the signal handler fires
   **Then** the GTK application's `quit()` is called instead of `std::process::exit(0)`
   **And** the close command is sent through the MPD command channel
   **And** control flow returns to `main()` after the GTK main loop exits

3. **Given** the mock MPD server is used in tests
   **When** the `close` command is sent
   **Then** the mock server breaks out of its client handler cleanly (already implemented: `mock.rs` line 106)

## Tasks / Subtasks

- [x] Task 1: Add `Close` variant to `MpdCommand` enum (AC: #1)
  - [x] Add `Close` to `MpdCommand` in `state_machine.rs`
  - [x] Handle `MpdCommand::Close` in `connected_loop`: send `"close"` via `adapter.send_command()`, then return to exit the loop
  - [x] The close command should be the last action before the loop exits — no status fetch after

- [x] Task 2: Replace `process::exit(0)` with `app_handle.quit()` in signal handlers (AC: #2)
  - [x] In `main.rs`, change SIGINT handler to call `glib::idle_add(|| { app_handle.quit(); ControlFlow::Break })` instead of `std::process::exit(0)`
  - [x] Same for SIGTERM handler
  - [x] The signal handlers already store the stop flag — keep that behavior
  - [x] Challenge: `app_handle` is not available at the signal handler registration point — it's created inside `app.run()`. Solution: use `glib::idle_add` with a channel or shared cell that gets wired up when the app activates.

- [x] Task 3: Send `Close` command after `app.run()` returns (AC: #1)
  - [x] After `app.run()` returns, send `MpdCommand::Close` via `cmd_tx`
  - [x] Wait briefly (100ms max) for the event loop to process the close
  - [x] Then call `event_loop.signal_stop()` and join the thread as before
  - [x] Update the comment and log messages to reflect the new shutdown flow

- [x] Task 4: Update integration tests (AC: #3)
  - [x] Verify existing tests still pass after the changes
  - [x] The mock server already handles `close` by breaking the client loop (line 106 of `mock.rs`)

## Dev Notes

### Current State

**main.rs (lines 45-83):**
```rust
// SIGINT handler currently does:
sig_stop.store(true, Ordering::Release);
glib::idle_add(|| std::process::exit(0));  // <-- This is the problem
glib::ControlFlow::Break

// SIGTERM handler: same pattern

// After app.run():
info!("Shutting down MPD connection");
event_loop.signal_stop();  // <-- Sets AtomicBool, but thread gets killed by process::exit
```

The `std::process::exit(0)` call kills the process immediately — no destructors run, no `close` sent, the code after `app.run()` never executes.

**state_machine.rs:**
- `MpdEventLoop::spawn()` creates a thread with the state machine loop
- `connected_loop()` handles commands in a `recv_timeout` loop
- When `cmd_rx` is disconnected (sender dropped), `RecvTimeoutError::Disconnected` is returned and `connected_loop` exits
- No `Close` command variant exists yet

**Architecture reference: `architecture.md` §3c/§7:**
- "ShuttingDown flag on AppState (backed by AtomicBool)"
- "MPD IO thread: exits idle loop, closes socket, terminates."
- Shutdown order: flag -> persist state -> UI teardown -> MPD disconnect -> compute stop -> log flush
- SIGTERM/SIGINT handlers initiate graceful shutdown; second signal forces immediate exit

### Architecture Compliance

1. **Threading model**: All MPD communication stays on the background thread. The `Close` command is sent through the existing `cmd_tx` channel, same as any other `MpdCommand`. The GTK main loop is not involved in the actual close wire protocol.

2. **Shutdown order** (from architecture.md §5): ShuttingDown flag -> persist state -> UI teardown -> MPD disconnect -> compute stop -> log flush. This story addresses the "MPD disconnect" phase — the `close` command sent over TCP. State persistence and log flush are out of scope.

3. **No unwrap/expect**: The `Close` command send must use `if let Ok(...)` or similar fallible patterns, since the channel might already be disconnected during shutdown.

4. **Mock compatibility**: The mock MPD server (`mock.rs` line 106) already handles `close` by breaking out of the client handler loop. No mock changes needed.

### Signal Handler Design

The signal handlers cannot use `std::process::exit(0)` because that prevents cleanup. Instead:

```rust
// Use a shared OnceCell or similar to wire up the app handle
// OnceCell pattern (not actual implementation):
static APP_HANDLE: OnceLock<gtk4::Application> = OnceLock::new();

// In main(), before signal handlers:
// let app = App::new(...);
// APP_HANDLE.set(app.get_application_handle()).ok();

// In signal handler:
glib::idle_add(|| {
    if let Some(app) = APP_HANDLE.get() {
        app.quit();
    }
    glib::ControlFlow::Break
});
```

A simpler alternative: use a shared `Option<Application>` behind a `Mutex` that gets set during `connect_activate`. Or even simpler: signal handlers set a flag, and the main event loop checks it at idle.

Simplest viable approach: Store the `Application` handle in a `OnceLock` or `static Mutex<Option<Application>>` that gets set inside `connect_activate`, and the signal handlers call `app.quit()` via `glib::idle_add`.

### Implementation Details

**MpdCommand::Close handling in connected_loop:**
```rust
MpdCommand::Close => {
    log::info!("[MPD] received Close command, sending close to MPD");
    let _ = adapter.send_command("close");
    return;  // Exit connected_loop — outer state machine sees Disconnected and will check stop flag
}
```

**main.rs shutdown after app.run():**
```rust
info!("Shutting down MPD connection");

// Send close command to MPD for graceful shutdown
if cmd_tx.send(MpdCommand::Close).is_ok() {
    // Give the event loop a moment to process the close
    thread::sleep(Duration::from_millis(100));
}

event_loop.signal_stop();

// Join with timeout (unchanged)
let handle = event_loop.into_handle();
let deadline = Instant::now() + Duration::from_secs(3);
// ... rest of join loop unchanged
```

### Source Tree Components to Touch

| File | Change |
|------|--------|
| `src/mpd/state_machine.rs` | Add `Close` variant to `MpdCommand`; handle it in `connected_loop` |
| `src/main.rs` | Replace `process::exit(0)` with `SHUTDOWN_REQUESTED` flag; send `Close` after `app.run()` |
| `src/ui/mod.rs` | Check `SHUTDOWN_REQUESTED` in 30ms timer; call `app.quit()` via captured handle |
| `src/mpd/mock.rs` | Record `close` command in received list before breaking |
| `src/lib.rs` | Add `pub(crate) static SHUTDOWN_REQUESTED: AtomicBool` |

### Testing Standards Summary

- `cargo test` must pass — the mock MPD server already handles `close`
- No new integration tests needed for this story, but existing tests must pass
- Manual verification: app exits cleanly without error messages about dropped connections

### References

- [Source: epics.md §Story 6.6] — Graceful MPD Shutdown requirements
- [Source: architecture.md §3c] — Shutdown Coordination
- [Source: architecture.md §7] — Shutdown phase sequence
- [Source: architecture.md §5] — Shutdown Phase Ordering (ShuttingDown flag -> persist -> UI teardown -> MPD disconnect -> compute stop -> log flush)
- [Source: main.rs:45-83] — Current signal handlers and shutdown code
- [Source: state_machine.rs:13-37] — MpdCommand enum
- [Source: mock.rs:106] — Mock server close handling

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### File List

- `src/mpd/state_machine.rs` — Add `Close` variant and handler
- `src/main.rs` — Replace `process::exit(0)` with `SHUTDOWN_REQUESTED` flag; send `Close` after `app.run()`
- `src/ui/mod.rs` — Check `SHUTDOWN_REQUESTED` in 30ms timer; call `app.quit()` via captured `shutdown_app` handle
- `src/mpd/mock.rs` — Record `close` command in received list before breaking the client loop
- `src/lib.rs` — Add `pub(crate) static SHUTDOWN_REQUESTED: AtomicBool`
- `tests/smoke_test.rs` — Add `test_close_command` integration test

### Completion Notes

- Implementation uses `AtomicBool` flag approach: signal handlers set `SHUTDOWN_REQUESTED`, the 30ms UI timer detects it and calls `app.quit()` via a captured Application handle. This avoids needing to store the non-Send `gtk4::Application` in a static.
- `Close` variant added to `MpdCommand`; handler sends `"close"` via `adapter.send_command("close")`, then sets the stop flag and returns from `connected_loop`.
- After `app.run()` returns in `main()`, the `Close` command is sent through the cloned `cmd_tx` channel. A 100ms sleep gives the MPD thread time to process it before `signal_stop()` is called.
- The mock server was updated to record `close` in the received commands list (previously it broke without recording).
- All 12 tests pass (11 existing + 1 new `test_close_command`).
