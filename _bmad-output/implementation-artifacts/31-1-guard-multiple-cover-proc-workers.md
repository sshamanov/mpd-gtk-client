# Story 31.1: Guard Against Multiple Cover Proc Workers on Reconnect

Status: done

## Story

As a developer,
I want only one Cover Proc worker thread active at a time,
so that concurrent index.json writes from multiple workers cannot corrupt the cache index.

## Acceptance Criteria

1. **Single Cover Proc worker guaranteed**
   - Given the MPD connection drops and reconnects
   - When a new Cover Proc worker is spawned before the old one has fully terminated
   - Then the new spawn is blocked until the old worker has exited
   - And index.json is never read or written by two threads simultaneously

2. **Worker clears running guard on exit**
   - Given the Cover Proc worker is running
   - When the worker exits (stop flag or channel disconnect)
   - Then the running guard is cleared atomically

3. **Backward compatibility**
   - Given all existing tests pass
   - When the guard is applied
   - Then `cargo test` shows zero regressions

## Technical Requirements

### Fix: Arc<AtomicBool> Running Guard

1. Add `running: Arc<AtomicBool>` parameter to `cover_proc::spawn()`
2. CAS `false → true` before spawning; if already true, log warn + return
3. Worker clears flag on all exit paths
4. Guard created in state machine thread outside `connected_loop`, survives reconnections

### Key Files

| File | Change |
|------|--------|
| `src/coverart/cover_proc.rs` | Add `running` param, CAS check, clear on exit |
| `src/mpd/state_machine.rs` | Create guard, pass through connected_loop to spawn |

## References
- [Source: epics.md §1473-1488] Story definition
- [Source: src/coverart/cover_proc.rs:39-84] spawn() implementation
- [Source: src/mpd/state_machine.rs:374-382,461-496] Reconnect cycle and connected_loop
