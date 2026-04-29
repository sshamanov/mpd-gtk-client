# Story 5a.1: Toast Notifications

Status: ready-for-dev

## Story

As a user,
I want to see transient notifications for important events,
so that I'm aware of queue changes and connection status without intrusive dialogs.

## Acceptance Criteria

1. **Toast notification system** — Overlay widget in the bottom-right corner of the window:
   - 3-second auto-dismiss
   - Stacked (max 3 visible)
   - Click to dismiss
   - Slide-in animation (CSS transition)

2. **Queue sync notification** — When queue is updated by external changes:
   - Toast: "Queue synchronized with N change(s)"
   - Triggered when `MpdEvent::Queue` arrives with different content

3. **`cargo test` passes**

## Tasks / Subtasks

- [ ] Task 1: Build toast notification widget (AC: 1)
- [ ] Task 2: Wire to MPD events (AC: 2)
- [ ] Task 3: Verify no regressions (AC: 3)

## Dev Notes

### Toast Implementation

```rust
struct ToastOverlay {
    container: Fixed,
    toasts: Vec<Toast>,
}
fn push_toast(&mut self, message: &str) {
    let label = Label::new(Some(message));
    // Position at bottom-right, auto-dismiss after 3s
    glib::timeout_add_once(3s, || { remove_toast(); });
}
```

### What NOT to Do
- Do NOT implement settings dialog
- Do NOT implement mode orchestrator (5a-2)

## Dev Agent Record

### Completion Notes List

### File List
