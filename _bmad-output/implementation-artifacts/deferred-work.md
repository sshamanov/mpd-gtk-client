## Deferred Work (Current)

This file lists known issues, bugs, and missing features that are deferred. Items marked **[RESOLVED]** have been addressed by architecture decisions in `architecture.md` or `prd.md` and remain as implementation items only.

### Resolved by Architecture (2026-04-29)

| Deferred Item | Resolution |
|---------------|------------|
| Dead MPD connection never triggers reconnect | **[RESOLVED in architecture.md §4-5]** Connection lifecycle: 3 consecutive failures → reconnect. Proven pattern from validation session. |
| `MpdEvent::Reconnected` dead code | **[RESOLVED]** Variant removed, proven in validation session. |
| Settings changes require app restart | **[RESOLVED in architecture.md §5a]** Live reconnect via `MpdCommand::Reconnect` with epoch counter. |
| Search index not rebuilt on library update | **[RESOLVED in architecture.md §9c, §10a]** Index rebuild lifecycle defined. Skip-if-unchanged optimization documented. |
| Cover art `set_cover_path` dead code | **[RESOLVED in architecture.md §Cover Art Pipeline]** Full pipeline designed with widget registry pattern. Pending implementation. |
| Toast auto-dismiss race | **[RESOLVED]** Generation counter fix proven in validation session. |

### Remaining — Deferred

#### UI & Interaction

- **Grouped-mode double-click plays wrong album** — FlowBox child index includes group headers but `album_names` doesn't. Reindexing needed. [src/ui/]
- **Queue key handler uses stale `item_ids` after rapid Shift+Up/Down** — Race window <10ms. Queue event rebuilds map. [src/ui/]
- **`populate_album_grid` and `populate_grouped_grid` run synchronously on GTK thread** — Large libraries may freeze UI. GtkGridView factory pattern designed but not implemented. [src/ui/]
- **Cue/DSD rows clickable but navigate to same directory (no-op)** — Needs proper navigation/playback. [src/ui/widgets/folder_tree.rs]
- **`glib::timeout_add_local` at 30ms can starve GTK main loop under heavy load** — 64-event batch limit mitigates. Full fix needs GTK4 frame clock integration.

#### MPD Protocol & Data

- **`search_albums` misses `AlbumArtist` tag** — Compilations show empty artists. Needs `AlbumArtist:` parsing in adapter. [src/mpd/mod.rs]
- **`list_albums_grouped` loses artist for Date/Genre groupings** — MPD protocol limitation. Per-album fetch needed for non-Artist groupings. [src/mpd/]
- **`InsertNext` uses potentially stale `current_pos` from last poll** — Command response is immediate; poll staleness is bounded by idle response time.
- **`search_albums` stale artist across tracks when Artist tag missing** — Rare edge case. `file:` line reset added but edge cases remain.

#### Infrastructure

- **`MpdAdapter` has no `Drop` — no clean MPD close on shutdown** — Socket closes on process exit. Graceful close adds ~50ms to shutdown.
- **SIGINT cleanup via `process::exit(0)` skips graceful MPD disconnect** — `idle_add` defers exit. Full cleanup needs GTK lifecycle work.
- **`ExponentialBackoff` derive(Clone) is misleading** — Maintenance risk only. Never actually cloned.
- **Config loaded twice during startup** — Minor I/O waste. Config unlikely to change between calls.
- **Folder tree Left/BackSpace at root sends `ListDirectory("")`** — Harmless no-op.
- **Unicode symbols in GTK labels may not render on all systems** — Portability concern.

#### Cover Art (pending implementation)

All items in `src/coverart/mod.rs` are pending implementation:
- CoverProvider cache read (designed)
- AlbumArtProvider MPD binary fetch (designed)
- ReadPictureProvider fallback (designed)  
- ActualRead queue in idle cycles (designed)
- Widget registry integration for in-place updates (designed)
