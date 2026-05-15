# Story 34.3: Fix Misleading Notifications Module Doc Comment

Status: done

## Story

As a developer,
I want the notifications module doc comment to accurately describe the event flow,
so that future maintainers are not confused by "architecture-speak" leaking into documentation.

## Acceptance Criteria

1. **Accurate doc comment**
   - Given the `src/notifications/mod.rs` file
   - When line 3 is read
   - Then the comment `Two paths for toast events after 'reduce()':` is corrected
   - And the updated comment accurately describes that toast events arrive from the GTK thread's event loop via the `toast_tx` channel
   - And the comment explains the two actual paths: (1) GTK in-app toast overlay, (2) D-Bus desktop notification via NotificationRouter

2. **No architecture jargon**
   - Given the updated doc comment
   - When a developer reads it
   - Then it does not reference internal architecture concepts like `reduce()` that have no connection to the notification routing

## Technical Requirements

- The `reduce()` function exists in `src/state/mod.rs` and processes state changes, but it has nothing to do with routing events to the notification router.
- The comment `Two paths for toast events after 'reduce()':` suggests that toast events flow through `reduce()` before reaching notifications, which is incorrect.
- The actual flow: GTK thread receives `MpdEvent`s from the event channel, processes them (including calling `reduce()` for state), and then forwards `Toast`/`Connected`/`Disconnected` variants to the Notification Router via the `toast_tx` channel. The Notification Router never calls `reduce()`.
- Fix: Replace the misleading doc comment with an accurate description of the two notification paths and how events reach them.

## References
- [Source: epics.md] Epic 34: General Code Quality — Story 34.3
- [Source: deferred-work.md] Code review 28-4-notification-router — `reduce()` referenced in doc comment but doesn't exist (architecture-speak leak)
- [Source: src/notifications/mod.rs:3] Misleading doc comment
- [Source: src/ui/mod.rs:2598] Actual forwarding site — `ftx.try_send(fwd)`
