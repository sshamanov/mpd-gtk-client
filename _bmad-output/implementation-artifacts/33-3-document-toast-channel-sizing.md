# Story 33.3: Document Toast Channel Sizing

Status: done

## Story

As a developer,
I want the sizing rationale for the Toast channel (256), event channel (1024), and search channel (64) documented,
so that future maintainers understand the capacity planning.

## Acceptance Criteria

1. **Channel buffer size rationale documented**
   - Given the channel instantiation sites
   - When a developer reads the code
   - Then each channel's buffer size is accompanied by a comment explaining the rationale for the chosen capacity
   - And the three channel sizes are cross-referenced with their relative throughput expectations

2. **Three channels documented**
   - Given the Toast/NotificationRouter channel (`mpsc::sync_channel::<MpdEvent>(256)` in main.rs line 260)
   - When a developer reads the instantiation site
   - Then a comment explains: Toast events are rare (connection state changes, at most a few per minute), so 256 slots is generous — never fills under normal operation; backpressure is not a concern for this channel
   - Given the Search worker command channel (`mpsc::sync_channel::<SearchCommand>(64)` in state_machine.rs line 329)
   - When a developer reads the instantiation site
   - Then a comment explains: Search commands are triggered by keystrokes (max ~6 per second with 150ms debounce), so 64 slots absorbs bursts; `try_send` drops when full as intentional backpressure
   - Given the event channel (1024, wherever it's defined)
   - When a developer reads the instantiation site
   - Then a comment explains: Event channel carries all MPD events including rapid cover updates during scrolling; 1024 slots absorbs the burst from a full grid population

## Technical Requirements

- No behavioral changes — documentation only.
- Add inline comments at each channel instantiation site.
- The event channel (1024) is likely in `main.rs` or `state_machine.rs` — find and document it.
- The search channel (64) is at `state_machine.rs` line 329.
- The toast channel (256) is at `main.rs` line 260.

## References
- [Source: epics.md] Epic 33: Notification Router Lifecycle — Story 33.3
- [Source: deferred-work.md] Code review 28-4-notification-router — Toast channel (256) sizing undocumented
- [Source: src/main.rs:260] Toast channel instantiation
- [Source: src/mpd/state_machine.rs:329] Search channel instantiation
