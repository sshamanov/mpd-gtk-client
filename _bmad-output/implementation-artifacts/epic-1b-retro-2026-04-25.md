# Epic 1b Retrospective — Album Experience

**Date:** 2026-04-25
**Epic:** 1b — Album Experience
**Stories:** 4 (1b-1 through 1b-4, all done)

## Summary

The album browsing experience: cover grid, hover controls, grouped views, and basic search. 4 stories built on the Epic 1a shell.

## What Went Well

- **GtkOverlay for hover buttons**: Overlay with `EventControllerMotion` for sensitivity toggling works well. CSS opacity transition + sensitivity gating prevents accidental clicks on invisible buttons.
- **FlowBox for cover grid**: Simple grid layout with homogeneous wrapping. Works well for the flat album view with cell size_request.
- **Grouped views**: MPD's `list album group {type}` command provides clean grouped data. The generic parser handles Artist/Date/Genre uniformly.
- **Search with debounce**: `glib::timeout_add_local_once` with `Cell<Option<SourceId>>` cancellation pattern is correct. 150ms debounce feels responsive.
- **Error logging**: `log::error!` added to all previously-silent error paths (MPD command failures, RwLock poisoning).
- **Double-click wiring**: `FlowBox::connect_child_activated` with `set_activate_on_single_click(false)` maps correctly to double-click for play-album.
- **Code review rigor**: Each story's code review caught critical issues — tag name mapping, artist-album pairing, timer cancellation, group restore on search clear.

## Challenges

- **MPD tag name mapping (1b-3 CRITICAL)**: Button labels "Artists"/"Years"/"Genres" sent verbatim as MPD tag names. MPD expects "Artist"/"Date"/"Genre". Grouped views were entirely non-functional against a real MPD server until fixed.
- **Album-artist pairing bug (1b-4 CRITICAL)**: Deferred push pattern in `search_albums` used the wrong artist — pushing previous album with current artist from a different track.
- **FlowBox homogeneous with headers**: Headers in grouped mode couldn't span full width. Required non-homogeneous mode with size_request workaround.
- **ToggleButton radio grouping**: Initial implementation had no mutual exclusion — multiple buttons could appear simultaneously active.
- **Search debounce timer not cancelled on stop_search**: Escape/clear didn't cancel pending timer, allowing stale search to overwrite group restore.
- **Backspace-to-empty**: No grouped view restore when user cleared search field character by character (only Escape worked).

## Key Insights

- **Always test against actual MPD**: Grouped views tag mapping bug would have been caught immediately by testing against a real server.
- **GTK4 radio behavior**: `ToggleButton` requires explicit `set_group()` for mutual exclusion — not automatic.
- **Two-pass parsing for MPD responses**: Complex MPD response parsing (like `search any` with album dedup) needs careful handling. First-pass artist mapping, second-pass result building eliminates cross-album artist bleeding.
- **Timer lifecycle**: Debounce timers must be cancelled in ALL exit paths (stop_search, search_changed empty, search_changed new query) to prevent stale callback races.
- **N+1 queries are expensive**: `list album group` followed by per-album `find` queries blocks the MPD thread. Always prefer single-query grouped responses.

## Action Items

1. **Test against real MPD before marking complete** — Some bugs (tag mapping, protocol edge cases) only surface against actual MPD
   Owner: Process
   
2. **Standardize MPD escaping** — Escape `\n`, `\r` in all command-building paths (`addid`, `find_album_uris`, `search_albums`)
   Owner: Developer
   Priority: Medium (pre-existing surface, not triggered in practice)
   
3. **Keep two-pass parsing pattern** — For MPD responses where cross-entry state matters, two-pass parsing prevents bugs
   Owner: Process (design guidance)
   
4. **Add generation counter to async requests** — Prevent stale search results from overwriting newer ones
   Owner: Developer
   Priority: Low (partially wired in 1b-4, filtering not implemented)
