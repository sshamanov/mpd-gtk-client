# Story 14.4: libnotify Notification Integration

Status: done

## Story

As a user,
I want the application to optionally show desktop notifications for connection events,
so that I can be informed when MPD disconnects or reconnects even when the window is minimized.

## Acceptance Criteria

1. **Desktop notifications on connection events**
   - **Given** the application is running and libnotify is enabled in config
   - **When** the MPD connection drops
   - **Then** a persistent desktop notification is shown with the message "MPD disconnected — retrying..."
   - **And** the notification is dismissed when the connection is restored

2. **Opt-in only**
   - **Given** libnotify is disabled in config (default: `[notifications] libnotify = false`)
   - **When** a connection event occurs
   - **Then** no desktop notification is shown
   - **And** in-app toast notifications continue to work normally

3. **Notification types**
   - **Given** a connection-related event occurs
   - **When** libnotify is enabled
   - **Then** the following events trigger notifications:
     - Connection lost → persistent "MPD disconnected — retrying..." (until resolved)
     - Connection restored → brief "MPD reconnected" notification
     - Reconnection failed after all retries → "MPD connection failed — check settings"

4. **Graceful failure**
   - **Given** libnotify is enabled but the notification daemon is not running
   - **When** the application tries to send a notification
   - **Then** the error is logged at debug level
   - **And** the application continues without desktop notifications

## Tasks / Subtasks

- [x] (AC: 1-4) No new dependency — uses existing zbus from MPRIS (story 14-2)
- [x] (AC: 2) Add `[notifications] libnotify = false` to config struct
- [x] (AC: 1, 3) Implement notification service as background thread
  - [x] Polls SharedState.connection for transitions (Connected/Disconnected/Error)
  - [x] Replaces "disconnected" notification with "reconnected" using notification ID
  - [x] Cooldown on repeated disconnect notifications
- [x] (AC: 4) Handle missing D-Bus session bus gracefully (logged at debug)

## Dev Notes

- **No `notify-rust` crate:** Uses `zbus::blocking::Connection` to call `org.freedesktop.Notifications.Notify` D-Bus method directly. Shared zbus connection architecture decided in elicitation (see architecture.md §687).
- **Polling approach:** Background thread polls `SharedState.connection` every 2 seconds for transitions. No event channel plumbing needed.
- **Notification ID tracking:** D-Bus Notify returns a notification ID. We reuse it when reconnecting to replace the "disconnected" notification.
- **Cooldown:** 30-second cooldown on repeated disconnect notifications prevents spam during flapping connections.
- **Timeout:** Persistent (0) for connection failures, 5s for transient notifications.
- **Feature-gated:** Requires `--features mpris` (zbus dependency). Both MPRIS and libnotify share the same zbus compile-time gate.
- **Opt-in by default:** `[notifications] libnotify = false` — privacy-by-design.

### Source Files to Touch
- `src/config/mod.rs` — Add `notifications.libnotify` field
- `src/notifications.rs` — New file, notification service
- `src/main.rs` — Conditional spawn

### Testing
- Unit test for config parsing with notifications section
- Mock notification daemon test
- Edge cases: daemon not running, rapid connect/disconnect events

## References

- [Source: architecture.md §743] Notification & System Integration — libnotify optional integration
- [Source: prd.md §194] System Integration — notification area requirements
- [Source: epics.md §14] Epic 14: CLI & Desktop Integration

## Dev Agent Record

### Agent Model Used

N/A

### Debug Log References

N/A

### Completion Notes List

- Story file created by do-plan workflow
- Uses zbus directly for D-Bus Notifications calls — no notify-rust crate
- Polling approach avoids event channel plumbing (SharedState connection field)
- Feature-gated behind mpris (shared zbus dependency)
- Notifications thread runs independently of MPRIS (creates its own zbus connection)
- 2s poll interval, 30s disconnect cooldown, persistent notification on failure

### File List

- `Cargo.toml`
- `src/config/mod.rs`
- `src/notifications.rs` (new)
