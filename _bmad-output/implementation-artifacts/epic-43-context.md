# Epic 43 Context: Architecture Cleanup and Code Structure Normalization

## Source of Truth

The architecture.md reconciliation review identified 4 stale ADR status labels and 6 structural discrepancies between the authoritative architecture document and the actual source code. The implementation plan was approved and fully executed as of 2026-05-22.

## Epic Goal

Reconcile architecture.md with source code, split the 3141-line monolithic `ui/mod.rs` into maintainable sub-modules, implement deferred architectural commitments (app.rs, strings.rs, single reducer, ErrorSink removal).

## Key Architectural Constraints

From architecture.md:
- **Presenter purity**: `src/presenters/` must never import gtk4/gdk4/gdk-pixbuf — enforced
- **app.rs thin wiring**: Application wiring only — no business logic
- **Single reducer**: `reduce(AppState, MpdEvent)` is the sole state mutation entry point
- **No deferred items**: All architecture.md commitments are in-scope for this epic

## Story Sequence (dependency order)

| # | Story | Depends On | Files Created |
|---|-------|-----------|---------------|
| 43-1 | Standalone quick changes | nothing | None (modifies existing) |
| 43-2 | Extract app.rs wiring | 43-1 | `src/app.rs` |
| 43-3 | Split ui/mod.rs sub-modules | 43-2 | `grid.rs`, `queue.rs`, `now_playing.rs`, `settings.rs`, `help.rs`, `bottom_panel.rs`, `theme.rs` |
| 43-4 | Extract event loop | 43-3 | `src/ui/event_loop.rs` |
| 43-5 | Reducer + ErrorSink cleanup | 43-1 | Modifies `state/mod.rs` |
| 43-6 | Centralize strings.rs | 43-3 | `src/strings.rs` |
| 43-7 | Update architecture.md | all above | Modifies `architecture.md` |

## Critical Cross-Cutting Concerns

- **No regressions**: `cargo build` must pass after each story
- **No GTK behavior changes**: All widget behavior preserved — this is structural refactoring only
- **Thread safety**: Presenters stay pure, UI mutations stay on GTK main thread
- **The tick callback extraction (43-4)**: The hardest step — uses `UiHandles` struct to hold ~80 widget references captured by the closure

## Planning Artifacts Referenced

- `_bmad-output/planning-artifacts/architecture.md` — ADR status labels, module map, reduce() spec, ErrorSink spec
- `.claude/plans/glittery-wiggling-puzzle.md` — Detailed 15-step implementation plan
