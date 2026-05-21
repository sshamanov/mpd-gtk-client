# Story 43-1: Standalone Quick Changes

## Status: done
baseline_commit: 76a9c9772d0f93de20bd1713be20483e33d4d0fd

## Context

Architecture.md and source code have diverged. Three quick standalone fixes that don't depend on anything else. See `.claude/plans/glittery-wiggling-puzzle.md` Steps 1-3 for full context.

## Tasks

### [x] Task 1: Fix application ID (#5)
- **File:** `src/ui/mod.rs:347`
- Change `"com.github.schaman.mpd-client"` to `"com.mpdclient.app"`
- **Verify:** `cargo build` passes

### [x] Task 2: CSS embedding — document decision
- The architecture.md calls for `gresource` + `build.rs` for CSS embedding
- Code already uses `include_str!("style.css")` at `ui/mod.rs:2865` — this embeds CSS at compile time, functionally equivalent to gresource but simpler
- Add a note to architecture.md line ~1555: CSS uses `include_str!`, not gresource. No build.rs needed.
- **Verify:** `cargo build` passes

### [x] Task 3: Delete dead ErrorSinkEvent
- **File:** `src/errors.rs`
- Remove `ErrorSinkEvent` enum (lines 15-19) — confirmed unused via grep
- Remove `UserFacingError` trait — confirmed unused via grep
- Remove `ErrorLevel` enum — duplicated by `ToastLevel` in `mpd/state_machine.rs`
- Keep module with empty docstring or minimal content
- **Verify:** `cargo build --lib` passes, `grep -rn "ErrorSinkEvent\|UserFacingError\|ErrorLevel" src/` returns nothing

### [x] Task 4: Architecture.md status labels
- Update 4 stale ADR status labels in `architecture.md`:
  - L349: "image crate ... unused" → "IMPLEMENTED"
  - L447: "GtkLayout ... pending" → "IMPLEMENTED"
  - L818: "Now-Playing ... PLANNED" → "IMPLEMENTED"
  - L1250: "Keybinding ... PARTIALLY IMPLEMENTED" → "IMPLEMENTED"

## Acceptance Criteria
1. App ID matches `com.mpdclient.app`
2. `include_str!` approach documented in architecture.md
3. ErrorSinkEvent and UserFacingError removed, no references remain
4. 4 ADR status labels updated
5. `cargo build` passes

## Suggested Review Order

**Code changes**

- Application ID corrected to match architecture.md authoritative value
  [`mod.rs:347`](../../src/ui/mod.rs#L347)

- Dead ErrorSinkEvent/UserFacingError/ErrorLevel types removed — zero references across codebase
  [`errors.rs:1`](../../src/errors.rs#L1)

**Architecture doc updates**

- Four stale ADR status labels updated: Cover Art, GtkLayout, Now-Playing, Keybinding — all now IMPLEMENTED
  [`architecture.md:349`](../planning-artifacts/architecture.md#L349)

- CSS embedding mechanism documented: `include_str!` replaces gresource/build.rs specification
  [`architecture.md:1553`](../planning-artifacts/architecture.md#L1553)
