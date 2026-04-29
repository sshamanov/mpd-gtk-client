# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

A GTK4-based MPD (Music Player Daemon) client in Rust with two primary workflows:
1. **Album Mode** — visual cover grid with hover controls, grouped views, local search
2. **Folder Mode** — directory browser with breadcrumbs, cue/DSD normalization

## Architecture

Single Rust crate (edition 2024, MSRV 1.85). Two threads: GTK4 main loop + MPD background thread.
Channels: `sync_channel<MpdEvent>(1024)` for MPD→UI, `mpsc::channel<MpdCommand>` for UI→MPD.
No async runtime — `std::thread` over tokio (4.2MB vs 15MB binary).

**Full architecture decisions:** `_bmad-output/planning-artifacts/architecture.md`

### Module Map
| Module | Purpose |
|--------|---------|
| `src/main.rs` | Entry point, env_logger init, signal handlers, GTK app launch |
| `src/mpd/mod.rs` | MPD TCP protocol adapter (`MpdAdapter`) |
| `src/mpd/state_machine.rs` | Background thread: connection lifecycle, command dispatch, MPD idle loop |
| `src/mpd/mock.rs` | Mock MPD server for integration tests |
| `src/state/mod.rs` | Application state (`SharedState = Arc<RwLock<AppState>>`) |
| `src/ui/mod.rs` | GTK4 UI: window, menus, album grid, queue, now-playing, settings |
| `src/ui/widgets/` | Reusable widgets: album_cover, folder_tree, toast |
| `src/config/mod.rs` | TOML config persistence (`~/.config/mpd-client/config.toml`) |
| `src/search/mod.rs` | Local keyword→album search index (hash-based, no MPD round-trip) |
| `src/coverart/mod.rs` | Cover art: MPD albumart/readpicture pipeline with disk cache (designed, not yet implemented) |
| `src/constants.rs` | Layout constants |
| `src/errors.rs` | Error types |

## Development Status

- **Phase**: v1 implementation complete — all 7 epics done
- **Tests**: 11 integration tests with mock MPD server (`cargo test`)
- **Pending**: integration tests for real MPD, keyboard shortcut docs in-app
- **BMad**: installed for project management (`_bmad/` directory)

## Build & Run

```bash
cargo build
cargo test                    # 11 integration tests
RUST_LOG=info cargo run       # normal operation
RUST_LOG=debug cargo run      # verbose MPD protocol logging
```

## Key Files

- **Design/PRD**: `_bmad-output/planning-artifacts/prd.md`
- **Architecture decisions**: `_bmad-output/planning-artifacts/architecture.md`
- **Sprint status**: `_bmad-output/implementation-artifacts/sprint-status.yaml`
- **Deferred work**: `_bmad-output/implementation-artifacts/deferred-work.md`
- **Config**: `~/.config/mpd-client/config.toml` (host/port, auto-created on first save)

## Authoritative Documents

These are the single sources of truth. Every other .md file is historical or supporting:

| Document | Authority | Path |
|----------|-----------|------|
| Claude instructions | Operations | `CLAUDE.md` |
| Product requirements | Design | `_bmad-output/planning-artifacts/prd.md` |
| Technical decisions | Engineering | `_bmad-output/planning-artifacts/architecture.md` |
| Implementation breakdown | Planning | `_bmad-output/planning-artifacts/epics.md` |
| UX specification | Design | `_bmad-output/planning-artifacts/ux-design-specification.md` |
| Sprint plan/status | Execution | `_bmad-output/implementation-artifacts/sprint-status.yaml` |
| Deferred work | Planning | `_bmad-output/implementation-artifacts/deferred-work.md` |

## Non-Authoritative Files

These files exist for reference and are NOT authoritative for current decisions:
- `FEEDBACK-GPT.md` — external architecture review, historical reference
- `_bmad-output/implementation-artifacts/*.md` — story-level implementation records

## Document Policy

### Document Hierarchy

| Document | Authority | Content |
|----------|-----------|---------|
| `CLAUDE.md` | Operations | How to work in this repo, document policy |
| `_bmad-output/planning-artifacts/prd.md` | Design | FRs/NFRs, personas, journeys, interaction rules, layout specs |
| `_bmad-output/planning-artifacts/architecture.md` | Engineering | ADRs, threading, protocol, modules, all implementation specs |
| `_bmad-output/planning-artifacts/epics.md` | Planning | Epics, stories, sprint plan |
| `_bmad-output/planning-artifacts/ux-design-specification.md` | Design | UX layout, interaction design |
| `_bmad-output/implementation-artifacts/*` | Execution | Sprint status, retrospectives, deferred work |

### Single Source of Truth

Each concern has exactly one authoritative document. If content exists in two places, one is wrong:
- Product decisions → `prd.md` only
- Technical decisions → `architecture.md` only
- Claude instructions → `CLAUDE.md` only

### Temporary Documents Policy

Exploration findings, enhancement plans, v2 addendums, and cross-project analyses are created as temporary scratch documents. After successful review or advanced elicitation, their content MUST be:

1. **Merged inline** into the appropriate authoritative document (prd.md or architecture.md)
2. **The temp file deleted** — no competing sources of truth

This prevents the document drift pattern where old design and new design coexist in separate files. The elicitation and checks validate the content; the merge makes it canonical; the delete keeps it clean.

## Code Conventions

- No async — all blocking I/O on background thread, all UI on GTK main thread
- Log via `log` crate + `env_logger`; errors at error level, channel-full at warn, timing at debug
- No unwrap/expect in library code; `if let Ok(...)` patterns for fallible operations
- MPD adapter commands return `Result<_, Error>`; callers log errors and continue
- GTK4 0.11 with v4_14 feature; glib 0.20, gdk4 0.11

## Git Commit Policy

### Granularity

Each commit represents one **practical step forward** — a change that is self-contained, reviewable, and has a clear purpose. Not every file write, but every logical unit of work.

### What Gets Committed

| Type | When to Commit | Example Message |
|------|---------------|-----------------|
| **Completed documents** | After review/elicitation, merged to authoritative location, temp files deleted | `docs: absorb async runtime decision into architecture.md` |
| **Sprint artifacts** | Sprint plan created or updated | `plan: sprint-2 plan with 4 stories` |
| **Completed stories** | Story implementation done, tests pass | `feat: implement MPD idle protocol with try_clone()` |
| **Code review results** | Review completed, issues resolved or accepted | `review: address code review findings on cover pipeline` |
| **Debug session fixes** | Root cause identified, fix approved, verified working | `fix: reset BufReader after albumart binary read` |
| **Architecture decisions** | ADR finalized, added to architecture.md | `docs: ADR for plchanges incremental queue updates` |

### What Does NOT Get Committed

- Temporary scratch documents (enhancement plans, v2 addendums, cross-project analyses) — these must be **absorbed into authoritative docs and deleted** before commit
- Intermediate file writes during editing
- Temporary files in gitignored paths (`.claude/`, `target/`, `node_modules/`)
- Failed experiments or debugging attempts — squash or don't stage

### Workflow

1. Create scratch/temp documents in working tree (gitignored paths or not yet staged)
2. Review, elicit, validate the content
3. **Merge findings** into authoritative documents (prd.md, architecture.md)
4. **Delete** the scratch/temp files
5. **Commit** — one commit per logical step, covering code + doc changes together

This keeps the git history clean and meaningful: every commit is a real event in the project's progression, not a snapshot of the editor buffer.
