---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7, 8]
workflowStatus: 'complete'
completedAt: '2026-04-28'
inputDocuments:
  - "design.md"
  - "DESIGN-validation-report.md"
  - "CLAUDE.md"
  - "ux-design-specification.md"
  - "ux-design-specification-enhanced.md"
  - "design.md"
  - "CLAUDE.md"
workflowType: 'architecture'
project_name: 'mpd-client'
user_name: 'User'
date: '2026-04-23'
---

# Architecture Decision Document

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

**V2 refinements (2026-04-28):** Cover art pipeline, MPD idle protocol, command batching, Unix socket detection, multi-profile connections, libadwaita integration, and MPRIS configuration are now documented inline in their respective ADR sections below.

## Project Context Analysis

### Requirements Overview

**Functional Requirements (40+ FRs across 7 categories):**

| Category | Count | Key Architectural Implications |
|----------|-------|-------------------------------|
| Playback | 4 FRs | Command dispatch via MPD channel; GTK main loop for now-playing presentation |
| Queue | 8 FRs | Undo stack tied to next mutation (not timer); format-mismatch detection during sync |
| Browsing | 10+ FRs | Dual presenters; Folder Mode needs tree state persistence, multi-disc aggregation, format-aware browsing; album grid needs sort/group primitives |
| Layout | 6 FRs | 70/30 split; mode-adaptive; 150ms resize debounce |
| Cover Art | 7 FRs | 3-tier fetch + scroll-aware progressive loading (300ms stop timer); multi-resolution caching; OOM-safe LRU eviction |
| Search | 8+ FRs | Omnibox (no dropdowns/rails); local in-memory index; uncapped artist/album/year/genre results; 100-cap track results |
| Drag & Drop | 7 FRs | Cross-mode insertion; transactional commit on drop; unified MIME type |

**Non-Functional Requirements (22 NFRs across 7 categories) — key drivers:**
- **Performance:** <2s library load, <100ms interaction, <50ms search, >90% cover cache hit, <200MB RAM total
- **Reliability:** >99.5% uptime, <5s reconnect, <1s queue sync, graceful degradation per MPD protocol version
- **Accessibility:** ≤3-click play, WCAG 2.1 AA, keyboard navigation, high-contrast mode
- **Compatibility:** MPD ≥0.19 (minimum), full PCM/DSD support, >95% cue accuracy

### Scale & Complexity

- **Domain:** Desktop Linux application (GTK4/Rust)
- **Complexity:** Intermediate
- **Estimated components:** 30-35 (revised from ~20)
- **Estimated LOC:** 25,000-40,000 Rust

### Technical Constraints & Dependencies

- MPD TCP text protocol dependency; protocol version variance (0.19–0.24+)
- GTK4 single-threaded main loop; Linux-only (X11/Wayland, AT-SPI, D-Bus, Unix sockets)
- Fully offline-capable; no cloud infrastructure
- Rust ownership model shapes concurrency decisions

### Cross-Cutting Concerns

1. **Thread safety across 4 thread domains:** MPD I/O thread (mpsc channels), cover fetch thread pool (mpsc + oneshot), search thread (oneshot request/response), GTK UI thread (state owner). Minimal shared state behind `Arc<RwLock<>>` — only cover cache.
2. **Dual-mode consistency with single queue:** Undo history cleared on mode switch; queue state is shared across modes.
3. **Spec-aware album identity:** `(file_size, mtime)` primary key (rsync-style); different technical specs = different masterings; file change = straight to "no match"; metadata fallback only when file metadata unavailable.
4. **Memory budget (200MB cap):** Cover art session cache (~150MB), search index (~5MB), widget tree (~30MB), application state (~15MB). Per-subsystem budgets prevent one subsystem from starving others.
5. **Search omnibox:** Single text field (no dropdowns/rails); local in-memory index for <50ms full-text response; Ctrl+F focuses/selects-all; Ctrl+Z for queue undo; immediate visual feedback on keystroke.
6. **Cover art rendering pipeline:** Highest implementation risk — priority queue + 300ms scroll-stop timer + multi-resolution cache (grid thumbnail/queue thumbnail/full-res). Prototype first to validate memory and performance.
7. **Startup phase gates:** 5s per-phase timeout; non-essential phases skip on timeout; MPD connect is essential.
8. **Privacy by design:** No telemetry, no crash reporting, opt-in online cover lookups, configurable rate limiting.

## Architecture Decision Record: Shared Queue with Dual Presentation

**Decision:** Structure the single playback queue as a linear `QueueStore` with stateless computed projections for each mode-specific presenter.

### Options Considered

1. **Linear QueueStore with computed projections** — One ordered list of `QueueItem` structs in application state. Album/Track presenters project views from it as pure functions.
2. **Separate mode-specific queue stores** — Album mode and folder mode each maintain their own queue, synchronized via events.
3. **Single store with embedded presentation logic** — Queue store holds both presentation forms and exposes mode-specific accessors.

### Decision

**Option 1: Linear QueueStore with computed projections**

### Rationale

- Single source of truth prevents sync bugs between modes
- Grid-to-linear mapping isolated in testable `GridCoordinateMapper` utility
- Presenters are pure stateless functions — no hidden state
- Adding future presentation modes requires zero state changes

### Explicit Trade-offs Accepted

- Grid↔linear coordinate mapping adds implementation complexity
- Projection recomputation on every store change (mitigated by change detection)
- Column count changes (responsive) trigger full re-projection

### Key Architectural Interfaces

- `QueueStore: Vec<QueueItem>` — linear, ordered, single source of truth
- `AlbumQueuePresenter::project(&QueueStore) -> AlbumGridViewModel` — grouping + column mapping
- `TrackQueuePresenter::project(&QueueStore) -> TrackListViewModel` — flat render
- `GridCoordinateMapper` — converts `(row, col, zone)` to `linear_index` given column count and album boundaries
- Current Album Track Window: derived view filtered to current album, scroll anchor resets on track change — not a separate queue

## Architecture Decision Record: Application State Architecture

**Decision:** Structure application state with type-enforced separation between shared state and mode-local state using an `ActiveMode` enum.

### Options Considered

1. **Type-enforced enum separation** — `AppState` contains `SharedState` always accessible, and `ActiveMode` enum (`Album(AlbumModeState)` | `Folder(FolderModeState)`) enforcing compiler-checked access.
2. **Monolithic AppState** — Single flat struct with all state fields; mode-specific fields accessed directly with runtime guards.
3. **Hub-and-spoke with dynamic dispatch** — Central state hub holds shared state; mode stores are trait objects plugged/unplugged on mode switch.

### Decision

**Option 1: Type-enforced enum separation**

### Rationale

- Compiler enforces mode isolation — folder browsing state is inaccessible from album scope
- Shared state always accessible without pattern matching
- Clean mode switch: old mode store dropped, new one created in its place
- Two-phase mode switch (layout computation → store swap → re-render) prevents stale state
- Rust enums with pattern matching make illegal states unrepresentable

### Key Architectural Interfaces

```rust
struct SharedState {
    playback: PlaybackState,
    queue: QueueStore,
    connection: ConnectionState,
}

struct AlbumModeState {
    browsing: AlbumBrowsingState,   // scroll pos, selected album, group expansion
    layout: AlbumLayoutPrefs,
    drag_drop: DragDropState,
}

struct FolderModeState {
    browsing: FolderBrowsingState,  // expanded paths, selected track, scroll pos
    layout: FolderLayoutPrefs,
    drag_drop: DragDropState,
}

enum ActiveMode {
    Album(AlbumModeState),
    Folder(FolderModeState),
}

struct AppState {
    shared: SharedState,
    mode: ActiveMode,
}
```

### Explicit Trade-offs Accepted

- Mode-local state lost on app restart (by design — session-only)
- Mode switch must be atomic — partial transitions not representable
- Adding new modes requires extending the enum (+ new state structs)
- Drag operations cancelled on mode switch (ephemeral state dropped)

## Architecture Decision Record: MPD Adapter Architecture

**Decision:** Dedicated background thread with MPD state machine, command channel, and idle-based event push.

### Options Considered

1. **Dedicated thread with state machine** — Background thread runs the MPD protocol loop; command/event channels bridge to GTK main loop. MPD `idle` provides push-based state updates.
2. **Async Rust (tokio/async-std)** — Async TCP stream with MPD protocol handlers; futures drive reconnection, command dispatch, and idle loop.
3. **Inline polling on GTK main loop** — Periodic timer on the GTK main loop sends status commands and processes responses synchronously.

### Decision

**Option 1: Dedicated thread with state machine**

### Rationale

- MPD's synchronous TCP protocol requires non-blocking dispatch — background thread keeps IO off the GTK main loop
- `idle` command provides push-based state updates rather than constant polling
- State machine enum maps naturally to Rust's type system: illegal states (e.g., sending commands while disconnected) are unrepresentable
- Separate channel for binary data (cover art) prevents large blobs from blocking command dispatch
- Pending command queue during disconnection preserves user intent until reconnection
- Fixed 1s retry interval (not exponential). Same backoff for initial connect and reconnect.

### Key Architectural Interfaces

```rust
enum MpdState {
    Disconnected { retry_count: u32 },  // fixed 1s retry, count tracks total disconnect duration
    Connecting { start_time: Instant },
    Connected { stream: TcpStream, protocol_version: String },
    Error { error: MpdError, will_retry_at: Instant },
}

enum MpdCommand {
    Play, Pause, Stop, Next, Previous,
    Seek(f32),
    Add(String),
    Move(i32, i32),
    Delete(i32),
    Clear,
    PlaylistInfo,
    Status,
    CurrentSong,
    ListAll(String),
    Search(SearchQuery),
    ReadPicture(String),
}

enum MpdEvent {
    StateChanged(PlaybackState),
    QueueChanged(Vec<QueueItem>),
    Disconnected,
    Reconnected,
    Error(MpdError),
}
```

- Commands sent via `mpsc::Sender<MpdCommand>`, events received via `mpsc::Receiver<MpdEvent>` (on GTK main loop)
- Binary data for cover art uses a separate channel to avoid head-of-line blocking
- Idle timeout: 35s default (client-side, since MPD's default idle timeout is infinite)
- Pending command queue: drained in order on reconnection

### Explicit Trade-offs Accepted

- Thread + channel overhead (~a few KB per channel)
- Commands queued during disconnection execute in batch on reconnect (ordering preserved)
- Pending command queue has configurable max depth (default 1000) — commands beyond limit are rejected with toast
- Idle timeout must exceed MPD's response time for cover art fetches
- Binary data (cover art) requires dedicated channel
- Thread lifecycle must be managed on app shutdown

### v2 Refinement: Idle Protocol (2026-04-28)

**Decision:** True MPD `idle`/`noidle` protocol with `TcpStream::try_clone()` for thread-safe socket access. CoverGrid pattern: worker thread blocks on `idle` when queue empty; main thread writes `noidle\n` to socket clone to break idle.

**Thread safety:** `TcpStream::try_clone()` creates two file descriptors sharing the same TCP connection. Worker owns the read clone (blocks on `recv` for idle responses). Main-thread-side writer uses the write clone to send `noidle\n` and commands. Safe because MPD protocol is half-duplex — read and write never contend.

**Fallback behavior:**
- If `idle` returns transient error (connection reset, timeout): fall back to 100ms `status` polling for 10 cycles (1 second), then retry `idle`.
- If `idle` returns "unknown command" (MPD < 0.19 or idle disabled): fall back to 500ms polling permanently.

**Status: NOT IMPLEMENTED — in-scope for V1 (epic 25).** Currently uses 500ms polling in `connected_loop` (line 548). Idle protocol would replace this with event-driven updates via `TcpStream::try_clone()` for thread-safe socket access.

**Proven pattern:** dead connection detection via 3 consecutive `fetch_full_update()` failures → return from `connected_loop` → outer state machine triggers reconnect with exponential backoff.

## Architecture Decision Record: Cover Art Pipeline

**Decision (v2, 2026-04-28):** Two-layer split: **CoverProvider** (fast synchronous cache read) and **ActualRead** (background fetch queue). Replaces the earlier layered-provider-chain design which was never implemented.

### Options Considered

1. **CoverProvider + ActualRead two-layer split** — CoverProvider reads disk cache synchronously, returns immediately or not at all. ActualRead runs in idle cycles, fetches via MPD protocol, writes cache, emits events. No cascading fallback chain.
2. **Layered provider chain with deduplication** — Original v1 design: `LocalLookupProvider` → `EmbeddedArtProvider` → `OnlineLookupProvider` with session + disk caches. Replaced by Option 1 — the two-layer split decouples cache reads from fetching, allowing any provider (including online) to be added to ActualRead without affecting the fast cache path.
3. **Event-driven cover resolution** — Cover requests emitted as events; multiple handlers process independently. Flexible but risks duplicate work.

### Decision

**Option 1: CoverProvider + ActualRead two-layer split**

### Rationale

- **CoverProvider is always fast** — pure cache read, never blocks, never falls through. Returns `(PathBuf, MD5, Option<Timestamp>)` or `None`. No cascading, no fallback chain, no waiting.
- **ActualRead is never intrusive** — runs one album per idle cycle, never blocks the command loop. Two providers:
  - **AlbumArtProvider** (primary): fetches via MPD `albumart <uri>`, MD5-hashes binary data, compares to cache hash. Emits `CoverRefreshed(id, Vec<u8>)` only on actual difference.
  - **ReadPictureProvider** (fallback, always enabled): fetches via MPD `readpicture <uri>`, compares timestamp. Emits only when newer.
- **Content-addressed for albumart** (MD5, no timestamp available from MPD) vs **time-addressed for readpicture** (MPD provides mtime).
- **CoverRefreshed carries raw bytes** — UI thread decodes immediately, background thread writes cache independently. No path-based handoff.
- **No loops** — CoverProvider and ActualRead never call each other.
- **Revalidation on reconnect/library change**: enqueue visible albums into ActualRead. Emit only if hash/timestamp differs from cache. No mass emission.

### Key Architectural Interfaces

```rust
// Fast synchronous cache lookup — never blocks
trait CoverProvider {
    fn get(&self, album_id: &str) -> Option<(PathBuf, Md5Hash, Option<Timestamp>)>;
}

// Background fetch — runs in idle cycles
trait CoverFetcher {
    fn fetch(&mut self, album: &Album, adapter: &mut MpdAdapter) -> Option<CoverResult>;
}

struct CoverResult {
    data: Vec<u8>,
    hash: Md5Hash,
    timestamp: Option<Timestamp>,
}
```

### Cover Art Event Flow

```
UI thread: CoverProvider::get(id) → cache hit? return path immediately
                                      → cache miss? return None, enqueue in ActualRead

Background idle cycle: ActualRead processes one album
  → AlbumArtProvider: albumart <uri> → MD5 hash → compare to cache hash
    → different? write cache, emit CoverRefreshed(id, hash, Vec<u8>)
    → same? skip (no emission)
  → if albumart returned error:
    ReadPictureProvider: readpicture <uri> → compare timestamp
    → newer? write cache, emit CoverRefreshed(id, hash, Vec<u8>)

UI thread: receives CoverRefreshed → decode bytes → GdkTexture → redraw widget
```

### Binary Protocol Notes (from validation session)

**Proven patterns:**
- `albumart <uri> 0` returns `size: N\n` then N bytes. Offset increments (0, 8192, 16384...) for multi-chunk reassembly.
- `BufReader` must be reset per response — stale buffer positions corrupt subsequent reads. Fixed by creating fresh `BufReader` per `albumart` response.
- Widget registry (`HashMap<String, Picture>`) enables in-place cell updates via `set_filename()` + `queue_draw()`, avoiding full grid rebuild.

### Explicit Trade-offs Accepted

- MD5 is content-addressing, not security — sufficient for dedup against MPD's own responses
- Online cover lookup is optional, opt-in via `online-cover-art` feature flag — disabled by default for privacy
- Online lookups rate-limited to 1 request/second (configurable) with exponential backoff on failure
- Cover art loads incrementally (one per idle cycle) — visible items may take several cycles on first load
- Revalidation on reconnect emits only on actual change — but the first revalidation pass requires fetching all visible album covers

### Cover Art Image Pipeline (v2 refinement)

**Decision:** Single-resolution disk cache with on-demand decode to GdkTexture. No multi-resolution thumbnail pre-generation.

- **Cache format:** JPEG on disk, keyed by MD5 hash of albumart binary. Metadata sidecar stores hash and optional timestamp.
- **Decode:** Background thread decodes via `gdk-pixbuf` or `image` crate. Result sent as `Vec<u8>` in CoverRefreshed event.
- **Scale:** GTK4's `GtkPicture` handles aspect-ratio-aware display natively — no manual downscale needed.
- **No session cache:** The widget registry holds `GtkPicture` references for visible items only. Non-visible items are dropped by GTK's widget hierarchy.

### Explicit Trade-offs Accepted (Image Pipeline)

- On-demand decode per cover load (~5ms for JPEG decode) — acceptable since covers load one per idle cycle (100ms)
- No GPU-side texture cache — covers are re-decoded when scrolled back into view
- Cache size on disk bounded only by available space

### Proven Patterns from Validation Session

The following patterns were validated against a real MPD instance and are incorporated into the architecture:

- **Local grouping cache:** Flat album list cached on the background thread during the initial `ListAlbumsGrouped("Albums")` response. Switching between Albums/Artist/Years/Genres views regroups locally from the cached list — zero MPD round-trips. Search index rebuild and cover re-fetch also skipped when album count is unchanged.
- **Widget registry for covers:** `HashMap<String, Picture>` maps album names to GTK Picture widgets. Cover events update cells in-place via `set_filename()` + `queue_draw()`, avoiding full grid rebuild.
- **BufReader lifecycle:** Fresh `BufReader` per `albumart` binary response. The underlying `TcpStream` buffer position desynchronizes after a binary read; a new `BufReader` prevents stale buffer corruption.
- **Dead connection detection:** 3 consecutive `fetch_full_update()` failures trigger return from `connected_loop`. The outer state machine then handles reconnection with exponential backoff. Previously, errors were silently logged and the loop continued indefinitely.

## Architecture Decision Record: Layout & Responsive Architecture

**Decision:** Stateless `LayoutService` computing column counts from available width, cached by window-width bucket, emitting change events to subscribers.

### Key Details

- 5 responsive breakpoints: 20 columns, 4, 2, 1 for album grid — determined by available width and configurable min column width
- `GridCoordinateMapper` converts `(row, col, zone)` to linear queue index: pure function, column-count aware, album-boundary aware
- User overrides (min column width, max columns) stored in config TOML, merged with computed defaults
- Rail proportions (70/30) stored as ratios not px — survive DPI/window-size changes
- Column count changes trigger full re-projection of the album grid presenter

### Explicit Trade-offs Accepted

- Responsive re-flow during window resize triggers full grid re-projection
- Mitigated by 150ms debounce on resize events and projection caching per breakpoint
- User override of min column width can produce layouts that look sparse on wide screens

## Architecture Decision Record: Search Architecture

**Decision:** Single omnibox search bar (Chrome-style) with off-thread full-text matching across all metadata fields, 150ms input debounce, immediate visual feedback on keystroke. Artist/album/year/genre results uncapped; track results hard-capped at 100. No dropdowns, rails, or separate search UI elements.

### Key Details

- **Single omnibox input** replaces all modal/destructive search UI (dropdowns, separate search rails, category pickers). One text field, always visible, accepts any query. Placeholder text: `"Search music, artists, years…"` for discoverability.
- **Full-text matching across all fields (both modes):** artist (album artist, composer, multiple tag sources, case-insensitive), album, year (release year AND record year — important for remaster identification), genre, format (e.g., searching `DSD` filters by format), track name.
- **Folder Mode extended fields:** additionally searches file name and normalized directory path (from the flattened/normalized layer). Folder Mode also searches standard tags (album, artist) — it's an extended search, not a different system.
- **Result ordering:** artist → album → year → genre results first (no cap — same as full library display; the grid/tree handles it). Track results shown at the bottom, only when few matches (< 20), with a hard cap of 100 track results maximum. Rationale: short queries like "I" or "you" match too many tracks to be meaningful; specific queries like "cucumber" produce few useful results. Track section is shown proportionally.
- **Immediate visual feedback:** on keystroke, the omnibox shows a subtle progress indicator — the debounce is for result computation only, not for UI responsiveness.
- **Mode-scoped results:** Album Mode shows matching albums in the grid (filtered view); Folder Mode shows matching tracks and folders highlighted.
- **Keyboard shortcut:** `Ctrl+F` focuses the omnibox. Tab from omnibox advances into results.
- **Local search index** required — MPD's `search` command doesn't support cross-field full-text queries. A lightweight in-memory index over cached metadata (artist, album, year, genre, format, track) enables <50ms response. Built on startup from MPD metadata, incrementally updated on library changes.
- **Empty search** restores the full unfiltered view.
- Fuzzy matching with configurable threshold (default 0.6), results sorted by relevance score.
- Minimum query length: 2 characters. Single-character queries return no results (prevents noise from accidental key presses).

### Explicit Trade-offs Accepted

- Single omnibox means no field-specific search UI — users type free-form and trust full-text matching
- Local search index adds ~5MB/10K-track memory overhead (included in the 200MB budget)
- Track results capped at 100 — broad track searches may miss edge matches (acceptable: track search is secondary to album/artist/year)
- Track section shown/hidden based on result count — discontinuity in results UI
- Two mode scopes with different field sets means search behavior differs subtly between modes

## Architecture Decision Record: Command List Batching

**Decision:** Add `MpdCommand::Batch(Vec<MpdCommand>)` variant for atomic bulk operations. Use exclusively for all-or-nothing operations.

### Key Details

- **Protocol:** MPD `command_list_begin`/`command_list_end` wraps multiple commands atomically.
- **Batch operations:** `PlayAlbum`, `AddAlbum` — strictly transactional. If any command fails, MPD aborts the entire list.
- **No batch:** `DeleteId`, `MoveId`, `InsertNext` — partial failure is acceptable.
- **Before:** adding a 20-track album = 21 channel messages + 21 MPD round-trips.
- **After:** adding a 20-track album = 1 Batch message + 1 MPD command_list with 21 commands.

### Explicit Trade-offs Accepted

- MPD's abort-on-first-failure behavior means batching is only safe for genuinely transactional operations
- Not suitable for remove/delete where partial failure should be tolerated

## Architecture Decision Record: Queue Update Strategy (plchanges)

**Decision:** Incremental queue updates via MPD's `plchanges <version>` command instead of full `playlistinfo` re-fetch on every change.

### Key Details

- **Protocol:** `status` returns `playlist: <version>` (32-bit counter). `plchanges <version>` returns only songs added or changed since that version.
- **Deletions:** `plchanges` does not report removals. Detect by cross-referencing current local positions against reported playlist length. Any position past the new length was deleted.
- **Full sync:** Every 50 incremental updates, do a full `playlistinfo` to reconcile. Also on version wrap-around detection (new_version < old_version with delta > 1M).
- **UI benefit:** Surgical `items_changed()` updates on the model preserve scroll position, selection, and animation state. Full `playlistinfo` re-fetch replaces the entire model, losing view state.

### Explicit Trade-offs Accepted

- Deletion reconciliation is inherently heuristic — full sync every 50 updates ensures correctness
- Version wrap is extremely unlikely (2^32 mutations) but handled via the threshold check
- Stores `playlist_version` in state — must persist across reconnections

## Architecture Decision Record: Unix Socket Auto-Detection

**Decision:** Auto-detect MPD Unix socket at connection time with fallback chain.

### Priority Order

1. `$XDG_RUNTIME_DIR/mpd/socket`
2. `/run/mpd/socket`
3. TCP `localhost:6600`
4. Connection settings dialog (if all of the above fail)

Skipped entirely if user has manually configured a host in config. Each failed `connect()` returns `ECONNREFUSED` or `ENOENT` — harmless, just try the next path.

### Explicit Trade-offs Accepted

- ~0.3ms added to startup for the three connect attempts
- Stale Unix sockets (from dead MPD process) return ECONNREFUSED — detected correctly, falls through to TCP

## Architecture Decision Record: Multi-Profile Connections

**Decision:** Support multiple named connection profiles in config.

### Config Format

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

### Key Details

- `--profile <name>` CLI flag for headless switching
- Auto-detect on first connect saves into "default" profile
- Profile selector in connection dialog
- No profile editing UI in v1 — profiles are hand-edited in TOML

## Architecture Decision Record: libadwaita Integration

**Decision:** Add `libadwaita` (adw crate) as a dependency. Replace 3+ custom widget implementations.

### Widget Replacements

| Custom Widget | Replaced By |
|--------------|-------------|
| Toast overlay | `Adw.ToastOverlay` |
| Navigation stack | `Adw.NavigationView` |
| Responsive sidebar ↔ bottom-sheet | `Adw.MultiLayoutView` + `Adw.BottomSheet` |
| Mode switcher (album/folder) | `Adw.ViewSwitcher` |

### Key Details

- Requires `libadwaita >= 1.6` at runtime (included in GNOME Platform runtime)
- Flatpak builds use `org.gnome.Platform` which includes libadwaita
- Non-GNOME users: available in all major distros as standalone library
- `adw = "0.8"` crate dependency in `Cargo.toml`

### Explicit Trade-offs Accepted

- Adds ~3MB to binary size (libadwaita shared library)
- API churn risk — adw crate evolves rapidly (0.5 → 0.8 in ~18 months)
- Requires Adwaita runtime on non-GNOME desktops

**Status: PARTIALLY IMPLEMENTED — in-scope for V1.** ToastOverlay and ViewSwitcher are done (story 22-1). NavigationView evaluated as not applicable (flat view structure). MultiLayoutView + BottomSheet for responsive right-rail is a separate story (epic 27).



**Decision:** `FolderNormalizer` trait with strategy pattern — each strategy normalizes a known directory pattern into a consistent artist/album/track hierarchy, falling back to raw filesystem presentation.

### Key Details

- Strategy chain (first match wins): CueSheetStrategy → DsdFolderStrategy → SingleAlbumStrategy → CompilationStrategy → RawFsStrategy
- CueSheetStrategy: collapses `.cue` + split tracks into one logical album entry. Validates that referenced track files exist before normalizing; missing files prevent normalization and are logged.
- DsdFolderStrategy: collapses `DSF/` subfolder + `.m3u` into one logical album
- Normalization is purely a presentation-layer concern — the underlying queue still uses MPD URIs
- User can "explode" a normalized entry back to raw files (context menu: "Show raw files")

### Explicit Trade-offs Accepted

- Normalization hides filesystem reality from power users browsing Folder Mode
- "Show raw files" context menu entry provides escape hatch
- Strategy chain adds complexity for edge cases (mixed cue + raw files in same directory)
- New audio format conventions require new strategies

## Architecture Decision Record: Drag & Drop Cross-Cutting Architecture

**Decision:** Unified `DndService` handling source detection, drop target validation, and visual feedback, with mode-specific renderers for ghost images.

### Key Details

- Drag sources: album grid cards (Album Mode), folder tree tracks (Folder Mode), queue items (both modes)
- Drop targets: queue (both modes), playlist (future)
- Unified MIME type `application/x-mpd-queue-item` carrying queue index + source mode tag
- Drag ghost rendered by active mode's DndRenderer: album card ghost vs track row ghost
- Drop position indicator: horizontal line between queue items, snapped to album boundaries in Album Mode
- Cross-mode drag allowed — item always lands as a track entry; Album Mode mini-grid updates on next projection

### Explicit Trade-offs Accepted

- Cross-mode drag (track from Folder Mode into Album Mode queue) creates presentation mismatch — single track has no natural position in an album-grouped grid
- Resolved by inserting at linear queue level and letting album projector group under "Various Artists" or standalone
- Cross-mode drops cannot be previewed visually in the source mode's rendering style

## Architecture Decision Record: Track Identity & Queue Synchronization

**Decision:** Multi-factor track identity using `(file_size, mtime, path)` triple (rsync-style), with fallback to metadata match when file metadata is unavailable. Different technical specs (sample rate, bit depth) distinguish different masterings.

### Key Details

- **Primary key:** `(file_size, mtime)` — behaves like rsync. On reconnect/resync, check file size then modification time. If either differs, the file has changed.
- **Secondary key:** `file_path` — fallback when size/mtime unavailable (network mounts, protocol limitations).
- **Metadata fallback:** When file metadata is unavailable, fall back to `(artist, album, year, sample_rate, bit_depth)` match.
- **Album identity:** Two albums are "the same" only when effective artist + album title + year + technical specs all match. Different specs = definitely different albums (different masterings).
- **Track-level spec variance:** When tracks within an album have differing specs, use a deterministic heuristic for album-level identity (e.g., track 1's spec). Choice is arbitrary but consistent.
- **Power user file changes:** If file size or mtime changed, treat as "no match" — the user modified files intentionally, queue entry is removed. No attempt to reconcile.
- **Edge cases** (minimal or conflicting metadata): treat as "no match" — don't over-engineer.
- Conflict resolution chain: 1) exact track match (file_size + mtime) → match found, keep in queue. 2) file metadata available but doesn't match → no match, remove from queue. 3) file metadata unavailable → fall back to (artist+album+year+specs) match. 4) no match → remove from queue with toast notification.
- Stale entries (deleted files) removed from queue with toast notification.
- Race condition guard: user modifications during library scan are timestamped and take priority over sync merge.

### Explicit Trade-offs Accepted

- Hash computation is not used — size+mtime is faster and sufficient for the rsync-style model
- Network mounts may not support efficient stat() — degraded to path-only matching
- Spec-based album identity means the same album released at 16/44.1 and 24/192 appears as two separate entries — correct by design
- Power user file changes always result in queue removal (no smart reconciliation) — intentional trade-off
- Metadata fallback is inherently less reliable than file metadata; used only when size/mtime unavailable

## Architecture Decision Record: Error Handling Architecture

**Decision:** Typed error propagation using `thiserror` enums with a three-tier error model (Recoverable, Retryable, Fatal), surfaced to the UI through a unified `ErrorSink`.

### Key Details

- **Recoverable:** Transient errors shown as toast notifications (3s timeout) — e.g., "MPD connection lost, retrying..."
- **Retryable:** Errors with automatic retry and backoff — e.g., cover art download failure, MPD command timeout
- **Fatal:** Errors requiring user action — e.g., config corruption, MPD protocol version mismatch. Modal dialog with specific guidance.
- `ErrorSink` aggregator collects errors from all layers (MPD thread, cover art compute worker, search) and routes them to the appropriate presentation
- **Refined per patterns elicitation:** ErrorSink is now a typed channel (`mpsc::Sender<ErrorSinkEvent>`) with a `glib::idle_add` consumer on the main loop, not a formal aggregator struct. See Elicitation-Driven Refinements → ErrorSink. The three-tier model (Recoverable/Retryable/Fatal) and `user_facing_message()` interface remain unchanged.
- Each error type implements `user_facing_message()` returning a localized, actionable string
- Internal errors (unwrap-able invariants) use `expect` with descriptive messages — no catch-all `unwrap()`

### Explicit Trade-offs Accepted

- Toast errors may be missed if user is focused elsewhere — mitigated by persistent error badge on connection status indicator
- Fatal errors terminate the current operation rather than degrading gracefully
- `ErrorSink` is a shared resource requiring `Arc<Mutex<>>` across threads

## Architecture Decision Record: Configuration Management

**Decision:** Single TOML file at `~/.config/mpd-client/config.toml` loaded at startup, validated against a schema, merged with hardcoded defaults, and written back on preference changes.

### Key Details

- **Schema version field** in config file enables migration on read
- **Corruption recovery:** parse failure on read triggers backup creation (`config.toml.bad`) and regeneration from defaults with user notification
- **Auto-save:** preference changes written immediately (debounced 500ms), not on app close — survives crash
- **What goes in config:** connection details, column width prefs, window geometry, mode preference, cover art cache limits, rate-limit settings
- **What stays in session-only state:** scroll positions, expanded folder paths, drag state, search query
- **CLI flags** override config values for the session only (not persisted)

### Explicit Trade-offs Accepted

- TOML parsing adds a dependency (`toml` crate)
- Config file is plaintext — no encryption for MPD password (relied on filesystem permissions)
- Schema migration requires maintaining migration code for old versions

## Architecture Decision Record: Testing Architecture

**Decision:** Hybrid strategy — unit tests for state/presenter logic, integration tests against a mock MPD server, and manual GUI testing for visual/cross-cutting concerns.

### Key Details

- **Unit tests** cover: `QueueStore` mutations, `GridCoordinateMapper`, `FolderNormalizer` strategies, `SearchService` relevance scoring, cover art provider chain
- **Mock MPD server** (`mock-mpd`): a minimal TCP server implementing a subset of the MPD protocol, controllable via channels to simulate disconnections, slow responses, and error conditions
- **State machine tests** cover all MpdState transitions including disconnection during `idle`, reconnection backoff, and pending command queue drain
- **Presenter tests** assert that given a `QueueStore`, the projected ViewModel matches expected structure
- **No GUI unit testing** — GTK widget testing is notoriously brittle; visual regression would require a separate infrastructure
- Tests live in `tests/` for integration, inline `#[cfg(test)]` for unit tests

### Explicit Trade-offs Accepted

- Mock MPD server may drift from real MPD behavior over protocol versions
- No automated GUI testing means drag-and-drop, responsive layout, and visual polish require manual QA
- Cover art pipeline tests with real files increase test suite runtime

## Architecture Decision Record: Logging & Observability

**Decision:** Structured logging with `log` + `env_logger` crate, three verbosity levels (info, debug, trace), rotating file output at debug+ levels, stderr at info level.

### Key Details

- **Info:** User-visible events — connection state changes, queue mutations, mode switches, library sync
- **Debug:** Internal flow — command dispatch, cache hits/misses, state machine transitions, timing data
- **Trace:** Protocol-level — raw MPD command/response hex, GTK event stream, drag gestures
- Log file at `~/.local/share/mpd-client/log/` with rotation (3 files, 5MB each)
- No sentry/crash-reporting — privacy-first design; user opts in to share logs manually
- Performance-critical paths (cover art render, grid re-projection) use timing macros that output at debug level

### Explicit Trade-offs Accepted

- Trace-level logging on the MPD protocol adds overhead; disabled by default, enabled via `RUST_LOG=mpd_client=trace`
- Log files on disk are a privacy concern for metadata-sensitive users (filenames, paths in logs)
- No centralized crash reporting means bugs in the wild rely on user reproduction

## Architecture Decision Record: Theming & UI Architecture

**Decision:** GTK4 CSS theming with a single stylesheet (`style.css`), dark theme default, no runtime theme switching in v1.

### Key Details

- Stylesheet covers: album grid card layout, hover button positioning, folder tree indentation, queue item spacing, rail proportions
- GTK4 CSS variables for colors, spacing, font sizes — enables easy theme tweaking without recompile
- Dark theme as default with sufficient contrast for long listening sessions
- Light theme CSS exists but is compile-time gated (`feature = "light-theme"`)
- No user-facing theme switcher in v1 — simplifies testing and eliminates a class of regressions
- Custom widget styling via CSS name bindings, not inline style properties

### Explicit Trade-offs Accepted

- No runtime theme switching means users who want light theme need to recompile
- GTK4 CSS limitations (no variable scoping, limited selector nesting) lead to some repetition in the stylesheet
- Dark theme default may be jarring for users on light-themed desktops (mitigated by respecting GTK4 theme preference: prefer-dark detection)

## Architecture Decision Record: Startup/Shutdown Lifecycle

**Decision:** Phase-gated initialization with explicit dependency graph and reverse-order graceful shutdown with timeout.

### Key Details

- **Phases (strict order):** Config load → Logging init → MPD connect → State restore → UI build → Cover art service start
- Each phase declares dependencies (e.g., MPD connect requires config); phase gate blocks until dependencies are ready
- **Phase timeout:** each phase has a 5s timeout. Non-essential phases (cover art service) log and continue on timeout; essential phases (config, MPD) surface a fatal error
- Phase failure is fatal only for essential phases (config, MPD); non-essential phases (cover art service) log and continue
- **Shutdown order:** `ShuttingDown` flag set → **persist state** (read AppState.mode, write to config) → UI teardown → MPD disconnect → compute worker stop → logging flush
- **Refined per elicitation:** state persist moved to immediately after shutdown signal (before any teardown). Prevents AppState changes during UI teardown (focus-loss callbacks, widget destruction) from being lost. See Elicitation-Driven Refinements (Rubber Duck Debugging) → Shutdown Phase Ordering.
- Shutdown timeout per phase (3s default); timed-out phases are abandoned, not blocked
- SIGTERM/SIGINT handlers initiate graceful shutdown; second signal forces immediate exit
- MPD command queue drained before disconnect during shutdown — pending commands are lost, not partially executed

### Explicit Trade-offs Accepted

- Phase-gated startup adds latency vs. eager initialization (~200ms total)
- Hard shutdown (second SIGINT) may leave MPD in inconsistent state (partial playlist)
- State persistence on shutdown relies on filesystem being writable at that moment

## Architecture Decision Record: IPC & CLI Architecture

**Decision:** Single-instance application with D-Bus MPRIS integration for media control, CLI flags for session-level overrides, and second-instance detection via Unix socket.

### Key Details

- **MPRIS D-Bus integration:** Implement `org.mpris.MediaPlayer2` Player interface via `zbus` crate (`zbus = { version = "5", default-features = false, features = ["blocking"], optional = true }`). Player interface only: Play, Pause, PlayPause, Stop, Next, Previous, Seek, SetPosition, OpenUri. Standard properties (PlaybackStatus, Metadata, Position, Volume, CanGoNext, etc.). D-Bus bus name: `org.mpris.MediaPlayer2.mpdclient`.
- **Blocking API, no tokio:** `default-features = false` excludes zbus's tokio-based async runtime. The `blocking` feature uses `zbus::blocking::Connection` which spawns a single internal IO thread for D-Bus message processing — compatible with the std::thread-only model (see ADR: Async Runtime Decision §790). Verified: `cargo tree -p zbus --no-default-features -f blocking` shows zero tokio dependencies.
- **Disabled by default:** `[mpris] enabled = false` in config. Feature-gated: `[features] mpris = ["zbus"]`.
- **Implementation:** New file `src/mpris.rs` (feature-gated). Struct `MprisPlayer { cmd_tx: mpsc::Sender<MpdCommand> }` with `#[zbus(interface)]` derive macro. All 15 methods map one-to-one to `cmd_tx.send(MpdCommand::*)` — no new code paths. Registered at startup via `connection.object_server().at("/org/mpris/MediaPlayer2", player)`.
- **Shared D-Bus connection:** The single `zbus::blocking::Connection` created for MPRIS is also used for libnotify D-Bus calls (story 14-4), avoiding a separate D-Bus dependency for notifications. Notify calls use `connection.call_method()` directly — no `notify-rust` crate needed.
- **Second-instance detection:** Lock file at `~/.cache/mpd-client/lock` on startup; second instance sends CLI-parsed action (e.g., `--toggle-playback`) to running instance via Unix socket, then exits
- **CLI flags:** override config values for session only; `--mpd-host`, `--mpd-port`, `--profile`, `--mode`, `--start-playing`, `--toggle-playback`, `--next`, `--prev`
- Remote actions received via Unix socket are dispatched as internal commands, not re-parsed
- No separate IPC thread — Unix socket listener runs on the GTK main loop via `gio` socket API

### Explicit Trade-offs Accepted

- MPRIS D-Bus adds a runtime dependency on a D-Bus session bus
- Unix socket lock file must be cleaned up on crash (mitigated by using `gio`-managed socket binding that auto-cleans)
- CLI flags parsed by the first instance from the second instance lose positional context (no way to say "play this album in the already-running window")

## Architecture Decision Record: Accessibility Architecture

**Decision:** GTK4's built-in accessibility tree as primary mechanism, supplemented by explicit keyboard navigation model and WCAG 2.1 AA compliance targets.

### Key Details

- **Keyboard navigation model:** Tab/Shift-Tab for focus traversal between major zones (grid/folder tree, queue, search, controls); arrow keys within grids/lists; Enter/Space for activation; Escape to dismiss/deselect
- **Grid keyboard model:** arrow keys move focus in 2D (respecting column count); Enter on card = select, double-Enter = play (configurable via accessibility prefs)
- **Folder tree keyboard model:** arrow up/down for track selection; left/right for expand/collapse; Enter for play
- **Screen readers:** GTK4's `AtkObject` accessible descriptions for all custom widgets; cover art labeled as "Album: [title] by [artist]"
- **Color contrast:** WCAG 2.1 AA minimum (4.5:1 for text, 3:1 for UI elements); high-contrast CSS variant
- **Focus indicators:** Visible focus ring on all interactive elements; never rely solely on color to convey state
- No custom accessibility abstractions — use GTK4's built-in support rather than wrapping a11y in internal traits

### Explicit Trade-offs Accepted

- GTK4's accessibility tree is Linux-only (AT-SPI) — no cross-platform a11y benefit
- Custom widgets (album grid, drag ghosts) require manual AtkObject implementation
- Keyboard navigation in the album grid (2D arrow movement across album boundaries) is non-trivial to implement correctly
- Accessibility testing requires AT-SPI tooling not part of standard CI pipeline

## Architecture Decision Record: Build & Packaging Architecture

**Decision:** Standard Rust `cargo` build with Flatpak as primary distribution format, `.deb`/`.rpm` as secondary, CI-driven release pipeline.

### Key Details

- **Build system:** `cargo` with `Cargo.toml` feature flags for optional components (`light-theme`, `online-cover-art`, `mpris`)
- **Flatpak:** Primary distribution target; manifest at `build-aux/flatpak/mpd-client.yml`; uses GNOME runtime for GTK4; MPD access via `--socket=mpd` or filesystem access to MPD socket
- **Linux packages:** `.deb` (Debian/Ubuntu) and `.rpm` (Fedora/RHEL) generated from CI; `install` target in `Makefile` for `pacman`/manual installs
- **Desktop integration:** `.desktop` file with proper categories (`Audio;AudioVideo;Player`), MIME types for audio files, icon at standard paths
- **Version scheme:** Semantic versioning (vMAJOR.MINOR.PATCH) with git tag-based release identification
- **Minimum Rust version:** MSRV: 1.85 (edition 2024 requirement), checked in CI
- **Dependency audit:** `cargo-deny` in CI for license compliance and known vulnerability scanning

### Explicit Trade-offs Accepted

- Flatpak bundle size includes GNOME runtime dependency (~400MB download, ~1.2GB installed)
- No Windows/macOS builds — MSRV policy and GTK4 dependency make cross-platform impractical
- `.deb`/`.rpm` packages are distribution-specific and may lag behind Flatpak releases
- Feature flags increase CI matrix complexity (4 build configurations × 2 architectures minimum)

**Status: OUT OF SCOPE for v1.** No Flatpak manifest, no CI pipeline, no `.deb`/`.rpm` packaging. The project is built via `cargo build` and installed manually. Desktop file with proper `Categories` is tracked as a separate story (NFR-O2). Packaging and CI may be addressed post-v1 when a release process is established.

## Architecture Decision Record: Notification & System Integration

**Decision:** Layered notification system — in-app toast notifications for transient events, optional MPRIS for desktop environment integration, no system tray in v1.

### Key Details

- **Toast notifications:** In-app overlay, bottom-right corner, 3s default display, stacked (max 3 visible). Types: queue sync, cover art errors, library changes, connection status
- **Toast types are non-modal** — no user action required; click-to-dismiss optional
- **MPRIS integration** provides: lock screen playback info, media key support, desktop environment "now playing" display
- **No system tray icon in v1** — desktop environment tray support is inconsistent across Linux DEs (GNOME removed it); MPRIS provides equivalent functionality for background control
- **Notification area:** Optional libnotify integration for persistent notifications (e.g., "MPD disconnected — retrying") — disabled by default, opt-in via config. Uses the same `zbus::blocking::Connection` as MPRIS (see ADR: IPC & CLI Architecture §687), calling `org.freedesktop.Notifications` interface directly — no additional D-Bus dependency
- **Desktop file** registers for common audio MIME types — file manager "Open with" works for audio files

### Explicit Trade-offs Accepted

- No system tray means window must remain open (or minimized) to access the client
- MPRIS requires D-Bus — no control from `playerctl` without a session bus
- Toast notifications are purely visual — no screen reader announcement for toasts in v1
- libnotify integration (opt-in) may duplicate in-app toasts when both are enabled

## Architecture Decision Record: Concurrency & Threading Model

**Decision:** Two dedicated threads (MPD IO, compute worker) communicating with the GTK main loop via typed channels, with minimal shared mutable state behind `Arc<RwLock<>>`.

### Key Details

- **Thread topology:**
  - **GTK main loop thread** — UI rendering, event handling, presenter projections (always runs on the main thread per GTK4 requirements)
  - **MPD IO thread** — TCP stream read/write, idle loop, reconnection state machine. Communicates via `mpsc::Receiver<MpdCommand>` (inbound) and `mpsc::Sender<MpdEvent>` (outbound to main thread)
  - **Compute worker** — Cover art fetch/decode/image processing and search query execution on a shared priority queue. Cover art has priority (visible items first); search yields every 5 cover jobs. Communicates via priority job queue (inbound) and typed result channels (outbound). Replaces dedicated search thread and cover art thread pool from earlier design — see Elicitation-Driven Refinements → Thread Model.
- **Synchronization primitives:**
  - `Arc<Mutex<QueueStore>>` — guarded by the main thread's event loop; all mutations happen on the main thread in response to MPD events or user actions
  - `Arc<RwLock<SessionCache>>` — read-heavy cover art cache; multiple threads read, only cover art thread pool writes
  - `Arc<AtomicBool>` — connection status flag, cancellation tokens for background operations
  - All other state is single-threaded on the main thread (config, browsing state, drag state)
- **Channel types:** `mpsc` for multi-producer streams (MPD events, toast notifications), `oneshot` for request-response (search queries, cover art lookups)
- **No shared mutable state across threads** beyond the explicitly listed caches — prefer message passing over locks
- **Deadlock prevention:** lock ordering established (SessionCache before QueueStore); no nested lock acquisitions; all locks held for <1ms

### Explicit Trade-offs Accepted

- Three-thread architecture is heavier than a single-threaded async approach (~1MB stack per thread)
- Thread pool for cover art complicates cancellation (in-flight HTTP request can't be aborted cheaply)
- Arc<Mutex<>> on QueueStore is a contention point during rapid MPD updates (mitigated by event batching)
- Channel backpressure must be explicitly managed — an overwhelmed main thread can't "drop" GTK events
- **Future migration path:** If state-to-widget binding complexity grows (multiple widgets observing the same state), migrate `SharedState` to a `glib::Object` subclass with `ParamSpec` properties. This enables GTK4's native `bind_property()` cross-thread binding, eliminating manual `idle_add` wiring. Deferred to post-v1 — the channel approach is simpler and sufficient for the current widget count.

## Architecture Decision Record: Async Runtime Decision

**Decision:** `std::thread` for background workers, GTK main loop for UI. No tokio/async-std.

### Why Not tokio

| Factor | std::thread | tokio |
|--------|-------------|-------|
| Binary size (minimal) | 4.2 MB | 15 MB (+260%) |
| Extra dependencies | 0 | 14 crates (mio, bytes, socket2, etc.) |
| Complexity | `thread::spawn` + channels | `#[tokio::main]`, async/await, compatible IO |
| MPD protocol fit | Natural — single connection, blocking reads | Overkill — tokio excels at many concurrent connections |

### Rationale

- GTK4 main loop is its own event loop. Adding tokio means running a second event loop alongside GTK's — more complexity, more failure modes.
- MPD protocol is inherently synchronous: send command, read response, wait for "OK". No benefit from async I/O for a single TCP connection with request–response semantics.
- Background work (MPD I/O, cover art processing) is a perfect fit for dedicated `std::thread` workers communicating via channels.
- **zbus exception (confirmed compatible):** `zbus = { default-features = false, features = ["blocking"] }` uses zbus's blocking API which spawns a single internal IO thread — no tokio dependency. This is a bounded exception: the IO thread is internal to the connection, the public API is blocking, and the thread count grows by exactly 1 (not a thread pool). See ADR: IPC & CLI Architecture §687 for details.
- Confirmed by spike build comparison (`notes/ASYNC_RUNTIME_DECISION.md`, now archived in this section).

## Architecture Decision Record: Session Persistence & State Restoration

**Decision:** V1 scope — persist last active mode only. Three-tier persistence (full session restore, window geometry, etc.) deferred to post-v1.

### V1 Approach

- Only the last active mode (Album/Folder) is persisted — a single key in the config file (`~/.config/mpd-client/config.toml`).
- Written immediately on mode switch (no debounce).
- Read on startup — no corruption risk (single key, atomic write).
- Everything else (scroll positions, expanded folders, search query, window geometry) is ephemeral — lost on close.

### Full Design (Deferred Post-V1)

The following three-tier design is documented for future implementation but is **out of scope for v1**:

- **Ephemeral (lost on close):** Scroll positions, expanded folder state, drag position, search query text, undo history
- **Session-restorable (persist on shutdown, restore on startup):** Last active mode, window geometry (size, position, maximized state), last-selected album/track ID, MPD host/port (overrides config)
- **Persistent cache (write-through, LRU):** Cover art disk cache, metadata cache, search index (if built)
- **Session file:** `~/.local/share/mpd-client/session.toml` — written on graceful shutdown, read on startup

- **Ephemeral (lost on close):** Scroll positions, expanded folder state, drag position, search query text, undo history
- **Session-restorable (persist on shutdown, restore on startup):** Last active mode, window geometry (size, position, maximized state), last-selected album/track ID, MPD host/port (overrides config)
- **Persistent cache (write-through, LRU):** Cover art disk cache, metadata cache, search index (if built)
- **Session file:** `~/.local/share/mpd-client/session.toml` — written on graceful shutdown, read on startup
- **Write strategy:** session file written on graceful shutdown only (not on every state change) — avoids IO churn and flash wear; crash means session state is lost, not corrupted
- **Corruption handling:** parse failure on session file read → skip restore, log warning, start fresh (session restore is best-effort, not critical)
- No auto-save timer for session state — deliberate: the only acceptable restored state is a consistent snapshot from shutdown; auto-save during active browsing could restore an inconsistent mid-action state

### Explicit Trade-offs Accepted

- Crashes lose session state entirely (scroll positions, expanded folders) — user must re-navigate
- Window geometry restore may fail on multi-monitor setups (monitor disconnected between sessions)
- Session file is plain TOML — contains file paths (privacy consideration)
- No incremental session save means long sessions with a crash at the end lose all browsing context

## Architecture Decision Record: No Plugin Architecture (Explicit Non-Decision)

**Decision:** No plugin/extension system in v1. All functionality is compiled into the binary. This is an explicit non-decision documented to prevent scope creep.

### Rationale

- Plugins would require a stable ABI or a scripting runtime — both add significant complexity with no clear benefit for the v1 use case
- MPD already provides the extensibility boundary: anything MPD can do, the client can expose. Plugins between the client and MPD add a layer without clear value
- The two-mode architecture (Album + Folder) covers the identified user workflows; an extension system would invite scope creep by making it easy to add "one more mode"
- Rust's trait system provides compile-time extension points (presenter traits, provider traits, normalizer strategies) without runtime overhead — if a new mode is needed later, it can be compiled in

### Future Considerations (Post-v1)

- If plugin demand emerges, the trait boundaries already exist: `QueuePresenter`, `BrowsingPresenter`, `CoverArtProvider`, `FolderNormalizer` are all trait-defined and could be loaded from dynamic libraries
- A plugin system would need: stable ABI for traits, dynamic loading crate (`libloading`), sandboxing considerations, and a plugin discovery protocol
- Documented here so the architecture isn't inadvertently made "plugin-hostile" — the trait boundaries should remain clean even though plugins aren't loaded dynamically in v1

### Explicit Trade-offs Accepted

- No third-party extensions without recompiling the application
- Trait boundaries must be designed well enough for compile-time extension, even if runtime loading isn't implemented
- Users who want custom behavior must fork the project or wait for features to be merged upstream
- The Rust compilation barrier (slow compile, unfamiliar language) discourages casual modification

## Architecture Decision Record: Custom Widget Architecture

**Decision:** Composite custom widgets built from standard GTK4 containers with CSS name bindings, avoiding `DrawingArea` custom rendering except for the album cover display.

### Key Details

- **Album Grid Widget:** `GtkFlowBox` subclass with custom layout manager respecting column count from `LayoutService`. Each child is a `GtkBox` (cover image + overlay buttons + metadata text). CSS name `album-grid-card` for per-card styling.
- **Folder Tree Widget:** `GtkTreeView` with custom `TreeModel` wrapping the normalized folder structure. Icon + text renderers for file types. CSS name `folder-tree`.
- **Queue Widgets:** `GtkListBox` in both modes with mode-specific row widgets — album queue uses mini cover + track list per row; folder queue uses filename + format row.
- **Cover Display:** `GtkPicture` for cover art — handles JPEG/PNG decoding natively. Fallback to a colored `GtkDrawingArea` with artist-hash color when no cover available.
- **Hover Button Overlays:** `GtkOverlay` with `GtkButton` children positioned via CSS (`position: absolute` equivalent). Visibility toggled by CSS `:hover` on parent — no JavaScript/action overhead.
- **Split Shell:** `GtkPaned` with 70/30 initial position, `resize()` method respecting `LayoutService` constraints.
- **No custom DrawingArea rendering** except the cover art fallback — standard GTK4 widgets are preferred for accessibility (free AtkObject tree), theming (free CSS support), and focus management.

### Explicit Trade-offs Accepted

- `GtkFlowBox` for album grid limits animation possibilities (no per-item animated insertion/removal without workarounds)
- `GtkTreeView` for folder tree is legacy GTK widget — `GtkColumnView` would be more modern but less battle-tested
- CSS hover overlays don't work on touchscreens (out of scope for v1)
- Custom layout manager for FlowBox is non-trivial to implement with GTK4's layout model

## Architecture Decision Record: Keybinding Architecture

**Decision:** Centralized `KeybindingService` mapping `(key, mods, context)` to actions, with mode-scoped contexts that automatically enable/disable on mode switch.

### Key Details

- **Keybinding table:** defined in a single `keybindings.rs` module as a `HashMap<(GdkKey, GdkModifierType, KeybindingContext), Action>` — one location to audit all shortcuts
- **Contexts:** `Global` (always active), `AlbumGrid` (Album Mode browsing), `FolderTree` (Folder Mode browsing), `Search` (search field focused), `Queue` (queue focused)
- Mode switch swaps active contexts: `AlbumGrid` vs `FolderTree` contexts are mutually exclusive
- **Action enum:** typed enum of all invocable actions — `PlayPause`, `NextTrack`, `ToggleMode`, `FocusSearch`, `QueueSelected`, etc. — same enum used by IPC, hover buttons, and menu items
- **Conflict detection:** compile-time assertion that no two mappings share the same `(key, mods, context)` — tested in `#[cfg(test)]`
- **Defaults:** `Ctrl+F` → FocusSearch (Global, selects all text in omnibox if already focused), `Ctrl+,` → OpenSettings (Global), `Ctrl+Z` → Undo (Global), `Space` → PlayPause (Global), `Escape` → ClearSelection/Dismiss (Global), `Enter` → ActivateSelection (context-dependent), arrow keys → navigation (context-dependent), `Tab`/`Shift+Tab` → focus traversal (Global, GTK-managed)
- No user-configurable keybindings in v1 — the mapping table is compile-time; customization deferred to post-v1

### Explicit Trade-offs Accepted

- Hardcoded keybindings mean no user customization without recompiling
- Context-based dispatch requires careful focus management — GTK's focus chain and custom contexts must stay in sync
- Global shortcuts (like Space for play/pause) must not conflict with GTK's built-in widget shortcuts (e.g., Space toggles buttons)
- `GdkKey` values are hardware-dependent — keyboard layout differences handled by GTK's key event normalization

**Status: PARTIALLY IMPLEMENTED — in-scope for V1.** All shortcuts work (Ctrl+F search, Ctrl+1/2 modes, Space play/pause, Ctrl+, settings, arrow keys, Enter, Delete, Shift+Up/Down queue reorder) but are wired as ad-hoc GTK accelerators and `EventControllerKey` handlers. Centralized `KeybindingService` with compile-time conflict detection still needs to be built for V1.

## Architecture Decision Record: Undo/Redo for Queue Operations

**Decision:** Stack-based undo for queue mutations only (add, remove, reorder, clear), limited to 50 entries, cleared on mode switch.

**V1 scope:** Not implemented. Deferred post-v1 — no undo in the first release.

### Key Details

- **Undoable operations:** `QueueAdd(items)`, `QueueRemove(indices)`, `QueueReorder(from, to)`, `QueueClear(previous_state)`
- **Each operation stores the inverse:** `QueueRemove` stores the removed items + their positions; `QueueAdd` stores the insertion indices; `QueueClear` stores a snapshot of the entire queue
- **Stack limit:** 50 entries → oldest entries dropped from bottom when exceeded
- **No redo stack** — simplified implementation; accidental undo can be undone by redoing the action
- **Scope:** only queue mutations are undoable. Non-queue actions (playback control, mode switch, search, config changes) have no undo
- **Mode switch** clears the undo stack — queue state is mode-independent, but undo history is considered ephemeral browsing context
- **Visual feedback:** toast "Undo: removed 3 tracks" on undo execution; button to reverse the undo (effectively redo)
- **Store:** `Vec<UndoEntry>` on `SharedState`, accessible from both modes
- **Availability:** Undo available until the next queue mutation occurs — a single undo slot per operation; once a new mutation happens, the previous undo is discarded. Prevents restoring stale state without invisible timers.
- **MPD sync:** undo operations are translated to MPD commands and sent through the normal command channel (same as any queue mutation)

### Explicit Trade-offs Accepted

- 50-entry limit may be too small for power users doing batch operations (mitigated by toast feedback and easy re-queue)
- No redo stack means "undo the undo" requires manual re-application
- Mode switch destroying undo history may surprise users who switch modes briefly and return
- QueueClear undo requires storing a full queue snapshot (~1MB for 10k tracks) — limited to avoid memory pressure

## Architecture Decision Record: Animation & Transition Architecture

**Decision:** CSS transitions for UI state changes, GTK4's built-in revealer/stack animations for structural transitions, no custom animation engine in v1.

### Key Details

- **CSS transitions:** hover button opacity (0→1, 150ms ease), drag ghost opacity, toast slide-in/out (200ms ease-out), queue item insertion highlight (300ms fade)
- **GTK4 revealer animations:** mode switch (crossfade between Album/Folder browsing pane, 200ms), search bar expand/collapse
- **GTK4 stack transitions:** right rail content changes (slide, 150ms) — now-playing → queue view
- **No custom animation engine** — GTK4's CSS and widget-level animations cover the identified needs. Custom interpolation would add complexity for marginal visual gain
- **Animation gating:** `prefers-reduced-motion` respected — all animations disabled when system accessibility setting is detected. Query via GTK4's `Gtk.Settings:gtk-enable-animations` which respects the desktop-wide setting
- **Performance:** all animations are GPU-composited via GTK4's renderer; no layout thrash (animations are opacity/transform only, not geometry changes)

### Explicit Trade-offs Accepted

- CSS transitions limited to GTK4's supported CSS properties (no custom easing curves beyond built-in)
- No per-item animation in album grid (items appear/disappear instantly during reflow) — `GtkFlowBox` doesn't support animated insertion/removal
- Mode switch animation is a 200ms crossfade — no zoom/scale transitions common in media apps
- `prefers-reduced-motion` is a desktop-wide setting — can't be overridden per-app without GTK4 API calls

## Architecture Decision Record: Cover Art Placeholder Strategy

**Decision:** Color-derived-from-artist-hash placeholders with layered visual design, no generic music-note or CD icon placeholders.

### Key Details

- **Hash-to-color:** SHA-256 of normalized artist name → first 3 bytes → HSL color with fixed saturation (35%) and lightness (55%) for consistent, genre-distinct colors
- **Visual design:** Solid color background with subtle diagonal gradient overlay (`linear-gradient` via CSS). Centered text of extracted dominant initial (first letter of artist name). Opacity 0.6 for text.
- **No generic icons** (music note, CD, vinyl) — they add visual noise without information value
- **Loading state:** pulsing skeleton (light gray with shimmer, 1s CSS animation) — shown when cover art is being fetched (session cache miss, provider chain in flight)
- **Error state:** same hash-color background but with `!` indicator (faint, small) — shown after all providers return negative (24h negative cache)
- **Dimensions:** placeholder renders at exact cover art dimensions (album grid card square, queue mini-grid square) — no layout shift when real art loads
- **Accessibility:** placeholder labeled as "Album: [title] by [artist]" via AtkObject — same as real cover art

### Explicit Trade-offs Accepted

- Hash-derived color is deterministic but may produce visually adjacent colors for unrelated artists (hash collision in hue space)
- No album-specific visual identity in placeholder — artist-level color only (album hash would cause color changes on re-scan)
- Skeleton loading state may be perceived as slower than showing a placeholder immediately
- Diagonal gradient overlay is a CSS-only approximation of a more sophisticated design

## Architecture Decision Record: Internationalization

**Decision:** No i18n framework in v1 — English-only UI with all user-facing strings centralized in a single module for future extraction.

### Key Details

- **String centralization:** All user-facing text in a `strings.rs` module as constants or simple functions — no inline string literals in widgets, no string formatting scattered across the codebase
- **Format:** `pub fn queue_synced(count: usize) -> String { format!("Queue synchronized with {count} library change(s)") }` — centralized, consistent, extractable
- **No gettext/fluent/fluent-bit integration in v1** — zero additional dependencies, zero build complexity
- **Future path:** `strings.rs` → gettext `.po` files or Fluent `.ftl` files — because strings are centralized, extraction is mechanical
- **Unicode handling:** all text processing uses Rust's standard Unicode-aware string handling; search uses Unicode normalization (NFD decomposition for accent-insensitive matching) via `unicode-normalization` crate
- **Locale-specific formatting:** numbers, dates, and durations formatted with `rust-icu` or manual implementation (ISO 8601 dates, standardized duration format) — avoids locale-dependent rendering that would change with system locale
- **Text direction:** LTR only in v1 — GTK4 supports RTL natively via text direction setting, but no RTL testing planned

### Explicit Trade-offs Accepted

- English-only UI excludes non-English-speaking users (mitigated by centralization for future translation)
- No gettext/fluent in v1 means first translation requires more setup than if the framework was present from day one
- Locale-independent number/date formatting means the UI doesn't adapt to the user's regional conventions
- RTL language support (Arabic, Hebrew) would require layout testing beyond v1 scope

## Architecture Decision Record: MPD Protocol Version Negotiation

**Decision:** Probe MPD protocol version on connect, maintain a feature capability matrix, and gracefully degrade or disable features when the connected MPD version doesn't support them.

### Key Details

- **Version detection:** parse protocol version string from MPD's initial banner response (`OK MPD {major}.{minor}`) during state machine's `Connecting` phase
- **Feature capability matrix:**
  - `readpicture` → requires MPD ≥ 0.24 — covers are fetched via `readpicture`; fallback to embedded or online-only for older versions
  - `albumart` → requires MPD ≥ 0.21 — used as secondary cover source when `readpicture` unavailable
  - `idle` + sub-systems → requires MPD ≥ 0.19 — push-based state updates (all supported versions)
  - `search` with type filters → requires MPD ≥ 0.20 — mode-scoped search filters
  - `list` with group → requires MPD ≥ 0.21 — album listing by group tags
  - `addid` with position → requires MPD ≥ 0.20 — insert-at-position queue operations
- **Graceful degradation:** unsupported features are hidden or replaced. E.g., MPD < 0.24: cover art section shows "Cover art unavailable (MPD ≥ 0.24 required)" instead of `readpicture` results. MPD < 0.20: search uses broader filters, results may be less precise
- **Capability cache:** capability matrix computed once per connection and cached on `SharedState` — no repeated probing
- **User visibility:** MPD protocol version shown in settings dialog; disabled features show a reason
- **Minimum supported version:** MPD ≥ 0.19 (the version that stabilized `idle`). Older versions receive a warning dialog at startup but the client attempts basic operation

### Explicit Trade-offs Accepted

- Feature matrix must be maintained as new MPD versions are released — missed features = missed optimization opportunities
- Graceful degradation paths multiply test cases (each feature × version combination)
- Minimum version of 0.19 excludes some older MPD installations (Debian stable may ship older versions in LTS)
- Protocol probing adds ~1 round-trip to connection setup time (~50ms)

### Compatibility Profiles (Formal)

The capability matrix is grouped into named profiles for deterministic feature detection:

| Profile | MPD Version | Features | Degradation |
|---------|-------------|----------|-------------|
| **Minimal** | 0.19 – 0.20 | `idle`, basic `status`/`currentsong`/`playlistinfo`, `add`/`delete`/`clear`/`next`/`previous` | No album art, no grouped views, no position-based insert |
| **Standard** | 0.21 – 0.23 | + `albumart`, `list` with group, `addid` with position, `search` with type filters | No `readpicture` (fallback: `albumart` only) |
| **Modern** | 0.24+ | + `readpicture`, full feature set | Full functionality |

**Transport profiles (orthogonal to MPD version):**

| Transport | Discovery | Auth |
|-----------|-----------|------|
| Unix socket | Auto-detect: `$XDG_RUNTIME_DIR/mpd/socket` → `/run/mpd/socket` | Filesystem permissions |
| TCP | Manual host:port config, fallback `localhost:6600` | MPD password |

**Profile detection on connect:**
1. Parse version from banner (`OK MPD {major}.{minor}`).
2. Capability matrix computed once, cached for connection lifetime.
3. If version parsing fails (non-standard MPD), assume Minimal profile — all optional features disabled.
4. If `idle` command returns "unknown command" (version detection failed), fall back to 500ms polling permanently.

## Subsystem Contracts

The following contracts define the guarantees, invariants, and fault behavior for each major subsystem. These are the commitments that calling code can rely on.

### Cover Cache Contract

**Guarantees:**
- `CoverProvider::get(id)` returns `Option<(PathBuf, Md5Hash, Option<Timestamp>)>` in <1ms. Never blocks, never falls through to MPD, never performs I/O beyond a stat() on the cache file.
- Cache data on disk is content-addressed via MD5 hash of the `albumart` binary. Same hash = identical data.
- For `readpicture` entries, a stored timestamp is compared against MPD's reported mtime. Same or older timestamp = no update.
- `ActualRead` enqueues at most one fetch per idle cycle. Never blocks the MPD command loop.

**Invariants:**
- Cache directory `~/.cache/mpd-client/covers/` exists and is writable. If not, `CoverProvider` returns `None` for all requests — no panic, no error.
- Cache file names are `{md5_hash}.jpg`. Sidecar metadata files (`.meta`) store the hash and optional timestamp.
- `CoverProvider` and `ActualRead` never call each other — no loops. CoverProvider reads, ActualRead writes.
- If a cache file fails to write (disk full, permissions), the error is logged and the fetch result is still emitted via `AppEvent::CoverRefreshed` — the UI can still display the cover from the event payload.

**Fault behavior:**
- Corrupt cache file (invalid JPEG header): deleted, re-fetched on next access. No cascading failure.
- Cache directory unwritable: `ActualRead` logs warning, skips disk write, still emits `CoverRefreshed` with data. In-memory only for the session.
- Reconnect: all visible albums are re-enqueued for `ActualRead`. Emit only if hash/timestamp differs from cache. No mass emission.

### Queue Sync Contract

**Guarantees:**
- The local queue is always a consistent snapshot of MPD's queue. After every `plchanges` or `playlistinfo` response, the local `Vec<QueueEntry>` exactly matches MPD's queue at the time of the response.
- Incremental updates via `plchanges <version>` preserve UI state (scroll position, selection, animation). Full `playlistinfo` replaces the model entirely.
- `playlist_version` from MPD's `status` response is the authoritative version counter. Stored per-connection, reset on reconnect.

**Invariants:**
- `QueueEntry.position` always matches MPD's 0-based queue position.
- `QueueEntry.id` always matches MPD's playlist id (used for `deleteid`, `moveid`).
- Deletions detected by cross-referencing local positions against MPD's reported playlist length. Positions beyond the new length were removed.
- Version wrap detection: if `new_version < old_version` with delta > 1M, assume 32-bit wrap and trigger full `playlistinfo`.

**Fault behavior:**
- `plchanges` returns incomplete data: fall back to full `playlistinfo` on next cycle.
- MPD returns inconsistent queue (duplicate positions, missing items): log warning, full `playlistinfo` refresh.
- Disconnect: queue state frozen at last known state. No commands accepted during disconnect. On reconnect, full `playlistinfo` refresh.
- Every 50 incremental updates: forced full `playlistinfo` sync to prevent drift accumulation.

### Folder Normalization Contract

**Guarantees:**
- Normalization never hides ambiguity. If the raw folder structure can't be cleanly normalized, it is presented as-is with a visual indicator (ambiguous structure).
- All playable files remain accessible regardless of normalization outcome. Normalization is a presentation-layer concern — the underlying queue still uses MPD URIs.
- Normalization results are deterministic: same input → same output, every time.

**Invariants:**
- Strategy chain executes in strict order: CueSheet → DsdFolder → SingleAlbum → Compilation → RawFs. First match wins.
- CueSheet strategy: validates that referenced track files exist before normalizing. Missing files prevent normalization (falls through to next strategy).
- DSD folder detection: directory containing only `.dsf`/`.dsd` files → treat as single album. Directory name becomes album name.
- Normalization is session-only, never persisted. Re-computed on each directory listing.
- User "explode" action (context menu: "Show raw files") bypasses normalization for that directory only. Choice is session-only.

**Fault behavior:**
- Cue file parse error: log error, discard cue, fall back to treating the raw file as a single playable item.
- Mixed cue + raw files in the same directory: cue strategy matches first (if valid), remaining raw files appear as separate tracks.
- Unrecognized folder structure: RawFs strategy always matches as fallback. Never fails.

### Search Index Contract

**Guarantees:**
- Search results return within 50ms for 95% of queries (10K album library). Query processing runs on the Search worker thread — never blocks GTK.
- Initial index build completes within 5s for 50K tracks. Built on startup from MPD metadata.
- Incremental update on library change: skipped if album count unchanged. If changed, full rebuild — incremental updates to the index are not supported.

**Invariants:**
- Index size: <5MB per 10K tracks. If index exceeds 2x this bound, log warning and trigger rebuild.
- Index covers all metadata fields: album title, artist, album artist, year, genre, format, track title. Folder mode additionally indexes file name and directory path.
- Minimum query length: 2 characters. Single-character queries return no results.
- Result ordering: artist → album → year → genre first, track results capped at 100 and shown only when relevant (<20 track matches).

**Fault behavior:**
- Index corruption (checksum mismatch on load): trigger full rebuild. Search unavailable during rebuild (~5s). User sees status message.
- Library update during rebuild: current rebuild completes, then triggers a new rebuild. Avoids partial index state.
- Memory pressure: index lives on the Search worker thread, not in AppState. Worker termination (panic, shutdown) loses index — rebuilt on restart.


## Architecture Decision Record: Browsing Sort Architecture

**Decision:** Mode-specific sort strategies applied at the presenter level, with album grid offering user-selectable sort modes and folder tree using filesystem-order by default.

### Key Details

- **Album Mode sort strategies:** `ByArtist`, `ByYear(asc|desc)`, `ByAlbumName`, `ByDateAdded`, `ByLastPlayed`, `Manual` (user drag-reorder, session-only)
- **Default Album sort:** `ByArtist` — alphabetically by normalized artist name, then by year ascending for multiple albums by same artist
- **Folder Mode sort:** filesystem order by default (matching the directory listing). Optional sort by format, date modified, or name per user toggle
- **Sort is a presenter concern** — the `QueueStore` remains in linear (playback) order; sort only affects the browsing view. Album grid sorting projects albums in sort order; queue items remain in MPD playlist order
- **Sort state per mode** — remembered for the session, not persisted (session persistence ADR covers this: browsing state is ephemeral)
- **Manual sort:** in Album Mode, albums can be drag-reordered within the grid. Changes are session-only. The manual ordering is stored as an ordered list of album IDs. switching away from Manual sort mode preserves the manual order but doesn't apply it
- **MPD data source:** sort operates on the album/track metadata already loaded via MPD's `list` or `listall` commands. No additional MPD queries for sort — sorting is client-side on cached metadata
- **Performance:** client-side sort of album metadata (typical library: <10K albums) is <1ms benchmarked — no need for off-thread sorting

### Explicit Trade-offs Accepted

- Client-side sort means sort is only as correct as the cached metadata from MPD
- Manual sort (session-only) is lost on crash — user must re-order
- Folder Mode filesystem-order may interleave albums split across directories
- `ByLastPlayed` sort requires tracking play history, which MPD provides via `playlistinfo` but not as a persistent per-album attribute

**Status: REPLACED by Metadata Caching (epic 26).** The original ADR proposed a dedicated sort presenter module. Instead, the approach is to pull all album metadata (year, genre, format, artist, cover paths) into a local cache, similar to the cover art cache. Sorting and grouping then operate on the cached metadata without MPD round-trips. This supersedes the dedicated sort module approach. See epic 26 for details.

## Architecture Decision Record: Crate & Module Organization

**Decision:** Single-crate project with a flat module hierarchy, no workspace partitioning in v1. Module boundaries aligned with architectural layers.

### Key Details

- **Single crate** (`mpd-client`) — no workspace sub-crates for v1. Rationale: faster iteration, simpler build, no inter-crate versioning overhead. Re-evaluate if compile times exceed 30s incremental.
- **Module hierarchy:**
  - `main.rs` — entry point, GTK app construction, CLI parsing, startup phase orchestration
  - `app/` — GTK application wiring, action registration, CSS, resource loading, main loop
  - `state/` — `AppState`, `SharedState`, `ActiveMode`, mode-specific state structs
  - `mpd/` — MPD adapter: state machine, command/event types, protocol parsing, connection management
  - `queue/` — `QueueStore`, `QueueItem`, queue mutation logic, undo stack, sync verification
  - `presenters/` — `AlbumQueuePresenter`, `TrackQueuePresenter`, `GridCoordinateMapper`, `FolderNormalizer`
  - `coverart/` — `CoverArtService`, providers, caches, rate limiter, priority queue
  - `search/` — `SearchService`, relevance scoring, fuzzy matching, mode-scoped search strategies
  - `layout/` — `LayoutService`, responsive breakpoints, column computation
  - `ui/` — widgets, CSS, theming. Sub-modules: `widgets/` (custom widgets), `theme/` (CSS, colors). Each custom widget follows the GTK4 Rust subclass pattern: `widget_name.rs` + `widget_name/imp.rs` (private implementation). See Custom Widget Architecture ADR for widget catalog.
  - `config/` — config loading, schema, migration, CLI flag parsing
  - `logging/` — logger setup, rotation, output formatting
  - `ipc/` — Unix socket listener, MPRIS D-Bus interface
  - `utils/` — string normalization, hash utilities, shared small helpers
- **Module visibility:** `pub(crate)` for internal cross-module API, `pub` only for crate-level exports. No `pub` on implementation details.
- **Cyclic dependency prevention:** arrows point inward — `ui/` imports `presenters/`, `presenters/` imports `state/` and `queue/`, `queue/` is dependency-free within the project. Enforced by `cargo-cyclonedx` or manual audit in CI.
- **Presenter/UI boundary rule:** `presenters/` owns all ViewModel types and projection logic — pure functions, no GTK imports. `ui/` owns all GTK widget code — imports presenter ViewModels but never performs projection logic. A presenter's `project()` method is the only way to produce a ViewModel. This prevents projection logic from leaking into widget callbacks.

### Explicit Trade-offs Accepted

- Single crate means full recompile on any change — no incremental workspace caching
- Flat hierarchy may grow too large; can split into workspace crates post-v1 when module boundaries are proven
- Inward-pointing dependency rule may produce occasional trait or type duplication to avoid cycles

## Architecture Decision Record: GTK4 Application Wiring

**Decision:** `GtkApplication` subclass pattern with centrally registered `GAction` entries for all invocable actions, CSS loaded from embedded resources, window state managed by the application singleton.

### Key Details

- **Application structure:** `MpdClientApp` struct wrapping `gtk::Application`. Holds `Arc<AppState>`, channel receivers, and service handles. Constructed in `main.rs`, run via `run()`.
- **Action registration:** All user-invocable actions registered as `GAction` entries during app startup. Actions include: mode toggle, playback control, queue operations, search focus, settings open, quit. Actions are enabled/disabled based on context (e.g., "pause" disabled when already paused).
- **Action-to-command flow:** `GAction` → `Action` enum → handler closure that either (a) sends to MPD command channel, (b) mutates state directly, (c) triggers UI transition. This centralizes all entry points (keyboard, menu, hover button, IPC) into one dispatch.
- **CSS loading:** Stylesheet loaded from embedded `gresource` at `com.mpdclient.style.css` path. CSS is compiled into the binary — no runtime file lookup, no missing-stylesheet failure mode. Hot-reload in debug builds via filesystem watch.
- **Window management:** Single window per application instance. `GtkApplicationWindow` with `GtkPaned` as root widget. Window geometry restored from session file on startup, saved on shutdown.
- **Resource system:** Icons, CSS, and UI definitions (if any `.ui` files) embedded via `gio` resource system. Compiled by `glib-compile-resources` at build time via `build.rs`.
- **Main loop integration:** GTK main loop drives the application. MPD event channel receiver is polled via `glib::idle_add()` or `g_timeout_add()` — adapter pushes events into the main loop's event queue for thread-safe processing.

### Explicit Trade-offs Accepted

- `GAction` registration adds boilerplate (~3 lines per action) vs inline signal handlers
- Embedded resources increase binary size by ~500KB (CSS, icons)
- Single-window architecture means no "open in new window" for comparing two views
- `glib::idle_add` polling of MPD event channel adds ~1ms latency vs immediate callback dispatch

## Architecture Decision Record: Technical Metadata Display & Audio Format Detection

**Decision:** Format metadata parsed from MPD's `file` tag extensions and `AudioFormat` field, displayed with mode-specific detail levels, with a format label normalization layer.

### Key Details

- **Data sources:** MPD provides audio format information via `AudioFormat` tag (e.g., `44100:24:2`) and file extension. Additional tags parsed from `file` path extension and MPD's `Format` tag when available.
- **Metadata normalization:** `AudioFormat` parser converts raw MPD fields into structured types:
  - `SampleRate` (44100, 48000, 96000, 192000, 384000 Hz)
  - `BitDepth` (16, 24, 32 bit) or DSD rate (DSD64 = 2822400, DSD128 = 5644800, DSD256 = 11289600, DSD512 = 22579200)
  - `Channels` (1=mono, 2=stereo, 3+=multichannel)
- **Format label generation:** `NormalizedFormat` enum → display string. Examples:
  - PCM 16/44.1 → `"16/44.1"` 
  - PCM 24/192 → `"24/192"`
  - DSD64 → `"DSD64 (2.8MHz)"`
  - DSD256 → `"DSD256 (11.2MHz)"`
- **Mode-specific detail levels:**
  - **Album Mode:** brief — shows format icon/label only when album has mixed formats (e.g., "16/44.1" in subtitle). Homogenous albums show no per-track format detail.
  - **Folder Mode:** full — shows sample rate, bit depth, file path, file size, duration for every track. Users need this detail for quality checking.
- **File extension:** parsed from MPD's `file` uri, used as secondary indicator when `AudioFormat` is unavailable (network streams, unsupported formats). Displayed as uppercase badge (e.g., `FLAC`, `DSF`, `WAV`).
- **Mixed-format detection:** `AlbumMetadata::has_mixed_formats()` checks all tracks in an album for varying format fields. If mixed, Album Mode shows format per-track breakdown in the current album track window.

### Explicit Trade-offs Accepted

- MPD's `AudioFormat` field is not guaranteed by the protocol — some streams may omit it. File extension fallback is less precise (e.g., `.flac` file could be 16-bit or 24-bit).
- DSD rate detection relies on sample rate heuristics (2822400 = DSD64) — future DSD rates (DSD1024) require updating the detection table
- Album Mode "mixed format" detection adds a scan pass over album tracks during grid projection (~O(n) per album, but n is typically <20 tracks)
- Folder Mode's per-track technical detail requires an additional MPD `listallinfo` query or caching the output of `playlistinfo` — not all metadata is available from the folder listing alone

## Starter Template Evaluation

### Primary Technology Domain

Desktop Linux application — **GTK4/Rust** via `gtk4-rs` bindings.

### Approach: First Principles — No External Starter Template

Evaluated from first principles: this is a **single-binary Rust desktop application** with no web backend, no mobile target, no cross-platform requirement. Traditional "starter templates" (web framework CLIs, mobile boilerplates) do not apply. The minimal viable scaffold is `cargo new` + dependency list.

### Selected "Starter": Cargo Binary Project + Explicit Dependency List

**Rationale for no external starter:**
- All existing MPD client libraries for Rust are unmaintained or target old protocol versions (user confirmed)
- GTK4-rs is actively maintained (latest: 0.11.0, Feb 2026) — version pin is the only setup needed
- Module structure must reflect the application's specific architecture (4 thread domains, dual-mode presenters, cover pipeline) — no generic template can anticipate this
- The architectural decisions (documented in ADRs above) define the module boundaries, not a starter generator

### Technology Stack

| Layer | Choice | Version/Constraint |
|-------|--------|-------------------|
| **UI** | `gtk4-rs` 0.11.x with feature `v4_14` + `libadwaita` (adw 0.8.x) | Minimum GTK 4.14.2 + libadwaita 1.6 runtime |
| **Image handling** | `gdk-pixbuf` (via GTK4) | Built into GTK4 stack |
| **HTTP** | `ureq` + `rustls` | Online cover art lookups (no tokio/openssl) |
| **Image processing** | `image` crate (jpeg, png, webp) | Decode + WebP thumbnail encoding |
| **Config** | `serde` + `toml` | TOML config parsing |
| **Logging** | `log` + `env_logger` | Standard Rust logging |
| **Unicode** | `unicode-normalization` | Accent-insensitive search |
| **MPD protocol** | Hand-rolled — `std::net::TcpStream` + `BufRead` | No existing library is current |
| **D-Bus** | `zbus` crate | MPRIS integration (disabled by default) |
| **Async runtime** | None — GTK main loop + dedicated `std::thread` | No tokio/async-std |

### GTK4 Compatibility Decision

| Target GTK4 | Status | Reasoning |
|-------------|--------|-----------|
| **4.14.2** | ✅ Required — feature flag `v4_14` | Enables `StringList`/`StringSorter` for search/queue, fill/stroke for cover grid |
| **4.6.2** | ❌ Not targeted | Would lose `v4_10`+ APIs (ListView improvements, column views); no maintained distro ships this anymore |
| **4.22.2** (build host) | ✅ Builds and runs | Backwards compatible — code using only `v4_14` APIs runs on any ≥4.14 runtime |

### Dependencies (Cargo.toml)

```toml
[package]
name = "mpd-client"
version = "0.1.0"
edition = "2024"     # gtk4-rs 0.11 uses Rust 2024 edition
rust-version = "1.85" # MSRV — edition 2024 requires ≥1.85

[dependencies]
gtk4 = { version = "0.11", features = ["v4_14"] }
glib = "0.20"
gdk-pixbuf = "0.20"
ureq = { version = "3", default-features = false, features = ["rustls", "gzip"] }
rustls = "0.23"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1.1"
dirs = "6"
log = "0.4"
env_logger = "0.11"
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
unicode-normalization = "0.1"
thiserror = "2"
```

### Module Structure

```
src/
  main.rs              # Entry point, CLI parsing, startup phases
  app.rs               # GtkApplication wiring, GAction registration, CSS
  state/               # AppState, SharedState, ActiveMode enum
  mpd/                 # MPD adapter: state machine, protocol, commands
  queue/               # QueueStore, undo stack, sync
  presenters/          # Album/Track queue projections, GridCoordinateMapper
  coverart/            # CoverArtService, provider chain, caches
  search/              # SearchService, index, relevance scoring
  layout/              # LayoutService, breakpoints, column computation
  ui/                  # Widgets, CSS bindings
  config/              # Config loading, CLI flags
  ipc/                 # Unix socket, MPRIS
  utils/               # Hash, string normalization
```

### Build & Distribution

```bash
cargo build --release    # produces target/release/mpd-client
strip target/release/mpd-client  # optional size reduction
```

Binary-only release — no Flatpak, no `.deb`/`.rpm`. User runs the binary directly.

### System Dependencies (gtk4-rs build requirements)

| Distribution | Packages |
|-------------|----------|
| **Arch Linux** | `sudo pacman -S gtk4 pkgconf` |
| **Fedora** | `sudo dnf install gtk4-devel pkgconfig` |
| **Debian/Ubuntu** | `sudo apt install libgtk-4-dev pkg-config` |
| **openSUSE** | `sudo zypper install gtk4-devel pkg-config` |

No other system libraries required — `ureq` uses `rustls` (no OpenSSL), `image` is pure Rust.

### First Implementation Milestones

Reverse-engineered from the goal state, these are the five smallest working programs that build toward the full application. Each milestone is a compilable, runnable checkpoint.

#### Milestone 0 — "It Opens"

Validate GTK4-rs toolchain: window appears, nothing else.

```rust
use gtk4::prelude::*;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.mpdclient.app")
        .build();

    app.connect_activate(|app| {
        let win = gtk4::ApplicationWindow::new(app);
        win.set_title("MPD Client");
        win.set_default_size(1200, 800);
        win.present();
    });

    app.run();
}
```

**Compile check:** first `cargo build` downloads + compiles gtk4-rs (~5–10 min). Subsequent builds incremental.

#### Milestone 1 — "It Connects"

Validate MPD protocol: pure TCP exercise, no GTK involved. Tests against localhost:6600 with error handling and human-readable output.

```rust
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, Shutdown};

fn main() {
    let mut stream = match TcpStream::connect("127.0.0.1:6600") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not connect to MPD at 127.0.0.1:6600");
            eprintln!("Is MPD installed and running? (Error: {e})");
            std::process::exit(1);
        }
    };

    let mut reader = BufReader::new(stream.try_clone()
        .expect("Failed to clone MPD socket"));

    // Read MPD banner: "OK MPD 0.24.x"
    let mut banner = String::new();
    reader.read_line(&mut banner)
        .expect("MPD did not send a banner response");
    banner = banner.trim().to_string();

    // Send status command
    writeln!(stream, "status").expect("Failed to send status command");
    let mut response = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("MPD disconnected during status response");
        if line == "OK\n" || line.starts_with("ACK") { break; }
        response.push_str(&line);
    }

    // Human-readable status
    let state = response.lines()
        .find_map(|l| l.strip_prefix("state: "))
        .unwrap_or("unknown");
    let artist = response.lines()
        .find_map(|l| l.strip_prefix("artist: "))
        .unwrap_or("");
    let title = response.lines()
        .find_map(|l| l.strip_prefix("title: "))
        .unwrap_or("");
    let elapsed = response.lines()
        .find_map(|l| l.strip_prefix("elapsed: "))
        .and_then(|s| s.split('.').next())
        .unwrap_or("0");

    println!("✓ Connected to MPD at 127.0.0.1:6600");
    println!("  Protocol: {banner}");
    if state == "play" && !artist.is_empty() {
        println!("  Now playing: {artist} — {title} ({elapsed}s)");
    } else if state == "pause" {
        println!("  Player: paused (at {elapsed}s)");
    } else {
        println!("  Player: stopped");
    }

    stream.shutdown(Shutdown::Both).ok();
}
```

**Goal:** `cargo run` prints friendly MPD connection status, then exits. Handles MPD-not-running gracefully.

#### Milestone 2 — "The Split Shell"

```rust
app.connect_activate(|app| {
    let win = gtk4::ApplicationWindow::new(app);
    win.set_title("MPD Client");
    win.set_default_size(1200, 800);

    let paned = gtk4::Paned::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .position(840)  // 70% of 1200
        .build();

    let left = gtk4::Label::new(Some("Album browsing area"));
    let right = gtk4::Label::new(Some("Rail — now playing + queue"));
    paned.set_start_child(Some(&left));
    paned.set_end_child(Some(&right));

    win.set_child(Some(&paned));
    win.present();
});
```

**Goal:** resizable split-pane window with 70/30 divider and placeholder labels. Validate `GtkPaned` + `LayoutService` baseline.

#### Milestone 3 — "It Connected in the Shell"

Integrate M1 (TCP MPD connection) into M2 (split shell): spawn a background thread that connects to MPD and pipes connection status into the right panel via `glib::idle_add`. Validates the full data path: MPD socket → background thread → channel → GTK main loop → widget text.

```rust
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.mpdclient.app")
        .build();

    let (tx_main, rx_main) = mpsc::channel::<String>();

    // Background thread: connect to MPD, read status, send to main thread
    thread::spawn(move || {
        let send_status = || -> Result<String, String> {
            let mut stream = TcpStream::connect("127.0.0.1:6600")
                .map_err(|e| format!("connect failed: {e}"))?;
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            let mut banner = String::new();
            reader.read_line(&mut banner).map_err(|e| format!("banner: {e}"))?;

            writeln!(stream, "status").map_err(|e| format!("cmd: {e}"))?;
            let mut resp = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).map_err(|e| format!("read: {e}"))?;
                if line == "OK\n" || line.starts_with("ACK") { break; }
                resp.push_str(&line);
            }

            let state = resp.lines()
                .find_map(|l| l.strip_prefix("state: ")).unwrap_or("unknown");
            let song = resp.lines()
                .find_map(|l| l.strip_prefix("song: ")).unwrap_or("—");
            Ok(format!("MPD {}\nstate: {}\nsong: {}", banner.trim(), state, song))
        };

        loop {
            let msg = match send_status() {
                Ok(s) => s,
                Err(e) => format!("MPD offline ({e})"),
            };
            if tx_main.send(msg).is_err() { break; }
            thread::sleep(std::time::Duration::from_secs(2));
        }
    });

    app.connect_activate(move |app| {
        let win = gtk4::ApplicationWindow::new(app);
        win.set_title("MPD Client");
        win.set_default_size(1200, 800);

        let paned = gtk4::Paned::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .position(840)
            .build();

        let left = gtk4::Label::new(Some("Album browsing area"));
        let right = gtk4::Label::new(Some("Connecting to MPD…"));
        paned.set_start_child(Some(&left));
        paned.set_end_child(Some(&right));

        win.set_child(Some(&paned));
        win.present();

        // Bridge: poll MPD channel on GTK idle, update right panel
        let rx = rx_main.clone();
        glib::idle_add_local(move || {
            while let Ok(msg) = rx.try_recv() {
                right.set_text(&msg);
            }
            glib::ControlFlow::Continue
        });
    });

    app.run();
}
```

**Goal:** split shell window where the right panel shows live MPD status (protocol version, state, current song) updating every 2 seconds. Validates: thread spawn, channel bridge, `glib::idle_add` wiring, and the M1→M2 integration path.

#### Milestone 4 — "Something in the Grid"

Fetch album list from MPD, populate `GtkFlowBox` with text labels. No covers, no hover buttons — just a scrollable list of album titles in grid layout.

```rust
// MPD: "list album artist <artist>" or "list album"
// → Feed into GtkFlowBox children with proper column wrapping
// → GridCoordinateMapper logic starts here (row/col to linear index)
```

**Goal:** scrollable album title grid, `LayoutService` breakpoints working (resize window → column count changes).

## Infrastructure & Deployment Decisions

### Current Phase: Pre-Distribution

**Decision:** No release infrastructure until there is a user to distribute to.

- The only commands that matter right now: `cargo build`, `cargo run` (against local MPD), `cargo test`
- No versioning scheme, no release process, no tags, no distribution packaging
- No CI/CD, no `release.sh` script, no `rust-toolchain.toml` pinning
- No mock MPD server — test against the real MPD instance on the same machine
- All distribution, packaging, versioning, and automation infrastructure is deferred until the product has a consumer

### Testing Approach

**Decision:** `cargo test` for regression catching, manual validation against real MPD.

- Unit tests inline with modules (`#[cfg(test)]`) for state/presenter logic
- Integration tests against real MPD in `tests/` directory
- No mock server — real MPD is always available on localhost:6600
- GUI testing remains manual per the Testing ADR
- Tests run locally, no CI test runner

## Implementation Patterns & Consistency Rules

### Pattern Categories Defined

**Key Conflict Points Identified:** 5 areas where AI agents could make different choices

### Naming Patterns

**Code Naming (Rust conventions, enforced by clippy):**

- Functions/methods: `snake_case` — `get_status()`, `compute_projection()`
- Types/traits/enums: `PascalCase` — `QueueStore`, `AlbumQueuePresenter`
- Variables: `snake_case` — `current_track`, `album_id`
- Modules: `snake_case` — `coverart::`, `presenters::`
- Constants: `SCREAMING_SNAKE_CASE` — `MAX_QUEUE_SIZE`, `DEFAULT_PORT`
- Error variants: `PascalCase` with descriptive name — `ConnectionFailed`, `ProtocolMismatch`
- File paths: same as module name — `presenters/album_queue.rs`, `mpd/state_machine.rs`

**No non-standard naming — Rust ecosystem conventions apply throughout.**

### Structural Patterns

**Error Handling: Per-module errors (Option A)**

```rust
// Each domain module defines its own error enum
// mpd/error.rs
#[derive(thiserror::Error)]
pub enum Error {
    #[error("Connection to {host}:{port} failed: {source}")]
    ConnectionFailed { host: String, port: u16, #[source] source: std::io::Error },
    #[error("Protocol error at line {line}: {detail}")]
    ProtocolMismatch { line: usize, detail: String },
    #[error("MPD returned error: {0}")]
    MpdRejected(String),
    #[error("Connection closed by peer")]
    ConnectionClosed,
}

// coverart/error.rs
#[derive(thiserror::Error)]
pub enum Error {
    #[error("No provider could serve cover for {album}: {reason}")]
    NoProvider { album: String, reason: String },
    #[error("Rate limited, retry after {secs}s")]
    RateLimited { secs: u64 },
    #[error("Cache write failed: {0}")]
    CacheIo(#[from] std::io::Error),
}
```

- Each module error implements `user_facing_message()` returning an actionable string
- Cross-layer errors use `From` impls or `Box<dyn Error>` at the UI boundary only
- All errors go through the `ErrorSink` aggregator on the main thread

**Event/Channel Payloads: Per-domain channels (Option B)**

```rust
// One channel per domain, typed payloads

// mpd/channel.rs
pub enum MpdEvent {
    Connected { version: String },
    Disconnected,
    PlaybackChanged(PlaybackStatus),
    QueueChanged(Vec<QueueItem>),
    VolumeChanged(u8),
    DatabaseUpdated,
}

// coverart/channel.rs
pub enum CoverArtEvent {
    Ready { album_id: String, path: PathBuf },
    Failed { album_id: String, reason: String },
}

// ui/channel.rs
pub enum UiCommand {
    ShowToast { message: String, level: ToastLevel, timeout_ms: u64 },
    ShowSettings,
    SwitchMode(Mode),
    Quit,
}
```

- Each receiver polled via `glib::idle_add_local()` on the GTK main loop
- No single giant `AppEvent` enum — receivers are explicit about what they handle
- UiCommands originate from actions, keybindings, IPC and are dispatched through the centralized `Action` enum

**Widget Structure: Directory per widget (Option A — gtk4-rs standard subclass pattern)**

```
ui/widgets/
├── mod.rs                    # re-exports
├── album_grid/
│   ├── mod.rs                # AlbumGrid public API: new(), connect_signals(), model()/view()
│   └── imp.rs                # AlbumGridPriv @implements GtkWidget, ObjectImpl, WidgetImpl
├── folder_tree/
│   ├── mod.rs
│   └── imp.rs
├── queue_album/
│   ├── mod.rs
│   └── imp.rs
├── queue_track/
│   ├── mod.rs
│   └── imp.rs
├── now_playing/
│   ├── mod.rs
│   └── imp.rs
├── cover_display/
│   ├── mod.rs
│   └── imp.rs
├── search_bar/
│   ├── mod.rs
│   └── imp.rs
├── toast_overlay/
│   ├── mod.rs
│   └── imp.rs
└── settings_dialog/
    ├── mod.rs
    └── imp.rs
```

- `mod.rs` exposes the outer widget struct and public API only (constructors, signal handlers, method stubs)
- `imp.rs` contains the `#[gtk::template]` impl block, trait impls (`ObjectImpl`, `WidgetImpl`), and private state
- CSS name set in `imp.rs` via `widget().set_css_name()`
- No inline styles — all styling via CSS name bindings

**Presenter Location: Flat `presenters/` module (Option A)**

```
src/
└── presenters/
    ├── mod.rs
    ├── album_queue.rs        # AlbumQueuePresenter: QueueStore → AlbumGridViewModel
    ├── track_queue.rs        # TrackQueuePresenter: QueueStore → TrackListViewModel
    ├── grid_coord.rs         # GridCoordinateMapper — inline in album_grid.rs for v1
    └── folder_norm.rs        # FolderNormalizer trait + strategies (CueSheet, DsdFolder, etc.)
```

- All projection logic in one place — no GTK imports in `presenters/`
- `mod.rs` re-exports only the presenter functions (not internal helpers)
- Each module is a set of stateless pure functions — no structs, no state
- `GridCoordinateMapper` lives in `presenters/browse/album_grid.rs` (small module, doesn't warrant a separate file in v1). Shared between album and folder modes via `pub(crate)` re-export from `presenters::browse`.

### Format Patterns

**Data Formats:**

- All internal structs: `#[derive(Debug, Clone, Serialize, Deserialize)]` with `serde` rename = `snake_case`
- MPD protocol parsing: map MPD's `Key: Value` lines into internal structs at the adapter boundary — never pass raw MPD maps to presenters or UI
- Event payloads: typed structs, not `HashMap<String, String>` — parsed at the MPD channel boundary
- Logging: structured `log` macros at three verbosity levels per the Logging ADR
- No JSON serialization in the client — config is TOML, communications are internal typed channels

### Communication Patterns

**Event System:**

- **Event naming:** `past_tense_adjective` — `Connected`, `Disconnected`, `PlaybackChanged`, `QueueChanged`, `DatabaseUpdated`
- **Command naming:** `imperative_verb` — `Play`, `Pause`, `AddToQueue`, `FocusSearch`
- **Event flow direction:** strictly inward — external sources (MPD, user input, IPC) → command channel → state mutation → broadcast to subscribers
- **No circular event flow:** event handlers never emit events; state changes trigger events, events don't trigger state changes

**State Management:**

- State mutations happen only on the GTK main loop (single-threaded)
- Background threads send events to main loop; main loop applies mutations
- `Store::update_*` methods are the single path to mutate state — no direct `state.write().await` outside of `Store`
- After mutation, `Store` broadcasts new state via `broadcast::Sender<AppState>` (defined in `state::EventBus`)
- Widgets subscribe to the broadcast and call `glib::idle_add` to schedule redraw

### Process Patterns

**Error Handling Flow:**

```
source (MPD/cover/search) → per-module Error → ErrorSink.aggregate()
  → Recoverable → toast notification (3s)
  → Retryable    → enqueue retry with backoff
  → Fatal        → modal dialog
```

- Every `Error` type implements `fn user_facing_message(&self) -> String`
- `ErrorSink` lives on the main thread, receives errors via channel from all threads
- No `unwrap()` in non-test code — use `expect("descriptive message")` for invariants

**Loading States:**

- Album grid: covers load progressively — skeleton shimmer on fetch, fade-in on ready
- Folder tree: expands immediately for cached paths, shows spinner for uncached
- Queue: always-present data (from MPD playlistinfo) — no loading state
- Search: immediate omnibox keystroke echo, 150ms debounce on result computation
- Connection: persistent indicator in rail header: green dot (connected), yellow (connecting), red (disconnected)

### Enforcement Guidelines

**Automated (checked by `scripts/check-patterns.sh`):**

1. `cargo clippy -- -D warnings -D clippy::unwrap_used` — zero warnings
2. Non-test code uses `expect("msg")`, never bare `unwrap()` — grep-enforced
3. `src/presenters/` contains no `gtk4`/`gdk4`/`gdk_pixbuf` imports — grep-enforced
4. `cargo test --lib` — all unit tests pass
5. `cargo test` — integration tests pass when feature enabled

**Human Review (code review checklist):**

- Per-module `thiserror` enums used throughout (no single catch-all `Error` type)
- Errors routed through ErrorSink, not `eprintln!` or panic
- `presenters::types` contains only string aliases, flat enums, and newtypes (scope-limit rule)

**Recommended Practices (not enforced, flagged in review):**

- Widget subclass pattern for complex widgets; single file acceptable for simple
- Store mutation only via `Store::update_*` methods
- Test placement: `#[cfg(test)]` for unit tests, `tests/` for integration
- Cascade flow documented as doc comment on initiating handler

### Pattern Examples

**Good Examples:**

- `mpd/adapter.rs` calls `send_command("status")` → returns `Vec<String>` → parsed into `HashMap<String, String>` → structured into `MpdEvent::PlaybackChanged(PlaybackStatus)` → sent to main thread
- `presenters/album_queue.rs` takes `&QueueStore` and `column_count`, returns `AlbumGridViewModel` — no GTK types imported, no side effects
- Widget `album_grid/imp.rs` overrides `WidgetImpl::measure()` and `WidgetImpl::snapshot()` — CSS name set to `"album-grid-card"`, children positioned via CSS

**Anti-Patterns (Avoid):**

- Parsing MPD response maps directly in a signal handler callback — always parse at the adapter boundary
- Importing `gtk4::*` in a presenter module — presenter modules must be pure data transformations
- Using `unwrap()` without `expect("context")` — every unwrap must document the invariant
- Writing state mutations in `glib::idle_add` closures — only `Store::update_*` mutates state
- Co-locating presenter logic inside widget files — presenters live in `presenters/`

### Consolidated Refinements (Authoritative Over ADRs)

The following represent the final state of all pattern refinements. **Where these conflict with the ADRs above, this section is authoritative.** The ADRs document the initial design; this section records adjustments made through analysis. Cross-reference notes have been added to affected ADRs (Error Handling, Concurrency & Threading, Startup/Shutdown) pointing here.

**1. Channel & Event Ordering**

- Causally related events (playback state + queue state) must share a single channel to preserve causal ordering. Per-domain channels are for *fire-and-forget* events (cover art, toasts) that have no ordering dependency.
- Monotonic sequence numbers are **not used** — `std::sync::mpsc` and `tokio::sync::mpsc` guarantee FIFO ordering; the GTK main loop processes from `glib::idle_add` sources in FIFO order. No reordering risk exists.
- Cross-channel event cascade flows (e.g., "next track" → MPD command → MPD response → state update → cover request) are documented as inline doc comments on the initiating handler method. No separate cascade documentation file.

**2. Error Handling**

- **All user-facing feedback** goes through `AppEvent::Toast` → `reduce()` → `AppState.toast_queue`. No separate ErrorSink channel. See §4b for the complete toast/notification design.
- **Three-tier severity** (`ToastLevel::Info | Warn | Error`) determines display behavior: Info auto-dismisses (3s), Warn auto-dismisses (5s), Error persists until user dismisses.
- **Expected vs. exceptional outcomes:** Expected outcomes (cover not found, search no results, album empty) are handled locally by calling code — return `Option` or `Result::Ok(None)`, never emit a Toast. Exceptional conditions (network timeout, parse failure, rate limit) emit `AppEvent::Toast`. Rule of thumb: if you'd `match` on it in normal flow, it's expected; if you'd `unwrap()` in a prototype, it's exceptional.
- **Any thread can emit `AppEvent::Toast`** — MPD thread (connection errors), Cover Proc (decode failures), Search worker (index corruption), GTK thread (user action confirmation). All go through the same channel, the same `reduce()`, the same notification routing.

**3. Thread Model**

**3a. Thread Topology**

```
                 ┌──────────────────────────────────────────────┐
                 │            GTK main thread                    │
                 │  AppEvent → reduce(AppState, AppEvent)        │
                 │  Shows in-app toast overlay from toast_queue  │
                 │  No blocking I/O, no decode, no model builds  │
                 │  NO D-Bus, NO desktop notifications           │
                 └────┬──────┬──────┬──────┬────────────────────┘
                      │      │      │      │
              mpsc channels (events/results)
                      │      │      │      │
                 ┌────┴──┐ ┌┴─────┐ ┌┴────┐ ┌┴──────────────┐ ┌┴────────────────┐
                 │ MPD   │ │ MPD  │ │Cover│ │  Search       │ │Notification    │
                 │ IO    │ │Cover │ │Proc │ │  Worker       │ │Router          │
                 │(pers.)│ │(tmp) │ │     │ │               │ │(lightweight)   │
                 │ idle  │ │album-│ │MD5  │ │ index rebuild │ │ reads toast     │
                 │ cmds  │ │art   │ │dec  │ │ query exec    │ │ events, fires   │
                 │ state │ │read  │ │cach │ │ rank & filter │ │ D-Bus notifs    │
                 └───────┘ └──────┘ └─────┘ └───────────────┘ └─────────────────┘
```

**6 threads total:**

| Thread | Role | Type | Communication |
|--------|------|------|---------------|
| **GTK main** | UI rendering, `reduce()`, in-app toast overlay | Single-threaded per GTK4 | Receives `AppEvent` via channel. NO D-Bus. |
| **MPD IO** | Idle loop, command dispatch, status, queue | Persistent connection | `mpsc::Sender<MpdCommand>` in, `mpsc::Sender<AppEvent>` out |
| **MPD Cover** | `albumart`/`readpicture` binary fetch | Created on demand, dropped when idle | Receives cover URIs via channel, sends raw bytes to Cover Proc |
| **Cover Proc** | Decode bytes → pixbuf, MD5 hash, disk cache write, thumbnail | Persistent worker | Receives raw bytes, emits `AppEvent::CoverRefreshed` |
| **Search** | Index rebuild, query execution, ranking | Persistent worker | Receives search/index commands, emits `AppEvent::SearchResults` |
| **NotificationRouter** | Reads Toast events, fires D-Bus desktop notifications | Lightweight, on-demand | Receives cloned `AppEvent::Toast` via channel receiver |

**3b. Thread Responsibilities**

**MPD IO thread (1 thread, 1 connection):**
- Runs the idle loop. Only MPD protocol commands and responses.
- Never decodes images, never writes to disk, never touches `AppState`.
- Emits `MpdEvent` (state, queue, playlist, album data) via channel.
- Cover art handling: on receiving `FetchCovers` command, sends the `albumart`/`readpicture` MPD commands, reads raw binary, forwards bytes to the MPD Cover thread via a dedicated channel. The MPD IO thread never decodes or processes cover data — it only ships raw bytes off-thread.

**MPD Cover thread (0-1, created on demand):**
- Opens a separate MPD connection for binary cover data. No idle loop, no state tracking.
- Receives `(uri, offset)` pairs, sends `albumart <uri> <offset>` on its socket, reads binary response, forwards to Cover Proc.
- Connection is created when covers need fetching, dropped after a configurable idle timeout (30s of no work).
- If the main MPD connection gets a new epoch (reconnect), this thread's connection is also dropped and recreated — stale connection data is useless.

**Cover Proc worker (1 thread):**
- Receives raw binary cover data.
- Decodes via `gdk-pixbuf` or `image` crate. Computes MD5 hash. Compares to cache hash.
- Writes to disk cache if new/different. Emits `AppEvent::CoverRefreshed(id, Vec<u8>)`.
- No MPD protocol knowledge. No GTK widget access. Filesystem access for cache only.

**Search worker (1 thread):**
- Owns the search index. Rebuilds on library change events.
- Receives search queries, runs full-text match, computes relevance scores, caps results.
- Emits `AppEvent::SearchResults`.
- No MPD protocol knowledge. No GTK access.

**NotificationRouter (lightweight, on-demand):**
- Spawned as a minimal thread with a cloned receiver for `AppEvent::Toast`.
- Receives `Toast` events. Checks `[notifications] mode` setting.
- If mode is `"desktop"` or `"both"`: fires desktop notification via D-Bus (org.freedesktop.Notifications).
- If mode is `"toast"` or `"both"`: does nothing — the GTK thread handles in-app toasts independently.
- No GTK dependency. No widget access. Filesystem/dependencies: only if D-Bus (`zbus`) is linked.
- Falls back silently to no-op if D-Bus session bus is unavailable or MPRIS is not enabled.

**3c. Shutdown Coordination**

All workers check a shared `ShuttingDown` flag (`AtomicBool`). When set:
- MPD IO thread: exits idle loop, closes socket, terminates.
- MPD Cover thread: drops connection, terminates on next wake.
- Cover Proc: skips queued jobs, terminates. Decode in progress is abandoned (OS reclaims memory).
- Search: skips queued queries, terminates.
- GTK thread: stops processing incoming events after draining the channel (events already in transit may arrive after shutdown signal — they are silently dropped by `reduce()` checking `ShuttingDown`).

**4. Single Reducer (State Mutation Ownership)**

**Rule:** All state mutations pass through a single `reduce(AppState, AppEvent) -> AppState` function. No widget, no worker, no event handler directly mutates `AppState`.

**Event flow:**

```
MPD IO      ──→ AppEvent ──→ channel ──→ GTK thread → reduce(state, event)
Cover Proc  ──→ AppEvent ──→                                                         
Search      ──→ AppEvent ──→                                                          
Any thread  ──→ AppEvent::Toast ──→                                                         
UI (user)   ──→ MpdCommand ──→ channel ──→ MPD IO thread → MPD protocol

Toast path after reduce():
  AppState.toast_queue → GTK thread: show in-app toast overlay (no D-Bus)
  Cloned channel → NotificationRouter: fire D-Bus desktop notification (no GTK)
```

**What reduce() controls:**
- `PlaybackUpdate` → replaces `SharedState.playback`, emits signal for widget refresh
- `Queue(Vec<QueueEntry>)` → replaces `SharedState.queue`, emits signal
- `Albums(Vec<Album>)` → replaces browsing state, triggers grid rebuild job
- `CoverRefreshed(id, bytes)` → updates cover registry, triggers widget redraw
- `SearchResults` → replaces search state, triggers results display
- `Connected/Disconnected` → updates connection state, triggers UI mode switch
- `Error` → routes to ErrorSink or shows toast

**What reduce() does NOT do:**
- No GTK widget operations — `reduce()` is a pure data transformation. Widget updates are triggered by signals/property notifications after reduce completes.
- No channel sends — `reduce()` doesn't emit new events. Event → state → UI, not event → state → new event.
- No blocking I/O — `reduce()` runs on the GTK thread. Must complete in <1ms.

**Enforcement:**
- `AppState` fields are not `pub`. Only `reduce()` can write them.
- Workers hold `Sender<AppEvent>`, never a reference to `AppState`.
- Widgets read `AppState` via shared reference (`Arc<RwLock<>>`), never write.
- This is the single reducer — no sub-reducers, no middleware, no delegation.
- `reduce()` is a plain function (not a method on AppState) to make testing trivial: `assert_eq!(reduce(state, event), expected_state)`.

**Trade-off accepted:** Single reducer means all state transitions are in one function (~200-300 lines for this app). This is simpler than splitting reducers per domain for an app this size. If the function grows beyond 500 lines, split into domain-specific helper called by the main `reduce()` (e.g., `reduce_playback()`, `reduce_queue()`, `reduce_cover()`), but keep the single entry point.

**4a. AppEvent Enum**

```rust
enum AppEvent {
    // MPD connection
    Connected, Disconnected,
    // Playback & queue
    StateChanged(PlaybackUpdate),
    Queue(Vec<QueueEntry>),
    LibraryChanged,
    // Albums & browse
    Albums(Vec<Album>),
    AlbumsGrouped(AlbumGroup),
    DirectoryListing(String, Vec<DirEntry>),
    // Search
    SearchResults(Vec<(String, String)>),
    // Cover art
    CoverRefreshed { album_id: String, data: Vec<u8> },
    // Covers batch progress
    CoverProgress { fetched: usize, total: usize },
    // UI feedback (replaces ErrorSink)
    Toast { message: String, level: ToastLevel },
    // User commands (from UI to MPD)
    Command(MpdCommand),
}
```

**4b. Toast & Notification Consolidation**

**Current problem:** ErrorSink has its own separate channel (`mpsc::Sender<ErrorSinkEvent>`). Non-error toasts fire ad hoc from widgets. Two pathways for the same purpose.

**Fix:** All toasts go through `AppEvent::Toast` → `reduce()` → `AppState.toast_queue`. ErrorSink is removed — its cases become `AppEvent::Toast`:

| Old ErrorSink | New AppEvent::Toast |
|---------------|-------------------|
| Recoverable | `Toast { level: Info \| Warn, message }` |
| Retryable | `Toast { level: Warn, message }` |
| Fatal | `Toast { level: Error, message }` (shown persistently, not auto-dismissed) |

**reduce() handles Toast:**
```rust
reduce(state, event) {
    match event {
        AppEvent::Toast { message, level } => {
            state.toast_queue.push_back(Toast { message, level, shown: false });
        }
        // ...
    }
}
```

**Notification routing (two independent consumers):**

After `reduce()` completes, two paths handle toasts independently:

**1. GTK thread:** Drains `AppState.toast_queue`. Shows in-app toast overlay (`GtkRevealer` with label). No I/O, no D-Bus. Always runs regardless of `[notifications] mode`. `ToastLevel::Error` always shows in-app — too important to be only in a desktop notification.

**2. NotificationRouter thread:** Receives cloned `AppEvent::Toast` stream via shared channel receiver. Checks `[notifications] mode`:
- `"desktop"` or `"both"`: fires D-Bus notification via `org.freedesktop.Notifications`.
- `"toast"` (default): no-op — GTK thread already handled it.

```rust
struct NotificationRouter {
    mode: NotificationMode,
    notifier: Option<DesktopNotifier>,  // None if D-Bus unavailable
}

enum NotificationMode {
    Toast,    // In-app only (default)
    Desktop,  // Desktop notification only
    Both,     // Both
}
```

- **Toast mode (default):** Shows in-app toast overlay (`GtkRevealer` with label, auto-dismiss after 3s). Non-intrusive, visible only when app window is focused.
- **Desktop mode:** Fires a desktop notification via D-Bus (org.freedesktop.Notifications or MPRIS). Visible even when window is minimized or in another workspace. Requires D-Bus session bus.
- **Both mode:** Shows in-app toast AND fires desktop notification. Useful for persistent issues during background playback.
- **Error level toasts** (`ToastLevel::Error`) are always shown in-app regardless of mode — they are too important to be only in a desktop notification that auto-dismisses.

**Config:**
```toml
[notifications]
mode = "toast"        # "toast" | "desktop" | "both"
```

The NotificationRouter falls back silently to no-op if D-Bus session bus is unavailable or the `zbus` dependency is not compiled in. Desktop notifications do NOT depend on the MPRIS feature flag — they use `org.freedesktop.Notifications` which is a different D-Bus interface. However, both require a D-Bus session bus, so in practice if one works the other likely does too.

**5. Reconnection Safety**

- On reconnect, the MPD adapter sends a `stop` command immediately. This prevents the previous session's playback from resuming unexpectedly on the new connection.
- **Epoch counter** on the MPD connection (`struct MpdConnection { epoch: u64 }`). Increments on each full disconnect → reconnect cycle. Each `MpdEvent` carries the epoch that produced it. The UI thread discards events whose epoch doesn't match the current connection — this prevents stale responses (cover art, queue state, playback status) from a previously-dead connection from being applied after a reconnect completed.
- **No command queue** — commands sent during disconnection are silently discarded. Rationale: MPD commands are stateful (play, pause, add, delete). Replaying them on a new connection could produce incorrect behavior (e.g., "play" on a fresh playlist that no longer has the expected song). The 500ms polling fallback ensures the UI refreshes within one cycle once connected.
- **Toast suppression:** If reconnect completes within 5s (transient blip), no "MPD disconnected" toast is shown. The connection indicator in the rail header (red → yellow → green) is sufficient feedback for brief disconnects. A toast is only shown if reconnect exceeds 5s.

**6. Connection Lifecycle**

**Unified connection logic:** Initial connect and reconnect use exactly the same code path. No distinction — the state machine enters `Disconnected` on startup the same way it enters `Disconnected` after a connection loss.

**Three states:**

- **Disconnected:** No socket. Retry timer running (fixed 1s interval). All `MpdCommand` variants silently discarded (see §4 — no command queue).
- **Connecting:** Socket connect attempt with 5s timeout (`TcpStream::connect_timeout`). On failure: wait 1s, retry. On success: transition to `Connected`. If another connect request arrives (settings change), the existing attempt is abandoned — `Reconnect` command sets an `abort_connect` flag.
- **Connected:** Socket alive. Idle loop running. Commands processed.

**Disconnect detection (within 1 second):**
- With idle protocol: a TCP socket death causes `recv` (blocked on `idle`) to return an error immediately — detection is sub-100ms.
- With polling fallback: 3 consecutive `fetch_full_update()` failures at 100ms intervals = 300ms. Well within the 1s target.
- On detection: immediately enter `Disconnected` state.

**Reconnect behavior (same for initial connect and reconnect):**
- Fixed 1s interval between retries (not exponential). No backoff ramp-up.
- **First 5 seconds:** Silent. No toast, no UI change beyond the connection indicator (green → yellow → red). The UI remains fully functional showing the last known state.
- **After 5 seconds:** Show "MPD disconnected" toast. UI shows disconnected state (connection indicator red, main panels show empty state with retry message).
- On successful reconnect: hide toast, restore full UI. Epoch counter increments — UI rejects stale `MpdEvent`s from the previous connection.

**Initial connect vs reconnect (the only difference):**
- On first startup, there is no "last known state" to display. Show "connecting..." immediately in the main panel.
- After 5 seconds of failed initial connection: show the connection settings panel with the error message.

**5a. Live Reconnect from Settings Change**

When the user changes MPD host/port/password in settings:

1. Settings dialog writes new params to `Arc<Mutex<(String, u16)>>`.
2. `MpdCommand::Reconnect` is sent to the MPD thread.
3. The MPD thread sets an `abort_connect` flag, drops the current socket (forcing `Connected` → immediate disconnect), and returns from `connected_loop`.
4. The outer state machine reads the updated params from the mutex and enters `Connecting`.
5. Epoch counter is incremented — any in-flight `MpdEvent` from the old connection will be rejected by the UI thread.
6. UI shows "Reconnecting..." indicator during the transition. Current playback state is cleared (was from old MPD). Queue is reloaded from new MPD on connect.
7. If the new connection fails (wrong host, port closed), the state machine enters `Disconnected` with backoff and the UI shows the connection settings panel with the error message.

**7. Shutdown**

- `ShuttingDown` flag on `AppState` (backed by `AtomicBool`). Set to `true` as the first step of the shutdown sequence.
- The `glib::idle_add` event handler checks `ShuttingDown` before processing `MpdEvent::Disconnected`. If shutting down, the event is silently dropped.
- The MPD thread and compute worker detect shutdown when their incoming channels close — they exit their event loops and terminate.
- **Responsive compute worker shutdown:** Each job in the compute worker's queue carries a `CancellationToken`. When `ShuttingDown` is set, the token is cancelled. Before starting each job, the compute worker checks the token — if cancelled, it skips the job and exits immediately. This prevents the worker from draining its entire queue during shutdown.

**8. Presenter Purity Rule (Canonical)**

- Presenters must not import `gtk4`, `gdk4`, or `gdk-pixbuf`. `glib` is allowed for string normalization.
- **Enforcement:** `gtk4` is a `[dev-dependency]` only. The library crate re-exports needed GTK types from `ui::gtk_reexport`. Presenter modules live under `src/presenters/` in the library crate — they cannot `use gtk4` because it's not in their dependency graph.
- **`presenters::types` module** is the bridge for non-GTK presentation outputs: format labels (`"DSD"`, `"Hi-Res"`), CSS class name strings, color hint enums, sort key strings.
- **`presenters::types` scope limits:** string type aliases, flat enums, integer/float newtypes, and small structs (≤3 fields, no behavior). No `impl` blocks beyond constructors, no color/layout/GTK-related types. The goal is to prevent GTK leakage, not to enforce arbitrary field counts.
- A presenter computes *what* to display. A widget decides *how* to display it (widget type, CSS class, layout).

**9. Widget Structure**

- **Complex widgets** (album grid, folder tree, queue views, now playing, cover display): use `widget_name/mod.rs` + `widget_name/imp.rs` subclass pattern.
- **Simple widgets** (search bar, toast overlay, settings dialog, connection indicator): single file `widget_name.rs` unless/until they grow complex enough to warrant the split.
- **Determining factor:** does the widget override `WidgetImpl` methods (`measure`, `snapshot`, `size_allocate`)? If yes → subclass pattern. If purely container + CSS composition → single file.

**10. Testing Strategy**

- **Mock MPD server** (`mpd/mock.rs` under `#[cfg(test)]`): lightweight TCP server on a random port, minimal protocol subset (`status`, `currentsong`, `playlistinfo`, `addid`, `delete`, `clear`, `next`, `previous`). Exposes control channels for simulating disconnections, slow responses, and error conditions.
- **Integration tests** (`tests/` directory): gated behind `#[cfg(feature = "integration-tests")]` (default off). Run against real MPD on localhost:6600.

**11. Operational Correctness**

**9a. Health-Check Strategy**

- **MPD connection health:** 3 consecutive `fetch_full_update()` failures → declare connection dead → trigger reconnect (proven in validation). Health check runs on every idle loop iteration (both idle protocol and polling fallback).
- **Cover cache health:** On startup, verify cache directory exists and is writable. If write fails, log warning, disable disk cache for the session (revert to in-memory only). Check cache integrity when writing: validate JPEG/PNG headers before replacing cache entry. Corrupt files are deleted and re-fetched.
- **Search index health:** On rebuild, verify index size against expected bounds (<5MB per 10K tracks). If index exceeds bound by 2x, log warning, clear and rebuild. Index corruption detected by checksum mismatch on load → trigger full rebuild.
- **Config health:** Parse validation on load. Invalid schema → rename to `.bad`, regenerate from defaults, notify user. Missing optional fields → filled with defaults silently.

**9b. Failure Budgets (Informational)**

Not enforced as SLAs — documented as engineering targets for sizing decisions:

| Resource | Budget | Monitoring |
|----------|--------|------------|
| MPD reconnect time | <5s (95th percentile) | Log reconnect duration at info level |
| UI freeze from MPD commands | <100ms per command | Log command duration at debug level (warn if >500ms) |
| Cover disk cache | 1GB max (configurable 100MB–5GB) | Log cache size on write, warn at 80% |
| Cover decode memory | <50MB working set for visible covers | Only visible covers kept in widget tree |
| Search index rebuild | <5s for 50K tracks | Log rebuild time at info level |
| Search index memory | <5MB per 10K tracks | Not monitored — design-time budget |
| Startup time | <3s cold, <1s warm | Log phase completion times at info level |

**9c. Diagnostics for Field Failures**

When a user reports a bug, the following diagnostics should be available:

- **Log file:** `~/.local/share/mpd-client/log/mpd-client.{ts}.log` — rotating (3 × 5MB). Info level captures: connection state changes, queue mutations, mode switches, library sync, cover fetch results. Debug level captures: MPD command round-trips, cache hits/misses, state machine transitions.
- **Connection diagnostics (info level):** connect success/failure with reason, reconnect attempts with backoff delay, idle protocol status (idle working / poll fallback / unknown command fallback), 3-consecutive-failure events.
- **Cover diagnostics (debug level):** albumart/readpicture result per album, cache hit/miss, BufReader lifecycle events, decode failures with image dimensions.
- **Crash signature:** Last 100 log lines + panic message. Captured by panic hook in `main.rs` — writes to `~/.local/share/mpd-client/crash-{ts}.log` before exiting.

**9d. Recovery from Corrupted State**

| Component | Detection | Recovery |
|-----------|-----------|----------|
| Config TOML | Parse failure on load | Rename to `.bad`, regenerate defaults, notify user |
| Session state TOML | Parse failure on load | Skip restore, log warning, start fresh |
| Cover cache file | Invalid JPEG/PNG header on read | Delete corrupt file, re-fetch |
| Cover cache directory | Write failure | Disable disk cache for session, use in-memory only |
| Search index | Checksum mismatch on load | Trigger full rebuild |
| MPD protocol desync | Unexpected response to idle/command | Increment epoch, force disconnect, reconnect

**12. Performance Architecture**

**10a. Large-Library Rendering Strategy**

The album grid uses `GtkGridView` with `GtkBuilderListItemFactory` (CoverGrid pattern). This is GTK4's virtualized grid — it only creates widgets for visible cells, not the entire library. No manual virtualization needed.

Key properties:
- **GtkGridView** creates widgets on-demand as cells scroll into view. Cells outside the viewport have no widget overhead.
- **GtkBuilderListItemFactory** binds from a `GioListStore` model — no per-cell widget construction in Rust code.
- **Album count:** 10K albums × 1 cell each → ~10K widgets maximum if all are visible (impossible — typical screen shows 20-30 covers). Memory pressure from grid cells is negligible.
- **Cover images:** Only visible cells hold decoded textures. On scroll, GTK4 recycles list items — old cells are reused, not accumulated.
- **Folder tree:** `GtkTreeView` with `TreeStore` — virtualized by GTK4 natively.

**Performance cliffs and mitigations:**

| Operation | Cost | Mitigation |
|-----------|------|------------|
| First album load (10K albums) | ~1s (MPD list album + per-album find) | Command list batching, background load |
| Group view switch (Albums → Artists) | 0ms (local cache) | Grouping cache — zero MPD round-trips |
| Cover decode on scroll | ~5ms per visible cover | One per idle cycle, non-blocking |
| Queue re-fetch (5000 items) | ~5ms parse, ~200KB data | plchanges incremental reduces to ~100 bytes |
| Search rebuild on library change | ~50ms for 10K albums | Skip if album count unchanged |
| Mode switch (Album ↔ Folder) | ~150ms (GTK widget swap) | Presenter swap, no MPD call |

**10b. Measurable Acceptance Thresholds**

| Test | Library Size | Target | Measurement |
|------|-------------|--------|-------------|
| Album list load | 10,000 albums | <2s | ListAlbums → Albums event time |
| Music directory listing | 10,000 entries | <2s | ListDirectory round-trip |
| Grouped album load | 10,000 albums | <2s | ListAlbumsGrouped round-trip |
| Cover grid scroll (fps) | 10,000 covers | 60fps stable | GTK frame profiler |
| Queue fetch (incremental) | 5,000 items | <10ms | plchanges round-trip |
| Queue fetch (full) | 5,000 items | <200ms | playlistinfo round-trip |
| Search query | 10,000 albums | <50ms | Search-to-results time |
| Search index rebuild | 10,000 albums | <5s | Rebuild time |
| Mode switch | Any | <500ms | Mode switch time |
| Startup (cold cache) | 10,000 albums | <3s | Total startup time |
| Memory (sustained) | 10,000 albums | <200MB | Resident memory after 5min idle |

**10c. Worst-Case Library Design Targets**

| Metric | Worst-Case | Degradation Behavior |
|--------|-----------|---------------------|
| Album count | 100,000 | Album list load 15-20s. Show spinner, don't block UI. |
| Queue size | 50,000 items | Full playlistinfo ~2MB, ~50ms parse. plchanges reduces to ~1KB. |
| MPD network latency | 200ms RTT | Command latency rises, throughput unchanged. Spinner for long ops. |
| Cover dimensions | 4096×4096 px | Decode ~50ms. Downscale on decode, never display at full resolution. |

**13. Enforcement**

- `scripts/check-patterns.sh` automates 6/8 enforcement rules:
  1. `cargo clippy -- -D warnings -D clippy::unwrap_used`
  2. No bare `unwrap()` in non-test source (grep check — transitional)
  3. No GTK imports in `src/presenters/` (grep check)
  4. `cargo test --lib` passes
  5. (covered by clippy) Widget directory convention — human review
  6. ErrorSink routing — human review (3 items total)
- **Cannot-be-automated rules** (widget convention, ErrorSink routing, presenters::types scope) are code review items.
- If a rule can't be mechanically enforced in `check-patterns.sh`, it must fit in short-term memory (5 items max).

### Elicitation-Driven Refinements (Rubber Duck Debugging)

The following gaps were identified by tracing complete user flows through the patterns at progressively detailed levels:

**1. First-Run Config Generation**

No config file exists on first launch. The current patterns treat missing config as a fatal error, but first-run is a normal condition.

- On startup, if `~/.config/mpd-client/config.toml` does not exist: write a default config with sensible values (localhost:6600, dark theme, album mode) and proceed. No error, no modal — the app just works.
- If the file exists but is corrupted: create a `.bad` backup, regenerate defaults, show a toast notification once: "Config file was corrupted. Reset to defaults. Old config backed up at config.toml.bad."
- Distinguish "file not found" (create defaults silently) from "file found but unreadable" (backup + regenerate + notify).

**2. Cover Request Widget Rule**

The album grid widget must debounce cover requests during scrolling (50ms after last scroll event) and only request covers for currently-visible grid cells. Explicitly forbidden: requesting covers for all albums in the loaded dataset. The widget controls request volume, not the compute worker.

**3. Dropped Command Feedback During Disconnect**

Commands sent while the MPD thread is disconnected must not silently fail. The `MpdCommand` sender checks a shared `AtomicBool` (`connected`) before dispatching. If disconnected, the dispatch returns an error → ErrorSink toast: "Not connected to MPD" (Recoverable). ErrorSink deduplicates identical messages within a 2-second window to avoid toast spam.

**4. Reconnect: Queue Re-Push and State Recovery**

After auto-stop on reconnect, the client's `QueueStore` (pre-disconnect) and MPD's playlist (now empty) are out of sync.

- Read the client's `QueueStore` and re-push all items to MPD via `addid` commands. Re-sync positions from MPD's response.
- Epoch guard: re-push only on epoch increment (full disconnect). Transient blips keep the queue state.
- Right rail shows: "Reconnected. Your queue has been restored." (non-modal, auto-dismisses on user interaction or 8s).

**5. Shutdown Phase Ordering**

Revised shutdown sequence: `ShuttingDown` flag set → **persist state** (read AppState.mode, write to config) → UI teardown → MPD disconnect → compute worker stop → logging flush. State persist happens immediately after shutdown signal, not embedded in the middle of teardown phases.

### Elicitation-Driven Refinements (User Persona Focus Group)

The following adjustments were identified by evaluating the architecture against two target user personas (Alex "The Collector" — Album Mode, Jordan "The Checker" — Folder Mode):

**1. Mode-Aware Search Track Cap**

The fixed 100-track result cap (originally designed for Album Mode's track-scoped results) is too restrictive for Folder Mode, where searches are often format-based queries (e.g., "DSD", "24/192") that legitimately match many tracks.

- Album Mode: keep 100-track cap (artist/album/year/genre results are the primary path; track results are secondary).
- Folder Mode: raise cap to 500 tracks (folder mode searches are file- and format-driven; tracks are the primary result type).
- Configurable via config file: `search_track_cap_album = 100`, `search_track_cap_folder = 500`.

**2. Hover Button Minimum Size**

Hover buttons (play, queue, insert) on album grid cards must have a minimum interactive area of 24×24px for reliable mouse targeting. Documented in widget CSS guidelines — the `GtkButton` child inside the `GtkOverlay` must set `width-request` and `height-request` to at least 24.

**3. Normalized Entry Visual Indicator**

Folder Normalizer strategies that collapse multi-file structures (cue sheets, DSD folders) into a single logical entry must provide a visual indicator in the folder tree:

- A "Normalized" badge or distinct indent/icon on collapsed entries.
- The badge must be discoverable but subtle — a small dimmed label (e.g., "Normalized — cue sheet") shown on hover or selection.
- SVG icon prefix consideration: consider a small "stack" or "folder-merge" icon for v2; plain text badge for v1.

## Project Structure & Boundaries

### Complete Project Directory Structure

```
mpd-client/
├── Cargo.toml
├── Cargo.lock
├── .gitignore
├── CLAUDE.md
├── README.md
├── MIGRATION.md
├── src/
│   ├── main.rs
│   ├── errors.rs
│   ├── constants.rs
│   ├── state/
│   │   └── mod.rs
│   ├── mpd/
│   │   ├── mod.rs
│   │   ├── state_machine.rs
│   │   └── mock.rs             # [cfg(test)]
│   ├── queue/                  # [future] QueueStore extracted from state
│   ├── presenters/             # [future] Separate presenter layer
│   ├── coverart/
│   │   ├── mod.rs
│   │   ├── providers.rs        # [future] AlbumArtProvider + ReadPictureProvider
│   │   └── caches.rs           # [future] Disk cache, LRU, rate limiter
│   ├── search/
│   │   └── mod.rs
│   ├── layout/                 # [future] LayoutService
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── gtk_reexport.rs
│   │   └── widgets/
│   │       ├── mod.rs
│   │       ├── album_cover.rs
│   │       ├── folder_tree.rs
│   │       └── toast.rs
│   ├── config/
│   │   └── mod.rs
│   ├── ipc/                    # [future] Unix socket + MPRIS
├── tests/
│   ├── common/
│   │   └── mod.rs
│   └── smoke_test.rs
```

### Requirements to Structure Mapping

| FR Category | Directory | Key Files |
|-------------|-----------|-----------|
| Playback (4 FRs) | `src/mpd/` | `mod.rs`, `state_machine.rs` |
| Queue (8 FRs) | `src/state/` | `mod.rs` (QueueStore inline) |
| Browsing (10+ FRs) | `src/ui/` | `mod.rs`, `widgets/album_cover.rs`, `widgets/folder_tree.rs` |
| Layout (6 FRs) | `src/constants.rs` | (constants only; LayoutService is `[future]`) |
| Cover Art (7 FRs) | `src/coverart/` | `mod.rs` |
| Search (8+ FRs) | `src/search/` | `mod.rs` |
| Drag & Drop (7 FRs) | `src/ui/` | `mod.rs` |

### Architectural Boundaries

**Thread boundaries (3 threads):**
- GTK main loop: `src/ui/`, `src/app.rs`, `src/state/` — UI rendering, state mutations. **Not `Send`** (GTK widgets are main-thread only).
- MPD IO thread: `src/mpd/` — TCP, state machine, command dispatch. **Must be `Send`** — no `RefCell` or non-`Send` types.
- Compute worker: `src/coverart/`, `src/search/` — blocking IO + CPU work. **Must be `Send`**.

**Channel boundaries (typed channels between threads):**
- `mpd::MpdCommand` → `mpsc::Sender` → MPD IO thread → `mpsc::Receiver<mpd::MpdEvent>` → main loop
- `coverart::CoverJob` → `mpsc::Sender` → compute worker → `mpsc::Receiver<coverart::CoverResult>` → main loop
- `search::SearchQuery` → `oneshot::Sender` → compute worker → `oneshot::Receiver<search::SearchResult>` → main loop
- `errors::ErrorSinkEvent` → `mpsc::Sender` → main loop `glib::idle_add` consumer

**Module import rules (enforced by convention + check-patterns.sh):**
- `mpd/` must **never** import from `ui/`, `presenters/`, or `layout/` — IO thread to GTK dependency causes runtime crashes.
- `presenters/` must **never** import `mpd/` directly — goes through `state/` or `queue/`. Coupling presentation to transport breaks testability.
- `app.rs` must remain **thin** — wiring only (create state, create presenters, create window, connect signals). No queue manipulation, filtering, or layout calculations.
- Layout ownership: `layout/` owns shell split and rail proportions. Widgets (`now_playing`, etc.) never set their own container size — the `workspace` orchestrator applies proportions.

**Presenter/UI boundary (no GTK imports in presenters):**
- `src/presenters/` — pure functions, no `gtk4` dependency. All projection logic. Split into `browse/` and `queue/` subdirectories.
- `src/ui/` — GTK widgets, imports presenter ViewModels, never performs projection.
- `src/ui/gtk_reexport.rs` — re-exports GTK types needed by widget code only.
- `src/presenters/types.rs` — non-GTK output types (format labels, CSS class names).
- **Search strategy:** `search_bar` widget renders the input. Mode-specific presenter injects the query implementation — Album Mode fuzzy-matches artist/album metadata, Folder Mode matches filenames and paths. Search logic is not baked into the widget.

**Mode switch lifecycle:**
- `workspace.rs` orchestrator owns the active set of widgets and handles mode transitions.
- On mode switch: widgets are **hidden/detached**, not destroyed — preserves browsing context (scroll position, tree depth, selected item).
- A destroyed `folder_tree` on mode switch back would lose the user's place; the orchestrator prevents this.

### File Organization Patterns

**Complex widgets** (override WidgetImpl): `widget_name/{mod.rs, imp.rs}`
**Simple widgets** (CSS composition only): `widget_name.rs`
**Per-module errors**: each domain module defines its own `pub enum Error` via `thiserror`
**Module docstrings**: every `mod.rs` (and directory-level `mod.rs`) must have a `//!` docstring explaining what lives in the module and what doesn't. This prevents scope creep (developers adding files to the nearest directory rather than the right one).
**Thread annotation**: each of the five critical modules (`mpd/`, `state/`, `presenters/`, `ui/`, `queue/`) documents its thread affinity in its `mod.rs` docstring.
**Tests**: unit tests as `#[cfg(test)] mod tests` at bottom of each module; integration in `tests/`; shared test helpers in `tests/common/mod.rs`.
**Resources**: embedded via `gio` resource system (`build.rs` + `.gresource.xml`)
**Layout constants**: shell split ratio, rail width, section proportions live in `src/constants.rs` — not scattered across `app.rs`, `presenters/`, and `ui/`.

### Integration Points

**External integrations:**
- MPD server at `localhost:6600` (TCP, text protocol)
- Online cover art: MusicBrainz → Discogs → Last.fm (HTTP, rate-limited)
- MPRIS D-Bus: `org.mpris.MediaPlayer2` interface on session bus
- Second instance IPC: Unix socket at `~/.cache/mpd-client/lock`

**Startup phase order:**
1. Config load (generate defaults on first run)
2. Logging init
3. MPD connect (5s timeout; failure = fatal modal)
4. State restore (last mode from config)
5. UI build (window, widgets, CSS load)
6. Cover art service start (non-essential; skip on timeout)

## Architecture Validation Results

### Coherence Validation

**Decision Compatibility — ✅ PASS**

All technology choices are compatible:
- GTK4 0.11 (Rust bindings) + Rust 2024 edition + MSRV 1.85 — no version conflicts
- `ureq` + `rustls` (no OpenSSL dep) — compatible with Linux-only target
- `image` crate WebP feature + GTK4's `gdk-pixbuf` — separate decode paths, no conflict
- MPD TCP text protocol + `std::net::TcpStream` — no async runtime required
- No dependency on tokio/async-std — GTK main loop + `std::thread` is the concurrency model

**Pattern Consistency — ✅ PASS**

Architecture decisions and implementation patterns are consistent:
- ADRs define *what* (decisions), patterns define *how* (consistency rules) — no overlap
- Per-module errors (ADR) → per-module `thiserror` enums (pattern) → `ErrorSink` channel (pattern) — consistent
- Three-tier error model (ADR) → Recoverable/Retryable/Fatal (pattern) — consistent
- MPD background thread (ADR) → typed channels (pattern) → thread labeling in structure — consistent
- Presenter purity (ADR) → `gtk4` as dev-dep (pattern) → `gtk_reexport.rs` (structure) — consistent

**Structure Alignment — ✅ PASS**

The project structure supports all architectural decisions:
- 11 module directories map to 11 architectural concerns — no orphaned concerns
- `presenters/browse/` + `presenters/queue/` reflect the dual-mode architecture
- `workspace/{mod.rs, imp.rs}` provides the mode orchestrator the UX requires
- Thread labeling in module docstrings supports the 3-thread model
- `tests/common/mod.rs` supports the mock-MPD testing strategy

### Requirements Coverage Validation

**Functional Requirements Coverage — ✅ PASS**

| FR Category | Architecture Coverage | Key ADR/Pattern |
|-------------|----------------------|-----------------|
| Playback (4 FRs) | MPD adapter with command/event channels, state machine | MPD Adapter ADR |
| Queue (8 FRs) | QueueStore with mutations, sync, re-push on reconnect | Queue + Track Identity ADRs |
| Browsing (10+ FRs) | Dual presenters (Album + Folder), FolderNormalizer strategies | Presenter patterns |
| Layout (6 FRs) | LayoutService, breakpoints, 70/30 Paned, 150ms debounce | Layout ADR |
| Cover Art (7 FRs) | Layered provider chain, LRU session cache, priority queue | Cover Art ADR |
| Search (8+ FRs) | Omnibox, local in-memory index, mode-aware cap (100/500) | Search ADR + Persona refinement |
| Drag & Drop (7 FRs) | Unified DndService, cross-mode insertion, MIME type | Drag & Drop ADR |

**Non-Functional Requirements Coverage — ✅ PASS**

| NFR Category | Coverage |
|-------------|----------|
| Performance (<2s library, <100ms interaction, <50ms search, <200MB RAM) | Addressed by: lightweight index, single-crate build, compute worker thread, session cache budget (150MB cover, 5MB search, 30MB widget, 15MB state) |
| Reliability (>99.5%, <5s reconnect, <1s queue sync) | Addressed by: exponential backoff reconnection, epoch-gated re-push, auto-stop on reconnect, queue sync via idle loop |
| Accessibility (≤3-click, WCAG 2.1 AA, keyboard nav) | Addressed by: hover controls + double-click access, GTK4 AT-SPI tree, keyboard grid model, high-contrast CSS |
| Compatibility (MPD ≥0.19, PCM/DSD, >95% cue) | Addressed by: protocol version probing, graceful degradation matrix, FolderNormalizer strategies |

### Implementation Readiness Validation

**Decision Completeness — ✅ PASS**
- 19 Architecture Decision Records, each with rationale and explicit trade-offs accepted
- All technologies specified with version constraints
- Tech stack table with dependency list and MSRV
- Cross-cutting concerns mapped (privacy, thread safety, memory budget)

**Structure Completeness — ✅ PASS**
- Complete file tree with all source files, tests, scripts, resources
- Every file has a one-line purpose annotation
- Thread boundaries, channel boundaries, and import rules documented
- Mode orchestrator lifecycle documented

**Pattern Completeness — ✅ PASS**
- 9 implementation patterns defined with canonical rulings
- Enforcement guidelines with automated check script (6/8 rules)
- Concrete good examples and anti-patterns
- All refinements consolidated with authoritative-over-ADR status

### Gap Analysis

**Critical Gaps — None**
- All architectural decisions are documented with rationale and trade-offs
- All FR categories are covered by at least one ADR or pattern
- Project structure is complete and unambiguous

**Important Gaps — None**
- Pattern refinements are consolidated into a single authoritative section
- Cross-reference notes added to affected ADRs
- "Where do I put X?" answers are unambiguous

**Nice-to-Have Gaps:**
- `tests/common/mod.rs` needs to be created before first integration test (noted in structure — it's an empty scaffold)
- Module docstrings (`//!` comments) are required by convention but not yet written (they're implementation-phase work)
- The `MIGRATION.md` file for workspace→single-crate collapse is defined in the tree but not yet populated with `git mv` commands

### Architecture Readiness Assessment

**Overall Status:** READY FOR IMPLEMENTATION

**Confidence Level:** HIGH (for agent consistency and structural clarity)

**Assumptions / Risks:**
- **Memory budget (200MB) is untested.** The estimated 15KB per WebP 300px thumbnail may be optimistic — real covers may be 25-40KB, pushing the 10K-entry session cache from 150MB to 250-400MB. Tune during Milestone 3/4 (album grid with covers). Adjust cache size or entry count downward if needed.
- **gtk4-rs 0.8 → 0.11 migration** is required. The existing workspace code uses gtk4 0.8; the architecture specifies 0.11 with `v4_14` feature flag. This is a breaking change. See Pre-Milestone 0 below.

**Key Strengths:**
- Single source of truth: one `architecture.md` file covers all decisions, patterns, and structure
- Unambiguous module boundaries: import rules, thread affinity, and scope limits are documented, not assumed
- AI agent ready: enforcement script, anti-patterns, and concrete examples prevent inconsistency
- Cross-referenced: ADRs point to refinements; refinements override ADRs; no silent contradictions

**Areas for Future Enhancement:**
- Module docstrings (implementation-phase task, tick before first PR)
- Migration from current workspace to single crate (one-time `git mv` operation)
- Integration test scaffolding (`tests/common/mod.rs` — create before first integration test)

### Implementation Handoff

**AI Agent Guidelines:**
- Follow all ADRs and patterns as documented in this file
- Where Consolidated Refinements conflict with original ADRs, the Consolidated Refinements are authoritative
- Respect module import rules: `mpd/` never imports `ui/`; `presenters/` never imports `mpd/` directly
- Place widget tests inline as `#[cfg(test)]`; integration tests in `tests/` with feature gate
- Run `scripts/check-patterns.sh` before marking any implementation task complete
- Keep `app.rs` thin — wiring only, no business logic
- All `mod.rs` files must have `//!` docstrings explaining module scope and thread affinity

**First Implementation Priority:**

**Pre-Milestone 0 — gtk4-rs upgrade:** Upgrade gtk4-rs from 0.8 (current workspace) to 0.11 with `v4_14` feature in the existing workspace. Fix any API breakage. Verify compilation. Then restructure from workspace to single crate.

**Milestone 0 — "It Opens":** Validate GTK4-rs toolchain — window appears with title and default size.

**Milestone 1 — "It Connects":** MPD protocol via TCP — connect, send status, parse response, print to console.

**Milestone 2 — "The Split Shell":** GTK Paned window with 70/30 split and placeholder labels.

**Milestone 3 — "It Connected in the Shell":** Background thread connects to MPD, pipes status into the right panel via `glib::idle_add`. Validates full data path: socket → thread → channel → GTK main loop → widget.

**Milestone 4 — "Something in the Grid":** MPD album list → `GtkFlowBox` with album title labels. `LayoutService` breakpoints working on resize.
