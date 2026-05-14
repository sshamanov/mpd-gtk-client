# Story 31.2: Prevent MPD Cover Thread Stall When Cover Proc Panics

Status: done

## Story

As a developer,
I want the MPD Cover thread to detect when the Cover Proc worker has stopped consuming results,
so that the MPD Cover thread does not block forever on a full result channel.

## Acceptance Criteria

1. **Non-blocking send with stall detection**
   - Given the Cover Proc worker panics and stops consuming from the result channel
   - When the MPD Cover thread attempts `result_tx.send(result)`
   - Then the send does not block indefinitely
   - And the MPD Cover thread detects the stall and exits via its idle timeout (30s)
   - And a log message is emitted at error level

2. **Normal operation unaffected**
   - Given the Cover Proc worker is running normally
   - When the MPD Cover thread sends results
   - Then behavior is unchanged — results are delivered as before
   - And no additional latency is introduced

## Technical Requirements

- `mpsc::SyncSender::send()` blocks when the channel buffer is full. If the Cover Proc worker panics, it stops receiving, the channel fills up, and the MPD Cover thread blocks forever.
- The `result_tx` in `src/mpd/cover.rs` uses `let _ = result_tx.send(...)` which is blocking via `SyncSender::send`.
- Fix options:
  - (a) Replace `SyncSender::send` with a loop using `try_send` + sleep + stop-flag check → simpler, keeps `std::sync::mpsc`
  - (b) Use `send_timeout` from a crossbeam channel → adds dependency
  - Option (a) is preferred: replace `let _ = result_tx.send(result)` with `loop { match result_tx.try_send(result) { Ok(_) => break, Err(TrySendError::Full(_)) => { if stop.load(...) { break; } thread::sleep(small_delay); }, Err(TrySendError::Disconnected(_)) => break } }`
- The MPD Cover thread already has a 30-second idle timeout that will naturally shut it down if no new jobs arrive

## References
- [Source: epics.md] Epic 31: Cover Pipeline Reliability — Story 31.2
- [Source: deferred-work.md] Code review 28-2-cover-proc-worker — MPD Cover thread blocking send can stall permanently if Cover Proc panics
- [Source: src/mpd/cover.rs:99-153] Blocking `result_tx.send()` calls
- [Source: src/coverart/cover_proc.rs] Cover Proc worker that receives results
