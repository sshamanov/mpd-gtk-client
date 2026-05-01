## Deferred Work (Current)

This file lists known issues, bugs, and missing features that are deferred. Items marked **[RESOLVED]** have been addressed by architecture decisions in `architecture.md` or `prd.md` and remain as implementation items only.

### Resolved

| Deferred Item | Resolution | Date |
|---------------|------------|------|
| Dead MPD connection never triggers reconnect | **[RESOLVED in architecture.md §4-5]** Connection lifecycle: 3 consecutive failures → reconnect. | 2026-04-29 |
| `MpdEvent::Reconnected` dead code | **[RESOLVED]** Variant removed. | 2026-04-29 |
| Settings changes require app restart | **[RESOLVED in architecture.md §5a]** Live reconnect via `MpdCommand::Reconnect` with epoch counter. | 2026-04-29 |
| Search index not rebuilt on library update | **[RESOLVED in architecture.md §9c, §10a]** Index rebuild lifecycle defined. | 2026-04-29 |
| Cover art `set_cover_path` dead code | **[RESOLVED in architecture.md]** Full pipeline designed with widget registry pattern. | 2026-04-29 |
| Toast auto-dismiss race | **[RESOLVED]** Generation counter fix. | 2026-04-29 |
| `search_albums` misses `AlbumArtist` tag | **[RESOLVED in code]** Commit `68f3593` — `AlbumArtist:` lines parsed, used as fallback when per-track `Artist:` missing. | 2026-04-29 |

### Skipped (2026-04-29)

These items are real but not worth implementing at this stage — zero or negligible impact:

| Item | Reason |
|------|--------|
| `ExponentialBackoff` derive(Clone) is misleading | Never actually cloned. Maintenance risk only. |
| Config loaded twice during startup | Minor I/O waste on a tiny TOML file. |
| Folder tree Left/BackSpace at root sends `ListDirectory("")` | Harmless MPD no-op. |
| Unicode symbols in GTK labels may not render | Portability concern. All modern Linux systems render them. |

### Active Backlog

#### UI & Interaction

- **Grouped-mode double-click plays wrong album** — FlowBox child index includes group headers but `album_names` doesn't. Reindexing needed. [src/ui/]
- **Queue key handler uses stale `item_ids` after rapid Shift+Up/Down** — Race window <10ms. Queue event rebuilds map. [src/ui/]
- **`populate_album_grid` and `populate_grouped_grid` run synchronously on GTK thread** — Large libraries may freeze UI. GtkGridView factory pattern designed but not implemented. [src/ui/]
- **Cue/DSD rows clickable but navigate to same directory (no-op)** — Needs proper navigation/playback. [src/ui/widgets/folder_tree.rs]
- **`glib::timeout_add_local` at 30ms can starve GTK main loop under heavy load** — 64-event batch limit mitigates. Full fix needs GTK4 frame clock integration.

#### MPD Protocol & Data

- **`list_albums_grouped` loses artist for Date/Genre groupings** — MPD protocol limitation. Per-album fetch needed for non-Artist groupings. [src/mpd/]
- **`InsertNext` uses potentially stale `current_pos` from last poll** — Command response is immediate; poll staleness is bounded by idle response time.
- **`search_albums` stale artist across tracks when Artist tag missing** — Rare edge case. `file:` line reset added but edge cases remain.

#### Infrastructure

- **`MpdAdapter` has no `Drop` — no clean MPD close on shutdown** — Socket closes on process exit. Graceful close adds ~50ms to shutdown.
- **SIGINT cleanup via `process::exit(0)` skips graceful MPD disconnect** — `idle_add` defers exit. Full cleanup needs GTK lifecycle work.

#### Cover Art (v2 pipeline)

Current `src/coverart/mod.rs` is a basic single-file fetcher (v1). The v2 architecture in `architecture.md` designs a full `CoverProvider + ActualRead` two-layer pipeline that remains unimplemented:
- CoverProvider cache read (synchronous, no fallthrough)
- AlbumArtProvider MPD binary fetch (content-addressed via MD5)
- ReadPictureProvider fallback (timestamp-compared)
- ActualRead priority queue with scroll-aware loading (one album per idle cycle)
- Online cover lookup support (opt-in, rate-limited)
- Widget registry integration for in-place cell updates

#### Deferred from: code review of story 14-2 (2026-05-01)

- **MPRIS PropertiesChanged signal emission** — Property getters read SharedState directly, sufficient for playerctl/mpris-remote. Lock screen and GNOME Shell media controls don't auto-update without signal emission.
- **SharedState playback fields never populated** — Pre-existing: MPD StateChanged handler in ui/mod.rs extracts metadata to local variables but never writes to SharedState. `s.current.track`, `s.current.album` always None. Affects any code reading current track metadata from SharedState, including MPRIS, not just this story.
- **D-Bus session bus disconnection mid-session** — No monitoring or reconnection for D-Bus session bus drops. MPRIS silently stops responding if D-Bus restarts.
