## Deferred from: code review of 1a-1-mpd-connection-and-app-shell (2026-04-25)

- 1-second polling instead of MPD idle command [src/mpd/state_machine.rs] — deferred, fixed in v1: 500ms independent polling
- BufReader wraps cloned file descriptor — potential desync [src/mpd/mod.rs] — deferred, pre-existing: works in practice under normal request-response patterns
- Unicode symbols in GTK labels may not render on all systems [src/ui/mod.rs] — deferred, pre-existing: portability concern, not a correctness bug

## Deferred from: code review (post-v1-validation, 2026-04-27)

### Architecture & Threading
- Dead MPD connection never triggers reconnect — background thread loops on errors [src/mpd/state_machine.rs:242] — deferred: requires connection health-check redesign
- `MpdEvent::Reconnected` is dead code — state machine never emits it [src/mpd/state_machine.rs:43, src/ui/mod.rs:626] — deferred: Connected handles both cases
- `MpdAdapter` has no `Drop` — no clean MPD close on shutdown [src/mpd/mod.rs:81-84] — deferred: socket closes on process exit
- SIGINT cleanup via `process::exit(0)` skips graceful MPD disconnect [src/main.rs:44-60] — deferred: idle_add defers exit; full cleanup needs GTK lifecycle work

### UI & Interaction
- Queue key handler uses stale `item_ids` after rapid Shift+Up/Down [src/ui/mod.rs:508-536] — deferred: race window <10ms in practice; Queue event rebuilds map
- Toast auto-dismiss race between overlapping `show_toast` calls [src/ui/widgets/toast.rs:62-66] — deferred: rare in practice; needs timer cancellation
- `glib::timeout_add_local` at 30ms can starve GTK main loop under heavy load [src/ui/mod.rs:594] — deferred: 64-event batch limit mitigates; full fix needs virtualization
- `populate_album_grid` and `populate_grouped_grid` run synchronously on GTK thread [src/ui/mod.rs:808-850] — deferred: large libraries (>10k albums) may freeze UI
- Double-click in grouped mode plays wrong album — FlowBox child index includes headers but `album_names` doesn't [src/ui/mod.rs:148,835] — deferred: rework needed for index mapping
- Cover art `set_cover_path` is dead code with fragile widget tree traversal [src/ui/widgets/album_cover.rs:120-134] — deferred: pending cover art implementation

### MPD Protocol & Data
- `search_albums` misses `AlbumArtist` tag, producing empty artists for compilations [src/mpd/mod.rs:283-290] — deferred: needs `AlbumArtist:` parsing
- `list_albums_grouped` loses artist for Date/Genre groupings — MPD protocol limitation [src/mpd/mod.rs:386-387] — deferred: Artist grouping fixed; Date/Genre would need per-album fetch
- `search_albums` stale artist across tracks when Artist tag missing [src/mpd/mod.rs:279] — deferred: `file:` line reset added; edge case rare in practice
- `InsertNext` uses potentially stale `current_pos` from last poll [src/mpd/state_machine.rs:398-424] — deferred: 500ms max staleness; rare impact
- Cue/DSD rows clickable but navigate to same directory (no-op) [src/ui/widgets/folder_tree.rs:186-207] — deferred: needs proper navigation/playback implementation

### Settings & Config
- Settings changes require app restart — no live reconnect [src/ui/mod.rs:367-403] — deferred: requires MPD event loop restart capability
- `Config::load()` called twice during startup [src/main.rs, src/ui/mod.rs] — deferred: minor I/O waste; config unlikely to change between calls

### Search & Index
- Search index not rebuilt on MPD library update while connected [src/ui/mod.rs:592-595] — deferred: reset on reconnect only; no idle-update detection

### Other
- `ExponentialBackoff` derive(Clone) is misleading — fresh copy with same `current` value [src/mpd/state_machine.rs:82-85] — deferred: never actually cloned; maintenance risk only
- Folder tree Left/BackSpace at root sends unnecessary `ListDirectory("")` [src/ui/widgets/folder_tree.rs:68-72] — deferred: harmless no-op
