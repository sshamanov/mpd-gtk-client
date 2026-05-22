# Story 43-7: Final Architecture.md Reconciliation

## Status: done

## Context

After all code changes from stories 43-1 through 43-6 are complete, update architecture.md to reflect the final state. This is the doc-only story that closes the reconciliation loop.

## Tasks

### Task 1: Update module map (§1503-1534)
Replace the old `ui/` listing with the new sub-module structure:
```
src/ui/
  mod.rs             — scaffolding, build_ui(), sub-module declarations
  grid.rs            — AlbumCell, reposition(), placeholder functions
  queue.rs           — MiniGridItem, rebuild_mini_fixed(), queue list
  now_playing.rs     — PlaybackDisplay, handle_now_playing(), update_now_playing()
  settings.rs        — Settings dialog builder
  help.rs            — Shortcuts help dialog
  bottom_panel.rs    — Bottom transport bar for narrow mode
  theme.rs           — CSS, high-contrast, memory monitoring
  event_loop.rs      — UiHandles struct, process_events() tick callback
  gtk_reexport.rs    — GTK re-exports for widget code
  widgets/           — Reusable custom widgets
```

### Task 2: Update structural notes
- ~1555: Note CSS embedding uses `include_str!` (not gresource, no build.rs)
- ~1666: Update `app.rs` description — thin wiring with `run()` function
- ~2225: Update thread count table — add metadata thread + MPRIS threads
- ~885-894: Update ErrorSink section — replaced by `MpdEvent::Toast`
- ~1529: Remove "implementation is deferred" note for ui sub-module split

### Task 3: Update implementation readiness section
- Remove references to deferred gaps that are now resolved
- Note that `strings.rs` exists for centralized strings
- Note that `app.rs` exists for application wiring

### Task 4: Add note about MpdEvent vs AppEvent naming
Architecture.md §4a specifies `AppEvent` enum. The code uses `MpdEvent` for the same role. Add a note: "MpdEvent serves the AppEvent role described in Consolidated Refinements §4."

## Acceptance Criteria
1. Architecture.md module map matches actual code structure
2. All 4 stale ADR status labels updated (done in Story 43-1, verify here)
3. Deferred-gap notes removed or marked resolved
4. Document passes review — no "should exist but doesn't" contradictions
