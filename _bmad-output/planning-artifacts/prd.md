# DESIGN

---
workflowType: 'prd'
workflow: 'edit'
classification:
  domain: 'music-player'
  projectType: 'desktop-application'
  complexity: 'intermediate'
inputDocuments: ['DESIGN.md']
stepsCompleted: ['step-e-01-discovery', 'step-e-02-review', 'step-e-03-edit']
lastEdited: '2026-04-22'
editHistory:
  - date: '2026-04-21'
    changes: 'Address validation gaps: success criteria, search functionality, cover art policies, technical specifications'
  - date: '2026-04-22'
    changes: 'Fix validation report issues: FR format violations, NFR measurability gaps, implementation leakage, project-type gaps; incorporate user tech stack (GTK4/Rust/Linux-only)'
---
## Scoped Features (2026-04-28)

The initial release implementation is complete. This section documents scoped architecture refinements based on cross-project analysis (Ario C/GTK3, Plattenalbum Python/GTK4, CoverGrid Python/GTK4) and advanced elicitation. All prior decisions remain valid unless explicitly overridden below.

### Technology Stack
- **Rust** edition 2024, MSRV 1.85, single crate
- **GTK4** 0.11 with v4_14 feature + **libadwaita** 0.8 (adw crate)
- **std::thread** with `mpsc::sync_channel` / `mpsc::channel`
- **zbus** crate for MPRIS D-Bus (disabled by default, opt-in via config)
- **env_logger**, **TOML** (dirs crate), **Linux-only**

### MPD Idle Protocol (replaces 500ms poll)

**Before:** 500ms polling loop calling `fetch_full_update()` (status + currentsong) every tick. External changes detected by `last_song_pos` comparison.

**After:** True MPD `idle`/`noidle` protocol (CoverGrid pattern). Worker thread blocks on `idle` when command queue is empty. Main thread writes `noidle\n` to socket clone (via `TcpStream::try_clone()`) to break idle when sending commands. Thread safety: half-duplex MPD protocol means read clone and write clone never contend. If `idle` returns a transient error, fall back to 100ms poll for 10 cycles then retry idle. If `idle` returns "unknown command", fall back to 500ms poll permanently.

**Rationale:** Eliminates poll overhead entirely. State changes (from other MPD clients) are reflected immediately, not within 500ms. The `try_clone` pattern is safe because MPD's protocol is serialized — you never read and write simultaneously.

### Cover Art Pipeline (replaces stub)

**Two-layer split architecture:**

- **CoverProvider** — pure cache read, must be blazing fast. Takes album ID, reads disk cache, returns `(PathBuf, MD5, Option<Timestamp>)` or `None`. Never blocks, never falls through to providers.

- **ActualRead** — background fetch queue, runs one album per idle cycle. Two providers:
  - **AlbumArtProvider** (primary): fetches via MPD `albumart <uri>`, computes MD5 of binary data, compares to cache hash. Emits `CoverRefreshed(id, Vec<u8>)` only if hash differs.
  - **ReadPictureProvider** (fallback, always enabled): fetches via MPD `readpicture <uri>`, compares timestamp. Emits only if timestamp is newer.

**Key rules:**
- Content-addressed for `albumart` (MD5 hash, no timestamp available)
- Time-addressed for `readpicture` (MPD provides modification time)
- CoverRefreshed carries raw bytes, not a path — UI decodes immediately, background writes cache independently
- CoverProvider and ActualRead never call each other (no loops)
- On reconnect/library change: enqueue visible albums into ActualRead, emit only on actual difference
- Cache: `~/.cache/mpd-client/covers/{md5_hash}.jpg` with metadata sidecar

### Queue Updates (replaces full playlistinfo)

**Before:** Full `list_queue` → playlistinfo re-fetch on every Queue event.

**After:** Incremental via `plchanges <version>`. Store `playlist_version` from status response. On playlist change signal, call `plchanges(last_version)` to get only changed items. Handle deletions by cross-referencing positions with reported playlist length. Every 50th update: full `playlistinfo` sync to reconcile. On version wrap-around (32-bit counter): full sync.

**Rationale:** Preserves UI state (scroll position, selection, animation) during surgical updates. Full re-fetch replaces the entire model, losing view state.

### Command List Batching (new)

Add `MpdCommand::Batch(Vec<MpdCommand>)` variant. MPD's `command_list_begin`/`command_list_end` wraps multiple commands atomically. If any command fails, the entire list is aborted — this is a feature, not a bug.

**Batch operations:** PlayAlbum, AddAlbum (all-or-nothing, transactional).
**No batch:** DeleteId, MoveId, InsertNext (partial failure acceptable).

Before: adding a 20-track album = 20 `addid` + 1 `play` = 21 individual channel messages + 21 MPD round-trips.
After: adding a 20-track album = 1 Batch message + 1 MPD command_list with 21 commands.

### Unix Socket Auto-Detection (new)

Priority: `$XDG_RUNTIME_DIR/mpd/socket` → `/run/mpd/socket` → TCP localhost:6600 → connection settings dialog. Skipped entirely if user has configured a host. Each failed connect is harmless (ECONNREFUSED/ENOENT, just try next).

### Multi-Profile Connections (new, previously out of scope)

Config supports `[profiles.<name>]` sections. `--profile <name>` CLI flag. Auto-detect on first connect saves into "default" profile. Profile selector in connection dialog. No profile editing UI in the release — profiles are hand-edited.

### MPRIS D-Bus Integration (new, previously out of scope)

MPRIS v2.1 Player interface via `zbus` crate. D-Bus bus name `org.mpris.MediaPlayer2.mpdclient`. Player interface only (Play/Pause/Stop/Next/Previous/Seek/SetPosition + standard properties). **Disabled by default** — `[mpris] enabled = false` in config. Connect MPRIS method calls to existing MpdCommand channel — no new code paths.

### libadwaita Integration

Added `adw` crate dependency. Replaces:
- Custom toast → `Adw.ToastOverlay`
- Manual navigation stack → `Adw.NavigationView`
- Responsive breakpoint CSS → `Adw.MultiLayoutView` + `Adw.BottomSheet`
- Mode switch → linked `ToggleButton`s (`.linked` CSS class) — replaced `Adw.ViewSwitcher` due to icon removal limitations

Requires libadwaita >= 1.6 at runtime (included in GNOME runtime, available in all major distros).

### Queue Model (unchanged)
Single track-oriented queue `ListBox` in the right rail. Album mode mini cover grid derived from same linear source. Track queue in folder mode remains flat. Dual-presenter architecture verified as correct.

### Known Limitations & Out of Scope
See `_bmad-output/implementation-artifacts/deferred-work.md` for the full historical list. Additions:
- Cover art cache revalidation on reconnect/library change
- MPRIS disabled by default — no D-Bus until user opts in
- Profile management UI out of scope for the release
- ReadPictureProvider timestamp comparison edge cases on some MPD versions

### Tests
17 integration tests with mock MPD server. Scoped test additions:
- MPD idle protocol (idle/noidle handshake, socket clone behavior)
- Cover art binary protocol (albumart/readpicture parsing, partial reads)
- CoverProvider cache hit/miss/refresh
- plchanges incremental update + version wrap
- Command list batch atomicity
- Unix socket auto-detection order
- Multi-profile config loading/fallback

---

## Versioning Policy

All work is classified as one of:

| Status | Meaning |
|--------|---------|
| **Scoped** | Planned for the release — may be done, in-progress, or pending |
| **Out of scope** | Explicitly excluded from the release — kept for historical reference only |

There is no "V1", "V2", "deferred", or other version-numbered status. The project has one release. Features are either scoped or out of scope for that release.

---

## Executive Summary

This MPD client focuses on two core music listening workflows with minimal feature creep:

1. **Album‑first listening** – Visual grid browsing for known collections
2. **Folder‑first discovery** – Text‑oriented technical view for new music

**Vision:** A calm, utilitarian desktop client that respects the music rather than competing with general‑purpose music managers.

**Differentiator:** Dual‑mode architecture with consistent playback core but mode‑specific presentation layers, optimized for real‑world MPD library structures (cue files, DSD folders, mixed metadata).

**Target Users:** 
- Music collectors with large, well‑tagged libraries (Album Mode)
- Technical listeners checking new acquisitions, DSD/PCM formats, folder structures (Folder Mode)
- Users who prefer direct file/folder access alongside traditional album‑based browsing

**Technology Stack:**
- **Frontend:** GTK4 (native Linux desktop toolkit)
- **MPD Adapter:** Rust (protocol handling, binary data processing)
- **Platform:** Linux only (primary target; no Windows/macOS support planned)

The product stays narrow by design—no social features, no cloud sync, no playlist management beyond MPD’s native capabilities.

## Success Criteria

### Business Outcomes
- **User adoption:** ≥80% of target users (MPD users with >1000 tracks) prefer client over existing MPD clients for daily listening within 30 days
- **Workflow efficiency:** Album‑mode users complete "browse→select→play" in ≤3 clicks; folder‑mode users identify audio format/organization issues 50% faster than with file managers

### Technical Performance
- **Playback uptime:** ≥99.5% (MPD‑dependent outages excluded)
- **Library load time:** <2 seconds for 50,000‑track library on SSD
- **Cover art cache hit rate:** >90% after initial library scan
- **MPD connection recovery:** <5 seconds after transient network loss
- **UI responsiveness:** All interactions <100ms (95th percentile)
- **Memory footprint:** <200MB RAM for typical library (10,000‑50,000 tracks)

### Quality Metrics
- **Audio format support:** 100% of MPD‑supported PCM/DSD formats displayed correctly
- **Cue file handling:** ≥95% of standard cue sheets parsed correctly with fallback to raw file
- **Drag‑and‑drop accuracy:** 100% of queue reorder operations preserve intended order
- **State synchronization:** Queue/playback state matches MPD with <1s latency

## Product Scope & Principles

**MVP Scope:** The client focuses exclusively on two core workflows—album‑first listening and folder‑first discovery—with minimal feature creep. No social features, cloud sync, or playlist management beyond MPD’s native capabilities.

**Growth Vision:** Future releases could consider advanced metadata editing, multi‑room playback, or mobile companion apps, but these are explicitly out of scope for the current release.

**Design Principles:**
- Album-first by default
- Playback controls are always visible
- Primary click behavior should stay simple and direct
- Search exists, but is hidden by default
- Queue is secondary to browsing and playback
- Visual noise should stay low
- Artwork matters in album mode and in now playing
- Folder mode should stay text-first and technical

## User Journeys

### Personas

#### The Album‑First Listener (Alex)
- **Role:** Music collector with a large, well‑tagged library
- **Goals:** Relaxed listening, revisiting known albums, building mood‑based queues
- **Frustrations:** Cluttered interfaces, excessive metadata, track‑focused views that obscure album context
- **Key Needs:** Visual album browsing, quick “play this album” flow, album‑level queue management
- **Success Metric:** Completes “browse→select→play” in ≤3 clicks, maintains queue across listening sessions

#### The Folder‑First Technical Listener (Jamie)
- **Role:** Audiophile, new‑music checker, format‑aware listener
- **Goals:** Verify audio quality, inspect folder structures, check DSD/PCM formats, organize incoming music
- **Frustrations:** Album‑centric views that hide file/folder reality, missing technical metadata, slow navigation
- **Key Needs:** Text‑first folder tree, per‑track format display, direct file/folder access, precision seeking
- **Success Metric:** Identifies audio format/organization issues 50% faster than with file managers

### Journey 1: Album‑First Listening

**Scenario:** Alex wants to listen to a favorite album while working.

**Entry Point:** App opens in Album Mode by default, showing cover grid.

**Steps:**
1. **Browse visually** – Scan cover grid; optionally switch to `Artists`, `Years`, or `Genres` grouped view
2. **Select album** – Single‑click to preview; double‑click to clear queue, enqueue album, start playback
3. **Manage queue** – Use hover controls (`+` add to queue, `←` insert after current, `>` play now) or drag‑and‑drop
4. **Monitor playback** – Now Playing shows cover, track info, concise format label; Current Album track window shows upcoming tracks
5. **Jump within album** – Click any track in Current Album window to skip directly

**Success Criteria:**
- Alex completes browse→select→play in ≤3 clicks
- Queue reflects album‑level intent (mini cover grid, album‑oriented reordering)
- Visual noise stays low; album art remains central
- Hover controls appear instantly without distracting

### Journey 2: Folder‑First Discovery & Checking

**Scenario:** Jamie just downloaded a new DSD album and needs to verify its structure and quality.

**Entry Point:** Switch to Folder Mode (manual action).

**Steps:**
1. **Navigate folder tree** – Expand/collapse folders with single click; see per‑track technical metadata (DSD128, 24/96, etc.)
2. **Inspect format details** – DSD rows show `DSD`/`DSD128` first; PCM rows show bit‑depth/sample‑rate (16/44, 24/192)
3. **Play for checking** – Double‑click folder to clear queue, enqueue folder content, start playback; double‑click track to start from that track
4. **Use precision seeking** – Interact with prominent seekbar in Folder Mode rail for quick scrubbing
5. **Manage track queue** – Queue appears as plain track list; reorder with drag‑and‑drop; follow‑on‑leave scrolling keeps current track visible

**Success Criteria:**
- Jamie identifies format/organization issues 50% faster than with file manager
- Technical metadata is visible at a glance without clicking through
- Folder‑to‑playback flow feels direct and unmediated by album assumptions
- Seekbar provides adequate scrubbing precision for quick checking

### Journey 3: Mixed‑Mode Workflow Switching

**Scenario:** User starts in Album Mode for casual listening, then needs to check a newly acquired folder.

**Steps:**
1. **Start in Album Mode** – Browse, queue, play as normal
2. **Switch modes** – One‑click mode toggle changes left panel (grid ↔ folder tree) while right rail adapts proportions
3. **Preserve playback state** – Now Playing, current track, queue continue uninterrupted
4. **Work in Folder Mode** – Expand target folder, inspect tracks, optionally enqueue
5. **Return to Album Mode** – Switch back; album grid remembers scroll position, selection, queue remains intact

**Success Criteria:**
- Mode switch completes in <500ms with no playback interruption
- Right rail proportions adjust smoothly (Album: 40/20/40 → Folder: 55/45)
- Queue presentation changes (album grid ↔ track list) but underlying queue order is preserved
- No data loss or reset of browsing state

### Cross‑Journey Consistency Rules

1. **Playback controls always visible** – Previous, play/pause, next, seekbar present in both modes
2. **Right rail persistence** – Shell split (70/30) and rail width (`clamp(320px, 30vw, 420px)`) stable across journeys
3. **Queue as single source of truth** – Underlying linear queue shared; only presentation differs (album grid vs track list)
4. **Search as fallback** – Hidden by default; available as inline filter when browsing insufficient
5. **Graceful degradation** – MPD disconnection shows inline empty panel, not blocking error; missing covers/metadata omitted silently

## Domain Requirements

**Audio Format Standards:** The client must correctly display all PCM and DSD formats supported by MPD, including sample rates (44.1kHz–384kHz), bit depths (16‑32bit), DSD rates (DSD64–DSD512), and channel counts (mono, stereo, multichannel).

**Metadata Handling:** Support ID3v2, Vorbis comments (FLAC), APE tags (Monkey's Audio), and MP4/iTunes metadata (ALAC). Display fallback to filename when tags missing.

**Cue Sheet Parsing:** Parse standard cue sheets (.cue) with multiple FILE/WAVE entries, track indices, and metadata. Fallback to raw file playback on parse errors.

**Cover Art Standards:** Prioritize embedded artwork (ID3 APIC, FLAC pictures), then folder‑level images (cover.jpg, folder.jpg), then online lookup (MusicBrainz, Discogs).

**MPD Protocol Compliance:** Adhere to MPD protocol 0.23+ for command/response patterns, binary data handling (album art), and connection management.

## Innovation Analysis

**Competitive Differentiation:**
- **vs. ncmpcpp:** Visual album grid (vs. text‑only), hover controls, dual‑mode architecture
- **vs. Cantata:** Simplified UI focused on two workflows (vs. feature‑rich), technical metadata prominence in folder mode
- **vs. gmpc:** Modern GTK4 interface, cue/DSD folder normalization, consistent right‑rail design

**Unique Value Propositions:**
1. **Dual‑mode architecture** – Album‑first vs. folder‑first workflows with shared playback core
2. **Technical metadata visibility** – Per‑track format display without clicking through
3. **Real‑world library normalization** – Cue sheets, DSD folders, mixed metadata handled transparently
4. **Calm, utilitarian design** – No social features, cloud sync, or playlist management beyond MPD

## Project‑Type Requirements

**Desktop Application Characteristics (Linux‑Only):**
- **Platform Support:** Linux only (primary target; no Windows/macOS support planned)
- **Update Strategy:** Updates handled by OS package manager (apt, dnf, pacman, etc.); no built‑in auto‑update mechanism
- **Offline Operation:** Full functionality without internet (MPD local server); online features (cover lookup) degrade gracefully
- **System Integration:** System tray icon with playback controls, notification area integration, desktop entry with proper categories
- **Keyboard Shortcuts:** Comprehensive shortcut support (space=play/pause, left/right=seek, m=mode toggle)
- **Theming Support:** Follow system dark/light theme; optional high‑contrast mode for accessibility
- **Accessibility:** Screen‑reader compatibility, keyboard‑only navigation, sufficient color contrast (WCAG 2.1 AA)
- **Resource Efficiency:** Memory footprint <200MB, CPU usage <5% idle, startup time <3 seconds on SSD

## Layout

### Overall Shell

Use a split view with a persistent right rail.

- Left side: main browsing area
- Right side: now playing, transport, and queue-related content

Initial split:

- Left: 70%
- Right: 30%

This split is a starting point, not a hard commitment.

Initial sizing target:

- Right rail width: `clamp(320px, 30vw, 420px)`

The UI should be ready to move later toward `75/25` if browsing benefits from it.

### Right Rail

The right rail is persistent in both modes. It should not rely on tabs by default. Its internal content changes by mode while the shell stays stable.

#### Album Mode Right Rail

Initial vertical proportions:

- Now Playing: 40%
- Current Album track window: 20%
- Album Queue: 40%

These are starting values and must remain flexible.

Now Playing contains:

- cover art when available
- track title
- artist and album
- concise playback format info
- playback controls
- seekbar

Current Album contains:

- a limited-height scrollable track window for the currently playing album
- played and current track context first
- as many next tracks as fit in the available height

Album Queue contains:

- mini cover grid of queued albums
- left-to-right, then top-to-bottom ordering
- covers at 50% of left-panel album size as the starting point
- starting layout is `3x2`
- enough space to take up roughly half of the rail when needed

#### Folder Mode Right Rail

Initial vertical proportions:

- Now Playing: 55%
- Queue: 45%

Folder mode now playing is more transport-focused.

- seekbar matters more here than in album mode
- cover art may be shown if available
- queue is visible at all times and takes the rest of the rail below now playing
- queue is a plain track list, not an album mini-queue
- queue scroll should only move when the current track would leave the viewport
- queue follow behavior should keep the current track visible without unnecessary repositioning
- current track should be indicated by accent styling only, without a dedicated play icon
- selected items that are not currently playing should use a subtle border or outline

### Layout Refinement Rules

#### Responsive Breakpoints
- **Desktop wide (≥1400px):** Shell split 75/25, right rail width `clamp(360px, 25vw, 480px)`
- **Desktop standard (1000‑1399px):** Shell split 70/30, right rail width `clamp(320px, 30vw, 420px)` (default)
- **Desktop narrow (800‑999px):** Shell split 65/35, right rail width `clamp(300px, 35vw, 400px)`
- **Tablet (600‑799px):** Single‑column layout, right rail becomes bottom drawer (toggleable)
- **Mobile (<600px):** Not supported (desktop‑only application)

#### Minimum & Maximum Sizes
- **Minimum window width:** 800px (enforced by WM/OS)
- **Minimum left pane width:** 400px (below this, switch to single‑column layout)
- **Minimum right rail width:** 280px (below this, collapse non‑essential elements)
- **Maximum right rail width:** 480px (even on ultra‑wide screens)
- **Album cover minimum size:** 120px (grid shrinks columns before reducing cover size)
- **Album cover maximum size:** 240px (grid adds columns before enlarging covers)

#### Grid Adaptation Rules (Album Mode)

**Cover grid column count:**
- ≥1400px: 6 columns
- 1200‑1399px: 5 columns  
- 1000‑1199px: 4 columns
- 800‑999px: 3 columns
- <800px: switch to single‑column list (fallback)

**Album queue grid adaptation:**
- Right rail width ≥400px: 3 columns (`3x2` grid)
- Right rail width 350‑399px: 2 columns (`2x3` grid)
- Right rail width <350px: 1 column (vertical list of covers)

**Grid item sizing:**
- Cover size scales with column count to fill available width (maintaining aspect ratio 1:1)
- Hover controls scale proportionally (buttons stay touch‑target size ≥24px)
- Group headers fixed height 32px; remain sticky during scroll

#### Folder Tree Adaptation Rules
- **Indentation per level:** 16px (fixed, not responsive)
- **Row height:** 32px (compact), 40px (comfort) – user preference
- **Font size:** Track titles 13‑15px, metadata 11‑13px (scale with system DPI)
- **Metadata truncation:** Priority: DSD/PCM label → bit‑depth/sample‑rate → file extension → full path
- **Horizontal scroll:** Enable only when metadata cannot fit (show scrollbar on hover)

#### Right Rail Internal Proportions Adaptation

**Album Mode proportions adjust based on height:**
- Height ≥600px: 40/20/40 (Now Playing / Current Album / Album Queue)
- Height 450‑599px: 45/15/40
- Height <450px: 50/0/50 (Current Album hidden; tracks accessible via Now Playing expander)

**Folder Mode proportions adjust based on height:**
- Height ≥500px: 55/45 (Now Playing / Queue)
- Height <500px: 60/40
- **Queue height minimum:** 160px (below this, queue becomes scrollable popover)

#### Single‑Column Layout (Tablet Mode)
- **Trigger:** Window width <800px OR left pane width <400px
- **Layout:** Full‑width browsing view; right rail slides up as bottom drawer
- **Drawer toggle:** Button in header; keyboard shortcut `Ctrl+B`
- **Drawer height:** 50% of window height by default; adjustable split handle
- **State preservation:** When returning to split view, restore previous proportions

#### High‑DPI/Retina Support
- **Image assets:** 2× resolution for displays with scaling ≥1.5
- **Iconography:** Vector SVG where possible; pixel‑aligned at 1×, 2×
- **Font scaling:** Respect system text size preferences (GTK text‑scale factor)
- **Touch targets:** Minimum 44×44px for interactive elements when touch detected

#### Animation & Transition Rules
- **Mode switch:** 300ms cross‑fade with simultaneous layout proportion adjustment
- **Pane resize:** Live preview while dragging split handle; commit on release
- **Grid column change:** 200ms morph animation (items reposition smoothly)
- **Drawer slide:** 250ms ease‑in‑out; respect system reduced‑motion preference
- **Hover controls:** 100ms fade‑in; instant fade‑out

#### User Customization & Persistence
- **Saved per‑mode:** Split ratio, rail width, internal proportions, column count preference
- **Session‑only:** Scroll positions, expanded folders, selected items
- **Export/import:** Layout profile export (JSON) for sharing across installations
- **Reset:** Individual reset buttons for each layout dimension; global reset to defaults

#### Accessibility Considerations
- **Keyboard navigation:** Full traversal via Tab/Shift‑Tab; arrow keys within grids/lists
- **Screen readers:** ARIA labels for all interactive elements; cover art `alt` text as “Album: [title] by [artist]”
- **High contrast mode:** Detect system contrast preference; override cover art with solid colors if needed
- **Reduced motion:** Respect `prefers‑reduced‑motion` CSS/media query equivalent

### MPD Compatibility Notes

**Layout Constraints Imposed by MPD:** The 70/30 split and right‑rail width must accommodate MPD's binary album‑art protocol (max 16384×16384 pixels, transferred as binary blob). Grid rendering should handle missing cover art gracefully (MPD may return empty or placeholder). Folder tree expansion depth limited by MPD's `listall` command performance (∼10,000 entries). Responsive breakpoints should consider MPD's metadata fetch latency (50‑200ms per album).

## Playback And Queue Design

### Playback Controls

Playback controls must always be visible.

- previous
- play/pause
- next
- seekbar

Seekbar behavior by mode:

- Album mode: visible but not central
- Folder mode: more important, easy to scrub visually, not precision-first

### Album Mode Queue

In album mode, the queue is conceptually an album queue, not a track queue.

- Show queued albums as a mini cover grid
- Order covers left to right, then top to bottom
- Give the currently playing album a distinct visual state
- Support drag and drop for exact placement
- Hovering the left or right side of a target album should indicate before or after insertion
- Start with a `3x2` mini-grid at 50% cover scale and refine later if needed
- Allow duplicate albums in queue
- Dragging an album off the queue grid removes it from the queue
- Duplicate albums should be rendered plainly, with no badge or stacked visual treatment

### Current Album Track Window

Album mode does not need a full track queue. It needs a limited window into the current album.

- Visible list height is limited by available space
- The first visible rows should start from the played/current area
- Show as many next tracks as fit
- Allow manual scrolling backward
- On track change, reset the window so the first visible row returns to the played/current area
- Clicking a track jumps playback to that track within the current album

### Folder Mode Queue

In folder mode, the queue is primarily a track queue.

- Show it as a plain visible track list
- Keep it simple and readable rather than album-oriented
- Track-level ordering matters more than album grouping here
- Let it occupy most of the lower part of the right rail
- Follow the currently played track only when it would otherwise leave the viewport

### MPD Compatibility Notes

**MPD Protocol Implications for Playback:** MPD's `play`, `pause`, `stop`, `next`, `previous` commands are synchronous; UI must handle command latency (50‑150ms). Queue reorder uses MPD's `move` command with 0‑based indices; drag‑and‑drop must map to correct indices. Current Album track window depends on MPD's `playlistinfo` for track metadata; polling interval affects freshness. Folder mode seekbar precision limited by MPD's `seekcur` command resolution (seconds).

## Album Hover Controls

Album covers in album mode should expose three small hover-only buttons.

- Buttons appear instantly on hover
- Buttons should use normal UI sizing and should not be oversized
- Top-right: `+` adds album to queue
- Bottom-right: `<-` inserts album after the current album
- Bottom-left: `>` clears queue and plays album

Albums should also support context-menu actions for queue operations.

## Technical Information

### Folder Rows

Every visible track row in folder mode should include technical data because incoming albums may vary by track.

Priority when space is limited:

- For DSD: keep format first, such as `DSD` or `DSD128`
- For PCM: keep bit depth first, then sample rate, e.g. `16/44` or `24/96`
- Less important fields collapse first

### Now Playing Technical Info

Show concise, readable format information.

- Prefer short labels when they communicate enough
- Use compact labels such as `DSD`, `DSD128`, `16/44`, `24/96`, `16/88`, `24/192`
- If technical data is unavailable, omit the missing field instead of showing placeholders

## Covers And Artwork

### Cover Lookup (MPD-Driven)

Cover art is fetched exclusively via the MPD protocol. No local filesystem scanning or online lookup is performed by the client. See `_bmad-output/planning-artifacts/architecture.md` for the full pipeline design (CoverProvider + ActualRead with MPD albumart/readpicture).

### UI Behavior

- Cover grid shows placeholder (artist-hash color with album initials) while art loads
- Covers load incrementally — one per idle cycle, never blocking the UI
- When a cover arrives from the background pipeline, the UI redraws immediately
- No cover ever holds up browsing, playback, or search

### Failure States & Degradation

- **No cover found:** Subtle gradient placeholder with album initials, color derived from artist name
- **Lookup in progress:** Existing placeholder remains; no shimmer/animation
- **MPD unavailable:** Covers remain at last cached state


## Search Functionality

### Design Philosophy
- **Hidden by default:** Search is a fallback when browsing is insufficient, not a primary navigation method
- **Context‑aware:** Search scope adapts to current mode (Album vs Folder) and active view
- **Progressive disclosure:** Simple text field initially; advanced filters available via toggle
- **Performance‑first:** Live filtering must not block UI; debounced input with incremental results

### UI Placement & Interaction
- **Location:** Inline field in header/toolbar, right‑aligned (consistent across modes)
- **Visibility:** Collapsed to search icon (`⎘`) by default; expands to field on click/`Ctrl+F`
- **Keyboard shortcuts:** 
  - `Ctrl+F` / `Cmd+F`: Focus search field (expand if collapsed)
  - `Esc`: Clear search and collapse field if empty, otherwise just clear
  - `Enter`: Play first result (if any)
- **Clear button:** `×` appears when text entered; clears search and restores original view
- **Active state:** Subtle border/background change when field has focus

### Search Scope By Mode

#### Album Mode
- **Fields searched:** Album title, artist, year, genre, track titles
- **Grouped views:** Search preserves group headers; shows only groups/items with matches
- **Sorting:** Results sorted by relevance (exact title match > artist match > partial match)
- **Live filter:** Filters main grid in‑place; no separate results panel

#### Folder Mode
- **Fields searched:** Full path text, normalized folder names, track titles, raw filenames, file extensions
- **Folder tree:** Search expands matching folders automatically; collapses non‑matching siblings
- **Technical metadata:** **Not** searched (DSD/PCM labels, bit depth, sample rate) – avoids false positives
- **Live filter:** Highlights matching rows; grays out non‑matching items (still visible for context)

### Relevance Scoring & Ranking

#### Album Mode Scoring (weighted)
1. **Exact album title match:** 100 points
2. **Exact artist match:** 80 points  
3. **Partial album title match:** 60 points × match percentage
4. **Partial artist match:** 40 points × match percentage
5. **Track title match:** 30 points (per matching track, max 90)
6. **Year/genre exact match:** 20 points

#### Folder Mode Scoring
1. **Exact filename match:** 100 points
2. **Exact folder name match:** 80 points
3. **Partial path match:** 50 points × match percentage
4. **File extension match:** 10 points (only when query includes extension)

#### Thresholds
- **Minimum score:** 20 points (items below hidden)
- **Group visibility:** Group shown if any member scores ≥20
- **Sort order:** Descending score, then original order (for ties)

### Technical Implementation

#### Indexing Strategy
- **Album mode:** Pre‑compute search index during library load (album‑level inverted index)
- **Folder mode:** Build path‑based trie during folder tree construction
- **Memory budget:** Search index ≤5MB per 10,000 tracks
- **Update frequency:** Re‑index on library changes (incremental where possible)

#### Query Processing Pipeline
1. **Input normalization:** Lowercase, Unicode normalization, remove diacritics (optional)
2. **Tokenization:** Split on whitespace/punctuation; keep phrase quotes
3. **Scoring:** Parallel score calculation using index lookups
4. **Filtering:** Apply threshold, sort, limit to top 200 results
5. **UI update:** Debounced 150ms; cancel previous query if new input arrives

#### Performance Requirements
- **Response time:** 95% of queries complete <50ms on SSD with 50,000‑track library
- **UI thread:** All scoring/filtering off‑UI‑thread; only final results cause render
- **Memory:** Query processing allocates ≤10MB temporary working set
- **Cancelation:** Rapid typing cancels pending queries; only latest executes

### Advanced Features (Out of Scope)

#### Filter Operators
- **Field prefixes:** `artist:`, `year:`, `genre:`, `bitdepth:`, `samplerate:`
- **Boolean operators:** `AND` (default), `OR`, `NOT`
- **Range queries:** `year:1990‑1999`, `bitdepth:>16`
- **Saved searches:** Named search presets (e.g., “DSD albums”, “1990s jazz”)

#### Search‑Within‑Results
- **Refinement:** After initial search, subsequent searches filter the result set
- **Breadcrumb:** Show search history as clickable trail
- **Combine modes:** Search across both modes with unified results panel

### Error States & Edge Cases

#### Empty Results
- **Message:** “No matches for ‘query’” inline below search field (disappears after 3s)
- **Action:** Offer “Search in all modes” toggle if current mode has no results

#### Indexing In Progress
- **Placeholder:** “Building search index…” (only shown first time or after large library change)
- **Progress:** Percentage indicator for large libraries (>10,000 tracks)

#### Special Characters
- **Escaping:** Backslash escapes special operators (`artist\:` searches literal “artist:”)
- **Unicode:** Full UTF‑8 support; normalization forms NFC/NFD handled transparently

#### Very Large Result Sets
- **Limiting:** Show top 200 matches; “Show all 1,234 results” button expands virtual list
- **Virtual scrolling:** Only render visible rows (performance)

### Integration With Other Features

#### Search → Playback
- **Play first result:** `Enter` plays first match (clears queue, enqueues album/folder)
- **Play all results:** `Ctrl+Enter` enqueues all matches as a playlist
- **Context menu:** Right‑click any result for standard play/queue actions

#### Search → Navigation
- **Click result:** Selects item in main view (scrolls to position)
- **Arrow keys:** Navigate results list while search field focused
- **Tab completion:** Suggest matching artists/albums as you type (configurable)

### Success Metrics
- **Time‑to‑result:** 95% of searches show first results within 100ms of typing stop
- **Accuracy:** Precision (relevant results/total results) ≥80% for typical queries
- **Adoption:** ≥30% of users activate search at least once per session
- **Satisfaction:** User survey rating ≥4/5 for “search helps me find music faster”

### MPD Compatibility Notes

**MPD Search Command Limitations:** MPD's `search`, `find`, `list` commands have different performance characteristics; search must choose appropriate command based on scope. MPD search is case‑insensitive ASCII‑only; Unicode normalization required. Search results limited to MPD's database size (∼500,000 tracks); incremental search may need client‑side filtering. Search indexing must not block MPD command queue; background indexing preferred.

## Interaction Rules

Keep the main interaction model consistent.

### Album Mode

- Single click on album cover selects the album
- Double click on album cover clears the queue, enqueues the album, and starts playback from track 1
- `Add to queue` appends albums to the end of the queue
- Dragging an album from the main grid into the album queue inserts it at an exact position
- Dragging queued albums reorders them by exact before/after placement
- Dragging a queued album off the album queue grid removes it from the queue
- Manual album ordering in the main grid is allowed only in plain `Albums` view
- Manual album ordering in plain `Albums` view is session-only
- Albums can also be added through context menu actions
- Hover controls map to `+` add to queue, `<-` insert after current album, and `>` clear queue and play
- Duplicate albums in queue are allowed
- Album context menu contains `Play now`, `Play next`, and `Add to queue`
- Queued album item menu contains `Remove` and `Play now`

### Folder Mode

- Click folder: expand or collapse
- Double click folder: clear queue, enqueue folder or album, start playback from the beginning
- Click track: select
- Double click track: replace the current queue with the opened folder content and start playback from the selected track
- Drag and drop is supported for folder-mode queue interactions
- `Add to queue` appends folders, folder track batches, or tracks to the end of the queue
- Play next in folder mode inserts after the current track
- `Play next` on a folder inserts that folder's tracks as one ordered batch after the current track
- Folder and track context menus contain `Play now`, `Play next`, and `Add to queue`
- Queued folder-mode item menu contains `Remove` and `Play now`

## Requirements Specification

### Functional Requirements

#### Playback Control (FR‑P1–FR‑P4)
- **FR‑P1:** User can control MPD playback (play, pause, stop, next, previous)
- **FR‑P2:** System displays current playback state (playing/paused, position, duration)
- **FR‑P3:** User can seek within tracks via interactive seekbar
- **FR‑P4:** System handles MPD disconnection gracefully by showing disconnect indicator, preserving queue state, and attempting automatic reconnection with exponential backoff (1s, 2s, 4s, 8s, 16s)

#### Queue Management (FR‑Q1–FR‑Q8)
- **FR‑Q1:** System displays current queue in mode‑appropriate presentation (album grid vs track list)
- **FR‑Q2:** User can add albums/tracks/folders to queue via drag‑and‑drop
- **FR‑Q3:** User can reorder queue items via drag‑and‑drop
- **FR‑Q4:** User can remove items from queue via drag‑off or context menu
- **FR‑Q5:** User can “play now” (jump to item without clearing rest of queue)
- **FR‑Q6:** User can “play next” (insert after current item)
- **FR‑Q7:** System preserves queue across mode switches (underlying linear order unchanged)
- **FR‑Q8:** System synchronizes queue state with MPD, verifying consistency every 30 seconds with tolerance for up to 5 seconds of latency

#### Browsing & Navigation (FR‑B1–FR‑B10)
- **FR‑B1:** System provides Album Mode with cover‑grid browsing
- **FR‑B2:** System provides Folder Mode with expandable folder‑tree browsing
- **FR‑B3:** System supports grouped views in Album Mode (Artists, Years, Genres)
- **FR‑B4:** System pins group headers while scrolling in grouped views
- **FR‑B5:** User can manually reorder albums only in plain Albums view
- **FR‑B6:** System normalizes folder structures (cue files appear as single album entry with track list; DSD folders show as album with DSD track entries)
- **FR‑B7:** System displays technical metadata per‑track in Folder Mode (DSD/PCM labels)
- **FR‑B8:** System provides hover controls on album covers (`+`, `←`, `>`)
- **FR‑B9:** User can single‑click to select, double‑click to play
- **FR‑B10:** System provides search functionality with mode‑appropriate scope (Album Mode: title, artist, year, genre, track names; Folder Mode: full path, folder names, filenames)

#### Layout & UI (FR‑L1–FR‑L6)
- **FR‑L1:** System maintains persistent 70/30 split shell with right rail
- **FR‑L2:** System adapts right‑rail proportions by mode (Album: 40/20/40, Folder: 55/45)
- **FR‑L3:** System makes playback controls always visible
- **FR‑L4:** User can configure layout values (split ratio, rail width, proportions)
- **FR‑L5:** System remembers scroll positions and expansion state per mode
- **FR‑L6:** System provides smooth transitions between modes (<500ms)

#### Cover Art & Metadata (FR‑C1–FR‑C7)
- **FR‑C1:** System fetches cover art via local files, embedded artwork, online services
- **FR‑C2:** System caches cover art in‑memory (session) and on‑disk (persistent)
- **FR‑C3:** System rate‑limits online cover lookups (1 request/second default)
- **FR‑C4:** System retries failed cover lookups with exponential backoff (1s, 2s, 4s, 8s, 16s)
- **FR‑C5:** System displays “no cover” placeholder when no art available
- **FR‑C6:** System shows cover art in Now Playing regardless of mode
- **FR‑C7:** System displays concise format labels in Now Playing (DSD128, 24/96, etc.)

#### Search (FR‑S1–FR‑S8)
- **FR‑S1:** System provides live search with debounced input (150ms)
- **FR‑S2:** System searches album metadata in Album Mode (title, artist, year, genre, tracks)
- **FR‑S3:** System searches path/filename in Folder Mode (full path, folder names, filenames)
- **FR‑S4:** System ranks results by relevance score with configurable thresholds
- **FR‑S5:** System maintains search index with memory budget ≤5MB per 10,000 tracks
- **FR‑S6:** System processes queries without blocking UI with response <50ms (95th percentile)
- **FR‑S7:** User can use keyboard shortcuts (Ctrl+F focus, Esc clear, Enter play first)
- **FR‑S8:** System preserves original view state when search is closed

### Non‑Functional Requirements

#### Performance (NFR‑P1–NFR‑P6)
- **NFR‑P1:** Library load time <2 seconds for 50,000‑track library on SSD as measured by application startup profiling
- **NFR‑P2:** UI responsiveness: all interactions <100ms (95th percentile) as measured by UI responsiveness instrumentation
- **NFR‑P3:** Search response time <50ms for 95% of queries (50,000‑track library) as measured by query latency monitoring
- **NFR‑P4:** Cover art cache hit rate >90% after initial library scan as measured by cache statistics logging
- **NFR‑P5:** Memory footprint <200MB RAM for typical library (10,000‑50,000 tracks) as measured by system memory monitoring
- **NFR‑P6:** Cover subsystem memory <50MB RAM for 10,000‑track library as measured by subsystem memory allocation tracking

#### Reliability & Resilience (NFR‑R1–NFR‑R5)
- **NFR‑R1:** Playback uptime ≥99.5% (MPD‑dependent outages excluded) as measured by client‑side uptime logging
- **NFR‑R2:** MPD connection recovery <5 seconds after transient network loss as measured by reconnection timing instrumentation
- **NFR‑R3:** Graceful degradation: UI remains usable during MPD disconnection (inline empty panel) as verified by manual testing of disconnect scenarios
- **NFR‑R4:** Partial failure handling: continue operation when non‑critical features fail (cover lookup) as verified by fault injection testing
- **NFR‑R5:** Queue synchronization latency <1 second (client queue matches MPD queue) as measured by periodic queue hash comparison

#### Usability & Accessibility (NFR‑U1–NFR‑U4)
- **NFR‑U1:** Album‑mode “browse→select→play” flow completable in ≤3 clicks as measured by user task completion testing
- **NFR‑U2:** Folder‑mode users identify audio format/organization issues 50% faster than with file managers as measured by comparative task timing studies
- **NFR‑U3:** Keyboard navigation support for all primary functions (playback, queue, search) as verified by accessibility testing
- **NFR‑U4:** High‑contrast mode support for visually impaired users (configurable) as verified by WCAG 2.1 AA compliance testing

#### Compatibility & Interoperability (NFR‑C1–NFR‑C3)
- **NFR‑C1:** Support MPD protocol version 0.24.x (current stable) as verified by protocol compatibility testing
- **NFR‑C2:** Support audio formats: all PCM/DSD formats that MPD can decode as verified by format detection testing
- **NFR‑C3:** Support cue sheet parsing with ≥95% accuracy for standard cue sheets as measured by cue sheet parsing test suite

#### Security & Privacy (NFR‑S1–NFR‑S3)
- **NFR‑S1:** No collection or transmission of personal listening data as verified by network traffic analysis and privacy audit
- **NFR‑S2:** Optional online cover lookups (user‑configurable; disabled by default for privacy) as verified by configuration testing and network request monitoring
- **NFR‑S3:** Network usage cap configurable (default 500MB/month) with user warnings as measured by network usage tracking and warning trigger testing

#### Maintainability & Extensibility (NFR‑M1–NFR‑M3)
- **NFR‑M1:** Layered architecture with clear separation (MPD adapter, state, presenters, UI) as verified by architectural review and dependency analysis
- **NFR‑M2:** Configuration‑driven layout values (not hard‑coded) as verified by layout parameter modification testing
- **NFR‑M3:** Ability to replace playback backend later without UI redesign as verified by interface abstraction analysis and mock backend testing

#### Deployability & Operations (NFR‑O1–NFR‑O5)
- **NFR‑O1:** Zero-configuration operation with sensible defaults as verified by installation and first-run testing
- **NFR‑O2:** Auto-start capability when configured by user as verified by desktop environment integration testing
- **NFR‑O3:** Memory footprint monitoring with warning at >500MB RAM usage as measured by memory monitoring alerts
- **NFR‑O4:** Logging configurable by level (error, warn, info, debug) with rotation as verified by log level configuration and rotation testing
- **NFR‑O5:** Performance profiling support for large libraries (>100,000 tracks) as verified by profiling tool integration testing

### Traceability Mapping
Each requirement traces to one or more sections in this document or architecture:
- **Playback control:** `_bmad-output/planning-artifacts/architecture.md` — MPD Adapter Architecture
- **Queue management:** Playback And Queue Design, Interaction Rules
- **Browsing & navigation:** User Journeys, Interaction Rules, `_bmad-output/planning-artifacts/architecture.md` — Browsing Sort / Folder Normalization
- **Layout & UI:** Layout, `_bmad-output/planning-artifacts/architecture.md` — Layout & Responsive / libadwaita
- **Cover art:** Covers And Artwork, `_bmad-output/planning-artifacts/architecture.md` — Cover Art Pipeline
- **Search:** Search Functionality, `_bmad-output/planning-artifacts/architecture.md` — Search Architecture
- **Performance:** Success Criteria (Technical Performance)
- **Reliability:** `_bmad-output/planning-artifacts/architecture.md` — Startup/Shutdown Lifecycle, Notifications
- **Usability:** Success Criteria (Business Outcomes, Quality Metrics)

## Technical Specifications

### Audio Format Support Matrix
The client relies on MPD for actual playback; these specifications define what the client **displays** and **understands**.

| Format Family | Supported Variants | Display Label | Bit Depth Range | Sample Rate Range | Channel Count | Notes |
|---------------|-------------------|---------------|-----------------|-------------------|---------------|-------|
| **PCM (Uncompressed)** | WAV, AIFF, RAW | `16/44`, `24/96`, etc. | 16‑32 bit | 44.1‑384 kHz | 1‑8 | Display as `bit‑depth/sample‑rate`; support all MPD‑decodable PCM |
| **DSD (Direct Stream Digital)** | DSD64, DSD128, DSD256, DSD512 | `DSD`, `DSD128`, etc. | 1‑bit | 2.8‑22.6 MHz | 2‑8 | Show base rate multiplier (64, 128, 256, 512) when known |
| **FLAC (Lossless)** | FLAC Level 0‑8 | Same as PCM | 16‑32 bit | 44.1‑384 kHz | 1‑8 | Treat as PCM for display; show `FLAC` badge optionally |
| **ALAC (Apple Lossless)** | ALAC | Same as PCM | 16‑32 bit | 44.1‑384 kHz | 1‑8 | Treat as PCM for display |
| **MP3 (Lossy)** | MPEG‑1/2/2.5 Layer III | `MP3` | — | 8‑48 kHz | 1‑2 | Show `MP3` only; no bit‑depth/sample‑rate detail |
| **AAC/OGG/Vorbis/Opus** | Various | `AAC`, `OGG`, `Opus` | — | — | 1‑2 | Show codec name only |
| **Cue Sheets** | `.cue` + image file | `CUE` | Inherits from referenced image | Inherits from referenced image | Inherits from referenced image | Parse cue for track splits; fallback to raw file on error |

#### Display Rules
- **Primary label:** For PCM: `bit‑depth/sample‑rate` (e.g., `24/96`). For DSD: `DSD` or `DSD128`.
- **Secondary label:** Codec name if lossy (MP3, AAC) or if user prefers (FLAC, ALAC).
- **Truncation:** Sample rates >192 kHz shown as `24/192+`; DSD512 as `DSD512`.
- **Unknown values:** Omit missing fields rather than showing placeholders.

### Playback Feature Matrix

| Feature | MPD‑Dependent | Client Responsibility | Support Level |
|---------|--------------|----------------------|---------------|
| **Gapless playback** | Yes (MPD) | Client must not interfere | Full (relies on MPD) |
| **Crossfade** | Yes (MPD) | UI toggle for MPD crossfade setting | Configurable via MPD |
| **Replay Gain** | Yes (MPD) | Display Replay Gain status if available | Read‑only display |
| **Audio output selection** | Yes (MPD) | Output selector if MPD supports it | Conditional |
| **Streaming URLs** | Yes (MPD) | URL entry field (out of scope) | Future |
| **Equalizer** | Yes (MPD) | EQ UI if MPD supports DSP | Future |

### Performance Thresholds

#### Library Operations
- **Scan/load time:** <2s for 50,000 tracks (SSD, cold cache)
- **Incremental update:** <500ms for 100 new tracks
- **Search index build:** <5s for 50,000 tracks (background, low priority)
- **Cover art pre‑fetch:** <30s for 1,000 missing covers (background, rate‑limited)

#### UI Responsiveness
- **Frame rate:** 60 FPS stable during scrolling (grid, list)
- **Interaction latency:** <100ms for 95% of user actions (click, hover, drag)
- **Mode switch:** <500ms (including layout proportion adjustment)
- **Drag‑and‑drop feedback:** <16ms (immediate visual response)

#### Memory & Storage
- **RAM footprint:** <200MB for 50,000‑track library
- **Cover cache memory:** <50MB for 10,000 covers (512×512 JPEG)
- **Search index:** ≤5MB per 10,000 tracks
- **Disk cache:** 1GB default (configurable 100MB‑5GB)
- **Session state:** <10MB serialized (scroll positions, expansions, preferences)

#### Network & I/O
- **MPD command latency:** <100ms round‑trip (local socket)
- **Cover fetch rate limit:** 1 request/second default (configurable)
- **Online metadata budget:** 500MB/month default (configurable)
- **Reconnection timeout:** Exponential backoff (1s, 2s, 4s… max 60s)

### Compatibility Matrix

#### MPD Versions
- **Minimum:** MPD 0.24.x (0.24.9 latest stable)
- **Recommended:** MPD 0.24.x (full DSD support, improved metadata)
- **Tested:** MPD 0.24.x (0.24.9 at time of release)

#### Operating Systems
- **Primary:** Linux (GTK4 native)
- **Secondary:** None (Linux‑only target)
- **Package formats:** Flatpak (primary), .deb/.rpm (Linux distribution packages)

#### File Systems & Libraries
- **Music directory:** Any POSIX filesystem MPD can access
- **Library size:** Up to 500,000 tracks (tested); no hard limit
- **Cover art sources:** Local files (`cover.jpg`), embedded artwork (ID3v2, FLAC), online (MusicBrainz, Discogs)
- **Cue sheet parsing:** Standard cue format (ISO‑8859‑1/UTF‑8, FILE/WAVE commands)

### Error Tolerance Levels

| Component | Tolerance Level | Recovery Action | User Impact |
|-----------|----------------|-----------------|-------------|
| **MPD connection** | High | Automatic retry with exponential backoff | Temporary “disconnected” inline panel |
| **Cover art fetch** | Medium | Skip after 3 retries; use placeholder | Visual only; playback unaffected |
| **Cue sheet parsing** | Medium | Fall back to treating cue as single playable file | Loss of track splits; still playable |
| **Metadata extraction** | Low | Omit missing fields; continue with available data | Incomplete display; playback unaffected |
| **Search index** | Low | Rebuild on next library scan; disable search temporarily | Search unavailable until rebuild |
| **UI layout** | High | Fall back to default proportions (70/30, 40/20/40, 55/45) | Layout reset; functionality intact |

### MPD Protocol Alignment

**Cross‑Reference with MPD Protocol Documentation:**
- **Audio Format Support:** Verify matrix against MPD's `decoders` command output
- **Playback Features:** MPD's `commands` list determines available transport commands (play, pause, stop, next, previous, seek, consume, random, repeat, single)
- **Metadata Retrieval:** MPD's `tagtypes` defines available metadata fields; client must handle missing tags gracefully
- **Binary Data Handling:** MPD's `readpicture` and `albumart` command limits (size, format) affect cover art implementation
- **Connection Management:** MPD's `idle` command for state sync vs. polling trade‑offs; recommended polling interval 1‑2 seconds
- **Queue Operations:** MPD's `playlistinfo`, `playlistid`, `move`, `delete`, `add` commands map to queue model
- **Search Commands:** MPD's `search`, `find`, `list` performance characteristics differ; client must choose based on scope
- **Error Responses:** MPD's `ACK` error codes (50‑58) must be mapped to user‑friendly messages

**Verification Method:** During implementation, cross‑check each technical specification against MPD protocol documentation (https://mpd.readthedocs.io).

### Technical Architecture Reference

See `_bmad-output/planning-artifacts/architecture.md` for the full technical architecture including:
- Threading model and MPD idle protocol
- Cover art pipeline (CoverProvider + ActualRead)
- State management and presenters
- Queue processing and plchanges strategy
- Command batching, Unix socket detection
- UI widget architecture and libadwaita integration
- MPRIS D-Bus integration
- Layout service and responsive breakpoints

### Proven Implementation Patterns

Real-MPD validation confirmed the following patterns (see `architecture.md` "Proven Patterns from Validation Session"):
- Dead connection detection (3 consecutive failures → reconnect)
- Cover art binary protocol (albumart multi-chunk reassembly, BufReader fix)
- Widget registry for in-place cover cell updates
- 64-event batch limit in UI event processing

## Detailed Interaction Flows

### App Entry

1. Open in album mode
2. Show `Albums` as the default view
3. Show the main cover grid on the left
4. Show the persistent right rail on the right
5. Keep playback controls visible at all times

### Album Listening Flow

#### Browse

1. User lands in album grid
2. User visually scans covers
3. User may switch grouped view to `Artists`, `Years`, or `Genres`
4. Group headers remain pinned while scrolling

#### Select And Play Album

1. User single-clicks an album cover to select it
2. User double-clicks an album cover to clear the queue
3. The selected album is enqueued
4. Playback starts from track 1
5. Now playing updates in the right rail
6. Album queue updates as a mini cover grid
7. Hover controls appear instantly while hovering the album cover

#### Queue Additional Albums

1. User drags an album from the main grid into the album queue, uses the `+` hover button, or uses a context-menu action
2. Insertion target is shown before or after a queued album
3. Album is inserted at the exact target position
4. Queue grid updates left to right, then top to bottom

#### Reorder Album Queue

1. User drags a queued album cover
2. UI shows before or after insertion marker relative to the hovered album
3. Album is dropped into the target position
4. Queue order updates without changing the shell

#### Inspect Current Album Tracks

1. Right rail shows the current album track window
2. The first visible rows start at the played/current context
3. As many next tracks are shown as fit into the available height
4. User may scroll backward to earlier tracks
5. On track change, the window resets to the played/current anchor

#### Jump Within Current Album

1. User clicks a track in the current album track window
2. Playback jumps to that track
3. Playback continues through the album from there
4. Current track state updates in the track window

### Folder Discovery Flow

#### Enter Folder Mode

1. User switches from album mode to folder mode
2. Left panel changes from cover grid to folder-oriented list
3. Right rail stays persistent
4. Now playing remains visible

#### Browse Folders

1. User scans text-first rows
2. Track rows display technical information directly
3. DSD rows prioritize compact format labels like `DSD` or `DSD128`
4. PCM rows prioritize bit depth, then sample rate

#### Expand Folder

1. User clicks a folder row
2. Folder expands inline to show tracks
3. User can collapse it again with another click

#### Play Folder Or Album

1. User double-clicks a folder row
2. Queue is cleared
3. Folder or album content is enqueued
4. Playback starts from the beginning
5. Right rail updates now playing and queue state

#### Play Track

1. User clicks a track row to select it
2. User double-clicks the track row to replace the queue with the opened folder content
3. Playback starts from the selected track
4. Queue remains visible as a plain track list
5. Queue scroll follows only if the current track would leave the viewport

#### Use Seek In Folder Mode

1. User interacts with the always-visible seekbar in the right rail
2. Seekbar should feel visually easy to scrub
3. Precision-first behavior is not required

### Queue Presentation Flow

#### Album Mode

1. Queue is presented as albums
2. Queued albums appear as small covers
3. The currently playing album has a distinct state
4. User can reorder by drag and drop
5. Initial queue layout is `3x2` at 50% cover scale
6. Manual album reordering lasts only for the current session

#### Folder Mode

1. Queue is presented as tracks
2. Queued tracks appear as a plain visible text list
3. Queue scroll follows the current track only when it would leave the viewport
4. Reordering is allowed in queue context
5. `Play now` on a queued item jumps to it without rebuilding the queue

### Search Flow

1. Search is hidden by default
2. User explicitly opens search as an inline field in the header when browsing is insufficient
3. Search scope is limited to the current mode and active view
4. In album mode, search matches all available metadata
5. In folder mode, search matches full path text, normalized folder names, track titles, raw filenames, and file-related text
6. Search results are shown as a live in-place filter of the current view
7. In grouped album views, search keeps groups and shows only groups and items with matches
8. Closing search restores the previous scroll position and folder expansion state, then returns focus to the main browsing surface
