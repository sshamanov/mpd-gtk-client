# Story 33.1: Join Notification Router Thread on Shutdown

Status: done

## Story

As a developer,
I want the notification-router thread to be joined on shutdown,
so that the thread's resources are properly cleaned up and we can verify it exited cleanly.

## Acceptance Criteria

1. **spawn() returns JoinHandle**
   - Given `notifications::router::spawn()` is called
   - When the function creates the background thread
   - Then it returns `std::thread::JoinHandle<()>`
   - And the caller stores the handle for later joining

2. **Thread joined on shutdown**
   - Given the application is shutting down
   - When `notif_stop.store(true, ...)` is called
   - Then the notification-router thread exits within 500ms
   - And the main thread calls `join()` on the thread handle (with 1-second timeout)
   - And if the thread does not exit in time, a warning is logged but shutdown proceeds

3. **Normal exit logging unchanged**
   - Given the notification-router thread exits cleanly
   - When it receives the stop signal
   - Then the existing `log::info!("[notification-router] Thread terminated")` message is logged

## Technical Requirements

- Currently `notifications::router::spawn()` returns nothing — the `JoinHandle` from `std::thread::Builder::spawn()` is discarded.
- Fix: Change `spawn()` signature to return `JoinHandle<()>`.
- In `main.rs`: store the handle and call join on shutdown.
- Join with timeout: `handle.is_finished()` polling with `thread::park_timeout(Duration::from_millis(100))` (same pattern as the MPD event loop join in main.rs lines 300-312).
- The join code should be placed alongside the existing notif_stop signal (main.rs line 315) so it runs before exit.
- The router thread's `rx.recv_timeout(Duration::from_millis(500))` loop already checks `stop.load(Ordering::Relaxed)` every iteration, so response time is <500ms.

## References
- [Source: epics.md] Epic 33: Notification Router Lifecycle — Story 33.1
- [Source: deferred-work.md] Code review 28-4-notification-router — Router thread never joined on shutdown
- [Source: src/notifications/router.rs:91-124] spawn() function — needs to return JoinHandle
- [Source: src/main.rs:257-266] NotificationRouter spawn site
- [Source: src/main.rs:300-312] Existing MPD event loop join pattern — follow this pattern
