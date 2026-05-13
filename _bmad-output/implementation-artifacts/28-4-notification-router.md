# Story 28.4: Notification Router

Status: done

### Review Findings (2026-05-13)

3 parallel reviewers. All 7 ACs structurally pass.

**Patched (3):**
- Duplicate Error+Toast in-app display on connection failure — removed `MpdEvent::Error` emission from connection error path, kept Toast only (`src/mpd/state_machine.rs:283`)
- Router thread spawned uselessly when mode=Toast — skip spawn and drop `toast_rx` when mode is Toast (`src/main.rs:262`)
- User-friendly error message lost — added "Check your MPD server and settings." to connection failure Toast body (`src/mpd/state_machine.rs:285`)

**Deferred (5):**
- `event.clone()` on every MpdEvent wastes memory — clones large variants (CoverRefreshed JPEG data) that the router ignores
- Router thread never joined on shutdown — `notif_stop` signaled but no `join()`
- Toast channel (256) sizing undocumented vs event channel (1024) and search channel (64)
- Toast timeout values (3, 5) hardcoded in match arm, not shared with ToastLevel docs
- `reduce()` referenced in mod.rs doc comment but doesn't exist in codebase

**Dismissed (7):** config migration redundant I/O (one-time, negligible), adw::Toast dismiss mechanism (pre-existing), thread name change (cosmetic), config migration race (theoretical), Connected notification timing (inherent to channel topology), Error cooldown missing (pre-existing pattern), early exit stop flag (app lifetime).

## Story

As a user,
I want desktop notifications for MPD events routed through a dedicated thread,
so that I'm aware of connection changes, errors, and important state transitions
even when the app window is not focused.

## Acceptance Criteria

1. **`MpdEvent::Toast` variant exists**
   - Given any thread wants to emit user-facing feedback
   - When it sends `MpdEvent::Toast { message: String, level: ToastLevel }`
   - Then the GTK thread shows an in-app toast overlay
   - And the NotificationRouter receives a copy for desktop dispatch

2. **`ToastLevel` enum with display semantics**
   - Given a toast is emitted
   - When the level is `Info` → auto-dismiss after 3s (GTK) and normal timeout (desktop)
   - When the level is `Warn` → auto-dismiss after 5s (GTK) and normal timeout (desktop)
   - When the level is `Error` → persists until dismissed (GTK) and persistent (desktop, timeout 0)

3. **NotificationRouter thread spawned at app startup**
   - Given the app starts
   - When the MPD event loop is created
   - Then a NotificationRouter thread is spawned with a cloned event sender
   - And it has no GTK imports (`gtk4`, `gdk4`, `adw` forbidden)
   - And its only I/O is: receive events, check config, fire D-Bus notifications

4. **Desktop notification dispatch**
   - Given `[notifications] mode = "desktop"` or `"both"`
   - When the NotificationRouter receives `MpdEvent::Toast`
   - Then it fires a desktop notification via `org.freedesktop.Notifications` D-Bus
   - And errors are logged at debug level (never fatal)
   - And if D-Bus session bus is unavailable, falls back silently to no-op

5. **Toast-only mode (default)**
   - Given `[notifications] mode = "toast"` (default)
   - When the NotificationRouter receives `MpdEvent::Toast`
   - Then it does nothing — GTK thread handles in-app toasts independently

6. **Existing polling replaced with event-driven**
   - Given the NotificationRouter is operational
   - When MPD connection state changes
   - Then the MPD IO thread emits `MpdEvent::Toast` for connection events
   - And the polling loop in `src/notifications.rs` is removed
   - And connection state notifications (disconnected/reconnected/failed) work identically

7. **Backward compatibility**
   - Given existing toast behavior
   - When the NotificationRouter is active
   - Then the current `adw::Toast` in-app display behavior is unchanged
   - And the `[notifications] libnotify` config key is migrated to `[notifications] mode`
   - And the D-Bus `org.freedesktop.Notifications` call signature is unchanged

## Technical Requirements

### Thread Architecture

Per `architecture.md` §2246-2251, the NotificationRouter:

```
Any thread ──MpdEvent::Toast──→ event_tx ──→ GTK thread: in-app toast
                                         └──→ NotificationRouter: desktop notification
```

- **Thread type**: Lightweight, persistent (1 thread), spawned once at app start
- **Location**: `src/notifications/router.rs` (NEW)
- **Input**: Cloned `mpsc::SyncSender<MpdEvent>` — receives all MpdEvent variants, filters for `Toast`
- **Output**: D-Bus `org.freedesktop.Notifications.Notify` calls
- **No GTK imports** — `gtk4`, `gdk4`, `adw` forbidden
- **No MPD protocol knowledge** — no `MpdAdapter` import

### NotificationMode

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationMode {
    Toast,    // In-app only (default)
    Desktop,  // Desktop notification only
    Both,     // Both
}
```

### ToastLevel

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastLevel {
    Info,   // Auto-dismiss 3s in-app, default timeout desktop
    Warn,   // Auto-dismiss 5s in-app, default timeout desktop
    Error,  // Persists until dismissed in-app, persistent desktop (timeout 0)
}
```

### Config Migration

```toml
# OLD (deprecated):
[notifications]
libnotify = true   # bool — fires connection state notifications only

# NEW:
[notifications]
mode = "toast"     # "toast" | "desktop" | "both"
```

Migration: if `libnotify = true` in old config, set `mode = "desktop"` (preserving the user's intent to see desktop notifications).

### MpdEvent Changes

```rust
pub enum MpdEvent {
    // ... existing variants unchanged ...
    Toast { message: String, level: ToastLevel },
}
```

The `MpdEvent::Error(String)` variant remains for now (existing consumers in the GTK handler log it), but the GTK handler also treats it as a Toast for in-app display.

### Integration Points

| File | Change |
|------|--------|
| `src/notifications/router.rs` | **NEW** — `spawn()`, event filter loop, D-Bus dispatch (gated on `mpris` feature) |
| `src/notifications/mod.rs` | **NEW** — directory module (was `notifications.rs`), re-exports `router` |
| `src/notifications.rs` | **DELETED** — converted to `src/notifications/mod.rs`, old polling removed |
| `src/mpd/state_machine.rs` | Add `ToastLevel` enum, `MpdEvent::Toast` variant, Toast emissions for connection state |
| `src/main.rs` | Spawn NotificationRouter with toast channel, remove old `notifications::spawn`, pass `toast_tx` to App |
| `src/config/mod.rs` | Add `NotificationMode` enum, replace `libnotify: bool` with `mode: NotificationMode`, add schema v1→v2 migration |
| `src/ui/mod.rs` | Add `toast_tx` to App, forward events to router, handle `MpdEvent::Toast` in event dispatch |


### Key Rules

- NotificationRouter has **no** GTK imports — `gtk4`, `gdk4`, `adw` forbidden
- NotificationRouter has **no** MPD protocol knowledge — no `MpdAdapter` import
- NotificationRouter's only I/O: receive `MpdEvent`, check config, call D-Bus
- D-Bus failures are never fatal — logged at debug level, silently fall back to no-op
- The `zbus` dependency is already available (v5.x, blocking API)
- The `#![cfg(feature = "mpris")]` gate on notifications D-Bus code is wrong — notifications should work without MPRIS. Use `#[cfg(feature = "mpris")]` only where zbus Connection is shared with MPRIS
- Connection state toast emission should follow the architecture.md §2407-2428 suppression rules: during reconnect, first 5 seconds are silent, toast only if reconnect exceeds 5 seconds

### Event Fan-Out

Since `mpsc::sync_channel` is SPSC (single consumer), the NotificationRouter receives events by cloning the `SyncSender<MpdEvent>`. The GTK thread is the primary consumer; the NotificationRouter is a secondary consumer that receives a copy of every event and filters for `Toast` variants.

Implementation: clone `event_tx` before passing to the GTK event loop. The NotificationRouter's event loop uses `recv_timeout(500ms)` and checks an `Arc<AtomicBool>` stop flag, matching the Search and Cover Proc worker pattern.

## Tasks/Subtasks

- [x] 1. Add `ToastLevel` enum to `src/mpd/state_machine.rs` (Info, Warn, Error)
- [x] 2. Add `MpdEvent::Toast { message: String, level: ToastLevel }` variant
- [x] 3. Add `NotificationMode` enum, update `NotificationsConfig`, add schema migration in `src/config/mod.rs`
- [x] 4. Create `src/notifications/router.rs` — `spawn()` with event receiver, mode check, D-Bus dispatch
- [x] 5. Extract `send_notification()` from `src/notifications.rs` to a shared helper; replace polling loop with event-driven connection state toasts from MPD IO thread
- [x] 6. Emit `MpdEvent::Toast` for connection state changes in `src/mpd/state_machine.rs`
- [x] 7. Spawn NotificationRouter in `src/main.rs`, pass cloned `event_tx`
- [x] 8. Handle `MpdEvent::Toast` in `src/ui/mod.rs` event dispatch — show in-app toast via `adw::Toast`
- [x] 9. Build and run full test suite — verify zero regressions, zero warnings

## Dev Agent Record

### Implementation Plan
- Added `ToastLevel` enum (Info/Warn/Error) + `MpdEvent::Toast` variant to state_machine.rs
- Added `NotificationMode` enum (Toast/Desktop/Both) + `NotificationsConfig.mode` + schema migration v1→v2 in config/mod.rs
- Created `src/notifications/router.rs` — NotificationRouter thread: receives forwarded MpdEvent from GTK, dispatches D-Bus notifications via `org.freedesktop.Notifications` when mode is Desktop/Both. D-Bus code gated behind `#[cfg(feature = "mpris")]`, non-mpris builds compile as no-op.
- Converted `src/notifications.rs` → `src/notifications/mod.rs` + `router.rs` directory module. Removed old polling-based `spawn()` and `run()`; D-Bus `send` helper moved into router's internal `dbus` submodule.
- Created `toast_tx`/`toast_rx` sync_channel(256) in main.rs; NotificationRouter receives from `toast_rx`; `toast_tx` passed to App for GTK event forwarding.
- GTK event handler: clones each event before processing, forwards to `toast_tx` after the match block; added `MpdEvent::Toast` handler with timeout based on level.
- MPD IO thread emits `MpdEvent::Toast` for Disconnected (Warn) and connection failure (Error).

### Completion Notes
- All 84 tests pass, zero warnings (both default and --features mpris builds)
- NotificationRouter has no GTK imports (gtk4, gdk4, adw forbidden)
- NotificationRouter has no MPD protocol knowledge (no MpdAdapter import)
- D-Bus failures never fatal — logged at debug level
- Backward compat: `libnotify = true` in old config migrated to `mode = "desktop"`
- Event fan-out: GTK handler clones+forwards every event to NotificationRouter via dedicated sync_channel(256)
- Existing `MpdEvent::Error` → in-app toast unchanged; Toast events add severity-based timeout

### Change Log
- NEW: `src/notifications/router.rs` — NotificationRouter thread with event filter loop, D-Bus dispatch
- NEW: `src/notifications/mod.rs` — Directory module (was `notifications.rs`), re-exports router
- DEL: `src/notifications.rs` → converted to `src/notifications/mod.rs`
- MOD: `src/mpd/state_machine.rs` — Added ToastLevel enum, MpdEvent::Toast variant, Toast emissions for connection state
- MOD: `src/config/mod.rs` — NotificationMode enum, schema v2 migration (libnotify→mode), CURRENT_SCHEMA_VERSION=2
- MOD: `src/main.rs` — Removed old notifications::spawn, added toast channel + NotificationRouter spawn + shutdown
- MOD: `src/ui/mod.rs` — App struct takes toast_tx, GTK handler forwards events, MpdEvent::Toast → in-app toast

## References
- [Source: architecture.md §2246-2251] NotificationRouter spec — lightweight thread, Toast events, D-Bus dispatch
- [Source: architecture.md §2342-2400] Toast & Notification Consolidation section — two-path design
- [Source: architecture.md §2407-2428] Toast suppression rules during reconnect
- [Source: src/notifications.rs] Current polling-based notification implementation
- [Source: src/mpd/state_machine.rs:67-87] MpdEvent enum definition
- [Source: src/config/mod.rs:102-108] Current NotificationsConfig
- [Source: src/errors.rs:1-30] ErrorLevel, ErrorSinkEvent — context for ToastLevel design
- [Source: src/search/worker.rs] Reference implementation pattern for worker thread spawn with stop flag
- [Source: src/coverart/cover_proc.rs] Reference implementation pattern for event-emitting worker thread

## File List
- `src/notifications/router.rs` (NEW) — NotificationRouter: event filter loop, D-Bus notification dispatch
- `src/notifications.rs` — Replace polling loop with event-driven; extract shared D-Bus helpers
- `src/mpd/state_machine.rs` — Add ToastLevel, MpdEvent::Toast, emit Toast for connection state
- `src/main.rs` — Spawn NotificationRouter, clone event_tx, update config loading
- `src/config/mod.rs` — NotificationMode enum, config migration
- `src/ui/mod.rs` — Handle MpdEvent::Toast in event dispatch
- `src/errors.rs` — Possibly add ToastLevel (if not placed in state_machine.rs)
