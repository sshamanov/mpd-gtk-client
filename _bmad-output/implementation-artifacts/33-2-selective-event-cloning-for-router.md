# Story 33.2: Selective MpdEvent Cloning for Router Forwarding

Status: done

## Story

As a developer,
I want only the `MpdEvent` variants that the Notification Router actually processes to be cloned and forwarded,
so that large event variants (CoverRefreshed with JPEG data, AlbumTracks with Vec<path>) are not unnecessarily cloned on every frame.

## Acceptance Criteria

1. **Selective cloning by event variant**
   - Given any `MpdEvent` is received by the GTK thread's event loop
   - When `event.clone()` on line 1953 of `ui/mod.rs` executes
   - Then the clone is replaced with a selective match that only clones variants needed by the Notification Router
   - And for all other variants, no clone occurs and `ftx.try_send` is skipped

2. **Router receives all needed events**
   - Given a `Toast`, `Connected`, or `Disconnected` event arrives
   - When the selective match runs
   - Then the event is cloned and forwarded to the router via `ftx.try_send`
   - And the router processes it normally (no behavior change)

3. **Large variants not cloned**
   - Given the event channel carries a `CoverRefreshed` with 160KB of RGBA data
   - When the GTK thread processes the event
   - Then the cover data is NOT cloned for the router
   - And the UI still processes the event normally for cover art updates

## Technical Requirements

- Current code (ui/mod.rs lines 1953 and 2598):
  ```rust
  let fwd = event.clone();
  // ... process event ...
  let _ = ftx.try_send(fwd);
  ```
- The `event.clone()` happens unconditionally before matching, copying every variant including large ones.
- Replace with:
  ```rust
  let router_event = match &event {
      MpdEvent::Toast { .. } | MpdEvent::Connected | MpdEvent::Disconnected => Some(event.clone()),
      _ => None,
  };
  // ... process event (match on `event`) ...
  if let Some(ev) = router_event {
      let _ = ftx.try_send(ev);
  }
  ```
- The router only matches on `Toast`, `Connected`, and `Disconnected` (confirmed in `router.rs::handle_event` at lines 133-164). All other variants are silently ignored with `_ => {}`.
- This change affects only the clone cost — no behavioral change to event processing or routing.

## References
- [Source: epics.md] Epic 33: Notification Router Lifecycle — Story 33.2
- [Source: deferred-work.md] Code review 28-4-notification-router — event.clone() on every MpdEvent wastes memory
- [Source: src/ui/mod.rs:1953] `let fwd = event.clone();` — the line to replace
- [Source: src/ui/mod.rs:2598] `let _ = ftx.try_send(fwd);` — the send site
- [Source: src/notifications/router.rs:133-164] `handle_event()` — only processes Toast/Connected/Disconnected
