# Story 17.3: Monitor and Reconnect D-Bus Session Bus

Status: done

## Story

As a user with MPRIS enabled,
I want the application to recover from a D-Bus session bus restart,
so that MPRIS media controls continue working without restarting the application.

## Acceptance Criteria

1. **D-Bus disconnection detection**
   - **Given** the application is running with MPRIS enabled and the D-Bus session bus is active
   - **When** the D-Bus session bus is restarted (e.g., `dbus-daemon --replace`, session logout/login)
   - **Then** the MPRIS module detects the disconnection within 5 seconds
   - **And** logs a warning: "MPRIS: D-Bus session bus disconnected"

2. **Automatic reconnection with retry**
   - **Given** D-Bus disconnection is detected
   - **When** reconnection is attempted
   - **Then** `Connection::new_session()` is called with retry: 1s, 2s, 4s backoff (max 3 retries)
   - **And** on success, the MPRIS interfaces (`MprisRoot`, `MprisPlayer`) are re-registered on the new connection
   - **And** `request_name("org.mpris.MediaPlayer2.mpdclient")` is called on the new connection

3. **Application continues without MPRIS after retry exhaustion**
   - **Given** all reconnection attempts fail (3 retries exhausted)
   - **When** the retry limit is reached
   - **Then** MPRIS integration is disabled for the session
   - **And** a warning is logged: "MPRIS: failed to reconnect after 3 attempts"
   - **And** the application continues normally (no crash, no hang, no D-Bus-related panics)

4. **Method dispatch uses current connection**
   - **Given** MPRIS method handlers (Play, Pause, Next, etc.) are called via D-Bus
   - **When** the connection is active
   - **Then** method handlers dispatch commands via `cmd_tx.send()` as before
   - **Given** the connection has been lost and not yet reconnected
   - **When** a D-Bus method is called
   - **Then** the method handler returns a D-Bus error gracefully (no crash, no unwrap)

5. **User-visible feedback**
   - **Given** MPRIS reconnection succeeds after a D-Bus restart
   - **When** the interfaces are re-registered
   - **Then** a toast notification is shown: "MPRIS reconnected"
   - **Given** MPRIS reconnection fails permanently
   - **Then** no toast is shown (logging only — MPRIS is not essential)

## Tasks / Subtasks

- [ ] (AC: 1-2) Implement D-Bus connection health monitoring
  - [ ] Spawn a monitoring thread in `mpris::init()` that checks `connection.is_connected()` every 5 seconds
  - [ ] Share the current connection via `Arc<Mutex<Option<Connection>>>` for method dispatch
- [ ] (AC: 2) Implement reconnection with backoff
  - [ ] On disconnect: attempt `Connection::new_session()` with 1s, 2s, 4s delays
  - [ ] On success: re-register `MprisRoot` and `MprisPlayer` interfaces
  - [ ] On success: re-request D-Bus name `org.mpris.MediaPlayer2.mpdclient`
- [ ] (AC: 3) Handle retry exhaustion gracefully
  - [ ] After 3 failed attempts, disable MPRIS for the session
  - [ ] Drop the monitoring thread, log warning
- [ ] (AC: 4) Make method dispatch robust to connection loss
  - [ ] Wrap `connection.object_server().at()` and `cmd_tx.send()` in error-handling
  - [ ] Return D-Bus errors on failure instead of panicking
- [ ] (AC: 5) Add toast notification on successful reconnection
  - [ ] Use existing toast mechanism via `cmd_tx` or shared channel

## Dev Notes

### zbus Connection Monitoring

zbus 5.x's `blocking::Connection` provides `is_connected()` method:
```rust
connection.is_connected()  // returns bool
```

However, this may not detect session bus disconnection immediately. An alternative is to attempt a `Connection::new_session()` periodically (the old connection's internal IO thread will eventually detect the TCP socket closure).

### Architecture

Current `mpris::init()` returns `Option<Connection>` with the lifetime managed by the caller (`_mpris_conn` in `main.rs`). The monitoring thread needs shared ownership of the connection.

The approach:
1. Change `mpris::init()` to wrap the connection in `Arc<Mutex<Option<Connection>>>`
2. Method handlers (MprisRoot, MprisPlayer struct methods) hold a clone of this `Arc<Mutex<Option<Connection>>>` and acquire a lock before each D-Bus call OR read from `state` and send to `cmd_tx` directly (they already do this)
3. Monitoring thread checks connection health, swaps the connection on reconnection

Wait — the current design has method handlers read `SharedState` and send to `cmd_tx` directly. The `Connection` is only used for:
- Registering interfaces (`connection.object_server().at(...)`)
- Requesting the bus name

The method handlers themselves don't use the connection. So the interface registration is the critical part that needs re-doing on reconnection. The `cmd_tx` and `SharedState` remain valid across D-Bus reconnections.

### Simplified Approach

Since method handlers don't depend on the `Connection` object (they use `cmd_tx` and `SharedState`), the reconnection process is:

1. Detect disconnection (via `is_connected()` or a ping)
2. Create new `Connection::new_session()`
3. Register interfaces on the new connection via `connection.object_server().at()`
4. Request the bus name

The old `Connection` is dropped. Method handlers keep working because they don't use the old connection.

### Changes to `mpris::init()`

Current signature:
```rust
pub fn init(cmd_tx: mpsc::Sender<MpdCommand>, state: SharedState, enabled: bool) -> Option<Connection>
```

After change — spawns monitoring thread internally, returns handle to allow graceful shutdown:
```rust
pub struct MprisHandle {
    stop_tx: Option<mpsc::Sender<()>>,
}

pub fn init(cmd_tx: mpsc::Sender<MpdCommand>, state: SharedState, enabled: bool) -> Option<MprisHandle>
```

Or keep it simple and don't return anything — the monitoring thread runs until the process exits. Stop signal via `cmd_tx` or shared `AtomicBool`.

### Monitoring Interval

Check every 5 seconds. This is a reasonable balance between responsiveness and overhead. D-Bus session bus restarts are rare in normal desktop operation.

### Toast Notification

Use the existing toast system. The toast channel should be available — pass a `mpsc::Sender<String>` or similar for toast messages from the MPRIS monitoring thread.

### Testing

- `cargo build` with `--features mpris` passes
- `cargo test` passes (zero regressions)
- Manual: launch with `RUST_LOG=debug`, kill D-Bus via `pkill dbus-daemon --session`, verify reconnection
- Manual: `playerctl --player=mpdclient status` after D-Bus restart
- Manual: `journalctl -f` to verify log messages about disconnection/reconnection
- No automated D-Bus tests in v1 (requires session bus)

### References

- [Source: epics.md §17] Epic 17: MPRIS & SharedState Integration (Story 17.3)
- [Source: src/mpris.rs §333-360] mpris::init() — current init, returns Option<Connection>
- [Source: src/mpris.rs §1-19] Interface setup and module structure
- [Source: src/main.rs §224-233] MPRIS initialization call site
- [Source: architecture.md §681-699] ADR: IPC & CLI Architecture
- [zbus docs: Connection] https://docs.rs/zbus/latest/zbus/blocking/struct.Connection.html#method.is_connected

### Dev Agent Record

#### Agent Model Used

Claude Code (deepseek-v4-flash)

#### Debug Log References

- Deferred from code review of story 14-2 (2026-05-01)
- Review finding: "D-Bus session bus disconnection mid-session — No monitoring or reconnection. Silent degradation on D-Bus restart."

#### Completion Notes List

#### File List

- `src/mpris.rs` (edit — add monitoring thread, reconnection logic, changed init signature)
- `src/main.rs` (edit — update MPRIS init call site if signature changes)
