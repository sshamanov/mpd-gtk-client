---
stepsCompleted: [1, 2]
inputDocuments:
  - "DESIGN.md"
  - "_bmad-output/planning-artifacts/architecture.md"
  - "_bmad-output/planning-artifacts/ux-design-specification.md"
  - "_bmad-output/planning-artifacts/ux-design-specification-enhanced.md"
---

# mpd-client - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for mpd-client, decomposing the requirements from the PRD, UX Design, and Architecture decisions into implementable stories.

## Requirements Inventory

### Functional Requirements

#### Playback Control (FR-P1–FR-P4)
- **FR-P1:** User can control MPD playback (play, pause, stop, next, previous)
- **FR-P2:** System displays current playback state (playing/paused, position, duration)
- **FR-P3:** User can seek within tracks via interactive seekbar
- **FR-P4:** System handles MPD disconnection gracefully by showing disconnect indicator, preserving queue state, and attempting automatic reconnection with exponential backoff

#### Queue Management (FR-Q1–FR-Q8)
- **FR-Q1:** System displays current queue in mode-appropriate presentation (album grid vs track list)
- **FR-Q2:** User can add albums/tracks/folders to queue via drag-and-drop
- **FR-Q3:** User can reorder queue items via drag-and-drop
- **FR-Q4:** User can remove items from queue via drag-off or context menu
- **FR-Q5:** User can "play now" (jump to item without clearing rest of queue)
- **FR-Q6:** User can "play next" (insert after current item)
- **FR-Q7:** System preserves queue across mode switches (underlying linear order unchanged)
- **FR-Q8:** System synchronizes queue state with MPD, verifying consistency every 30 seconds

#### Browsing & Navigation (FR-B1–FR-B10)
- **FR-B1:** System provides Album Mode with cover-grid browsing
- **FR-B2:** System provides Folder Mode with expandable folder-tree browsing
- **FR-B3:** System supports grouped views in Album Mode (Artists, Years, Genres)
- **FR-B4:** System pins group headers while scrolling in grouped views
- **FR-B5:** User can manually reorder albums only in plain Albums view
- **FR-B6:** System normalizes folder structures (cue files as single album entry; DSD folders as album with DSD track entries)
- **FR-B7:** System displays technical metadata per-track in Folder Mode (DSD/PCM labels)
- **FR-B8:** System provides hover controls on album covers (`+`, `←`, `>`)
- **FR-B9:** User can single-click to select, double-click to play
- **FR-B10:** System provides search functionality with mode-appropriate scope (Album Mode: title, artist, year, genre, track names; Folder Mode: full path, folder names, filenames)

#### Layout & UI (FR-L1–FR-L6)
- **FR-L1:** System maintains persistent 70/30 split shell with right rail
- **FR-L2:** System adapts right-rail proportions by mode (Album: 40/20/40, Folder: 55/45)
- **FR-L3:** System makes playback controls always visible
- **FR-L4:** User can configure layout values (split ratio, rail width, proportions)
- **FR-L5:** System remembers scroll positions and expansion state per mode
- **FR-L6:** System provides smooth transitions between modes (<500ms)

#### Cover Art & Metadata (FR-C1–FR-C7)
- **FR-C1:** System fetches cover art via local files, embedded artwork, online services
- **FR-C2:** System caches cover art in-memory (session) and on-disk (persistent)
- **FR-C3:** System rate-limits online cover lookups (1 request/second default)
- **FR-C4:** System retries failed cover lookups with exponential backoff
- **FR-C5:** System displays "no cover" placeholder when no art available
- **FR-C6:** System shows cover art in Now Playing regardless of mode
- **FR-C7:** System displays concise format labels in Now Playing (DSD128, 24/96, etc.)

#### Search (FR-S1–FR-S8)
- **FR-S1:** System provides live search with debounced input (150ms)
- **FR-S2:** System searches album metadata in Album Mode (title, artist, year, genre, tracks)
- **FR-S3:** System searches path/filename in Folder Mode (full path, folder names, filenames)
- **FR-S4:** System ranks results by relevance score with configurable thresholds
- **FR-S5:** System maintains search index with memory budget ≤5MB per 10,000 tracks
- **FR-S6:** System processes queries without blocking UI with response <50ms (95th percentile)
- **FR-S7:** User can use keyboard shortcuts (Ctrl+F focus, Esc clear, Enter play first)
- **FR-S8:** System preserves original view state when search is closed

### Non-Functional Requirements

#### Performance (NFR-P1–NFR-P6)
- **NFR-P1:** Library load time <2 seconds for 50,000-track library on SSD
- **NFR-P2:** UI responsiveness: all interactions <100ms (95th percentile)
- **NFR-P3:** Search response time <50ms for 95% of queries (50,000-track library)
- **NFR-P4:** Cover art cache hit rate >90% after initial library scan
- **NFR-P5:** Memory footprint <200MB RAM for typical library (10,000–50,000 tracks)
- **NFR-P6:** Cover subsystem memory <50MB RAM for 10,000-track library

#### Reliability & Resilience (NFR-R1–NFR-R5)
- **NFR-R1:** Playback uptime ≥99.5% (MPD-dependent outages excluded)
- **NFR-R2:** MPD connection recovery <5 seconds after transient network loss
- **NFR-R3:** Graceful degradation: UI remains usable during MPD disconnection
- **NFR-R4:** Partial failure handling: continue operation when non-critical features fail
- **NFR-R5:** Queue synchronization latency <1 second (client queue matches MPD queue)

#### Usability & Accessibility (NFR-U1–NFR-U4)
- **NFR-U1:** Album-mode "browse→select→play" flow completable in ≤3 clicks
- **NFR-U2:** Folder-mode users identify audio format/organization issues 50% faster than with file managers
- **NFR-U3:** Keyboard navigation support for all primary functions (playback, queue, search)
- **NFR-U4:** High-contrast mode support for visually impaired users (WCAG 2.1 AA)

#### Compatibility & Interoperability (NFR-C1–NFR-C3)
- **NFR-C1:** Support MPD protocol version 0.24.x (current stable)
- **NFR-C2:** Support audio formats: all PCM/DSD formats that MPD can decode
- **NFR-C3:** Support cue sheet parsing with ≥95% accuracy for standard cue sheets

#### Security & Privacy (NFR-S1–NFR-S3)
- **NFR-S1:** No collection or transmission of personal listening data
- **NFR-S2:** Optional online cover lookups (user-configurable; disabled by default)
- **NFR-S3:** Network usage cap configurable (default 500MB/month) with user warnings

#### Maintainability & Extensibility (NFR-M1–NFR-M3)
- **NFR-M1:** Layered architecture with clear separation (MPD adapter, state, presenters, UI)
- **NFR-M2:** Configuration-driven layout values (not hard-coded)
- **NFR-M3:** Ability to replace playback backend later without UI redesign

#### Deployability & Operations (NFR-O1–NFR-O2)
- **NFR-O1:** Zero-configuration operation with sensible defaults
- **NFR-O2:** Auto-start capability when configured by user

### Additional Requirements (Architecture)

- **AR-1:** Pre-Milestone 0: Upgrade gtk4-rs from 0.8 to 0.11 with `v4_14` feature flag. The existing workspace code uses gtk4 0.8; this is a breaking change requiring migration.
- **AR-2:** Restructure from workspace with 5 sub-crates to single crate with flat module hierarchy. Cargo.toml consolidation with correct feature flags.
- **AR-3:** Implement module docstrings (`//!` comments) on every `mod.rs` documenting scope and thread affinity.
- **AR-4:** Create `scripts/check-patterns.sh` for automated pattern enforcement (clippy, unwrap checks, GTK import checks).
- **AR-5:** Implement MockMpdServer in `mpd/mock.rs` under `#[cfg(test)]` for deterministic unit testing.
- **AR-6:** Create `tests/common/mod.rs` with shared test helpers before writing the first integration test.
- **AR-7:** Create `tests/smoke_test.rs` — end-to-end test wiring MockMpdServer → connect → status → verify parsed state.
- **AR-8:** Implement module import rules: `mpd/` never imports `ui/`; `presenters/` never imports `mpd/` directly; `app.rs` remains thin (wiring only).
- **AR-9:** Rename `ui/reexport.rs` to `ui/gtk_reexport.rs` per structural refinement.
- **AR-10:** Create `src/errors.rs` as shared home for `UserFacingError` trait, `ErrorSinkEvent`, `ErrorLevel`.
- **AR-11:** Create `src/constants.rs` for layout constants (split ratio, rail width, proportions).
- **AR-12:** Implement CancellationToken on compute worker jobs for responsive shutdown.
- **AR-13:** Queue re-push on reconnect with epoch-gated safety (auto-stop, re-push, "Reconnected" indicator).
- **AR-14:** MPRIS D-Bus integration for desktop media keys.

### UX Design Requirements

- **UX-DR1:** Hover controls on album grid cards — `+` (add to queue), `←` (insert after current), `>` (clear queue and play). GtkOverlay with CSS-positioned GtkButtons, visibility toggled by CSS `:hover`. Minimum interactive area 24×24px.
- **UX-DR2:** Double-click as secondary interaction — album mode: double-click album = clear queue + play; folder mode: double-click track/folder = immediate playback.
- **UX-DR3:** Persistent settings gear icon (top-right corner) — expands on hover, keyboard shortcut `Ctrl+,`. Always-accessible during all modes.
- **UX-DR4:** Toast notification system for transient events — bottom-right corner, 3-second display, stacked (max 3), click-to-dismiss. Queue sync notifications: "Queue synchronized with N change(s)."
- **UX-DR5:** Smart auto-connect to MPD (localhost:6600 default) — if fails, show settings dialog. Never ask again once configured.
- **UX-DR6:** Album Mode vs Folder Mode visual separation — Album Mode uses cover grid + album-oriented queue; Folder Mode uses folder tree + track-oriented queue (filenames, format, duration).
- **UX-DR7:** Chrome fade during active listening — queue and controls remain visible but visually recede during playback.
- **UX-DR8:** Preview persistence — album selection in right rail stays until playback starts or explicit deselect (no auto-fade during browsing).
- **UX-DR9:** Folder Mode queue precision — track list with filenames, format badges, duration. Clear insertion point indicator when adding tracks.
- **UX-DR10:** Album selection displays artist, album, year, format. Folder Mode additionally shows sample rate, bit depth, file path, technical audio details.
- **UX-DR11:** Queue synchronization transparency — subtle toast: "Queue synchronized with library changes" (3-second display).
- **UX-DR12:** Hash-derived placeholder colors for missing cover art — first 3 bytes of SHA-256(artist name) → HSL color at 35% saturation, 55% lightness.
- **UX-DR13:** Breadcrumb navigation bar — Album Mode: genre > artist > album; Folder Mode: filesystem ancestry.
- **UX-DR14:** Workspace/mode orchestrator — widgets hidden/detached on mode switch (not destroyed) to preserve browsing context.

### FR Coverage Map

FR-P1: Epic 1a (Playback control — play, pause, stop, next, previous)
FR-P2: Epic 1a (Display playback state)
FR-P3: Epic 1a (Seek within tracks)
FR-P4: Epic 1a (MPD disconnect handling, reconnect, error recovery)

FR-Q1: Epic 2 (folder track list) + Epic 3 (album mini-grid)
FR-Q2: Epic 3 (Queue add via drag-and-drop)
FR-Q3: Epic 3 (Queue reorder via drag-and-drop)
FR-Q4: Epic 3 (Queue remove)
FR-Q5: Epic 3 (Play now)
FR-Q6: Epic 3 (Play next)
FR-Q7: Epic 3 (Queue preservation across mode switches)
FR-Q8: Epic 3 (Queue sync with MPD)

FR-B1: Epic 1b (Album Mode cover-grid)
FR-B2: Epic 2 (Folder Mode folder-tree)
FR-B3: Epic 1b (Grouped views: Artists, Years, Genres)
FR-B4: Epic 1b (Sticky group headers)
FR-B5: Epic 1b (Manual album reorder)
FR-B6: Epic 2 (Folder normalization deferred post-ship)
FR-B7: Epic 2 (Technical metadata in Folder Mode)
FR-B8: Epic 1b (Hover controls)
FR-B9: Epic 1b (Single-click select, double-click play)
FR-B10: Epic 1b (basic MPD-native) + Epic 4a (local index, mode-scoped strategies)

FR-L1: Epic 1a (70/30 split shell)
FR-L2: Epic 3 (Rail proportions per mode)
FR-L3: Epic 1a (Always-visible playback controls)
FR-L4: Epic 5b (Configurable layout values)
FR-L5: Epic 5b (Scroll position persistence)
FR-L6: Epic 3 (Smooth mode transitions)

FR-C1: Epic 1b (local files) + Epic 4b (embedded tags, online)
FR-C2: Epic 4b (Cover art cache: session + disk)
FR-C3: Epic 4b (Rate-limited online lookups)
FR-C4: Epic 4b (Failed lookup retry)
FR-C5: Epic 4b (No-cover placeholder)
FR-C6: Epic 4b (Cover in now playing)
FR-C7: Epic 4b (Format labels in now playing)

FR-S1: Epic 1b (basic, via MPD search command) + Epic 4a (debounced, indexed)
FR-S2: Epic 1b (basic) + Epic 4a (full-text, mode-scoped)
FR-S3: Epic 1b (basic) + Epic 4a (full-text, mode-scoped)
FR-S4: Epic 4a (Relevance scoring)
FR-S5: Epic 4a (Search index memory budget)
FR-S6: Epic 4a (Non-blocking queries, <50ms)
FR-S7: Epic 1b (basic shortcuts) + Epic 4a (full shortcut set)
FR-S8: Epic 1b (basic) + Epic 4a (view state preservation)

## Epic List

### Epic 0: Toolchain (Pre-epic, Dev-Only)
No user-facing value. Infrastructure prerequisites: upgrade gtk4-rs from 0.8 to 0.11 (with `v4_14` feature), restructure workspace with 5 sub-crates to single crate with flat module hierarchy, create `scripts/check-patterns.sh`, initialize `tests/common/mod.rs` with shared test helpers, add module docstrings to all modules. **Async runtime spike:** confirm tokio vs. async-std choice for MPD connection and compute worker. **GTK4 spike:** create a blank `cargo new` project with gtk4 0.11 first — compile an empty window, then reverse-engineer the build.rs and Cargo.toml from there. Merge into real project only after blank project compiles. **Cap:** 5 calendar days maximum. If not done, ship incremental upgrade path (upgrade gtk4 in-place within existing workspace) and defer restructure.
**Depends on:** Nothing (first task)
**FRs covered:** None (no user-facing requirements)

### Epic 1a: Backbone (True MVP)
The user can launch the app, connect to MPD (with basic host/port config dialog if auto-connect fails), and control playback (play/pause/stop/next/prev/seek). The 70/30 split shell shows a placeholder browsing area on the left and now-playing display on the right. Connection lifecycle handles disconnects with reconnect indicator, error recovery strategy (stale-state guard, re-push queue on reconnect), and graceful degradation during network loss. **Compute worker thread** is built as infrastructure (thread::spawn + channel) — not for any specific feature, but as the foundation for all non-blocking background work (cover I/O in Epic 1b, search in Epic 4a). **This is the vertical slice end-to-end** — everything after this is additive.
**FRs covered:** FR-P1, FR-P2, FR-P3, FR-P4, FR-L1, FR-L3

### Epic 1b: Album Experience
The user can browse albums in a visual grid with local cover art, accessed via hover controls (`+`, `←`, `>`) and double-click interaction. The compute worker thread is built alongside the first cover use (local file lookup — `cover.jpg` etc.), providing non-blocking cover I/O. QueueCore API (add, play, clear) supports the "add and play" flow. Basic MPD-native search via `search` protocol command (no local index — inline text match on metadata fields). Basic MPRIS D-Bus integration for media keys (play/pause/next/prev — defines `MprisProvider` trait so Epic 5a extends without rewriting). Chrome fade during active listening built into the rail from the start. Album selection preview persists in the right rail during browsing. Hash-derived placeholder colors for albums without cover files (SHA-256 artist name prefix → HSL, ~20 lines).
**FRs covered:** FR-B1, FR-B8, FR-B9, FR-C1 (local files only), FR-B10 (basic), FR-S1 (basic), FR-S2 (basic), FR-S3 (basic)
**UX-DRs:** UX-DR1, UX-DR2, UX-DR7, UX-DR8, UX-DR10, UX-DR12

### Epic 2: Folder Mode
The user can browse music by folder structure with technical audio metadata (sample rate, bit depth, format badges). Track-oriented queue view in the right rail with track list, filenames, and duration. Breadcrumb navigation bar for filesystem ancestry. Keyboard navigation in the folder tree (arrow up/down for selection, left/right for expand/collapse, Enter to play). **Keyboard regression test** alongside the tree widget (~50 lines: simulate arrow key events, assert selection state) — re-run after every epic that touches the tree. Basic cue sheet and DSD folder detection strategies (~200 lines each, already designed in architecture document — `CueSheetStrategy` + `DsdFolderStrategy`). Full keyboard audit (focus rings, Tab traversal, screen reader) and normalization of edge cases (SACD ISO, multi-file DSD albums) deferred to Epic 5b.
**FRs covered:** FR-B2, FR-B6, FR-B7, FR-Q1 (folder view)
**UX-DRs:** UX-DR6, UX-DR9, UX-DR13

### Epic 3: Queue Power
The user can view, reorder, and modify the playback queue with drag-and-drop. Album-mode mini-grid queue view in the right rail. Folder-mode track-list queue view (built in Epic 2) gains DnD reorder, play now/next, remove. Queue syncs with MPD (every 30s). Mode switch preserves queue (linear order unchanged). Rail proportions adapt by mode (Album: 40/20/40, Folder: 55/45).
**FRs covered:** FR-Q1 (both views), FR-Q2, FR-Q3, FR-Q4, FR-Q5, FR-Q6, FR-Q7, FR-Q8, FR-L2, FR-L6

### Epic 4a: Local Search Index [CONDITIONAL]
The user gets fast <50ms search responses via an in-memory full-text index built from cached MPD metadata. Relevance scoring with configurable threshold (default 0.6). Mode-scoped strategies (album metadata in Album Mode, filenames/paths in Folder Mode). 150ms debounce on keystroke. Keyboard shortcuts (Ctrl+F focus, Esc clear, Enter play first). Non-blocking via compute worker. Search bar and basic MPD-native search already present from Epic 1b; this epic upgrades to indexed search for performance. **Condition:** verify that MPD's native `search` command exceeds <50ms on a representative library. If MPD search is fast enough, this epic is deferred — basic search in Epic 1b is sufficient for v1.
**FRs covered:** FR-S4, FR-S5, FR-S6, FR-S7, FR-S8

### Epic 4b: Cover Art (Online & Embedded) [DEFERRED]
The user gets cover art from embedded tags and online services (MusicBrainz, Discogs, Last.fm) with session cache (LRU, 10K entries) and disk cache (SHA-256 hashed files, 1GB limit, LRU eviction). Rate-limited online lookups (1 req/s default, 5 burst). Exponential backoff on failures. Local file covers already handled in Epic 1b (including hash-derived placeholder colors for albums without cover files). **Deferred to post-v1:** local covers in Epic 1b cover ~80% of users. Online covers (embedded tags, MusicBrainz, Discogs) are additive and do not block v1. Revisit if users report missing covers as a top frustration.
**FRs covered:** FR-C1 (embedded + online), FR-C2, FR-C3, FR-C4, FR-C5, FR-C6, FR-C7
### Epic 5a: Notifications & Orchestration
Toast notifications for transient events (queue sync, connection status — bottom-right, 3s display, stacked max 3). Workspace mode orchestrator (widgets hidden/detached on mode switch, not destroyed — preserves scroll position and selection). Full MPRIS metadata/position tracking and playerctl compatibility deferred to post-v1 (basic play/pause/next/prev already in Epic 1b).
**UX-DRs:** UX-DR4, UX-DR11, UX-DR14

### Epic 5b: Configuration & Accessibility
The user can configure MPD host/port, layout values (split ratio, rail width), and theme via a settings dialog. Settings gear icon in top-right corner (hover-expand, keyboard shortcut `Ctrl+,`). Full keyboard navigation audit (Tab/Shift-Tab focus traversal across all zones, focus ring visibility, screen reader support via AT-SPI — basic grid/tree keyboard nav already in Epics 1b and 2). High-contrast mode (WCAG 2.1 AA, `prefers-reduced-motion` respected). Scroll position persistence per mode. Smart auto-connect on first launch (if MPD connect fails, show settings dialog).
**NFRs covered:** NFR-U1, NFR-U2, NFR-U3, NFR-U4, NFR-C1, NFR-C2, NFR-C3, NFR-S1, NFR-S2, NFR-S3, NFR-O1
**FRs covered:** FR-L4, FR-L5
**UX-DRs:** UX-DR3, UX-DR5
