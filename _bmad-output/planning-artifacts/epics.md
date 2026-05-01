---
stepsCompleted: [1]
inputDocuments:
  - "DESIGN.md"
  - "_bmad-output/planning-artifacts/architecture.md"
  - "_bmad-output/planning-artifacts/ux-design-specification.md"
  - "_bmad-output/implementation-artifacts/deferred-work.md"
---

# mpd-client - Epic Breakdown (v1 from MVP)

## Overview

This document provides the epic and story breakdown for completing mpd-client v1, focused on requirements not yet implemented in the MVP baseline. MVP implements: Album Mode with cover grid, grouped views, local search, Folder Mode with breadcrumbs and CUE/DSD normalization, queue display with play/remove/reorder, now-playing display, settings dialog, toast notifications, and keyboard shortcuts.

## Requirements Inventory

### Functional Requirements (implemented ✓, remaining ✗)

#### Playback Control (FR-P1–FR-P4)
- **FR-P1:** ✗ User can control MPD playback (play, pause, stop, next, previous) — PARTIAL: basic controls work, seekbar missing
- **FR-P2:** ✓ System displays current playback state (playing/paused, position, duration) — PARTIAL: state icon works, elapsed/duration not shown
- **FR-P3:** ✗ User can seek within tracks via interactive seekbar
- **FR-P4:** ✗ System handles MPD disconnection gracefully — PARTIAL: indicator works, auto-reconnect works, but dead connection not detected mid-session

#### Queue Management (FR-Q1–FR-Q8)
- **FR-Q1:** ✗ System displays current queue in mode-appropriate presentation (album grid vs track list) — PARTIAL: track list only, no album mini-grid
- **FR-Q2:** ✗ User can add albums/tracks/folders to queue via drag-and-drop
- **FR-Q3:** ✗ User can reorder queue items via drag-and-drop — PARTIAL: Shift+Up/Down works, no drag-and-drop
- **FR-Q4:** ✗ User can remove items from queue via drag-off or context menu — PARTIAL: context menu Remove works, right-click Delete works, no drag-off
- **FR-Q5:** ✓ User can "play now" (jump to item without clearing rest of queue)
- **FR-Q6:** ✗ User can "play next" (insert after current item) — PARTIAL: InsertNext implemented but not surfaced in UI context menu
- **FR-Q7:** ✓ System preserves queue across mode switches
- **FR-Q8:** ✓ System synchronizes queue state with MPD every 30 seconds

#### Browsing & Navigation (FR-B1–FR-B10)
- **FR-B1:** ✓ System provides Album Mode with cover-grid browsing
- **FR-B2:** ✓ System provides Folder Mode with expandable folder-tree browsing
- **FR-B3:** ✓ System supports grouped views in Album Mode (Artists, Years, Genres)
- **FR-B4:** ✗ System pins group headers while scrolling in grouped views
- **FR-B5:** ✗ User can manually reorder albums only in plain Albums view
- **FR-B6:** ✗ System normalizes folder structures — PARTIAL: detection works, but CUE/DSD summary rows not properly interactive
- **FR-B7:** ✓ System displays technical metadata per-track in Folder Mode
- **FR-B8:** ✓ System provides hover controls on album covers
- **FR-B9:** ✓ User can single-click to select, double-click to play
- **FR-B10:** ✗ System provides search in Folder Mode — PARTIAL: Album Mode search works, Folder Mode search not implemented; grouped-mode double-click broken (header offset)

#### Layout & UI (FR-L1–FR-L6)
- **FR-L1:** ✓ System maintains persistent split shell with right rail
- **FR-L2:** ✗ System adapts right-rail proportions by mode (Album: 40/20/40, Folder: 55/45) — PARTIAL: simplified right rail without album track window
- **FR-L3:** ✓ System makes playback controls always visible
- **FR-L4:** ✗ User can configure layout values (split ratio, rail width, proportions)
- **FR-L5:** ✓ System remembers scroll positions and expansion state per mode
- **FR-L6:** ✗ System provides smooth transitions between modes (<500ms)

#### Cover Art & Metadata (FR-C1–FR-C7)
- **FR-C1:** ✗ System fetches cover art via local files, embedded artwork, online services
- **FR-C2:** ✗ System caches cover art in-memory (session) and on-disk (persistent)
- **FR-C3:** ✗ System rate-limits online cover lookups
- **FR-C4:** ✗ System retries failed cover lookups with exponential backoff
- **FR-C5:** ✓ System displays "no cover" placeholder when no art available — hash-derived color placeholders
- **FR-C6:** ✗ System shows cover art in Now Playing regardless of mode
- **FR-C7:** ✗ System displays concise format labels in Now Playing (DSD128, 24/96)

#### Search (FR-S1–FR-S8)
- **FR-S1:** ✓ System provides live search with debounced input (150ms)
- **FR-S2:** ✓ System searches album metadata in Album Mode
- **FR-S3:** ✗ System searches path/filename in Folder Mode
- **FR-S4:** ✗ System ranks results by relevance score with configurable thresholds — Story 9.8 added, not yet implemented
- **FR-S5:** ✓ System maintains search index with memory budget ≤5MB per 10k tracks
- **FR-S6:** ✓ System processes queries without blocking UI
- **FR-S7:** ✓ User can use keyboard shortcuts (Ctrl+F, Esc)
- **FR-S8:** ✓ System preserves original view state when search is closed

### Non-Functional Requirements (remaining gaps)

#### Performance
- **NFR-P1:** Library load time <2s for 50k tracks — NOT TESTED
- **NFR-P2:** UI responsiveness <100ms — PARTIAL: large libraries may freeze on grid rebuild
- **NFR-P4:** Cover art cache hit rate >90% — NOT APPLICABLE (cover art stub)

#### Reliability
- **NFR-R1:** MPD connection resilience — PARTIAL: dead mid-session connection not detected
- **NFR-R2:** MPD connection recovery <5s — PARTIAL: works on startup, not mid-session

#### Accessibility
- **NFR-U1:** Keyboard navigation — PARTIAL: queue keyboard works, folder tree Left/BackSpace works, no full keyboard nav audit

#### Maintainability
- **NFR-M1:** Code test coverage — PARTIAL: 11 integration tests with mock MPD, no real-MPD tests

### Additional Requirements (Architecture)

- **AR1:** MPD idle protocol — Architecture recommends `idle` for event-driven updates; currently polling at 500ms
- **AR2:** Connection health monitoring — Architecture specifies resilience requirements; dead connection not detected in connected_loop
- **AR3:** Cover art storage — Architecture defines disk cache structure at `~/.cache/mpd-client/covers/`
- **AR4:** Layout service — Architecture proposes LayoutService for configurable proportions and responsive breakpoints

### UX Design Requirements

- **UX-DR1:** Album Mode queue should display as cover mini-grid (not just track list)
- **UX-DR2:** Current Album track window — shows tracks of currently playing album
- **UX-DR3:** Interactive seekbar with position/duration display
- **UX-DR4:** Now Playing shows cover art, format badges (DSD128, 24/96)
- **UX-DR5:** Drag-and-drop visual feedback (ghost covers, drop indicators)
- **UX-DR6:** Smooth mode transitions with crossfade or slide animation
- **UX-DR7:** Group header pinning during scroll
- **UX-DR8:** Play Next in context menu for queue items

### Code Review Deferred Items (from deferred-work.md)

- **CR1:** Dead MPD connection never triggers reconnect — connected_loop hangs [critical]
- **CR2:** Double-click in grouped mode plays wrong album — header offset in FlowBox [critical]
- **CR3:** Queue context menu Remove fix applied; needs Play Next option added
- **CR4:** Toast auto-dismiss race on rapid successive errors
- **CR5:** `search_albums` misses `AlbumArtist` tag for compilations
- **CR6:** Cue/DSD rows navigable but don't show individual files
- **CR7:** Settings changes require app restart
- **CR8:** Search index not rebuilt on MPD library update while connected
- **CR9:** `MpdEvent::Reconnected` is dead code
- **CR10:** `ExponentialBackoff` Clone misleading
- **CR11:** Cover art `set_cover_path` dead code with fragile widget traversal

### FR Coverage Map

| FR | Epic | Status |
|----|------|--------|
| FR-P1 | MVP | Partial — basic controls, seekbar in Epic 7 |
| FR-P2 | MVP | Partial — state icon, elapsed/duration in Epic 7 |
| FR-P3 | Epic 8 | Seekbar |
| FR-P4 | Epic 6 | Connection health detection |
| FR-Q1 | MVP | Partial — track list, album mini-grid in Epic 10 |
| FR-Q2 | Epic 11 | Drag-and-drop add to queue |
| FR-Q3 | Epic 11 | Drag-and-drop reorder |
| FR-Q4 | Epic 11 | Drag-off remove, context menu exists |
| FR-Q5 | MVP | Done |
| FR-Q6 | Epic 10 | Play Next in context menu |
| FR-Q7 | MVP | Done |
| FR-Q8 | MVP | Done |
| FR-B1 | MVP | Done |
| FR-B2 | MVP | Done |
| FR-B3 | MVP | Done |
| FR-B4 | Epic 9 | Pinned group headers |
| FR-B5 | Epic 11 | Album reorder in plain view |
| FR-B6 | Epic 9 | CUE/DSD interactive rows |
| FR-B7 | MVP | Done |
| FR-B8 | MVP | Done |
| FR-B9 | MVP | Done |
| FR-B10 | Epic 9 | Folder Mode search, grouped double-click fix |
| FR-L1 | MVP | Done |
| FR-L2 | Epic 8, 10 | Album track window, mode proportions |
| FR-L3 | MVP | Done |
| FR-L4 | Epic 10 | Configurable layout |
| FR-L5 | MVP | Done |
| FR-L6 | Epic 10 | Smooth mode transitions |
| FR-C1 | Epic 7 | Cover art fetching |
| FR-C2 | Epic 7 | Cover art caching |
| FR-C3 | Epic 7 | Rate limiting |
| FR-C4 | Epic 7 | Retry logic |
| FR-C5 | MVP | Done (placeholder colors) |
| FR-C6 | Epic 7 | Cover in Now Playing |
| FR-C7 | Epic 8 | Format labels in Now Playing |
| FR-S1 | MVP | Done |
| FR-S2 | MVP | Done |
| FR-S3 | Epic 9 | Folder Mode search |
| FR-S4 | Epic 9 | Relevance ranking |
| FR-S5 | MVP | Done |
| FR-S6 | MVP | Done |
| FR-S7 | MVP | Done |
| FR-S8 | MVP | Done |

## Epic List

### Epic 6: Resilience & Connection Health
The app detects dead MPD connections mid-session, reconnects automatically with proper state recovery. Settings changes take effect without restart. Toast notifications handle rapid errors correctly. Search index stays current with MPD library changes. This is foundational — without it, the other epics can silently fail.
**FRs covered:** FR-P4, FR-Q8
**Code review:** CR1, CR4, CR7, CR8, CR9, CR10
**Architecture:** AR2

### Epic 7: Cover Art & Visual Polish
Users see real album artwork throughout the app — cover grid, Now Playing, queue — with format badges showing audio quality. Cover art is fetched from local files, embedded metadata, and online services, cached in memory and on disk. Highest user-visible impact after resilience.
**FRs covered:** FR-C1, FR-C2, FR-C3, FR-C4, FR-C5, FR-C6, FR-C7
**Code review:** CR11 (fix `set_cover_path` dead code)

### Epic 8: Playback Experience
Users can seek within tracks via an interactive seekbar, see elapsed and duration times, browse the currently playing album's tracks in a dedicated window, and see technical format information (DSD128, 24/96) in Now Playing.
**FRs covered:** FR-P1 (seekbar), FR-P2 (elapsed/duration), FR-P3, FR-L2 (album track window), FR-C7 (format badges)
**UX:** UX-DR2, UX-DR3, UX-DR4

### Epic 9: Search & Browse Polish
Folder Mode search works for paths and filenames. Group headers stay pinned while scrolling in grouped views. CUE/DSD summary rows are properly interactive. Grouped-mode double-click plays the correct album. Search handles AlbumArtist tags correctly.
**FRs covered:** FR-S3, FR-S4, FR-B4, FR-B10 (grouped double-click fix), FR-B6 (CUE/DSD interaction)
**Code review:** CR2, CR5, CR6

### Epic 10: Layout & Configuration
Users can adjust the split ratio, rail width, and mode proportions via settings. Mode switching is smooth and animated. The right rail shows an album mini-grid queue in Album Mode and Play Next appears in context menus.
**FRs covered:** FR-L4, FR-L6, FR-L2 (proportions), FR-Q1 (album mini-grid), FR-Q6 (Play Next in UI)
**UX:** UX-DR1, UX-DR6, UX-DR8
**Architecture:** AR4

### Epic 11: Drag & Drop
Users can drag albums from the grid into the queue, reorder queue items by dragging, and remove items by dragging them off the queue. Album reordering in plain Albums view is also enabled. Most technically complex epic, deferred to last.
**FRs covered:** FR-Q2, FR-Q3, FR-Q4, FR-B5
**UX:** UX-DR5

---

## Epic 6: Resilience & Connection Health
**Goal:** Detect dead MPD connections mid-session, reconnect automatically with proper state recovery. Settings changes take effect without restart. Toast notifications handle rapid errors correctly. Search index stays current with MPD library changes.

### Story 6.1: Dead Connection Detection in connected_loop

As a user,
I want the app to detect when the MPD connection drops mid-session,
So that it can automatically reconnect and restore functionality without me noticing the interruption.

**Acceptance Criteria:**

**Given** the MPD background thread is running in `connected_loop`
**When** any `adapter.send_command()` call returns an IO error or `read_line` returns 0 bytes
**Then** `connected_loop` exits and returns control to the outer state machine
**And** the state machine transitions to `MpdState::Disconnected` with preserved backoff state
**And** a `Disconnected` event is sent to the UI
**And** the connection indicator turns red
**And** reconnection proceeds with exponential backoff

**Given** the app reconnects successfully after a dropped connection
**When** the new `connected_loop` starts
**Then** a `Connected` event is sent to the UI with full state refresh (albums, queue, search index reset)

### Story 6.2: Toast Timer Management

As a user,
I want toast notifications to display for their full duration,
So that I can read error messages even when multiple errors occur in rapid succession.

**Acceptance Criteria:**

**Given** a toast is currently visible with 3 seconds remaining
**When** a second toast is triggered
**Then** the first toast's auto-dismiss timer is cancelled
**And** the second toast replaces the first and gets a fresh 3-second timer

**Given** a toast auto-dismiss timer fires
**When** no toast is currently visible
**Then** the timer is a no-op (no panic, no incorrect state)

### Story 6.3: Settings Live Reconnect

As a user,
I want MPD connection settings changes to take effect immediately,
So that I don't have to restart the app after changing the host or port.

**Acceptance Criteria:**

**Given** the user changes the MPD host or port in Settings and clicks Save
**When** the config is saved successfully
**Then** the current MPD connection is gracefully closed
**And** a new MPD event loop is started with the new host/port
**And** the UI shows the connecting state and reconnects

**Given** the user saves settings with the same host/port
**When** no change to connection parameters is detected
**Then** the connection is NOT restarted (no-op save)

### Story 6.4: Search Index on MPD Library Update

As a user,
I want search results to reflect the current state of my MPD library,
So that newly added or removed albums appear correctly in search without restarting the app.

**Acceptance Criteria:**

**Given** the app is connected to MPD
**When** the `playlist` version number in the status response changes (indicating library modification)
**Then** the local search index is rebuilt from the current album list

**Given** the app reconnects to MPD
**When** the `Connected` event fires
**Then** the search index is cleared and rebuilt from fresh data (already implemented)

### Story 6.5: Dead Code Cleanup — Reconnected and Backoff

As a developer,
I want the codebase to be free of dead and misleading code,
So that future maintenance is straightforward and the code accurately reflects runtime behavior.

**Acceptance Criteria:**

**Given** `MpdEvent::Reconnected` is never emitted by the state machine
**When** the cleanup is applied
**Then** the variant is removed from the enum (or wired up if there's a valid use case)
**And** all match arms handling it are removed or consolidated with `Connected`

**Given** `ExponentialBackoff` derives `Clone` but is never cloned in a meaningful way
**When** the cleanup is applied
**Then** the `Clone` derive is removed or documented with a comment explaining why it exists

### Story 6.6: Graceful MPD Shutdown

As a developer,
I want MPD to receive a clean `close` command on app shutdown,
So that the server doesn't see an abrupt disconnect and the client exits cleanly.

**Acceptance Criteria:**

**Given** the app is shutting down normally (window closed, Ctrl+Q, SIGINT/SIGTERM)
**When** the shutdown sequence begins
**Then** a `close` command is sent to MPD before the TCP socket is dropped
**And** the MPD background thread exits within 100ms of receiving the shutdown signal
**And** `process::exit(0)` is replaced with a proper GTK lifecycle shutdown that drains pending events

### Story 6.7: Queue Key Handler Race Condition

As a user reordering the queue,
I want rapid Shift+Up/Down key presses to always move the correct item,
So that the queue order is always what I intended.

**Acceptance Criteria:**

**Given** the user presses Shift+Up or Shift+Down rapidly (<50ms between presses)
**When** each key event fires before the previous queue update finishes
**Then** the `item_ids` map is read from the latest queue state, not a stale snapshot
**And** each reorder operation acts on the correct item index

**Technical Notes:** The race window is <10ms. The `item_ids` map is rebuilt on each `MpdEvent::Queue`. The fix should either (a) use the same generation counter that queue events carry, or (b) debounce rapid key events and operate on the final state only.

---

## Epic 7: Cover Art & Visual Polish
**Goal:** Users see real album artwork throughout the app — cover grid, Now Playing, queue. Cover art is fetched from local files, embedded metadata, and online services, cached in memory and on disk.

### Story 7.1: Local Cover Art Fetcher

As a user,
I want the app to find and display cover art from my local music files,
So that albums show their actual artwork instead of colored placeholders.

**Acceptance Criteria:**

**Given** an album with tracks in a local directory
**When** the cover fetcher scans for artwork
**Then** it checks in order: embedded artwork in audio files, `cover.jpg`/`cover.png` in the album directory, `folder.jpg`, `AlbumArt.jpg`, any `.jpg`/`.png` in the directory
**And** returns the first found image path

**Given** no local artwork is found
**When** the cover fetcher completes local scanning
**Then** it returns `None` to trigger online fallback (Story 7.2) or placeholder display

### Story 7.2: Online Cover Art Lookup with Caching

As a user,
I want the app to find album artwork from online services when local artwork is unavailable,
So that even obscure albums have cover art displayed.

**Acceptance Criteria:**

**Given** local cover fetching returned `None`
**When** the online lookup is triggered
**Then** it queries configured sources (MusicBrainz, Cover Art Archive) using album + artist
**And** respects rate limiting (1 request/second default)
**And** retries failed requests with exponential backoff (1s, 2s, 4s, 8s, 16s)

**Given** a cover is fetched online
**When** the image is downloaded
**Then** it is cached to disk at `~/.cache/mpd-client/covers/<hash>.jpg`
**And** it is stored in the memory session cache for immediate reuse

### Story 7.3: Cover Display in Album Grid

As a user,
I want album covers to show real artwork in the cover grid,
So that the visual browsing experience matches the placeholder layout with actual images.

**Acceptance Criteria:**

**Given** an album has a cached or fetched cover image
**When** the album grid cell is rendered
**Then** the cover image replaces the hash-derived color placeholder
**And** hover controls still work correctly over the image

**Given** an album has no cover image available
**When** the album grid cell is rendered
**Then** the existing hash-derived color placeholder is displayed

### Story 7.4: Cover Display in Now Playing

As a user,
I want the currently playing track's album art to appear in the Now Playing section,
So that I can see the artwork regardless of which mode I'm in.

**Acceptance Criteria:**

**Given** a track is currently playing (or paused)
**When** the Now Playing section updates
**Then** the album's cover art is displayed alongside the track info
**And** if no cover is available, the placeholder is shown

---

## Epic 8: Playback Experience
**Goal:** Users can seek within tracks, see elapsed/duration, browse the current album's tracks, and see format badges.

### Story 8.1: Interactive Seekbar

As a user,
I want to click or drag a seekbar to navigate within the current track,
So that I can jump to specific parts of long tracks.

**Acceptance Criteria:**

**Given** a track is playing or paused with a known duration and elapsed time
**When** the seekbar is rendered in the Now Playing section
**Then** it shows current position as a filled portion of the total duration bar
**And** displays elapsed time (mm:ss) on the left and remaining/duration on the right

**Given** the user clicks at a position on the seekbar
**When** the click event fires
**Then** the MPD seek command is sent with the target position in seconds
**And** the seekbar updates immediately to reflect the new position

### Story 8.2: Elapsed/Duration Display

As a user,
I want to see the current playback position and total track duration,
So that I know how far into the track I am and how much remains.

**Acceptance Criteria:**

**Given** a track is playing with elapsed time from MPD status
**When** the Now Playing section updates
**Then** elapsed time is displayed in mm:ss format
**And** total duration is displayed in mm:ss format
**And** both update every 500ms as the status poll provides new values

### Story 8.3: Current Album Track Window

As a user,
I want to see the tracks of the currently playing album in the right rail,
So that I can quickly jump to any track on the album without switching views.

**Acceptance Criteria:**

**Given** an album is currently playing
**When** the right rail is in Album Mode
**Then** a scrollable list of the current album's tracks appears below Now Playing
**And** the currently playing track is highlighted
**And** clicking any track sends `PlayPosition` for that track
**And** the list updates when the album changes

### Story 8.4: Format Badges in Now Playing

As a user,
I want to see technical audio format information next to the current track,
So that I can verify the playback quality (bit depth, sample rate, DSD rate).

**Acceptance Criteria:**

**Given** a track is playing with audio format metadata available
**When** the Now Playing section updates
**Then** a format badge is displayed (e.g., "16/44.1", "24/96", "DSD64", "DSD128")
**And** the badge is compact and styled consistently with the folder tree format badges
**And** if no format info is available, no badge is shown

---

## Epic 9: Search & Browse Polish
**Goal:** Folder Mode search, pinned group headers, CUE/DSD interactive rows, fixed grouped-mode double-click, AlbumArtist tag support, search relevance scoring.

### Story 9.1: Folder Mode Search

As a folder-mode user,
I want to search for folders and files by name,
So that I can quickly find specific directories or tracks in large folder structures.

**Acceptance Criteria:**

**Given** the user is in Folder Mode
**When** they type in the search bar
**Then** search matches against directory names, file names, and full paths
**And** results are displayed in a flat list with path breadcrumbs
**And** clicking a result navigates to that item in the folder tree
**And** the original folder view is restored when search is cleared

### Story 9.2: Pinned Group Headers While Scrolling

As a user browsing grouped album views,
I want group headers (Artist, Year, Genre names) to stay visible while scrolling,
So that I always know which group I'm looking at.

**Acceptance Criteria:**

**Given** the album grid is displaying a grouped view (Artist, Year, or Genre)
**When** the user scrolls down through albums in a group
**Then** the current group's header remains pinned at the top of the visible area
**And** when the next group's header reaches the top, it replaces the pinned header
**And** the transition is smooth without flickering

### Story 9.3: Fix Grouped-Mode Double-Click

As a user browsing grouped album views,
I want double-clicking an album cover to play that specific album,
So that I don't get the wrong album because of grid index mismatch.

**Acceptance Criteria:**

**Given** the album grid is in a grouped view with headers interleaved
**When** the user double-clicks an album cover
**Then** the `album_names` lookup correctly maps the FlowBox child index to the album
**And** the correct `PlayAlbum` command is sent with the right album name
**And** this works for any album in any position across all grouped views

**Status: ✅ DONE (2026-04-29)** — `album_names` -> `Vec<Option<String>>` with `None` for group headers.

### Story 9.4: CUE/DSD Row Playback

As a folder-mode user,
I want clicking a CUE or DSD summary row to play the associated tracks,
So that I can listen to cue sheet albums or DSD albums directly from the normalized view.

**Acceptance Criteria:**

**Given** a CUE summary row is displayed in the folder tree
**When** the user clicks or activates the row
**Then** all audio tracks associated with the cue sheet in that directory are added to the queue and playback starts

**Given** a DSD summary row is displayed
**When** the user clicks or activates the row
**Then** all DSD tracks (.dsf/.dff) in that directory are added to the queue and playback starts

### Story 9.5: AlbumArtist Tag Support in Search

As a user searching for compilation albums,
I want the search to correctly identify the album artist,
So that compilations and soundtracks show the right artist name instead of "Unknown Artist".

**Acceptance Criteria:**

**Given** an MPD search response includes `AlbumArtist:` lines
**When** the `search_albums` parser processes the response
**Then** `AlbumArtist:` is checked as a fallback when `Artist:` is missing for an album
**And** albums tagged only with `AlbumArtist:` display the correct artist in search results

**Status: ✅ DONE (2026-04-27, commit 68f3593)**

### Story 9.6: Fix list_albums_grouped Artist Loss for Date/Genre

As a user browsing grouped views,
I want all albums to show their artist name in Date and Genre groupings,
So that I can identify albums correctly even when MPD omits artist metadata.

**Acceptance Criteria:**

**Given** the user switches to a Date or Genre grouped view
**When** `list_albums_grouped` processes the MPD response
**Then** each album's artist is resolved by fetching per-album metadata when the grouped response lacks it
**And** the artist is displayed alongside each album in the grid
**And** albums without artist metadata show "Unknown Artist" instead of an empty string

**Technical Notes:** MPD's `list` with group-by does not return artist data for non-Artist groupings. Requires per-album `listalbumartist` or `search` fetch to backfill. Batch to avoid N+1.

### Story 9.7: Fix search_albums Stale Artist Edge Cases

As a user searching the library,
I want search results to always show the correct artist name,
So that I don't see the previous track's artist carried over to a tagless track.

**Acceptance Criteria:**

**Given** an album has tracks with mixed Artist tags (some present, some missing)
**When** `search_albums` processes the MPD response
**Then** each album's artist is reset at each `file:` boundary
**And** albums with entirely missing Artist tags show "Unknown Artist"
**And** the `AlbumArtist` fallback from Story 9.5 still applies when Artist is missing

**Technical Notes:** The `file:` line separator fix was added but edge cases remain when Artist tags span multiple tracks. Ensure the artist accumulator is reset per-track boundary, not per-album boundary.

### Story 9.8: Search Relevance Scoring

As a user searching the library,
I want search results sorted by relevance (exact album title matches first, then artist matches, then partial matches),
So that the most likely intended album appears at the top of the results.

**Acceptance Criteria:**

**Given** a search query is entered in Album Mode
**When** results are displayed
**Then** they are sorted by relevance score in descending order
**And** scoring follows the PRD weighting: exact album title (100pts), exact artist (80pts), partial album title (60pts x match%), partial artist (40pts x match%), track title (30pts per match, max 90pts), year/genre (20pts)
**And** results below the minimum score threshold (20pts) are excluded
**And** tied scores retain their original album list order

**Given** a search query is entered in Folder Mode
**When** results are displayed
**Then** scoring follows the PRD weighting: exact filename (100pts), exact folder name (80pts), partial path (50pts x match%), file extension (10pts)
**And** results below the minimum score threshold (20pts) are excluded

**Given** 200+ results match a query
**When** the result set is computed
**Then** only the top 200 highest-scoring results are displayed
**And** a "Show all N results" button is shown at the bottom of the results list

**Technical Notes:** Relevance scoring is a pure computation on the existing search match set — no additional MPD queries or index changes needed. The scoring function should be a standalone `pub fn score(query: &str, album: &Album) -> u32` in `src/search/mod.rs` that any caller can use without touching the search index. Folder Mode scoring requires the folder tree's normalized entry metadata (filename, folder path). The 200-result limit avoids rendering thousands of items; the "Show all" button lifts the cap. Sorting by descending score uses the existing album ordering as the tiebreaker (stable sort).

---

## Epic 10: Layout & Configuration
**Goal:** Configurable layout values, smooth mode transitions, album mini-grid queue, Play Next in context menu.

### Story 10.1: Configurable Layout Values

As a user,
I want to adjust the split ratio and right rail proportions,
So that I can customize the layout to my preference.

**Acceptance Criteria:**

**Given** the Settings dialog is open
**When** the user navigates to the Layout section
**Then** they can adjust: split ratio (default 0.7), right rail width limits, album mode proportions, folder mode proportions
**And** changes are persisted to config TOML
**And** the layout updates immediately when settings are saved

### Story 10.2: Smooth Mode Transitions

As a user,
I want switching between Album Mode and Folder Mode to be visually smooth,
So that the transition feels polished and professional.

**Acceptance Criteria:**

**Given** the user switches modes via Ctrl+1/Ctrl+2 or the View menu
**When** the mode switch is triggered
**Then** the content fades or slides with a transition lasting <500ms
**And** scroll positions are saved before and restored after the transition
**And** the UI remains responsive during the transition

### Story 10.3: Album Mini-Grid Queue

As an album-mode user,
I want the queue in the right rail to show album covers instead of a text track list,
So that the visual album-browsing experience extends to the queue.

**Acceptance Criteria:**

**Given** the user is in Album Mode with albums in the queue
**When** the right rail queue section renders
**Then** queued albums are displayed as a mini cover grid
**And** the currently playing album is highlighted
**And** hovering over a mini cover shows the album name
**And** clicking a mini cover selects it; double-clicking plays it

### Story 10.4: Play Next in Context Menu

As a user,
I want a "Play Next" option in queue context menus,
So that I can insert tracks to play right after the current track.

**Acceptance Criteria:**

**Given** the user right-clicks a queue item
**When** the context menu appears
**Then** a "Play Next" option is shown alongside "Play Now" and "Remove"
**And** clicking it sends `InsertNext` for that item
**And** the queue updates to show the item inserted after the current track

---

## Epic 11: Drag & Drop
**Goal:** Users can drag albums to queue, reorder queue items by dragging, and remove items by dragging off.

### Story 11.1: Drag Albums from Grid to Queue

As an album-mode user,
I want to drag album covers from the grid into the queue,
So that I can add albums to the queue intuitively without using buttons.

**Acceptance Criteria:**

**Given** the album grid and queue are visible
**When** the user drags an album cover and drops it on the queue area
**Then** the album is added to the end of the queue via `Add` command
**And** a visual ghost of the album cover follows the cursor during drag
**And** the drop target area highlights when the cursor is over a valid drop zone

### Story 11.2: Reorder Queue Items via Drag

As a user,
I want to reorder items in the queue by dragging them to new positions,
So that I can arrange the playback order intuitively.

**Acceptance Criteria:**

**Given** the queue has multiple items
**When** the user drags a queue item and drops it between two other items
**Then** a `MoveId` command is sent to move the item to the drop position
**And** a visual drop indicator shows the insertion point during drag
**And** the queue updates to reflect the new order

### Story 11.3: Remove Items via Drag-Off

As a user,
I want to remove items from the queue by dragging them off the queue area,
So that I can quickly clean up the queue without context menus.

**Acceptance Criteria:**

**Given** the queue has items
**When** the user drags a queue item outside the queue area and releases it
**Then** a `DeleteId` command is sent to remove the item
**And** visual feedback during drag indicates the item will be removed (e.g., red tint, trash icon)
**And** the queue updates to remove the item

### Story 11.4: Album Reorder in Plain Albums View

As an album-mode user in the plain Albums view,
I want to manually reorder the album grid,
So that I can organize albums in my preferred order for the session.

**Acceptance Criteria:**

**Given** the user is in the plain Albums view (not grouped)
**When** the user drags an album cover to a new position in the grid
**Then** the album is moved to the drop position in the grid display
**And** the new order persists for the session only
**And** switching to a grouped view and back to Albums preserves the custom order

---

## Epic 12: UI Responsiveness
**Goal:** Grid population and event processing run off the GTK main thread. Large libraries don't freeze the UI during rebuild or event bursts. App stays smooth at 60 FPS.

### Story 12.1: Async Grid Population via GtkGridView Factory

As a user with a large library,
I want the album grid to populate without freezing the UI,
So that I can scroll and interact while albums load.

**Acceptance Criteria:**

**Given** the app is populating the album grid (initial load, grouped view switch, search results)
**When** `populate_album_grid` or `populate_grouped_grid` would normally run synchronously
**Then** a `GtkGridView` factory pattern is used instead of manual `FlowBox` child insertion
**And** cell widgets are created in batches of 16 per idle cycle
**And** the UI remains responsive during population (>30 FPS maintained)
**And** cells are recycled when the model changes (not destroyed and recreated)

**Technical Notes:** The architecture notes GtkGridView factory pattern as the designed fix. Requires replacing `FlowBox` with `GtkGridView` + `GtkSliceListModel` + factory signal.

### Story 12.2: GTK4 Frame Clock Integration (Replace 30ms Timer)

As a user,
I want smooth scrolling and event processing without stutter,
So that the UI feels responsive even under heavy event load.

**Acceptance Criteria:**

**Given** the app processes MPD events at high frequency (queue updates, status changes)
**When** the 30ms `glib::timeout_add_local` timer fires
**Then** event processing is driven by GTK4's `GdkFrameClock` instead of a fixed-interval timer
**And** events are batched per-frame (max 64 per frame as a safety limit)
**And** no event processing happens between frames or during layout passes

**Technical Notes:** The current 30ms timer can starve the GTK main loop under heavy load. `GdkFrameClock::connect_frame_tick` fires once per monitor refresh, aligning work with vsync. The 64-event batch limit already exists as a mitigation.

---

## Epic 13: Cover Art v2 Pipeline
**Goal:** Replace the basic `CoverFetcher` with the two-layer `CoverProvider + ActualRead` architecture designed in `architecture.md`. MD5 content-addressed disk cache, scroll-aware loading, online lookup support.

### Story 13.1: CoverProvider — Synchronous Cache Read

As a developer,
I want a fast synchronous cache layer that never blocks the UI,
So that cover art for cached albums returns immediately without any fallthrough chain.

**Acceptance Criteria:**

**Given** an album has a cached cover image on disk
**When** `CoverProvider::get(album_id)` is called
**Then** it returns `Some((path, md5_hash, timestamp))` synchronously (no I/O wait, no blocking)
**And** the path is a valid JPEG at `~/.cache/mpd-client/covers/<md5>.jpg`

**Given** an album has no cached cover
**When** `CoverProvider::get(album_id)` is called
**Then** it returns `None`
**And** the caller can enqueue the album in ActualRead for background fetching

**Technical Notes:** In-memory index of `album_id → (path, hash, timestamp)` built on startup from the cache directory. No disk I/O at query time. Fast-path for `src/coverart/mod.rs`.

### Story 13.2: ActualRead — Background Fetch Queue

As a developer,
I want cover fetching to run one album per idle cycle in the background,
So that covers load incrementally without blocking MPD commands or the UI.

**Acceptance Criteria:**

**Given** the ActualRead queue has albums to process
**When** the background idle cycle fires
**Then** exactly one album is fetched per cycle
**And** the fetch uses `albumart <uri>` primary then `readpicture <uri>` fallback
**And** the binary data is MD5-hashed and compared against the cache hash
**And** only a different hash triggers `CoverRefreshed` emission
**And** identical hashes are silently skipped (no emission, no redraw)

**Given** a `readpicture` fetch succeeds
**When** the MPD response includes a timestamp
**Then** the timestamp is compared against the cached timestamp
**And** only newer timestamps trigger emission

**Technical Notes:** AlbumArtProvider (primary, content-addressed via MD5) and ReadPictureProvider (fallback, time-addressed). One album per idle cycle (~100ms) keeps the MPD command loop responsive.

### Story 13.3: Scroll-Aware Loading Priority

As a user with a large library,
I want visible albums to load their covers first,
So that scrolling doesn't trigger unnecessary fetches for off-screen items.

**Acceptance Criteria:**

**Given** the user scrolls through the album grid rapidly
**When** scroll stops for ≥300ms
**Then** covers for visible (and near-visible, ±1 row) albums are enqueued in ActualRead with high priority
**And** albums that scrolled out of view during rapid scrolling are NOT enqueued
**And** the 300ms stop timer resets on each scroll event

### Story 13.4: Widget Registry Integration for In-Place Updates

As a user,
I want cover art to appear in the grid as soon as it's fetched,
So that placeholders are replaced seamlessly without grid rebuilds.

**Acceptance Criteria:**

**Given** AlbumArtProvider emits `CoverRefreshed(id, data)`
**When** the UI thread receives the event
**Then** the `GdkTexture` is decoded from raw bytes on the main thread
**And** the widget registry (`HashMap<String, Picture>`) looks up the target cell
**And** `set_filename()` / `queue_draw()` updates the cell in-place
**And** no full grid rebuild is triggered

**Technical Notes:** The widget registry pattern is already validated and working in the v1 implementation. This story formalizes it for the v2 pipeline.

### Story 13.5: Online Cover Lookup (Opt-In)

As a user,
I want the app to optionally fetch covers from online sources when local/MPD art is unavailable,
So that even albums without embedded or folder art can have covers.

**Acceptance Criteria:**

**Given** AlbumArtProvider and ReadPictureProvider both returned no cover
**When** the `online-cover-art` feature flag is enabled
**Then** the album is enqueued for online lookup (MusicBrainz → Cover Art Archive → Discogs)
**And** requests are rate-limited to 1/second (configurable)
**And** failed lookups are retried with exponential backoff (1s, 2s, 4s, 8s, 16s max)

**Given** the `online-cover-art` feature flag is disabled
**When** an album has no locally available cover
**Then** no HTTP request is made (privacy-by-default)
**And** the album continues showing the hash-derived color placeholder

**Technical Notes:** Online lookup is opt-in via `Cargo.toml` feature flag (`online-cover-art`). Uses `ureq` + `rustls` for HTTP. Disabled by default.

---

## Epic 14: CLI & Desktop Integration
**Goal:** Enable desktop environment integration through CLI flags for session-level overrides, MPRIS D-Bus for media key support and lock screen controls, second-instance detection for single-instance operation, and optional libnotify notification support.
**ADRs:** §661 (IPC & CLI), §743 (Notification & System Integration)
**FRs covered:** PRD §194 (desktop integration), PRD §195 (keyboard shortcuts CLI)

### Story 14.1: CLI Argument Parsing

As a user,
I want to pass command-line flags to control the application at startup,
So that I can specify MPD host/port, startup mode, and initial actions without editing config files.

**Acceptance Criteria:**

**Given** the application is launched from the command line
**When** `--mpd-host <host>` is passed
**Then** the MPD connection uses the specified host (overrides config)
**And** the config file is not modified

**Given** the application is launched with `--mode album` or `--mode folder`
**When** the UI initializes
**Then** the specified mode is activated on startup (overrides last-saved mode)

**Given** the application is launched with `--start-playing` or `--toggle-playback`
**When** the connection to MPD is established
**Then** the corresponding action is dispatched

**Given** invalid flags are passed
**When** the CLI parser encounters an unrecognized argument
**Then** a helpful error message is printed to stderr
**And** the application exits with a non-zero status

**Technical Notes:** Use a lightweight CLI parser (`argh` or manual `std::env::args()` parsing — no clap dependency needed for <10 flags). Supported flags: `--mpd-host`, `--mpd-port`, `--profile`, `--mode`, `--start-playing`, `--toggle-playback`, `--next`, `--prev`, `--version`, `--help`. Config overrides are session-only (not persisted).

### Story 14.2: MPRIS D-Bus Integration

As a user,
I want the application to integrate with the Linux desktop via MPRIS D-Bus,
So that I can control playback with media keys, lock screen controls, and tools like `playerctl`.

**Acceptance Criteria:**

**Given** the application is running and MPRIS is enabled in config
**When** `playerctl play-pause` is invoked
**Then** playback toggles via the existing MPD command channel
**And** `playerctl status` returns the correct playback state

**Given** a track is playing
**When** a D-Bus client queries `org.mpris.MediaPlayer2.Player` properties
**Then** `PlaybackStatus`, `Metadata` (title, artist, album, art URL, length), and `Position` are returned correctly

**Given** MPRIS is disabled in config (default)
**When** the application starts
**Then** no D-Bus name is acquired
**And** no MPRIS-related code runs

**Technical Notes:** Implement `org.mpris.MediaPlayer2` and `Player` interfaces via `zbus` crate. D-Bus bus name: `org.mpris.MediaPlayer2.mpdclient`. Disabled by default: `[mpris] enabled = false` in config. MPRIS method calls map to existing `MpdCommand` channel — no new code paths for playback control. Feature flag: `mpris`.

### Story 14.3: Second-Instance Detection

As a user launching the application a second time,
I want the second instance to forward CLI actions to the already-running instance,
So that I don't get duplicate windows and my intended action still happens.

**Acceptance Criteria:**

**Given** an instance of the application is already running
**When** a second instance is launched with `--toggle-playback`
**Then** the second instance detects the running instance via a lock file
**And** forwards the action to the running instance via Unix socket
**And** the second instance exits without creating a window

**Given** no instance is running
**When** the application starts
**Then** a lock file is created at `~/.cache/mpd-client/lock`
**And** a Unix socket listener is started for incoming commands

**Given** the application exits normally
**When** the shutdown sequence runs
**Then** the lock file and Unix socket are cleaned up

**Technical Notes:** Lock file at `~/.cache/mpd-client/lock`. Unix socket listener runs on GTK main loop via `gio` socket API (no separate thread). Remote actions are dispatched as internal commands, not re-parsed. `gio`-managed socket binding auto-cleans on crash.

### Story 14.4: libnotify Notification Integration

As a user,
I want the application to optionally show desktop notifications for important events,
So that I can be informed of connection changes or playback events even when the window is minimized.

**Acceptance Criteria:**

**Given** the application is running and libnotify is enabled in config
**When** the MPD connection drops
**Then** a persistent desktop notification is shown: "MPD disconnected — retrying..."
**And** the notification is updated or dismissed when the connection is restored

**Given** libnotify is disabled in config (default)
**When** a connection event occurs
**Then** no desktop notification is shown (in-app toast only)

**Technical Notes:** Opt-in via config (`[notifications] libnotify = false` by default). Uses the same `zbus::blocking::Connection` as MPRIS (story 14-2) to call `org.freedesktop.Notifications` D-Bus interface directly — no `notify-rust` crate needed. Maps to existing toast event stream. Disabled by default for privacy — user must opt in.

---

## Epic 15: Connection Profiles
**Goal:** Support multiple named MPD connection profiles (local Unix socket, remote NAS, etc.) with persistent config and profile selection UI.
**ADRs:** §448 (Multi-Profile Connections)

### Story 15.1: Multi-Profile Connection Config

As a user with multiple MPD instances (local, NAS, work),
I want to define named connection profiles in the config,
So that I can switch between MPD servers without re-entering connection details.

**Acceptance Criteria:**

**Given** the config file contains multiple profiles
**When** the application reads the config
**Then** all profiles are parsed and available for selection
**And** the `default_profile` key determines the initial connection
**And** the `last_profile` key is updated on each manual profile switch

**Given** a profile is defined with a Unix socket path
**When** connecting via that profile
**Then** the adapter uses the Unix socket path directly (skips auto-detection)

**Given** no profiles are defined in config
**When** the application starts
**Then** the existing single-host behavior is preserved (backward compatible)
**And** the first successful auto-detect is saved as the "default" profile

**Technical Notes:** Config format:
```toml
[profiles.local]
host = "/run/mpd/socket"
[profiles.nas]
host = "192.168.1.100"
port = 6600
[general]
default_profile = "local"
last_profile = "local"
```
No profile editing UI in v1 — profiles are hand-edited in TOML. Profile selector in settings dialog.

### Story 15.2: Profile Selector in Settings Dialog

As a user,
I want to switch between connection profiles from the settings dialog,
So that I can change MPD servers without editing config files.

**Acceptance Criteria:**

**Given** the settings dialog is open
**When** the user navigates to the Connection section
**Then** a profile dropdown shows all defined profiles from the config
**And** the current profile is pre-selected

**Given** the user selects a different profile and clicks Save
**When** the settings are applied
**Then** the current MPD connection is gracefully closed
**And** a new connection is established using the selected profile's host/port/socket
**And** the `last_profile` key is updated in config

**Given** only one profile is defined
**When** the settings dialog opens
**Then** the profile dropdown is hidden (no selection needed)

**Technical Notes:** Profile dropdown uses `gtk4::DropDown` with a `StringList`. Connection restart uses the same `MpdCommand::Reconnect` mechanism. No profile add/edit UI in v1.

---

## Epic 16: Infrastructure & Code Quality
**Goal:** Config schema versioning with corruption recovery, rotating log file infrastructure, and window geometry persistence for startup state restoration.
**ADRs:** §589 (Configuration Management), §621 (Logging & Observability), §810 (Session Persistence)

### Story 16.1: Config Schema Version and Corruption Recovery

As a developer,
I want the config file to have a schema version with automatic corruption recovery,
So that future config changes are migratable and corrupt configs don't break the application.

**Acceptance Criteria:**

**Given** the config file has a `schema_version` field that is lower than the current version
**When** the config is loaded
**Then** a migration function is applied to update the config to the current version
**And** the migrated config is written back to disk

**Given** the config file is corrupt (invalid TOML, truncated, or unreadable)
**When** the config loader attempts to parse it
**Then** the corrupt file is backed up to `config.toml.bad`
**And** a fresh default config is created
**And** a warning is logged with the backup path

**Given** the config file is valid but missing the `schema_version` field
**When** the config is loaded
**Then** the schema version defaults to 1
**And** all migrations from version 1 to current are applied sequentially

**Technical Notes:** `schema_version: u32` field in `Config` struct, defaulting to 0 (unversioned). Migration functions are a `Vec<fn(&mut Config)>` indexed by version — each function transitions from version N to N+1. Current version starts at 1. Backup path: `~/.config/mpd-client/config.toml.bad`.

### Story 16.2: Rotating Log File

As a developer debugging issues,
I want the application to write logs to a rotating file in addition to stderr,
So that I can review past sessions' logs even after the terminal is closed.

**Acceptance Criteria:**

**Given** the application is running with `RUST_LOG=debug`
**When** log messages are emitted
**Then** they are written to both stderr (at configured level) and a log file
**And** the log file is at `~/.local/share/mpd-client/log/`
**And** log files rotate at 5MB each, keeping 3 rotated files

**Given** the log directory does not exist
**When** the application starts
**Then** the directory is created automatically

**Given** the log file cannot be written (permissions, disk full)
**When** a log write attempt fails
**Then** the error is silently ignored (logging failure is non-fatal)
**And** the application continues with stderr-only logging

**Technical Notes:** Use `log` + `env_logger` for the existing setup. Add a file appender via a custom logger or `log4rs` (minimal config, no polling). Log path: `~/.local/share/mpd-client/log/mpd-client.log`. Rotation: max 5MB per file, 3 files. Trace-level to file, info-level to stderr (configurable via `RUST_LOG`).

### Story 16.3: Window Geometry Persistence

As a user,
I want the application to remember my window size and position between sessions,
So that I don't have to resize and reposition the window every time I launch it.

**Acceptance Criteria:**

**Given** the user resizes and repositions the window
**When** the application exits normally
**Then** the window width, height, and (x, y) position are saved to the config file

**Given** the application is restarted after a normal exit
**When** the window is created
**Then** the window geometry from the saved config is applied
**And** the window appears at the saved position with the saved size

**Given** the saved position is off-screen (multi-monitor change)
**When** the window geometry is restored
**Then** GTK4's default window positioning is used as fallback
**And** the off-screen position is ignored

**Technical Notes:** Store `window_width`, `window_height`, `window_x`, `window_y` in `Config` struct. Write on graceful shutdown only (not on every resize — avoids IO churn). GTK4's `GtkWindow::get_default_size()` and `GtkWindow::get_position()` provide the values. Off-screen detection: check against available monitor geometry via `GdkDisplay::monitors()`.

---

## Epic 17: MPRIS & SharedState Integration

**Goal:** Fix MPRIS metadata output by populating SharedState `CurrentContext` from MPD state changes, emit MPRIS `PropertiesChanged` signals for lock-screen auto-update, and handle D-Bus session bus disconnection gracefully.

**ADRs:** §661 (IPC & CLI Architecture), §743 (Notification & System Integration)

**FRs covered:** FR-P2 (playback state display), PRD §194 (desktop integration)

### Story 17.1: Populate SharedState CurrentContext from MPD State Changes

As an MPRIS client,
I want `SharedState.current.track` and `SharedState.current.album` to be populated from MPD state changes,
So that `playerctl metadata` returns the correct title, artist, and album for the current track.

**Acceptance Criteria:**

**Given** a track is playing with artist, title, and album available from MPD
**When** the `StateChanged` handler in `ui/mod.rs` processes the `PlaybackUpdate`
**Then** `update_current_context()` is called with `CurrentContext { track: Some(track_info), album: Some(album_name) }`
**And** the fields are written to `SharedState` via the existing `Store::update_current_context()` method

**Given** no track is playing (MPD state is `stop`)
**When** the `StateChanged` handler processes the update
**Then** `update_current_context()` is called with `CurrentContext { track: None, album: None }`
**And** SharedState reflects the empty state

**Given** MPRIS (or any other consumer) reads `SharedState.current`
**When** a track is playing
**Then** `s.current.track` returns populated `Track` data with artist, title, and file path
**And** `s.current.album` returns the album name string

**Given** `playerctl metadata` is invoked while a track is playing
**When** the MPRIS interface reads from SharedState
**Then** `xesam:title`, `xesam:artist`, and `xesam:album` return real values (not empty)

**Technical Notes:** The `StateChanged` handler in `src/ui/mod.rs` (line ~1768) receives `MpdEvent::StateChanged(update)` with a `PlaybackUpdate` struct that already contains `artist`, `title`, `album`, `elapsed`, `duration`, and `state`. The handler calls `update_now_playing()` which updates GTK labels but never writes to `SharedState.current`. The fix is to add a `Store::update_current_context()` call alongside the existing `update_now_playing()` call. The `PlaybackUpdate` fields map to `CurrentContext.track` (construct a `Track` from `artist`, `title`, `file`, `duration`) and `CurrentContext.album` (from `album`). No GTK label changes needed — this is purely a SharedState write. The `Store` and `update_current_context()` method already exist; they are simply never called from the StateChanged handler.

### Story 17.2: Emit MPRIS PropertiesChanged Signal on Playback State Changes

As a user with lock screen media controls,
I want the MPRIS interface to emit `PropertiesChanged` signals on playback state transitions,
So that `playerctl status` and lock screen controls auto-update without manual polling.

**Acceptance Criteria:**

**Given** the application is running with MPRIS enabled
**When** playback state changes (playing → paused, paused → playing, stop, track change)
**Then** the MPRIS interface emits `org.freedesktop.DBus.Properties.PropertiesChanged` for `org.mpris.MediaPlayer2.Player`
**And** the signal carries the changed properties: `PlaybackStatus`, `Metadata`, and `Position`

**Given** `playerctl --follow` is monitoring the MPRIS interface
**When** a track change occurs
**Then** `playerctl --follow` outputs the updated metadata within 1 second of the change

**Given** playback state remains unchanged (same track, same state)
**When** no MPD event triggers a state transition
**Then** no `PropertiesChanged` signal is emitted (no duplicate emissions)

**Given** the MPRIS interface is queried via `playerctl status`
**When** playback transitions from playing to paused
**Then** the status update is received without requiring `playerctl status` to be re-invoked

**Technical Notes:** The `PlaybackUpdate` struct carries `state`, `artist`, `title`, `album`, `elapsed`, `duration`. A dedicated channel or callback from the UI thread to the MPRIS module is needed since the MPRIS struct is created in `mpris.rs` and the state changes arrive in `ui/mod.rs`. Options: (a) pass a `mpsc::Sender<PlaybackUpdate>` to the MPRIS module and emit signals from the zbus IO thread, or (b) store a reference to the emitted signal sender on a shared structure. Option (a) is simpler — the MPRIS module receives updates and emits `PropertiesChanged` via `connection.object_server().interface_set("org.mpris.MediaPlayer2.Player", ...).unwrap().set_properties(...)`. The zbus `ObjectServer` provides `context.path().interface::<T>()` for emitting property changes. Property emission is done on the zbus IO thread (not GTK thread). The `PlaybackUpdate` channel must buffer (latest value only) to avoid queuing stale updates.

### Story 17.3: Monitor and Reconnect D-Bus Session Bus

As a user with MPRIS enabled,
I want the application to recover from a D-Bus session bus restart,
So that MPRIS media controls continue working without restarting the application.

**Acceptance Criteria:**

**Given** the application is running with MPRIS enabled and the D-Bus session bus is active
**When** the D-Bus session bus is restarted (e.g., `dbus-daemon --replace`, session logout/login edge case)
**Then** the MPRIS module detects the disconnection within 5 seconds
**And** attempts reconnection with retry (1s, 2s, 4s backoff, max 3 retries)
**And** re-registers the MPRIS interfaces on successful reconnection

**Given** reconnection succeeds after a D-Bus restart
**When** the MPRIS interfaces are re-registered
**Then** `playerctl status` works again without restarting the application client
**And** a toast notification is shown: "MPRIS reconnected"

**Given** all reconnection attempts fail
**When** the retry limit is exhausted
**Then** MPRIS integration is disabled for the session
**And** a warning is logged: "MPRIS: failed to reconnect after 3 attempts"
**And** the application continues without MPRIS (no crash, no hang)

**Technical Notes:** zbus's `Connection` does not have built-in reconnection for the blocking API. The approach is: (a) spawn a monitoring thread that periodically checks connection health via `connection.is_connected()` (zbus 5.x provides this method), (b) on disconnect, attempt `Connection::new_session()` with retry, (c) on success, register interfaces on the new connection via `object_server().at()`. The monitoring interval is 5 seconds (check every 5s if connected). The monitoring thread is spawned in `mpris::init()` alongside the interface registration. Shared `Arc<Mutex<Option<Connection>>>` holds the current connection for MPRIS method dispatch - the monitoring thread swaps it on reconnection. Method handlers (Play, Pause, etc.) must handle the case where `cmd_tx.send()` fails during reconnection (log and return). This is NOT expected to be a common case - D-Bus session bus restarts are rare in normal desktop operation.

---

## Epic 18: Accessibility Compliance

**Goal:** Meet WCAG 2.1 AA compliance targets - high contrast mode for users with visual impairments and complete keyboard navigation.

**FRs covered:** NFR-U4 (high contrast mode), NFR-U1 (keyboard navigation completeness)

### Story 18.1: High Contrast Mode Support

As a user with visual impairment,
I want the application to support a high contrast theme variant,
So that all text and UI elements meet WCAG 2.1 AA contrast ratios (4.5:1 text, 3:1 UI elements).

**Acceptance Criteria:**

**Given** the application is running in default dark theme
**When** the user enables high contrast mode in Settings
**Then** a high-contrast CSS variant is applied with WCAG 2.1 AA contrast ratios
**And** all text elements meet minimum 4.5:1 contrast against their background
**And** all UI controls (buttons, sliders, indicators) meet minimum 3:1 contrast
**And** cover art placeholders are replaced with solid high-contrast colors when enabled

**Given** the system accessibility preference signals high contrast
**When** the application starts
**Then** the high contrast theme is loaded automatically (respects system contrast settings)

**Given** high contrast mode is active
**When** the user disables it in Settings
**Then** the normal theme is restored
**And** the preference is persisted in config

**Given** the user switches between normal and high contrast mode
**When** the theme changes
**Then** no widgets are left unstyled
**And** the transition is instant (no animation delay)

**Technical Notes:** High contrast mode is a CSS variant loaded alongside the base theme. GTK4 CSS variables (`@define-color`) are overridden for the high contrast palette. A new `high_contrast: bool` field is added to the `Config` struct. On toggle, the application CSS provider is updated without re-creating any widgets. High contrast palette: backgrounds (#000000 or #1e1e1e for dark HC, #ffffff for light HC), text (#ffffff or #e0e0e0 minimum), accents (maintain hue but increase lightness). Detection via `GtkSettings` contrast preference inspection.

### Story 18.2: Keyboard Navigation Audit and Completion

As a user who relies on keyboard navigation,
I want to traverse all interactive elements using only the keyboard,
So that I can fully operate the application without a mouse.

**Acceptance Criteria:**

**Given** the application is running
**When** the user presses Tab/Shift-Tab
**Then** focus traverses through all major zones in logical order: search bar, album grid/folder tree, queue, transport controls, mode switcher, menu
**And** no interactive element is unreachable by Tab
**And** the focus order follows visual left-to-right, top-to-bottom order

**Given** focus is in the album grid
**When** the user presses arrow keys
**Then** focus moves in 2D (respecting column count and group boundaries)
**And** Enter/Space activates the focused item (select or play)

**Given** focus is in the folder tree
**When** the user presses Left/Right arrow
**Then** the focused folder is collapsed/expanded
**And** Up/Down moves between rows

**Given** focus is in the queue
**When** the user presses Shift+Up/Shift+Down
**Then** the focused item is moved up/down in the queue

**Given** a keyboard shortcut is bound (Ctrl+F, Ctrl+1/2, Space, etc.)
**When** the shortcut is pressed
**Then** the corresponding action is triggered regardless of where focus is (global shortcuts)
**And** no shortcut conflicts with GTK4 built-in widget shortcuts

**Given** the audit is complete
**When** all findings are documented
**Then** a report lists each missing or broken keyboard interaction
**And** all identified issues are resolved or documented as known limitations

**Technical Notes:** Create a keyboard navigation audit checklist covering all interactive elements in both Album and Folder modes. Use GTK4's focus chain (`set_focus_child`, `set_can_focus`) and `GtkEventControllerKey` for custom key handling. Test with Tab-only navigation (no mouse) to verify completeness. Pay special attention to: album grid items, folder tree rows, queue items, context menus, settings dialog, hover buttons (show on focus, not just hover), seekbar (arrow keys for fine adjustment, PageUp/PageDown for coarse). Add accessible labels (`set_accessible_label`) for screen readers where missing.

---

## Epic 19: Desktop Integration

**Goal:** Complete desktop environment integration with auto-start support.

**FRs covered:** NFR-O2 (auto-start capability)

### Story 19.1: Auto-Start Support

As a user,
I want the application to start automatically when I log into my desktop,
So that my music client is always ready without manual launch.

**Acceptance Criteria:**

**Given** the user enables auto-start in Settings
**When** the setting is saved
**Then** an XDG autostart `.desktop` file is created at `~/.config/autostart/mpd-client.desktop`
**And** the file contains the correct `Exec=` path pointing to the installed binary
**And** the file contains `X-GNOME-Autostart-enabled=true`

**Given** the user disables auto-start in Settings
**When** the setting is saved
**Then** the autostart `.desktop` file is removed
**And** the application does not start automatically on next login

**Given** the auto-start file already exists
**When** the application is launched by the desktop environment at login
**Then** the application starts normally with full functionality (no special auto-start mode needed)

**Given** the installed binary path changes (update/reinstall)
**When** auto-start is already enabled
**Then** the `Exec=` path in the autostart file is updated on next settings save

**Given** the `~/.config/autostart/` directory does not exist
**When** auto-start is enabled
**Then** the directory is created automatically

**Technical Notes:** XDG autostart standard: `.desktop` file at `~/.config/autostart/`. The `Exec=` path should use the installed binary location. Add `auto_start: bool` field to `Config` struct (default false). UI toggle in Settings dialog, Connection or General section. The autostart file uses `Type=Application` with the same `.desktop` name as the application's desktop entry file. No startup delay needed (`X-GNOME-AutostartDelay=0`).

---

## Epic 20: Desktop Integration (Post-v1 Cleanup)

**Goal:** Complete desktop integration — proper `.desktop` file with categories and MIME types for application menu integration.

**ADRs:** §724 (Build & Packaging Architecture)

**NFRs covered:** NFR-O2 (Auto-start capability / desktop entry)

### Story 20.1: Install Desktop File with Proper Categories

As a user, I want the application to appear in the desktop application menu with a proper icon and category, so that I can launch it from my desktop environment's app launcher.

**Acceptance Criteria:**

**Given** the application is launched for the first time
**When** startup completes
**Then** a `.desktop` file is installed at `~/.local/share/applications/mpd-client.desktop`
**And** the file contains `Categories=Audio;Music;Player;`
**And** `Name=mpd-client`, `Type=Application`, `Terminal=false`

**Given** the binary path changes after an update
**When** the application starts
**Then** the `Exec=` path in the desktop file is updated

**Technical Notes:** Uses XDG Desktop Entry spec. Install at `~/.local/share/applications/`. Call `install_desktop_file()` at startup. Different from the autostart file (story 19.1) — both are needed for full desktop integration. Common audio MIME types in `MimeType=` field. See story file for full details.

---

## Epic 21: Performance & Observability (Post-v1 Cleanup)

**Goal:** Basic performance profiling instrumentation — timing macros for critical paths, frame rate monitoring, memory snapshots.

**ADRs:** §621 (Logging & Observability)

**NFRs covered:** NFR-O5 (Performance profiling support)

### Story 21.1: Performance Profiling Instrumentation

As a developer, I want basic performance profiling instrumentation built into the application, so that I can identify slow operations and measure UI responsiveness.

**Acceptance Criteria:**

**Given** the application is running with `RUST_LOG=debug`
**When** a critical operation completes
**Then** a log message at debug level includes the elapsed time in milliseconds
**And** operations exceeding 100ms are logged at warn level

**Given** the application is compiled in release mode
**When** any profiling path would execute
**Then** the timing code is compiled out (gated behind `debug_assertions`)

**Technical Notes:** Use `std::time::Instant`, no new dependencies. Key paths: MPD commands, grid repopulation, cover decode, search queries, frame clock ticks. Gate with `cfg!(debug_assertions)` for zero cost in release. Memory via `/proc/self/status`. See story file for full details.

---

## Epic 22: libadwaita Widget Integration

**Goal:** Replace custom composite widgets with libadwaita (`adw` crate) equivalents — Toast overlay, ViewSwitcher, NavigationView, MultiLayoutView.

**ADRs:** §474 (libadwaita Integration)

### Story 22.1: libadwaita Integration

As a developer, I want to replace custom composite widgets with libadwaita equivalents, so that the UI follows GNOME HIG conventions and reduces custom widget maintenance.

**Acceptance Criteria:**

**Given** the `libadwaita` feature is enabled
**When** the application runs
**Then** `Adw.ToastOverlay` replaces the custom toast widget
**And** `Adw.ViewSwitcher` replaces the custom group button bar for mode switching
**And** `Adw.NavigationView` manages the navigation stack
**And** `Adw.MultiLayoutView` + `Adw.BottomSheet` handle responsive sidebar layout

**Given** the `libadwaita` feature is NOT enabled
**When** the application is compiled
**Then** existing custom widgets are used (backward compatible)

**Technical Notes:** Feature-gated behind `libadwaita` Cargo feature. Requires `libadwaita >= 1.6` at runtime. See story file for full details and architecture.md §474 for ADR.

---

## Epic 23: Centralized Keybinding Service

**Goal:** Single `KeybindingService` mapping `(key, mods, context)` to actions with compile-time conflict detection.

**ADRs:** §893 (Keybinding Architecture)

### Story 23.1: Centralized Keybinding Service

As a developer, I want all keyboard shortcuts managed through a single KeybindingService, so that shortcuts are auditable, conflict-free, and consistently dispatched.

**Acceptance Criteria:**

**Given** the application runs
**When** any keyboard shortcut is pressed
**Then** the key event is routed through a centralized keybinding table

**Given** the user switches between Album and Folder modes
**When** mode-specific keys are pressed
**Then** only the active mode's context bindings are enabled

**Given** the keybinding table is compiled
**When** two entries share the same (key, mods, context)
**Then** a unit test fails (conflict detection)

**Technical Notes:** Single `src/keybindings.rs` module. Action enum shared by keyboard, menu, hover buttons, and IPC. See architecture.md §893 for ADR.

---

## Epic 24: User-Selectable Sort Modes

**Goal:** Sort mode selector in Album Mode — By Artist (default), By Year, By Album Name. Client-side sort on cached metadata.

**ADRs:** §1131 (Browsing Sort Architecture)

### Story 24.1: User-Selectable Sort Modes

As a user browsing by album grid, I want to choose between sort modes, so that I can organize the library in the way that best suits my current task.

**Acceptance Criteria:**

**Given** the user is in Album Mode
**When** the user selects a sort mode from a dropdown/button
**Then** albums are sorted client-side from the cached album list
**And** no MPD round-trip is needed for sorting

**Given** a grouped view is active
**When** a sort mode is selected
**Then** the sort applies within each group

**Technical Notes:** Sort operates on the album list already cached in memory. See architecture.md §1131 for full sort strategy design.
