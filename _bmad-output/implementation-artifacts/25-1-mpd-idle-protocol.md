# Story 25.1: MPD Idle Protocol Integration

Status: backlog

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

3. **Thread-safe idle break via try_clone**
   - **Given** the worker thread is blocked on `idle`
   - **When** a command arrives via `cmd_rx`
   - **Then** `noidle` is written to a socket clone on the main thread
   - **And** the worker's `idle` returns, allowing command processing
   - **And** after commands are processed, `idle` is re-entered

4. **Fallback when idle unavailable**
   - **Given** MPD < 0.19 or idle returns "unknown command"
   - **When** idle is attempted
   - **Then** fall back to 500ms polling permanently

## References
- [Source: architecture.md §239-249] MPD Idle Protocol ADR
- [Source: architecture.md §430-446] Unix socket auto-detection (try_clone pattern)

## File List
- `src/mpd/state_machine.rs` — Replace polling loop with idle/noildle pattern
- `src/mpd/mod.rs` — Add `idle()`/`noidle()` methods to MpdAdapter
