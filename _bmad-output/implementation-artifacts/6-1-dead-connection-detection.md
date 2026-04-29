---
story_id: "6.1"
story_key: "6-1-dead-connection-detection"
epic: "6: Resilience & Connection Health"
title: "Dead Connection Detection in connected_loop"
status: ready-for-dev
---

# Story 6.1: Dead Connection Detection

As a user, I want the app to detect MPD connection drops mid-session so it can reconnect automatically.

## Acceptance Criteria

1. When `adapter.send_command()` returns IO error or `read_line` returns 0 bytes, `connected_loop` exits
2. State machine transitions to `Disconnected` with preserved backoff
3. `Disconnected` event sent to UI (red indicator)
4. Reconnection proceeds with exponential backoff
5. On reconnect, full state refresh: albums, queue, search index reset

## Technical Context

**File:** `src/mpd/state_machine.rs` — `connected_loop()` function

**Current behavior:** All command handlers log errors with `log::error!` and continue. `fetch_full_update()` returns `None` on error but the loop keeps running. No code path exits `connected_loop` on connection failure.

**Fix:** After each `adapter` call that can fail, check if the error is fatal (IO error, protocol error). If so, return from `connected_loop` to fall through to the outer state machine's reconnect logic.

**Key insight:** The `send_command` method already returns `Err(Error::Protocol("Connection closed"))` on zero-byte reads and `Err(Error::Connection(io_err))` on IO errors. We just need to handle these instead of discarding them.

## Implementation Notes

- In `connected_loop`, after any `adapter` call returns an error, return from the function
- The periodic poll's `fetch_full_update()` returning `None` should also trigger exit
- The outer state machine already handles `Disconnected` → reconnect correctly
- Test: mock MPD server can disconnect and verify reconnect event flow
