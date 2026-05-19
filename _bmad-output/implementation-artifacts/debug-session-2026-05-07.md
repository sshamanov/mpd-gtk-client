# Debug Session 2026-05-07

## Versioning terminology cleanup — project-wide

### User request
"check project documentation for project status like versioning. it should be clearly stated there is not V2, it is only 'release', all undone tasks could be only two logical statuses: scoped or out of scope"

### Rules established
- No "V1", "V2", "V3", "pre-V1", "post-V1" terminology
- Only "release" — one release
- Tasks: only "scoped" (planned) or "out of scope" (excluded, historical)
- No "deferred" — it's a grey zone
- Descoped tasks stored for history
- Versioning policy must be in both CLAUDE.md and PRD.md, identical, under "Versioning Policy"

### Files changed

**CLAUDE.md:**
- "v1 implementation complete" → "Initial release implementation complete"
- "v2 addendums" → "scoped feature addendums" (all occurrences)
- "Deferred work" → "Out of scope (historical)" in all references
- Added "Versioning Policy" section (scoped/out-of-scope table + no-version-numbers statement)

**PRD.md:**
- "Implementation Notes (v2 — 2026-04-28)" → "Scoped Features (2026-04-28)"
- All "v1"/"v2" version tags removed from section headers and body
- "Before (v1)" / "After (v2)" → "Before" / "After"
- "previously deferred" → "previously out of scope"
- "Known Limitations & Deferred Work" → "Known Limitations & Out of Scope"
- "Advanced Features (Post‑v1)" → "Advanced Features (Out of Scope)"
- "in v1" qualifiers removed from interaction rules
- "v2+" → "out of scope"
- Added "Versioning Policy" section (identical to CLAUDE.md)

**architecture.md:**
- "V2 refinements" → "Scoped refinements"
- ADR headers: "v2 Refinement" → "Scoped Refinement", "v3 refinement" → "proposed", "v1 — superseded by v2" → "superseded by..."
- Bulk replacements: "in v1" → "in the release", "for v1" → "for the release", "out of scope for v1" → "out of scope for the release", "post-v1" → "out of scope", "deferred" → "out of scope"
- Fixed double-replacement artifacts ("out of scope to out of scope", "Deferred to out of scope")
- "V1 Approach" → "Current Approach", "V1 scope" → "Scoped"
- "Deferred Post-V1" → "Out of Scope"
- "v1 use case" → "current use case"

**deferred-work.md:**
- Complete restructure after user review of all 36 items
- "Deferred Work (Current)" → split into "Scoped Work" and "Out of Scope (Historical)"
- User reviewed each item individually, classified as scoped or out of scope

### Deferred work item review — user decisions

User reviewed all 36 items from deferred-work.md. Key decisions:

**Scoped (15 items):**
Epics 25 (idle protocol), 26 (metadata caching), 27 (responsive right rail), 20 (desktop file), 21 (profiling). Architecture items: idle protocol, KeybindingService, undo stack, theme switcher, CoverProvider+ActualRead pipeline. Bugs: queue key stale item_ids, synchronous grid populate, search_albums stale artist, MpdAdapter no Drop, SIGINT cleanup, D-Bus reconnection, config double-load, BackSpace ListDirectory("").

**Out of scope (21 items):**
Design decisions: three-tier persistence, keybinding customization, packaging/CI, plugins, i18n, RTL, touchscreen, workspace crates, tray icon. Resolved: dead connection detection, Reconnected dead code, settings reconnect, search index rebuild, cover path dead code, toast race, AlbumArtist search, grouped double-click (cannot reproduce), Cue/DSD no-ops (fixed), 30ms timer (replaced with frame clock), list_albums_grouped artist loss (cannot reproduce), InsertNext stale pos (cannot reproduce), MPRIS PropertiesChanged (code exists), SharedState playback fields (now populated). Skipped: ExponentialBackoff Clone, Unicode render.

### User corrections
1. First attempt renamed file sections prematurely — user said "you did it wrong, deferred work are for review, give me a list"
2. Items 19, 22, 24, 25: user confirmed cannot reproduce, moved to out of scope
3. Items 34, 35: user identified as real bugs, moved to scoped
4. Section name must be "Versioning Policy" in both files (not "Task Statuses")

---

## Right Rail — Starting Point Analysis

### Request
"Describe structure, architecture and flow, look into code, architecture, prd, tasks and debug session notes. Document starting point for debug session right rail, we will work on it"

### 1. Structure — Widget Tree

The right rail is a `GtkBox` (vertical, spacing 0) named `right_pane`, attached as the end child of a `GtkPaned` (horizontal) that splits the window 70/30 left/right.

```
GtkWindow
└── adw::ToastOverlay
    └── main_paned (Paned, Orientation::Horizontal)
        ├── start_child: left_pane_box (Box, vertical)
        │   ├── mode_content (contains mode stack + group bar)
        │   ├── album search entry
        │   └── left_stack (album grid / empty / loading)
        └── end_child: right_pane (Box, Orientation::Vertical, spacing: 0)
            ├── conn_indicator (Box, 4px height, connection state dot)
            ├── now_playing (Box, Orientation::Vertical, spacing: 6)
            │   ├── np_cover (Picture, 120x120, centered, initially hidden)
            │   ├── playback_icon (Label, unicode play/pause/stop)
            │   ├── track_title (Label, ellipsized)
            │   ├── track_artist (Label, ellipsized)
            │   ├── track_album (Label, ellipsized)
            │   ├── track_year (Label, css: "track-year")
            │   ├── time_display (Label, "MM:SS / MM:SS")
            │   ├── seekbar (Scale, horizontal, css: "seekbar")
            │   └── format_badge (Label, css: "format-badge", initially hidden)
            ├── track_win_label (Label, "Album Tracks", css: "queue-header")
            ├── track_win_scroll (ScrolledWindow, max_content_height: 180)
            │   └── fc_track_win (ListBox, Single selection)
            ├── queue_label (Label, "Queue", css: "queue-header")
            └── fc_queue_stack (gtk4::Stack)  ← mode-aware switching
                ├── mini_scroll (ScrolledWindow)  ← Album Mode
                │   └── mini_grid_view (GridView, min_columns:1, max_columns:3)
                └── queue_scroll (ScrolledWindow, vexpand)  ← Folder Mode
                    └── fc_ql (ListBox, Single selection)
```

**Key observations:**
- The right_pane is a flat `GtkBox` — no GtkPaned for internal proportions. The PRD-specified 40/20/40 (Album) and 55/45 (Folder) proportions are NOT enforced by widget layout. They are "starting values" and guidance only.
- `track_win_scroll` has `max_content_height(180)` as a hard cap — the current album track window is height-limited rather than proportion-based.
- `queue_scroll` has `vexpand(true)` — in both modes the queue section greedily takes remaining space.
- The `queue_stack` switches between mini grid (Album) and track list (Folder) via the frame clock tick callback detecting mode changes.

### 2. Architecture — Design Decisions

#### 2.1 Shell Split
- `GtkPaned` with `70/30` ratio as default (constant: `SHELL_SPLIT_RATIO = 0.7`)
- Configurable via settings dialog slider (range 0.5–0.9, step 0.025)
- Right rail width targets: `clamp(320px, 30vw, 420px)` per PRD, but this is aspirational — the current implementation uses only the Paned split ratio, no explicit min/max width enforcement

#### 2.2 Dual Queue Presentation (ADR: Shared Queue with Dual Presentation)
The architecture specifies a linear `QueueStore` with stateless computed projections:
- `AlbumQueuePresenter::project(&QueueStore) -> AlbumGridViewModel`
- `TrackQueuePresenter::project(&QueueStore) -> TrackListViewModel`

**Actual implementation:** These presenters do NOT exist as separate modules. The projection logic is inlined in `src/ui/mod.rs`:
- Mini grid: `MpdEvent::Queue` handler groups by album, deduplicates, populates `mini_grid_data` RefCell + `mini_grid_model` ListStore
- Track list: same Queue handler builds `GtkListBoxRow` widgets directly

This means projection logic is mixed with GTK widget code, violating the architecture's Presenter Purity Rule. No `src/presenters/` directory exists.

#### 2.3 Mode Adaptation
Mode switching triggers:
1. Left pane: `mode_stack.set_visible_child()` swaps album grid ↔ folder tree
2. Right rail: frame clock tick callback detects `fc_prev_mode != cur_mode` and calls `fc_queue_stack.set_visible_child()` to swap mini grid ↔ track list
3. `AppState.mode` updated via `state.write()`

The right rail's Now Playing section is **mode-invariant** — same widgets for both modes. The "Album Tracks" section is **always visible** (not hidden in Folder Mode per PRD 55/45 spec).

#### 2.4 Cover Art in Right Rail
Three update paths, all handled in `src/ui/mod.rs`:
- **CoverPaths event:** Updates `cover_paths` HashMap, mini_cover_widgets registry, now-playing cover
- **CoverRefreshed event:** Decodes raw JPEG bytes, scales to 200x200, updates grid + mini + now-playing
- **NowPlaying state change:** Looks up cover from `cover_paths` HashMap by album suffix match

#### 2.5 LayoutState (in SharedState)
```rust
pub struct LayoutState {
    pub shell_split: (f64, f64),       // (0.7, 0.3) default
    pub right_rail_width: f64,         // RAIL_WIDTH_MIN (320.0)
    pub album_mode_proportions: (f64, f64, f64),  // (0.4, 0.2, 0.4)
    pub folder_mode_proportions: (f64, f64),       // (0.55, 0.45)
}
```
These values exist in state but are **not wired** to actual widget layout. The `right_rail_width` and internal proportions are stored but not enforced by any layout logic.

### 3. Flow — Event & Data Flow

#### 3.1 Queue Population Flow
```
MPD playlistinfo response
  → MPD IO thread parses QueueEntry list
  → MpdEvent::Queue(Vec<QueueEntry>)
  → Frame clock tick processes up to 64 events/batch
  → Queue handler in ui/mod.rs (~line 2350):
      1. fc_ql.remove_all() (track list clear)
      2. Build per-track ListBoxRow widgets with context menu
      3. Highlight current track row
      4. Update item_ids for Delete key lookup
      5. Populate mini grid (album dedup, fc_mini_data, fc_mini_model)
      6. Set fc_mini_current_album for highlight
```

#### 3.2 Now Playing Update Flow
```
MPD status/currentsong response
  → PlaybackUpdate struct (state, song, artist, title, album, elapsed, duration, format, volume, year)
  → MpdEvent::StateChanged(update)
  → Frame clock tick handler:
      1. Track album change detection (fc_current_album)
      2. If album changed: send ListAlbumTracks command
      3. update_now_playing() → writes all GTK labels, cover, seekbar, format badge
      4. Populate SharedState.current (Track + Album) for MPRIS
      5. Forward to MPRIS PropertiesChanged emitter
```

#### 3.3 Cover Art in Right Rail Flow
```
CoverPaths event:
  cover_paths.insert(album_key, path)
  → Pixbuf::from_file_at_size(path, 200, 200)
  → Texture stored in cover_texture_cache
  → Update: grid widget, mini widget, now-playing (if current album)

CoverRefreshed event:
  Raw JPEG bytes → Pixbuf::from_read() → scale_simple(200, 200)
  → Same widget update path as CoverPaths

Now Playing cover lookup (StateChanged):
  cover_paths.iter().find(key ends with "||album_name")
  → cover.set_filename(path)  // only on StateChanged, not on cover events
```

#### 3.4 Mode Switch Flow
```
User action (button toggle or Ctrl+1/Ctrl+2):
  1. Save current mode scroll position to SharedState
  2. mode_stack.set_visible_child(new_mode_content)
  3. Restore target mode scroll position / folder expansion
  4. SharedState.mode = new_mode
  5. (Next tick) Frame clock detects mode change:
     fc_queue_stack.set_visible_child()  // Album→mini_scroll, Folder→queue_scroll
```

#### 3.5 Cover Kill Switch
`COVER_PUSH_ENABLED: AtomicBool` at `src/ui/mod.rs:22`. When false, CoverPaths and CoverRefreshed event handlers are no-ops. FetchCovers command handler logs and returns without fetching. Currently **set to true** (enabled).

### 4. Current State — What Works

| Feature | Status | Notes |
|---------|--------|-------|
| Shell split (70/30) | Working | Draggable separator, configurable in settings |
| Now Playing metadata | Working | Title, artist, album, year, time, format badge |
| Now Playing cover | Working | Via CoverPaths/CoverRefreshed events |
| Seekbar | Working | `connect_change_value` → MPD Seek command |
| Album Tracks window | Working | Lists current album tracks, click to jump, scroll resets on track change |
| Mini cover grid (Album) | Working | GridView 1-3 cols, album dedup, current highlight, double-click to play |
| Track list queue (Folder) | Working | ListBox with context menu (Play Now, Play Next, Remove) |
| Mode-aware queue swap | Working | Stack switches mini↔list on mode change |
| Queue context menu | Working | Right-click: Play Now, Play Next, Remove |
| Drag to queue | Working | DropTarget on queue_stack for album adds |
| Delete key removal | Working | EventControllerKey on queue_list |
| Shift+Up/Down reorder | Working | Optimistic update pattern for rapid keypresses |
| Connection indicator | Working | Color-coded dot: green/yellow/red/gray |

### 5. Known Issues & Gaps

#### 5.1 Architecture Debt
- **No presenter layer**: Projection logic is inline in `ui/mod.rs` rather than in `src/presenters/`. The architecture mandates pure-function presenters with no GTK imports, but the current implementation has all queue→widget logic in the GTK event handler.
- **LayoutState not enforced**: `right_rail_width`, `album_mode_proportions`, `folder_mode_proportions` are stored in AppState but not wired to any widget sizing.

#### 5.2 PRD Gaps
- **Internal proportions not enforced**: The 40/20/40 (Album) and 55/45 (Folder) vertical proportions are specified but the current layout uses a flat Box with max_content_height on the track window and vexpand on the queue — no proportional allocation.
- **Current Album hidden in Folder Mode**: The PRD says Folder Mode is 55% Now Playing / 45% Queue. The "Album Tracks" label and scrolled window are always visible regardless of mode.
- **Queue header label is static "Queue"**: Doesn't change to "Album Queue" vs "Track Queue" per mode.
- **Responsive breakpoints not implemented**: No tablet/single-column fallback (epic 27, backlog).

#### 5.3 Disabled Features
- **Queue drag reorder disabled** (line ~1855): `// queue_list.add_controller(reorder_target);` — Shift+Up/Down keyboard reorder works, but mouse drag reorder is commented out.
- **Drag-off removal disabled** (line ~1690): `// right_pane.add_controller(removal_target);` — removing items by dragging them off the queue is disabled.

#### 5.4 Code Smells
- **`_cover_provider` and `_actual_read`** in state_machine.rs are created but never used — dead code from the planned cover pipeline.
- **Queue polling at 300s** (line 1871): Originally 30s, stretched 10x for debugging. This means queue state can be up to 5 minutes stale from external MPD changes.
- **Mini grid data in RefCell**: `MiniGridData = Rc<RefCell<Vec<MiniGridItem>>>` — mutable state accessed from GTK callbacks, no ChangeDetection pattern.

### 6. Scoped Work Items

From sprint-status.yaml, the only backlog story touching the right rail:

- **27-1-responsive-right-rail** (epic 27, backlog): Replace GtkPaned with Adw.MultiLayoutView + Adw.BottomSheet for responsive breakpoints. On narrow windows (<800px), right rail becomes a toggleable bottom sheet.

Other right-rail-relevant items from deferred-work.md review (scoped):
- Queue key stale `item_ids` bug (scoped bug)
- KeybindingService (scoped) — would affect queue keyboard shortcuts
- CoverProvider+ActualRead pipeline (scoped) — cover art in right rail

### 7. Debug Session Notes from Prior Sessions (Relevant to Right Rail)

**2026-05-06 (ui-architecture-analysis.md):**
- "Tried to remove non-child" freeze root cause: `ListBox.remove()` in a `while first_child()` loop was replaced with `remove_all()` at 4 locations including `fc_track_win` (×2), `fc_fs_list`, `fc_ql`
- Widget tree documented showing right_pane hierarchy
- Remaining risk: Queue list (`fc_ql`) flicker during batch event processing — not crash-prone but visual glitch possible

**2026-05-05:**
- Cover art updates: `cover_widgets` cleared on every `batch_populate` — all cover Picture widgets dropped and recreated in bind callbacks. Wasteful but not crash-prone.

**2026-05-02:**
- 57 albums without embedded art show colored placeholders
- Composite keys (artist||album) prevent cover collisions
- Now-playing and album queue covers update correctly

### 8. Key Files

| File | Relevance |
|------|-----------|
| `src/ui/mod.rs` | All right rail widget construction (lines 1167-1857), event handling (lines 1986-2588), now-playing update (lines 2633-2679) |
| `src/state/mod.rs` | AppState, LayoutState, Store — state that backs the right rail |
| `src/constants.rs` | SHELL_SPLIT_RATIO, RAIL_WIDTH_MIN/MAX, ALBUM_MODE_RAIL, FOLDER_MODE_RAIL |
| `src/ui/style.css` | Mini grid styles, queue-current, queue-header, format-badge, seekbar |
| `src/mpd/state_machine.rs` | MpdCommand::ListQueue, MpdEvent::Queue, MpdCommand::ListAlbumTracks |
| `_bmad-output/planning-artifacts/prd.md` | Right rail layout spec (§Layout), queue design (§Playback And Queue Design) |
| `_bmad-output/planning-artifacts/architecture.md` | ADRs: Shared Queue with Dual Presentation, Now-Playing Consolidation, Drag & Drop, Threading Model |
| `_bmad-output/implementation-artifacts/27-1-responsive-right-rail.md` | Story spec for responsive right rail (backlog) |
| `_bmad-output/implementation-artifacts/10-3-album-mini-grid-queue.md` | Completed story for mini cover grid |
| `_bmad-output/implementation-artifacts/3-1-queue-display.md` | Completed story for initial queue display |
| `_bmad-output/implementation-artifacts/ui-architecture-analysis.md` | Widget hierarchy diagram, remove() bug root cause, remaining risks |
| `_bmad-output/implementation-artifacts/sprint-status.yaml` | Epic 27 (responsive right rail) is backlog |
