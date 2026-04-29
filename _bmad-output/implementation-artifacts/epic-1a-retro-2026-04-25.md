# Epic 1a Retrospective — Backbone (MVP)

**Date:** 2026-04-25
**Epic:** 1a — Backbone (True MVP)
**Stories:** 1 (1a-1: MPD Connection & App Shell)

## Summary

The vertical slice — MPD connection lifecycle, app shell with 70/30 split, now-playing display, graceful shutdown. Everything after this is additive.

## What Went Well

- **MPD background thread model**: `MpdEventLoop` with `mpsc` channels for command/event communication proved correct. The SyncSender backpressure + `glib::idle_add_local` polling pattern works well for thread-safe UI updates.
- **Exponential backoff**: Connection retry with capped exponential backoff handles MPD disconnection gracefully. The state machine tracks `Disconnected → Connecting → Connected` cleanly.
- **Graceful shutdown**: AtomicBool stop flag + thread join with timeout. SIGTERM/SIGINT handling via GLib unix signal integration.
- **Code review caught critical issues**: CssProvider missing for connection indicator, Paned position hardcoded at 70px instead of 70%, RwLock poisoning not handled — all fixed in review.

## Challenges

- **GTK4 API learnings**: `gtk4::pango::EllipsizeMode` vs `gtk4::EllipsizeMode`, `style_context()` deprecation in GTK 4.10, `WidgetExt::display()` return type difference from Option — several version-specific gotchas.
- **N+1 find album queries**: Added in 1b-1 but the pattern of querying MPD per album was identified as a performance concern during review.
- **Filesystem cover assumption**: Initial implementation assumed local filesystem access for cover art — removed after review established MPD-is-remote rule.

## Key Insights

- **MPD is remote**: Critical architectural decision established — no local filesystem access to MPD's music directory. Cover art must go through MPD protocol.
- **sync_channel with try_send**: Event loss accepted as design trade-off. StateChanged events can be dropped when UI is busy, but last known state is recovered on next poll.
- **RwLock poisoning**: Single-threaded UI means RwLock adds overhead with no concurrent readers. Parking_lot replacement or refcell simplification worth considering.

## Action Items

1. **Keep MPD-is-remote rule** — Documented in `docs/architectural-decisions.md`, referenced in all cover/epic planning
   Owner: Project
   
2. **Consider parking_lot RwLock** — Replace std::sync::RwLock to eliminate poisoning risk
   Owner: Developer
