# Story 36.1: Add MpdCommand::Batch Variant

Status: done

## Story

As a developer,
I want an `MpdCommand::Batch(Vec<MpdCommand>)` variant,
so that atomic bulk operations can be sent as a single channel message and executed as a single MPD command list.

## Acceptance Criteria

1. **Given** the MPD state machine processes an `MpdCommand::Batch(commands)`
   **When** the command is dispatched
   **Then** `adapter.command_list_begin()` is called before the first sub-command
   **And** each sub-command is written to the socket via `adapter.send_raw()`
   **And** `adapter.command_list_end()` is called after the last sub-command
   **And** responses are consumed until OK for each command in sequence

2. **Given** any sub-command in the batch fails (returns ACK)
   **When** the failure is detected
   **Then** the entire batch is aborted per MPD protocol semantics
   **And** an error is logged with the failing command and batch context
   **And** a `MpdEvent::Toast { level: Error, message: "Batch operation failed: ..." }` is emitted

3. **Given** a batch is empty
   **When** it reaches the dispatch handler
   **Then** it is a no-op (no MPD commands sent, no errors)

## Tasks / Subtasks

- [ ] Add `Batch(Vec<MpdCommand>)` variant to `MpdCommand` enum (AC: #1)
- [ ] Add `send_raw(&self, cmd: &str)` method to `MpdAdapter` that writes to stream without parsing response (AC: #1)
- [ ] Implement dispatch handler for Batch: call command_list_begin, iterate sub-commands with send_raw, call command_list_end (AC: #1)
- [ ] Add error handling: on first ACK response, abort batch, consume remaining responses, emit Toast (AC: #2)
- [ ] Add empty-batch guard: return immediately if commands vec is empty (AC: #3)
- [ ] Test: verify batch with 3 valid commands succeeds (mock MPD server)
- [ ] Test: verify batch with a failing command aborts and emits error toast

## Dev Notes

- `MpdAdapter::command_list_begin()` and `command_list_end()` already exist at `src/mpd/mod.rs:386-391`
- Need `send_raw()` on the adapter — writes a command line to the socket without waiting for a response line
- During batch mode, responses must still be consumed: each `command_list_begin` command produces an individual `list_OK` response, plus a final `OK` after `command_list_end`
- Error handling: iterate commands, after each send_raw, read one response line; if it starts with "ACK", emit error, skip remaining, still read responses to drain, then return
- The Batch variant should work with any `MpdCommand` variant that can be serialized to a single MPD command line

### References

- [Source: architecture.md#689] — Command List Batching ADR
- [Source: src/mpd/mod.rs:381-391] — existing command_list_begin/end on adapter
- [Source: src/mpd/state_machine.rs:17-63] — MpdCommand enum

## Dev Agent Record

### Agent Model Used

deepseek-v4-flash

### Completion Notes List

### File List
- src/mpd/mod.rs
- src/mpd/state_machine.rs
- src/mpd/mock.rs
