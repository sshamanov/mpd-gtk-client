# Project Retrospective — mpd-client v1

**Date:** 2026-04-26
**Epics:** 0, 1a, 1b, 2, 3, 4a, 4b, 5a, 5b — all done

## Summary

Complete MPD client with Album Mode (cover grid, hover controls, grouped views, local search), Folder Mode (directory browser, breadcrumbs, cue/DSD detection), queue management (display, play, remove, reorder), settings dialog, and toast notifications. Single Rust binary using GTK4 + MPD TCP protocol.

## Architecture Decisions That Held Up

- **std::thread over tokio** (4.2MB vs 15MB binary) — validated decision, no regrets
- **Single crate over workspace** — simplified builds, module separation is sufficient
- **MPD is remote** — architectural boundary established in Epic 1a, paid off repeatedly. All data flows through MPD protocol, no local filesystem assumptions
- **Channel-based threading** — `mpsc` between GTK main loop and MPD background thread works well. SyncSender backpressure with try_send accepted as trade-off
- **Rc<RefCell<Cell<>>** — shared state pattern for UI thread-only data works but creates verbose type signatures. Type aliases help

## What Went Well

- **Code review caught critical bugs**: Row activation widget mismatch, DSD formula bug, SourceId remove panic, album-artist pairing — all would have shipped broken
- **FlowBox cover grid**: Simple, functional, with homogeneous wrapping. Handles moderate library sizes acceptably
- **Hover controls with GtkOverlay**: Clean pattern with EventControllerMotion for sensitivity gating
- **Grouped views via MPD `list album group`**: Generic parser handles Artist/Date/Genre uniformly
- **Local search index**: Hash-based keyword→album index avoids MPD round-trips for search. Simple but effective
- **Folder tree with breadcrumbs**: Drill-down navigation works, breadcrumb segments are clickable
- **Queue with double-click/right-click/Delete**: Full interaction loop (play, remove, reorder via Shift+Up/Down)

## Challenges

- **GTK4 API learning curve**: Deprecated APIs (Dialog, style_context, allocation), version-specific gotchas (glib Propagation mismatch between 0.20/0.22), method name differences (clicked→emit_clicked)
- **SourceId::remove() panic**: `timeout_add_local_once` auto-removes after firing, calling `remove()` panics. Required refactoring to generation counter pattern
- **Rust 2024 reserved keywords**: `gen` as keyword broke several variable names
- **No test coverage for most features**: Mock MPD server is basic. Integration tests cover 3 smoke tests. Most features are untested against real MPD responses
- **Widget tree traversal fragility**: `row.child()` vs `icon.set_widget_name()` mismatch caused all navigation to be broken until code review caught it

## Key Insights

1. **Test against real MPD before shipping**: Most protocol bugs (tag name mapping, response parsing edge cases) only surface against actual MPD
2. **Cell<Rc<RefCell<...>>> patterns work but are verbose**: UI-thread-only state should use simpler types
3. **Code review subagents are effective**: 150+ findings across 14+ stories, many caught would-be-critical bugs
4. **Debounce/async patterns need generation counters**: Stale data from timers, channels, and event handlers requires explicit sequencing
5. **GTK4 CSS support is limited**: No animations, no pointer-events, pseudo-classes have limited support. Visual effects need different approaches

## Action Items

1. **Run against real MPD** — validation pass before claiming v1 complete
   Owner: Developer
   Priority: High

2. **Add integration tests** — Mock MPD server needs `search any`, `playlistinfo`, `lsinfo` response handlers
   Owner: Developer
   Priority: Medium

3. **Document keyboard shortcuts** — in-app help or tooltip listing
   Owner: Developer
   Priority: Low
