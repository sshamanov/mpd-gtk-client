# Story 6.8: Implement Drop for MpdAdapter to Send Clean MPD Close

Status: done

## Story

As a developer,
I want the `MpdAdapter` to implement `Drop` so that the TCP socket sends `close\n` to MPD before being dropped,
So that MPD never sees an abrupt TCP disconnect from the client.

## Acceptance Criteria

1. **Given** the `MpdAdapter` is dropped (e.g., during error recovery in the state machine, or during any code path where the adapter goes out of scope without an explicit `Close` command)
   **When** Rust's `Drop::drop()` runs
   **Then** the adapter sends `close\n` to the MPD TCP stream before the socket is closed
   **And** the write is best-effort (if the stream is already broken, the error is silently ignored)
   **And** no panic occurs in the Drop impl

2. **Given** an explicit `MpdCommand::Close` has already been sent to MPD before the adapter is dropped
   **When** `Drop::drop()` runs
   **Then** the drop is a no-op (no double-close, no error)

3. **Given** the application shuts down normally
   **When** `main.rs` sends `MpdCommand::Close` and the state machine processes it
   **Then** the `Drop` impl for `MpdAdapter` does not send `close\n` again (idempotent)
   **And** no warning or error is logged from the Drop path

## Tasks / Subtasks

- [ ] Add `closed: AtomicBool` field to `MpdAdapter` struct (AC: 2)
- [ ] Implement `Drop for MpdAdapter` that checks `closed` flag, writes `close\n` if not set (AC: 1)
- [ ] Update `MpdAdapter::close()` (or the `Close` command handler) to set the `closed` flag (AC: 2)
- [ ] Ensure Drop writes are best-effort (`let _ = writeln!`) — no panic, no unwrap (AC: 1)
- [ ] Verify that normal shutdown path via `MpdCommand::Close` + `main.rs` sequence works without triggering Drop's close (AC: 3)
- [ ] Test: simulate an error path where adapter is dropped without explicit Close, confirm `close\n` is sent to MPD (AC: 1)

## Dev Notes

- Currently `MpdAdapter` has no `Drop` implementation. When it goes out of scope, the `TcpStream` is dropped immediately, which sends a TCP RST or FIN to MPD.
- The fix: implement `Drop for MpdAdapter` that writes `close\n` to the writer. The write should be best-effort: `let _ = writeln!(self.writer, "close"); let _ = self.writer.flush();`.
- To avoid double-close, add a `closed: AtomicBool` flag. The existing `close()` method sets it. `Drop` checks and only writes if not set.
- The `MpdAdapter` is used in `src/mpd/state_machine.rs` inside the `Connected` state. Error paths that drop the adapter need the Drop impl to clean up.

### References

- Source: `_bmad-output/planning-artifacts/epics.md` Epic 6, Story 6.8
- Source: `src/mpd/mod.rs` — MpdAdapter struct, close() method
- Source: `src/mpd/state_machine.rs` — Connected state holding MpdAdapter
- ADR: `_bmad-output/planning-artifacts/architecture.md` §168 (MPD Adapter Architecture), §979 (Startup/Shutdown Lifecycle)
